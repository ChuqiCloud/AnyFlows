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
    BillingReservationId, GatewayPrincipal, GroupId, Quota, SubscriptionCycle, SubscriptionWindow,
    TokenId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, JsonValue, PaginatorTrait,
    QueryFilter, entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tokio::{sync::Barrier, task::JoinSet};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PoolOptions, QuotaRepository,
    QuotaRepositoryError,
    entity::{
        BillingReservationKey, TokenHash, billing_group_window_reservations, billing_reservations,
        groups, tokens, users,
    },
    quota::{QuotaMutationOutcome, QuotaRepositoryOperation, QuotaReservationStatus},
};

const TEST_QUOTA: i64 = 1_000_000;
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct WindowConfig {
    limits: [Option<i64>; 3],
    usages: [i64; 3],
    starts: [TimeDateTimeWithTimeZone; 3],
}

impl WindowConfig {
    fn new(
        limits: [Option<i64>; 3],
        usages: [i64; 3],
        starts: [TimeDateTimeWithTimeZone; 3],
    ) -> Self {
        Self {
            limits,
            usages,
            starts,
        }
    }
}

struct Fixture {
    pool: DatabasePool,
    repository: QuotaRepository,
    group_id: i64,
    principals: [GatewayPrincipal; 2],
}

impl Fixture {
    async fn close(self) -> Result<(), crate::DatabaseError> {
        let pool = self.pool.clone();
        drop(self);
        pool.close().await
    }
}

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-group-window-{}-{serial}.db",
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

#[tokio::test]
async fn blocking_group_windows_return_retry_or_permanent_shortage() -> Result<(), Box<dyn Error>> {
    let starts = current_window_starts()?;
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(200), Some(300)],
        [90, 190, 290],
        starts,
    ))
    .await?;
    let error = fixture
        .repository
        .precharge(reservation_id(1), fixture.principals[0], quota(11))
        .await
        .expect_err("任一共享窗口不足时不得创建部分预留");
    let QuotaRepositoryError::GroupWindowQuotaInsufficient {
        retry_after: Some(retry_after),
    } = error
    else {
        panic!("分组窗口不足应返回可恢复等待时间：{error:?}");
    };
    assert!(retry_after.seconds() > 0);
    assert_eq!(reservation_count(&fixture).await?, 0);
    assert_eq!(allocation_count(&fixture).await?, 0);
    fixture.close().await?;

    let hard = new_fixture(WindowConfig::new([Some(10), None, None], [0, 0, 0], starts)).await?;
    assert_eq!(
        hard.repository
            .precharge(reservation_id(2), hard.principals[0], quota(11))
            .await,
        Err(QuotaRepositoryError::GroupWindowQuotaInsufficient { retry_after: None })
    );
    assert_eq!(reservation_count(&hard).await?, 0);
    hard.close().await?;
    Ok(())
}

