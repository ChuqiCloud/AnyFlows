use std::{
    error::Error,
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use af_domain::{
    OrganizationCreditCurrency, OrganizationCreditInvoiceKey,
    OrganizationCreditRepaymentAllocation, OrganizationCreditRepaymentAllocationKey,
    OrganizationCreditRepaymentAmount, OrganizationCreditRepaymentKey,
    OrganizationCreditRepaymentSnapshot, OrganizationCreditRepaymentSourceKey,
    OrganizationCreditRepaymentSourceKind, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use tokio::{sync::Barrier, task::JoinSet};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, OrganizationCreate,
    OrganizationCreditRepaymentAllocationOutcome, OrganizationCreditRepaymentCreateOutcome,
    OrganizationCreditRepaymentRepository, OrganizationCreditRepaymentRepositoryError,
    OrganizationEntitlementWrite, OrganizationRepository, PoolOptions,
    entity::{
        groups, organization_credit_invoices, organization_credit_repayment_allocations,
        organization_wallet_ledger_entries, organization_wallets, users,
    },
};

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-credit-repayment-{}-{serial}.db",
            std::process::id()
        ));
        let mut sqlite_path = path.to_string_lossy().replace('\\', "/");
        if !sqlite_path.starts_with('/') {
            sqlite_path.insert(0, '/');
        }
        Self {
            path,
            url: format!("sqlite://{sqlite_path}?mode=rwc"),
        }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = std::path::PathBuf::from(format!("{}{}", self.path.display(), suffix));
            let _ = fs::remove_file(candidate);
        }
    }
}

struct Fixture {
    pool: DatabasePool,
    repository: OrganizationCreditRepaymentRepository,
    organization_id: OrganizationId,
    database: TestDatabase,
}

impl Fixture {
    async fn close(self) -> Result<(), crate::DatabaseError> {
        let pool = self.pool.clone();
        let database = self.database;
        drop(self.repository);
        drop(self.pool);
        pool.close().await?;
        drop(database);
        Ok(())
    }
}

#[tokio::test]
async fn repayment_replay_allocation_settlement_and_wallet_isolation() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture().await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let invoice = insert_invoice(&fixture, 3, 4, 100, now).await?;
    let repayment_snapshot = repayment(5, 6, fixture.organization_id, 100, now)?;

    let created = fixture
        .repository
        .record(repayment_snapshot.clone())
        .await?;
    let created_id = match created {
        OrganizationCreditRepaymentCreateOutcome::Created(record) => record.id(),
        OrganizationCreditRepaymentCreateOutcome::Existing(_) => panic!("首次登记必须创建"),
    };
    let replayed = fixture
        .repository
        .record(repayment_snapshot.clone())
        .await?;
    match replayed {
        OrganizationCreditRepaymentCreateOutcome::Existing(record) => {
            assert_eq!(record.id(), created_id);
        }
        OrganizationCreditRepaymentCreateOutcome::Created(_) => panic!("重放不得创建第二笔还款"),
    }
    let conflicting_key = repayment(5, 7, fixture.organization_id, 99, now)?;
    assert_eq!(
        fixture.repository.record(conflicting_key).await,
        Err(OrganizationCreditRepaymentRepositoryError::Conflict)
    );
    let conflicting_source = repayment(7, 6, fixture.organization_id, 100, now)?;
    assert_eq!(
        fixture.repository.record(conflicting_source).await,
        Err(OrganizationCreditRepaymentRepositoryError::Conflict)
    );

    let wallet_before = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(fixture.organization_id.get()))
        .one(fixture.pool.connection())
        .await?
        .expect("企业钱包应存在");
    let first = allocation(8, 5, 3, fixture.organization_id, 40, now)?;
    let first_created = fixture.repository.allocate(first.clone()).await?;
    match first_created {
        OrganizationCreditRepaymentAllocationOutcome::Created(record) => {
            assert!(!record.invoice_settled());
        }
        OrganizationCreditRepaymentAllocationOutcome::Existing(_) => {
            panic!("首次核销必须创建")
        }
    }
    match fixture.repository.allocate(first).await? {
        OrganizationCreditRepaymentAllocationOutcome::Existing(record) => {
            assert!(!record.invoice_settled());
        }
        OrganizationCreditRepaymentAllocationOutcome::Created(_) => panic!("重放不得重复核销"),
    }
    let second = allocation(9, 5, 3, fixture.organization_id, 60, now)?;
    match fixture.repository.allocate(second).await? {
        OrganizationCreditRepaymentAllocationOutcome::Created(record) => {
            assert!(record.invoice_settled());
        }
        OrganizationCreditRepaymentAllocationOutcome::Existing(_) => {
            panic!("第二笔核销必须创建")
        }
    }
    assert_eq!(
        fixture
            .repository
            .allocate(allocation(10, 5, 3, fixture.organization_id, 1, now)?)
            .await,
        Err(OrganizationCreditRepaymentRepositoryError::InvalidInvoiceState)
    );

    let invoice = organization_credit_invoices::Entity::find_by_id(invoice.id)
        .one(fixture.pool.connection())
        .await?
        .expect("账单应存在");
    assert_eq!(invoice.status, 3);
    assert_eq!(invoice.version, 2);
    assert!(invoice.settled_at.is_some());
    let allocations = organization_credit_repayment_allocations::Entity::find()
        .filter(
            organization_credit_repayment_allocations::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(allocations.len(), 2);
    assert_eq!(
        allocations.iter().map(|row| row.amount_minor).sum::<i64>(),
        100
    );

    let wallet_after = organization_wallets::Entity::find_by_id(wallet_before.id)
        .one(fixture.pool.connection())
        .await?
        .expect("企业钱包应存在");
    assert_eq!(wallet_after.quota, wallet_before.quota);
    assert_eq!(wallet_after.used_quota, wallet_before.used_quota);
    assert_eq!(wallet_after.frozen_quota, wallet_before.frozen_quota);
    assert_eq!(wallet_after.version, wallet_before.version);
    assert_eq!(
        organization_wallet_ledger_entries::Entity::find()
            .filter(
                organization_wallet_ledger_entries::Column::OrganizationId
                    .eq(fixture.organization_id.get()),
            )
            .count(fixture.pool.connection())
            .await?,
        0
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_allocations_cannot_exceed_invoice_amount() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    insert_invoice(&fixture, 11, 12, 100, now).await?;
    fixture
        .repository
        .record(repayment(13, 14, fixture.organization_id, 200, now)?)
        .await?;

    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = JoinSet::new();
    for allocation_key in [15, 16] {
        let repository = fixture.repository.clone();
        let barrier = barrier.clone();
        let write = allocation(allocation_key, 13, 11, fixture.organization_id, 60, now)?;
        tasks.spawn(async move {
            barrier.wait().await;
            repository.allocate(write).await
        });
    }
    barrier.wait().await;

    let mut created = 0;
    let mut rejected = 0;
    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok(OrganizationCreditRepaymentAllocationOutcome::Created(_)) => created += 1,
            Err(OrganizationCreditRepaymentRepositoryError::InvoiceOverallocated) => rejected += 1,
            other => panic!("并发核销结果无效：{other:?}"),
        }
    }
    assert_eq!((created, rejected), (1, 1));
    let rows = organization_credit_repayment_allocations::Entity::find()
        .filter(
            organization_credit_repayment_allocations::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].amount_minor, 60);

    fixture.close().await?;
    Ok(())
}

