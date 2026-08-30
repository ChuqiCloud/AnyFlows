use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey, OrganizationBudgetLimit,
    OrganizationBudgetPeriod, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationId, OrganizationMembershipId, OrganizationRole, Quota, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::entity::{
    billing_organization_reservations, groups, organization_budget_policies,
    organization_wallet_ledger_entries, organization_wallets, users,
};
use crate::{
    DatabaseOptions, DatabaseTimestamp, MigrationOptions,
    OrganizationApprovalBudgetExecutionOutcome, OrganizationApprovalBudgetExecutionWrite,
    OrganizationApprovalDecisionWrite, OrganizationApprovalExecutionRepository,
    OrganizationApprovalExecutionRepositoryError, OrganizationApprovalRepository,
    OrganizationApprovalRequestCreateOutcome, OrganizationApprovalRequestWrite,
    OrganizationApprovalTemplateWrite, OrganizationBudgetPolicyWrite, OrganizationBudgetRepository,
    OrganizationBudgetScope, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationMembershipCreate, OrganizationRepository, PoolOptions,
    organization_approval_budget_resource_summary,
};

struct Fixture {
    pool: crate::DatabasePool,
    organization_repository: OrganizationRepository,
    approval_repository: OrganizationApprovalRepository,
    budget_repository: OrganizationBudgetRepository,
    execution_repository: OrganizationApprovalExecutionRepository,
    organization_id: OrganizationId,
    owner_user_id: UserId,
    owner_membership_id: OrganizationMembershipId,
    member_user_id: UserId,
    member_membership_id: OrganizationMembershipId,
}

