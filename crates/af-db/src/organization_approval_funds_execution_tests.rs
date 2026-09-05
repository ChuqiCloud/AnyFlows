use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipId, OrganizationRole, QuotaDelta, UserId, WalletEventId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, Expr, PaginatorTrait, QueryFilter, Set};

use crate::entity::{
    groups, organization_approval_funds_executions, organization_approval_requests,
    organization_wallets, users,
};
use crate::{
    DatabaseOptions, DatabaseTimestamp, MigrationOptions, OrganizationApprovalDecisionWrite,
    OrganizationApprovalExecutionRepository, OrganizationApprovalExecutionRepositoryError,
    OrganizationApprovalFundsExecutionOutcome, OrganizationApprovalFundsExecutionWrite,
    OrganizationApprovalRepository, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestWrite, OrganizationApprovalTemplateWrite, OrganizationCreate,
    OrganizationEntitlementWrite, OrganizationMembershipCreate, OrganizationRepository,
    OrganizationWalletAdjustmentOutcome, OrganizationWalletAdjustmentWrite,
    OrganizationWalletRepository, PoolOptions,
};

struct Fixture {
    pool: crate::DatabasePool,
    organization_repository: OrganizationRepository,
    approval_repository: OrganizationApprovalRepository,
    wallet_repository: OrganizationWalletRepository,
    execution_repository: OrganizationApprovalExecutionRepository,
    organization_id: OrganizationId,
    owner_user_id: UserId,
    owner_membership_id: OrganizationMembershipId,
    member_user_id: UserId,
    member_membership_id: OrganizationMembershipId,
}