async fn new_fixture() -> Result<Fixture, Box<dyn Error>> {
    let database = TestDatabase::new();
    let options = DatabaseOptions::new(database.url.clone())?.with_pool_options(PoolOptions {
        max_connections: 4,
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set("credit-repayment-test-group".to_owned()),
        display_name: Set("Credit Repayment Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = users::ActiveModel {
        username: Set("credit-repayment-owner".to_owned()),
        email: Set(Some("credit-repayment-owner@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("credit-repayment-owner-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let organization_repository =
        OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let entitlement = OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10)?,
            false,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )?,
        "contract".to_owned(),
        None,
        None,
    )?;
    let organization = organization_repository
        .create(OrganizationCreate::new(
            "Credit Repayment Organization".to_owned(),
            "credit-repayment-organization".to_owned(),
            UserId::new(owner.id)?,
            entitlement,
        ))
        .await?
        .into_parts()
        .0;
    let repository =
        OrganizationCreditRepaymentRepository::new(pool.clone(), Duration::from_secs(10))?;
    Ok(Fixture {
        pool,
        repository,
        organization_id: organization.id(),
        database,
    })
}

async fn insert_invoice(
    fixture: &Fixture,
    invoice_key: u8,
    source_key: u8,
    amount_minor: i64,
    issued_at: i64,
) -> Result<organization_credit_invoices::Model, Box<dyn Error>> {
    let issued_at = TimeDateTimeWithTimeZone::from_unix_timestamp(issued_at)?;
    let due_at = TimeDateTimeWithTimeZone::from_unix_timestamp(
        issued_at.unix_timestamp() + 30 * 24 * 60 * 60,
    )?;
    Ok(organization_credit_invoices::ActiveModel {
        invoice_key: Set(OrganizationCreditInvoiceKey::new([invoice_key; 16])?.persistence_key()),
        organization_id: Set(fixture.organization_id.get()),
        source_kind: Set(1),
        source_key: Set(format!("{source_key:02x}").repeat(16)),
        currency: Set("CNY".to_owned()),
        amount_minor: Set(amount_minor),
        term_version: Set(1),
        period_kind: Set(1),
        period_days: Set(Some(30)),
        issued_at: Set(issued_at),
        due_at: Set(due_at),
        status: Set(1),
        status_changed_at: Set(issued_at),
        version: Set(1),
        settled_at: Set(None),
        voided_at: Set(None),
        created_at: Set(issued_at),
        updated_at: Set(issued_at),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?)
}

fn repayment(
    repayment_key: u8,
    source_key: u8,
    organization_id: OrganizationId,
    amount_minor: i64,
    received_at: i64,
) -> Result<OrganizationCreditRepaymentSnapshot, Box<dyn Error>> {
    Ok(OrganizationCreditRepaymentSnapshot::new(
        organization_id,
        OrganizationCreditRepaymentKey::new([repayment_key; 16])?,
        OrganizationCreditRepaymentSourceKind::BankTransfer,
        OrganizationCreditRepaymentSourceKey::new([source_key; 16])?,
        OrganizationCreditCurrency::new("CNY")?,
        OrganizationCreditRepaymentAmount::new(amount_minor)?,
        received_at,
    )?)
}

fn allocation(
    allocation_key: u8,
    repayment_key: u8,
    invoice_key: u8,
    organization_id: OrganizationId,
    amount_minor: i64,
    allocated_at: i64,
) -> Result<OrganizationCreditRepaymentAllocation, Box<dyn Error>> {
    Ok(OrganizationCreditRepaymentAllocation::new(
        organization_id,
        OrganizationCreditRepaymentAllocationKey::new([allocation_key; 16])?,
        OrganizationCreditRepaymentKey::new([repayment_key; 16])?,
        OrganizationCreditInvoiceKey::new([invoice_key; 16])?,
        OrganizationCreditCurrency::new("CNY")?,
        OrganizationCreditRepaymentAmount::new(amount_minor)?,
        allocated_at,
    )?)
}