#[tokio::test]
async fn approved_budget_policy_is_applied_once_and_does_not_touch_wallet()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("apply").await?;
    let scope = OrganizationBudgetScope::Organization;
    let period = OrganizationBudgetPeriod::UtcMonth;
    let limit = budget_limit(100)?;
    let effective_from = now() + 60;
    let request_id = create_request_and_decision(
        &fixture,
        11,
        OrganizationApprovalResourceType::Budget,
        organization_approval_budget_resource_summary(
            fixture.organization_id,
            scope,
            period,
            limit,
            effective_from,
            0,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let write = budget_write(
        &fixture,
        request_id,
        scope,
        period,
        limit,
        effective_from,
        0,
    )?;
    let wallet_before = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(fixture.organization_id.get()))
        .one(fixture.pool.connection())
        .await?;
    let ledger_before = organization_wallet_ledger_entries::Entity::find()
        .count(fixture.pool.connection())
        .await?;
    let reservations_before = billing_organization_reservations::Entity::find()
        .count(fixture.pool.connection())
        .await?;

    let applied = fixture
        .execution_repository
        .execute_budget_policy(write)
        .await?;
    let record = match applied {
        OrganizationApprovalBudgetExecutionOutcome::Applied(record) => record,
        OrganizationApprovalBudgetExecutionOutcome::Existing(_) => panic!("首次执行必须应用"),
    };
    assert_eq!(record.applied_policy_version(), 1);
    assert_eq!(record.expected_policy_version(), 0);
    assert_eq!(record.scope(), scope);
    assert_eq!(record.limit(), limit);
    assert_eq!(
        organization_budget_policies::Entity::find()
            .filter(
                organization_budget_policies::Column::OrganizationId
                    .eq(fixture.organization_id.get()),
            )
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert!(matches!(
        fixture
            .execution_repository
            .execute_budget_policy(write)
            .await?,
        OrganizationApprovalBudgetExecutionOutcome::Existing(_)
    ));
    let changed = budget_write(
        &fixture,
        request_id,
        scope,
        period,
        budget_limit(101)?,
        effective_from,
        0,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(changed)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    assert_eq!(
        organization_wallets::Entity::find()
            .filter(organization_wallets::Column::OrganizationId.eq(fixture.organization_id.get()))
            .one(fixture.pool.connection())
            .await?,
        wallet_before
    );
    assert_eq!(
        organization_wallet_ledger_entries::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        ledger_before
    );
    assert_eq!(
        billing_organization_reservations::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        reservations_before
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn budget_execution_rejects_resource_decision_permission_and_cross_org_mismatch()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("boundaries").await?;
    let scope = OrganizationBudgetScope::Organization;
    let period = OrganizationBudgetPeriod::UtcDay;
    let limit = budget_limit(50)?;
    let effective_from = now() + 60;
    let rejected_id = create_request_and_decision(
        &fixture,
        21,
        OrganizationApprovalResourceType::Budget,
        organization_approval_budget_resource_summary(
            fixture.organization_id,
            scope,
            period,
            limit,
            effective_from,
            0,
        ),
        OrganizationApprovalDecisionKind::Rejected,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(budget_write(
                &fixture,
                rejected_id,
                scope,
                period,
                limit,
                effective_from,
                0,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotApproved)
    );

    let member_resource_id = create_request_and_decision(
        &fixture,
        31,
        OrganizationApprovalResourceType::Member,
        organization_approval_budget_resource_summary(
            fixture.organization_id,
            scope,
            period,
            limit,
            effective_from,
            0,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(budget_write(
                &fixture,
                member_resource_id,
                scope,
                period,
                limit,
                effective_from,
                0,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch)
    );

    let permission_id = create_request_and_decision(
        &fixture,
        41,
        OrganizationApprovalResourceType::Budget,
        organization_approval_budget_resource_summary(
            fixture.organization_id,
            scope,
            period,
            limit,
            effective_from,
            0,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let unauthorized = OrganizationApprovalBudgetExecutionWrite::new(
        fixture.organization_id,
        permission_id,
        scope,
        period,
        limit,
        DatabaseTimestamp::from_unix_timestamp(effective_from)?,
        0,
        fixture.member_membership_id,
        fixture.member_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(unauthorized)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Forbidden)
    );

    let other = create_organization(&fixture, "boundaries-other").await?;
    let cross_org = OrganizationApprovalBudgetExecutionWrite::new(
        other,
        permission_id,
        scope,
        period,
        limit,
        DatabaseTimestamp::from_unix_timestamp(effective_from)?,
        0,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(cross_org)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotFound)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn budget_execution_rechecks_policy_version_and_expiration() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("freshness").await?;
    let scope = OrganizationBudgetScope::Organization;
    let period = OrganizationBudgetPeriod::UtcWeek;
    let initial_limit = budget_limit(75)?;
    fixture
        .budget_repository
        .create_policy(OrganizationBudgetPolicyWrite::new(
            fixture.organization_id,
            scope,
            period,
            initial_limit,
            DatabaseTimestamp::from_unix_timestamp(now() + 10)?,
            fixture.owner_user_id,
        ))
        .await?;
    let effective_from = now() + 20;
    let version_request_id = create_request_and_decision(
        &fixture,
        51,
        OrganizationApprovalResourceType::Budget,
        organization_approval_budget_resource_summary(
            fixture.organization_id,
            scope,
            period,
            budget_limit(80)?,
            effective_from,
            0,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(budget_write(
                &fixture,
                version_request_id,
                scope,
                period,
                budget_limit(80)?,
                effective_from,
                0,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let expired_effective_from = now() + 60;
    let expired_request_id = create_request_and_decision(
        &fixture,
        61,
        OrganizationApprovalResourceType::Budget,
        organization_approval_budget_resource_summary(
            fixture.organization_id,
            scope,
            period,
            initial_limit,
            expired_effective_from,
            1,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 1,
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        fixture
            .execution_repository
            .execute_budget_policy(budget_write(
                &fixture,
                expired_request_id,
                scope,
                period,
                initial_limit,
                expired_effective_from,
                1,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Expired)
    );
    fixture.pool.close().await?;
    Ok(())
}

fn budget_limit(value: i64) -> Result<OrganizationBudgetLimit, Box<dyn Error>> {
    Ok(OrganizationBudgetLimit::new(Some(Quota::new(value)?))?)
}

fn budget_write(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: i64,
    expected_version: i64,
) -> Result<OrganizationApprovalBudgetExecutionWrite, Box<dyn Error>> {
    Ok(OrganizationApprovalBudgetExecutionWrite::new(
        fixture.organization_id,
        request_id,
        scope,
        period,
        limit,
        DatabaseTimestamp::from_unix_timestamp(effective_from)?,
        expected_version,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?)
}

async fn create_request_and_decision(
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
            OrganizationApprovalTemplateKey::new(format!("budget-execution-{seed}"))?,
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
        name: Set(format!("approval-budget-execution-{suffix}")),
        display_name: Set(format!("Approval Budget Execution {suffix}")),
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
            format!("Approval Budget Execution {suffix}"),
            format!("approval-budget-execution-{suffix}"),
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
    Ok(Fixture {
        pool: pool.clone(),
        organization_repository,
        approval_repository: OrganizationApprovalRepository::new(
            pool.clone(),
            Duration::from_secs(5),
        )?,
        budget_repository: OrganizationBudgetRepository::new(
            pool.clone(),
            Duration::from_secs(5),
            Duration::from_secs(3600),
        )?,
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
            format!("Approval Budget Execution {suffix}"),
            format!("approval-budget-execution-{suffix}"),
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
        "approval-budget-execution-test".to_owned(),
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
        username: Set(format!("approval-budget-execution-{suffix}")),
        email: Set(Some(format!(
            "approval-budget-execution-{suffix}@example.com"
        ))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("approval-budget-execution-{suffix}-aff")),
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