#[tokio::test]
async fn unlimited_group_windows_accumulate_and_zero_limit_rejects() -> Result<(), Box<dyn Error>> {
    let starts = current_window_starts()?;
    let fixture = new_fixture(WindowConfig::new([None, None, None], [5, 6, 7], starts)).await?;
    let id = reservation_id(3);
    fixture
        .repository
        .precharge(id, fixture.principals[0], quota(10))
        .await?;
    assert_eq!(group_usages(&fixture).await?, [5, 6, 7]);
    fixture.repository.settle(id, quota(8)).await?;
    assert_eq!(group_usages(&fixture).await?, [13, 14, 15]);
    fixture.close().await?;

    let zero = new_fixture(WindowConfig::new([Some(0), None, None], [0, 0, 0], starts)).await?;
    assert_eq!(
        zero.repository
            .precharge(reservation_id(4), zero.principals[0], quota(1))
            .await,
        Err(QuotaRepositoryError::GroupWindowQuotaInsufficient { retry_after: None })
    );
    zero.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_users_and_tokens_share_group_capacity() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let options = DatabaseOptions::new(database.url.clone())?.with_pool_options(PoolOptions {
        max_connections: 8,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(15),
        ..PoolOptions::default()
    });
    let fixture = fixture_with_options(
        options,
        WindowConfig::new([Some(50), None, None], [0, 0, 0], current_window_starts()?),
    )
    .await?;
    let barrier = Arc::new(Barrier::new(11));
    let mut tasks = JoinSet::new();
    for marker in 10..20 {
        let repository = fixture.repository.clone();
        let principal = fixture.principals[usize::from(marker % 2)];
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository
                .precharge(reservation_id(marker), principal, quota(10))
                .await
        });
    }
    barrier.wait().await;

    let mut applied = 0;
    let mut limited = 0;
    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok(QuotaMutationOutcome::Applied) => applied += 1,
            Err(QuotaRepositoryError::GroupWindowQuotaInsufficient {
                retry_after: Some(_),
            }) => limited += 1,
            outcome => panic!("并发分组窗口准入返回意外结果：{outcome:?}"),
        }
    }
    assert_eq!((applied, limited), (5, 5));
    assert_eq!(reservation_count(&fixture).await?, 5);
    assert_eq!(allocation_count(&fixture).await?, 5);
    assert_eq!(group_usages(&fixture).await?, [0, 0, 0]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn different_groups_keep_independent_capacity() -> Result<(), Box<dyn Error>> {
    let starts = current_window_starts()?;
    let fixture = new_fixture(WindowConfig::new([Some(10), None, None], [0, 0, 0], starts)).await?;
    let second_group = create_group(
        &fixture.pool,
        3,
        WindowConfig::new([Some(10), None, None], [0, 0, 0], starts),
    )
    .await?;
    let second_principal = create_principal(&fixture.pool, second_group.id, 3).await?;
    let first_id = reservation_id(30);
    let second_id = reservation_id(31);
    fixture
        .repository
        .precharge(first_id, fixture.principals[0], quota(10))
        .await?;
    fixture
        .repository
        .precharge(second_id, second_principal, quota(10))
        .await?;
    fixture.repository.settle(first_id, quota(10)).await?;
    fixture.repository.settle(second_id, quota(10)).await?;
    assert_eq!(
        group_usages_by_id(&fixture.pool, fixture.group_id).await?,
        [10, 10, 10]
    );
    assert_eq!(
        group_usages_by_id(&fixture.pool, second_group.id).await?,
        [10, 10, 10]
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn refund_releases_commitment_and_pending_settlement_can_replay() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture(WindowConfig::new(
        [Some(10), None, None],
        [0, 0, 0],
        current_window_starts()?,
    ))
    .await?;
    let first = reservation_id(40);
    let second = reservation_id(41);
    fixture
        .repository
        .precharge(first, fixture.principals[0], quota(5))
        .await?;
    fixture
        .repository
        .precharge(second, fixture.principals[1], quota(5))
        .await?;
    assert!(matches!(
        fixture.repository.settle(first, quota(8)).await,
        Err(QuotaRepositoryError::GroupWindowQuotaInsufficient {
            retry_after: Some(_)
        })
    ));
    let pending = reservation(&fixture, first).await?;
    assert_eq!(
        pending.status,
        QuotaReservationStatus::SettlementPending.code()
    );
    assert_eq!(group_usages(&fixture).await?, [0, 0, 0]);

    fixture.repository.refund(second).await?;
    assert_eq!(
        fixture.repository.settle(first, quota(8)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(group_usages(&fixture).await?, [8, 8, 8]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn settlement_counts_only_windows_matching_the_frozen_snapshot() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(100), Some(100)],
        [0, 0, 0],
        current_window_starts()?,
    ))
    .await?;
    let id = reservation_id(50);
    fixture
        .repository
        .precharge(id, fixture.principals[0], quota(20))
        .await?;
    let key = BillingReservationKey::parse(&id.persistence_key())?;
    let allocation = billing_group_window_reservations::Entity::find_by_id(key.clone())
        .one(fixture.pool.connection())
        .await?
        .expect("分组窗口快照必须存在");
    assert_ne!(allocation.daily_window_start, allocation.created_at);
    billing_group_window_reservations::Entity::update_many()
        .col_expr(
            billing_group_window_reservations::Column::DailyWindowStart,
            Expr::value(allocation.created_at),
        )
        .filter(billing_group_window_reservations::Column::IdempotencyKey.eq(key))
        .exec(fixture.pool.connection())
        .await?;

    fixture.repository.settle(id, quota(12)).await?;
    assert_eq!(group_usages(&fixture).await?, [0, 12, 12]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn outcome_unknown_replay_never_duplicates_group_window_usage() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(100), Some(100)],
        [0, 0, 0],
        current_window_starts()?,
    ))
    .await?;
    let id = reservation_id(60);
    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Precharge);
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principals[0], quota(20))
            .await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principals[0], quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(allocation_count(&fixture).await?, 1);

    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Settle);
    assert_eq!(
        fixture.repository.settle(id, quota(12)).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture.repository.settle(id, quota(12)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(group_usages(&fixture).await?, [12, 12, 12]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn damaged_active_group_snapshot_fails_closed() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(100), Some(100)],
        [0, 0, 0],
        current_window_starts()?,
    ))
    .await?;
    let first = reservation_id(70);
    fixture
        .repository
        .precharge(first, fixture.principals[0], quota(20))
        .await?;
    let key = BillingReservationKey::parse(&first.persistence_key())?;
    billing_group_window_reservations::Entity::update_many()
        .col_expr(
            billing_group_window_reservations::Column::ReservedQuota,
            Expr::value(19_i64),
        )
        .filter(billing_group_window_reservations::Column::IdempotencyKey.eq(key.clone()))
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture
            .repository
            .precharge(reservation_id(71), fixture.principals[1], quota(1))
            .await,
        Err(QuotaRepositoryError::Invariant)
    );
    billing_group_window_reservations::Entity::update_many()
        .col_expr(
            billing_group_window_reservations::Column::ReservedQuota,
            Expr::value(20_i64),
        )
        .filter(billing_group_window_reservations::Column::IdempotencyKey.eq(key.clone()))
        .exec(fixture.pool.connection())
        .await?;
    billing_group_window_reservations::Entity::delete_by_id(key)
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture.repository.settle(first, quota(10)).await,
        Err(QuotaRepositoryError::Invariant)
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn expired_calendar_windows_reset_before_new_precharge() -> Result<(), Box<dyn Error>> {
    let stale = previous_window_starts()?;
    let fixture = new_fixture(WindowConfig::new(
        [Some(5), Some(5), Some(5)],
        [5, 5, 5],
        stale,
    ))
    .await?;
    let id = reservation_id(80);
    fixture
        .repository
        .precharge(id, fixture.principals[0], quota(5))
        .await?;
    let current = group(&fixture).await?;
    assert_eq!(
        [
            current.daily_usage,
            current.weekly_usage,
            current.monthly_usage
        ],
        [0, 0, 0]
    );
    assert_eq!(
        [
            current.daily_window_start,
            current.weekly_window_start,
            current.monthly_window_start,
        ],
        current_window_starts()?
    );
    fixture.repository.settle(id, quota(3)).await?;
    assert_eq!(group_usages(&fixture).await?, [3, 3, 3]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_tasks_never_create_or_consume_group_window_snapshot() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture(WindowConfig::new(
        [Some(0), Some(0), Some(0)],
        [0, 0, 0],
        current_window_starts()?,
    ))
    .await?;
    let id = reservation_id(90);
    fixture
        .repository
        .reserve_batch_task(id, fixture.principals[0], quota(5))
        .await?;
    assert_eq!(allocation_count(&fixture).await?, 0);
    fixture.repository.settle_batch_task(id, quota(3)).await?;
    assert_eq!(group_usages(&fixture).await?, [0, 0, 0]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn active_batch_task_does_not_block_a_regular_request() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture(WindowConfig::new(
        [Some(10), Some(10), Some(10)],
        [0, 0, 0],
        current_window_starts()?,
    ))
    .await?;
    let batch_id = reservation_id(91);
    fixture
        .repository
        .reserve_batch_task(batch_id, fixture.principals[0], quota(5))
        .await?;
    assert_eq!(allocation_count(&fixture).await?, 0);

    let request_id = reservation_id(92);
    fixture
        .repository
        .precharge(request_id, fixture.principals[0], quota(4))
        .await?;
    assert_eq!(allocation_count(&fixture).await?, 1);
    fixture.repository.settle(request_id, quota(3)).await?;
    assert_eq!(group_usages(&fixture).await?, [3, 3, 3]);

    fixture
        .repository
        .settle_batch_task(batch_id, quota(2))
        .await?;
    assert_eq!(group_usages(&fixture).await?, [3, 3, 3]);
    fixture.close().await?;
    Ok(())
}

async fn new_fixture(config: WindowConfig) -> Result<Fixture, Box<dyn Error>> {
    fixture_with_options(DatabaseOptions::new("sqlite::memory:")?, config).await
}

async fn fixture_with_options(
    options: DatabaseOptions,
    config: WindowConfig,
) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = create_group(&pool, 1, config).await?;
    let first = create_principal(&pool, group.id, 1).await?;
    let second = create_principal(&pool, group.id, 2).await?;
    Ok(Fixture {
        repository: QuotaRepository::new(pool.clone()),
        pool,
        group_id: group.id,
        principals: [first, second],
    })
}

async fn create_group(
    pool: &DatabasePool,
    marker: u8,
    config: WindowConfig,
) -> Result<groups::Model, sea_orm::DbErr> {
    groups::ActiveModel {
        name: Set(format!("group-window-{marker}")),
        display_name: Set(format!("分组窗口测试 {marker}")),
        daily_limit: Set(config.limits[0]),
        weekly_limit: Set(config.limits[1]),
        monthly_limit: Set(config.limits[2]),
        daily_usage: Set(config.usages[0]),
        weekly_usage: Set(config.usages[1]),
        monthly_usage: Set(config.usages[2]),
        daily_window_start: Set(config.starts[0]),
        weekly_window_start: Set(config.starts[1]),
        monthly_window_start: Set(config.starts[2]),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn create_principal(
    pool: &DatabasePool,
    group_id: i64,
    marker: u8,
) -> Result<GatewayPrincipal, Box<dyn Error>> {
    let user = users::ActiveModel {
        username: Set(format!("group-window-user-{group_id}-{marker}")),
        status: Set(1),
        default_group_id: Set(group_id),
        quota: Set(TEST_QUOTA),
        used_quota: Set(0),
        frozen_quota: Set(0),
        request_count: Set(0),
        aff_code: Set(format!("group-window-aff-{group_id}-{marker}")),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&format!("{marker:02x}").repeat(32))?),
        key_prefix: Set(format!("sk-af-group-{marker}")),
        name: Set(format!("group-window-token-{marker}")),
        status: Set(1),
        group_id: Set(Some(group_id)),
        remain_quota: Set(TEST_QUOTA),
        unlimited_quota: Set(true),
        used_quota: Set(0),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group_id)?,
    ))
}