#[tokio::test]
async fn approved_funds_adjustment_is_applied_once_and_linked_to_ledger()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("apply").await?;
    let write = funds_write(&fixture, 0x11, -25, "审批扣减", 2)?;
    let request_id = create_request_and_decision(
        &fixture,
        11,
        write.resource_summary(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let write = OrganizationApprovalFundsExecutionWrite::new(
        fixture.organization_id,
        request_id,
        write.event_id(),
        write.quota_delta(),
        write.reason().to_owned(),
        write.expected_wallet_version(),
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;

    let applied = fixture
        .execution_repository
        .execute_funds_adjustment(write.clone())
        .await?;
    let record = match applied {
        OrganizationApprovalFundsExecutionOutcome::Applied(record) => record,
        OrganizationApprovalFundsExecutionOutcome::Existing(_) => panic!("首次执行必须应用"),
    };
    assert_eq!(record.balance_before().units(), 100);
    assert_eq!(record.balance_after().units(), 75);
    assert_eq!(record.expected_wallet_version(), 2);
    assert_eq!(record.applied_wallet_version(), 3);
    assert!(record.ledger_entry_id() > 0);
    assert!(matches!(
        fixture
            .execution_repository
            .execute_funds_adjustment(write)
            .await?,
        OrganizationApprovalFundsExecutionOutcome::Existing(_)
    ));
    assert_eq!(
        organization_approval_funds_executions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    let wallet = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(fixture.organization_id.get()))
        .one(fixture.pool.connection())
        .await?
        .expect("企业钱包必须存在");
    assert_eq!(wallet.quota, 75);
    assert_eq!(wallet.version, 3);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn funds_execution_rechecks_resource_decision_permission_expiration_and_scope()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("boundaries").await?;
    let rejected_write = funds_write(&fixture, 0x21, 10, "审批拒绝", 2)?;
    let rejected_id = create_request_and_decision(
        &fixture,
        21,
        rejected_write.resource_summary(),
        OrganizationApprovalDecisionKind::Rejected,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(rebuild_write(&fixture, rejected_id, &rejected_write),)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotApproved)
    );

    let wrong_resource_write = funds_write(&fixture, 0x31, 10, "资源类型", 2)?;
    let wrong_resource_id = create_request_and_decision_with_type(
        &fixture,
        31,
        OrganizationApprovalResourceType::Member,
        wrong_resource_write.resource_summary(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(rebuild_write(
                &fixture,
                wrong_resource_id,
                &wrong_resource_write
            ),)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch)
    );

    let permission_write = funds_write(&fixture, 0x41, 10, "权限复验", 2)?;
    let permission_id = create_request_and_decision(
        &fixture,
        41,
        permission_write.resource_summary(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let unauthorized = OrganizationApprovalFundsExecutionWrite::new(
        fixture.organization_id,
        permission_id,
        permission_write.event_id(),
        permission_write.quota_delta(),
        permission_write.reason().to_owned(),
        permission_write.expected_wallet_version(),
        fixture.member_membership_id,
        fixture.member_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(unauthorized)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Forbidden)
    );

    let expired_write = funds_write(&fixture, 0x51, 10, "过期复验", 2)?;
    let expired_id = create_request_and_decision(
        &fixture,
        51,
        expired_write.resource_summary(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    // 直接构造已过期持久化事实，避免并发门禁的调度时延影响决定写入。
    let update = organization_approval_requests::Entity::update_many()
        .filter(organization_approval_requests::Column::Id.eq(expired_id.get()))
        .col_expr(
            organization_approval_requests::Column::ExpiresAt,
            Expr::col(organization_approval_requests::Column::CreatedAt).into(),
        )
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(update.rows_affected, 1);
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(rebuild_write(&fixture, expired_id, &expired_write),)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Expired)
    );

    let other = create_organization(&fixture, "other").await?;
    let cross_org = OrganizationApprovalFundsExecutionWrite::new(
        other,
        permission_id,
        permission_write.event_id(),
        permission_write.quota_delta(),
        permission_write.reason().to_owned(),
        permission_write.expected_wallet_version(),
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(cross_org)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotFound)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn funds_execution_rejects_wallet_version_and_balance_conflicts() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture("freshness").await?;
    let stale_write = funds_write(&fixture, 0x61, 10, "版本冲突", 2)?;
    let stale_id = create_request_and_decision(
        &fixture,
        61,
        stale_write.resource_summary(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let extra = OrganizationWalletAdjustmentWrite::new(
        WalletEventId::new([0x62; 16])?,
        fixture.organization_id,
        fixture.owner_user_id,
        QuotaDelta::new(1)?,
        "并发钱包变更".to_owned(),
    )?;
    assert!(matches!(
        fixture.wallet_repository.adjust(extra).await?,
        OrganizationWalletAdjustmentOutcome::Applied(_)
    ));
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(rebuild_write(&fixture, stale_id, &stale_write),)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let insufficient_write = funds_write(&fixture, 0x71, -200, "余额不足", 3)?;
    let insufficient_id = create_request_and_decision(
        &fixture,
        71,
        insufficient_write.resource_summary(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_funds_adjustment(rebuild_write(
                &fixture,
                insufficient_id,
                &insufficient_write
            ),)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::InsufficientQuota)
    );
    assert_eq!(
        organization_approval_funds_executions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

fn funds_write(
    fixture: &Fixture,
    seed: u8,
    delta: i64,
    reason: &str,
    expected_version: i64,
) -> Result<OrganizationApprovalFundsExecutionWrite, Box<dyn Error>> {
    Ok(OrganizationApprovalFundsExecutionWrite::new(
        fixture.organization_id,
        OrganizationApprovalRequestId::new(i64::from(seed))?,
        WalletEventId::new([seed; 16])?,
        QuotaDelta::new(delta)?,
        reason.to_owned(),
        expected_version,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?)
}

fn rebuild_write(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    write: &OrganizationApprovalFundsExecutionWrite,
) -> OrganizationApprovalFundsExecutionWrite {
    OrganizationApprovalFundsExecutionWrite::new(
        fixture.organization_id,
        request_id,
        write.event_id(),
        write.quota_delta(),
        write.reason().to_owned(),
        write.expected_wallet_version(),
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )
    .expect("测试命令已通过构造校验")
}

async fn create_request_and_decision(
    fixture: &Fixture,
    seed: u8,
    summary: String,
    decision_kind: OrganizationApprovalDecisionKind,
    expires_at: i64,
) -> Result<OrganizationApprovalRequestId, Box<dyn Error>> {
    create_request_and_decision_with_type(
        fixture,
        seed,
        OrganizationApprovalResourceType::Funds,
        summary,
        decision_kind,
        expires_at,
    )
    .await
}

async fn create_request_and_decision_with_type(
    fixture: &Fixture,
    seed: u8,
    resource_type: OrganizationApprovalResourceType,
    summary: String,
    decision_kind: OrganizationApprovalDecisionKind,
    expires_at: i64,
) -> Result<OrganizationApprovalRequestId, Box<dyn Error>> {
    let template = fixture
        .approval_repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            OrganizationApprovalTemplateKey::new(format!("funds-execution-{seed}"))?,
            resource_type,
            OrganizationApprovalTemplateDigest::new([seed; 32])?,
            fixture.owner_user_id,
            None,
        ))
        .await?;
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([seed; 16])?,
        fixture.organization_id,
        resource_type,
        summary,
        template.id(),
        template.version(),
        fixture.owner_user_id,
        expires_at,
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
                OrganizationApprovalDecisionKey::new([seed.wrapping_add(1); 16])?,
                fixture.organization_id,
                request_id,
                decision_kind,
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
        name: Set(format!("approval-funds-execution-{suffix}")),
        display_name: Set(format!("Approval Funds Execution {suffix}")),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner_user_id = create_user(&pool, group.id, &format!("{suffix}-owner")).await?;
    let member_user_id = create_user(&pool, group.id, &format!("{suffix}-member")).await?;
    let organization_repository =
        OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, owner) = organization_repository
        .create(OrganizationCreate::new(
            format!("Approval Funds Execution {suffix}"),
            format!("approval-funds-execution-{suffix}"),
            owner_user_id,
            entitlement(),
        ))
        .await?
        .into_parts();
    let member = organization_repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                member_user_id,
                OrganizationRole::Member,
                None,
                Some(owner_user_id),
            ),
        )
        .await?;
    let wallet_repository =
        OrganizationWalletRepository::new(pool.clone(), Duration::from_secs(5))?;
    let initial = OrganizationWalletAdjustmentWrite::new(
        WalletEventId::new([0x01; 16])?,
        organization.id(),
        owner_user_id,
        QuotaDelta::new(100)?,
        "测试初始额度".to_owned(),
    )?;
    assert!(matches!(
        wallet_repository.adjust(initial).await?,
        OrganizationWalletAdjustmentOutcome::Applied(_)
    ));
    Ok(Fixture {
        pool: pool.clone(),
        organization_repository,
        approval_repository: OrganizationApprovalRepository::new(
            pool.clone(),
            Duration::from_secs(5),
        )?,
        wallet_repository,
        execution_repository: OrganizationApprovalExecutionRepository::new(
            pool,
            Duration::from_secs(5),
        )?,
        organization_id: organization.id(),
        owner_user_id,
        owner_membership_id: owner.id(),
        member_user_id,
        member_membership_id: member.id(),
    })
}

async fn create_organization(
    fixture: &Fixture,
    suffix: &str,
) -> Result<OrganizationId, Box<dyn Error>> {
    Ok(fixture
        .organization_repository
        .create(OrganizationCreate::new(
            format!("Approval Funds Execution {suffix}"),
            format!("approval-funds-execution-{suffix}"),
            fixture.owner_user_id,
            entitlement(),
        ))
        .await?
        .into_parts()
        .0
        .id())
}

fn entitlement() -> OrganizationEntitlementWrite {
    OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10).unwrap(),
            false,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )
        .unwrap(),
        "approval-funds-execution-test".to_owned(),
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
        username: Set(format!("approval-funds-execution-{suffix}")),
        email: Set(Some(format!(
            "approval-funds-execution-{suffix}@example.com"
        ))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("approval-funds-execution-{suffix}-aff")),
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
