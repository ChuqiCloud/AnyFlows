use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipId, Protocol, UserId,
};
use rust_decimal::Decimal;
use sea_orm::{ActiveModelTrait, Set, entity::prelude::TimeDateTimeWithTimeZone};

use crate::entity::{groups, users};
use crate::{
    DatabaseOptions, DatabaseTimestamp, MigrationOptions,
    OrganizationApprovalContractPriceExecutionOutcome,
    OrganizationApprovalContractPriceExecutionWrite, OrganizationApprovalDecisionWrite,
    OrganizationApprovalExecutionRepository, OrganizationApprovalExecutionRepositoryError,
    OrganizationApprovalRepository, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestWrite, OrganizationApprovalTemplateWrite,
    OrganizationContractPriceRepository, OrganizationContractPriceWrite, OrganizationCreate,
    OrganizationRepository, PoolOptions, organization_approval_contract_price_resource_summary,
};

struct Fixture {
    pool: crate::DatabasePool,
    organization_id: OrganizationId,
    owner_user_id: UserId,
    owner_membership_id: OrganizationMembershipId,
    approval_repository: OrganizationApprovalRepository,
    contract_repository: OrganizationContractPriceRepository,
    execution_repository: OrganizationApprovalExecutionRepository,
}

#[tokio::test]
async fn approved_contract_price_execution_is_idempotent_and_conflict_safe()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("replay").await?;
    let effective_from = now() + 60;
    let prices = [
        Decimal::new(1, 2),
        Decimal::new(2, 2),
        Decimal::new(3, 2),
        Decimal::new(4, 2),
        Decimal::new(5, 2),
    ];
    let summary = summary(&fixture, "gpt-test", prices, effective_from, None, None)?;
    let request_id = create_approved_request(&fixture, 1, summary).await?;
    let write = execution_write(
        &fixture,
        request_id,
        "gpt-test",
        prices,
        effective_from,
        None,
        None,
    )?;

    let first = fixture
        .execution_repository
        .execute_contract_price(write.clone())
        .await?;
    let record = match first {
        OrganizationApprovalContractPriceExecutionOutcome::Applied(record) => record,
        OrganizationApprovalContractPriceExecutionOutcome::Existing(_) => {
            panic!("首次执行必须应用")
        }
    };
    assert_eq!(record.applied_version(), 1);
    assert_eq!(record.contract_price_id().get(), 1);
    assert!(matches!(
        fixture
            .execution_repository
            .execute_contract_price(write)
            .await?,
        OrganizationApprovalContractPriceExecutionOutcome::Existing(_)
    ));
    assert_eq!(
        fixture
            .contract_repository
            .list(fixture.organization_id)
            .await?
            .len(),
        1
    );

    let changed = execution_write(
        &fixture,
        request_id,
        "gpt-test",
        [
            Decimal::new(9, 2),
            prices[1],
            prices[2],
            prices[3],
            prices[4],
        ],
        effective_from,
        None,
        None,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_contract_price(changed)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approved_contract_price_execution_reuses_interval_and_scope_guards()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("interval").await?;
    fixture
        .contract_repository
        .create(OrganizationContractPriceWrite::new(
            fixture.organization_id,
            "gpt-test".to_owned(),
            Protocol::OpenAiChat,
            [Decimal::new(1, 2); 5],
            TimeDateTimeWithTimeZone::from_unix_timestamp(now() - 60)?,
            None,
            None,
            fixture.owner_user_id,
        ))
        .await?;
    let effective_from = now() + 60;
    let prices = [Decimal::new(2, 2); 5];
    let summary = summary(&fixture, "gpt-test", prices, effective_from, None, Some(1))?;
    let request_id = create_approved_request(&fixture, 2, summary).await?;
    let result = fixture
        .execution_repository
        .execute_contract_price(execution_write(
            &fixture,
            request_id,
            "gpt-test",
            prices,
            effective_from,
            None,
            Some(1),
        )?)
        .await;
    assert_eq!(
        result,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .contract_repository
            .list(fixture.organization_id)
            .await?
            .len(),
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

fn summary(
    fixture: &Fixture,
    model: &str,
    prices: [Decimal; 5],
    effective_from: i64,
    effective_until: Option<i64>,
    expected_version: Option<i64>,
) -> Result<String, Box<dyn Error>> {
    Ok(organization_approval_contract_price_resource_summary(
        fixture.organization_id,
        model,
        Protocol::OpenAiChat,
        prices,
        TimeDateTimeWithTimeZone::from_unix_timestamp(effective_from)?,
        effective_until
            .map(TimeDateTimeWithTimeZone::from_unix_timestamp)
            .transpose()?,
        expected_version,
    ))
}

fn execution_write(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    model: &str,
    prices: [Decimal; 5],
    effective_from: i64,
    effective_until: Option<i64>,
    expected_version: Option<i64>,
) -> Result<OrganizationApprovalContractPriceExecutionWrite, Box<dyn Error>> {
    Ok(OrganizationApprovalContractPriceExecutionWrite::new(
        fixture.organization_id,
        request_id,
        model.to_owned(),
        Protocol::OpenAiChat,
        prices,
        TimeDateTimeWithTimeZone::from_unix_timestamp(effective_from)?,
        effective_until
            .map(TimeDateTimeWithTimeZone::from_unix_timestamp)
            .transpose()?,
        expected_version,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?)
}

async fn create_approved_request(
    fixture: &Fixture,
    seed: u8,
    resource_summary: String,
) -> Result<OrganizationApprovalRequestId, Box<dyn Error>> {
    let template = fixture
        .approval_repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            OrganizationApprovalTemplateKey::new(format!("contract-price-{seed}"))?,
            OrganizationApprovalResourceType::ContractPrice,
            OrganizationApprovalTemplateDigest::new([seed; 32])?,
            fixture.owner_user_id,
            None,
        ))
        .await?;
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([seed; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::ContractPrice,
        resource_summary,
        template.id(),
        template.version(),
        fixture.owner_user_id,
        now() + 3_600,
    )?;
    let request_id = match fixture
        .approval_repository
        .create_request(OrganizationApprovalRequestWrite::new(request))
        .await?
    {
        OrganizationApprovalRequestCreateOutcome::Created(record) => record.id(),
        OrganizationApprovalRequestCreateOutcome::Existing(_) => panic!("测试申请键必须唯一"),
    };
    fixture
        .approval_repository
        .record_decision(OrganizationApprovalDecisionWrite::new(
            OrganizationApprovalDecision::new(
                OrganizationApprovalDecisionKey::new([seed.wrapping_add(10); 16])?,
                fixture.organization_id,
                request_id,
                OrganizationApprovalDecisionKind::Approved,
                Some(fixture.owner_user_id),
                None,
                now(),
            )?,
        ))
        .await?;
    Ok(request_id)
}

async fn new_fixture(suffix: &str) -> Result<Fixture, Box<dyn Error>> {
    let options = DatabaseOptions::new("sqlite::memory:")?.with_pool_options(PoolOptions {
        max_connections: 1,
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set(format!("approval-contract-price-{suffix}")),
        display_name: Set(format!("Approval Contract Price {suffix}")),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner_user_id = create_user(&pool, group.id, suffix).await?;
    let organization_repository =
        OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, owner) = organization_repository
        .create(OrganizationCreate::new(
            format!("Approval Contract Price {suffix}"),
            format!("approval-contract-price-{suffix}"),
            owner_user_id,
            entitlement(),
        ))
        .await?
        .into_parts();
    Ok(Fixture {
        pool: pool.clone(),
        organization_id: organization.id(),
        owner_user_id,
        owner_membership_id: owner.id(),
        approval_repository: OrganizationApprovalRepository::new(
            pool.clone(),
            Duration::from_secs(5),
        )?,
        contract_repository: OrganizationContractPriceRepository::new(
            pool.clone(),
            Duration::from_secs(5),
        )?,
        execution_repository: OrganizationApprovalExecutionRepository::new(
            pool,
            Duration::from_secs(5),
        )?,
    })
}

fn entitlement() -> crate::OrganizationEntitlementWrite {
    crate::OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(20, 20, 20).unwrap(),
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )
        .unwrap(),
        "approval-contract-price-execution-test".to_owned(),
        None,
        None,
    )
    .unwrap()
}

async fn create_user(
    pool: &crate::DatabasePool,
    group_id: i64,
    suffix: &str,
) -> Result<UserId, Box<dyn Error>> {
    let user = users::ActiveModel {
        username: Set(format!("approval-contract-price-{suffix}")),
        email: Set(Some(format!(
            "approval-contract-price-{suffix}@example.com"
        ))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("approval-contract-price-{suffix}-aff")),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(UserId::new(user.id)?)
}

fn now() -> i64 {
    DatabaseTimestamp::now_utc().unix_timestamp()
}