async fn group(fixture: &Fixture) -> Result<groups::Model, sea_orm::DbErr> {
    Ok(groups::Entity::find_by_id(fixture.group_id)
        .one(fixture.pool.connection())
        .await?
        .expect("测试分组必须存在"))
}

async fn group_usages(fixture: &Fixture) -> Result<[i64; 3], sea_orm::DbErr> {
    group_usages_by_id(&fixture.pool, fixture.group_id).await
}

async fn group_usages_by_id(
    pool: &DatabasePool,
    group_id: i64,
) -> Result<[i64; 3], sea_orm::DbErr> {
    let group = groups::Entity::find_by_id(group_id)
        .one(pool.connection())
        .await?
        .expect("测试分组必须存在");
    Ok([group.daily_usage, group.weekly_usage, group.monthly_usage])
}

async fn reservation(
    fixture: &Fixture,
    id: BillingReservationId,
) -> Result<billing_reservations::Model, sea_orm::DbErr> {
    Ok(billing_reservations::Entity::find_by_id(
        BillingReservationKey::parse(&id.persistence_key()).expect("测试预留标识必须有效"),
    )
    .one(fixture.pool.connection())
    .await?
    .expect("测试预留必须存在"))
}

async fn reservation_count(fixture: &Fixture) -> Result<u64, sea_orm::DbErr> {
    billing_reservations::Entity::find()
        .count(fixture.pool.connection())
        .await
}

async fn allocation_count(fixture: &Fixture) -> Result<u64, sea_orm::DbErr> {
    billing_group_window_reservations::Entity::find()
        .count(fixture.pool.connection())
        .await
}

fn current_window_starts() -> Result<[TimeDateTimeWithTimeZone; 3], Box<dyn Error>> {
    let now = u64::try_from(TimeDateTimeWithTimeZone::now_utc().unix_timestamp())?;
    window_starts_at(now)
}

fn previous_window_starts() -> Result<[TimeDateTimeWithTimeZone; 3], Box<dyn Error>> {
    let current = current_window_starts()?;
    let mut previous = current;
    for (index, cycle) in cycles().into_iter().enumerate() {
        let before = u64::try_from(current[index].unix_timestamp())?
            .checked_sub(1)
            .ok_or("测试窗口前驱时间不得下溢")?;
        let window = SubscriptionWindow::initial(cycle, before)?;
        previous[index] = timestamp(window.started_at())?;
    }
    Ok(previous)
}

fn window_starts_at(
    timestamp_seconds: u64,
) -> Result<[TimeDateTimeWithTimeZone; 3], Box<dyn Error>> {
    let mut starts = [TimeDateTimeWithTimeZone::UNIX_EPOCH; 3];
    for (index, cycle) in cycles().into_iter().enumerate() {
        starts[index] =
            timestamp(SubscriptionWindow::initial(cycle, timestamp_seconds)?.started_at())?;
    }
    Ok(starts)
}

fn cycles() -> [SubscriptionCycle; 3] {
    [
        SubscriptionCycle::Daily,
        SubscriptionCycle::Weekly,
        SubscriptionCycle::Monthly,
    ]
}

fn timestamp(value: u64) -> Result<TimeDateTimeWithTimeZone, Box<dyn Error>> {
    Ok(TimeDateTimeWithTimeZone::from_unix_timestamp(
        i64::try_from(value)?,
    )?)
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试预留标识必须有效")
}

fn quota(units: i64) -> Quota {
    Quota::new(units).expect("测试额度必须有效")
}
