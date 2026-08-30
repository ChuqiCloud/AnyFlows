use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey, OrganizationBudgetLimit,
    OrganizationBudgetPeriod, OrganizationDepartmentId, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationId, OrganizationMembershipId, OrganizationRole,
    OrganizationServiceAccountLocator, OrganizationServiceAccountName,
    OrganizationServiceAccountStatus, OrganizationTeamId, Quota, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::entity::{groups, organization_service_account_keys, users};
use crate::{
    DatabaseOptions, DatabaseTimestamp, MigrationOptions, OrganizationApprovalDecisionWrite,
    OrganizationApprovalExecutionRepository, OrganizationApprovalExecutionRepositoryError,
    OrganizationApprovalRepository, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestWrite, OrganizationApprovalServiceAccountExecutionAction,
    OrganizationApprovalServiceAccountExecutionOutcome,
    OrganizationApprovalServiceAccountExecutionWrite, OrganizationApprovalTemplateWrite,
    OrganizationBudgetPolicyWrite, OrganizationBudgetRepository, OrganizationBudgetScope,
    OrganizationCreate, OrganizationDepartmentCreate, OrganizationDepartmentRepository,
    OrganizationEntitlementWrite, OrganizationMembershipCreate, OrganizationRepository,
    OrganizationServiceAccountBudgetBindingRepository, OrganizationServiceAccountRepository,
    OrganizationTeamCreate, PoolOptions,
    organization_approval_service_account_budget_binding_resource_summary,
    organization_approval_service_account_status_resource_summary,
};

struct Fixture {
    pool: crate::DatabasePool,
    organization_repository: OrganizationRepository,
    approval_repository: OrganizationApprovalRepository,
    budget_repository: OrganizationBudgetRepository,
    binding_repository: OrganizationServiceAccountBudgetBindingRepository,
    account_repository: OrganizationServiceAccountRepository,
    execution_repository: OrganizationApprovalExecutionRepository,
    organization_id: OrganizationId,
    owner_user_id: UserId,
    owner_membership_id: OrganizationMembershipId,
    member_user_id: UserId,
    member_membership_id: OrganizationMembershipId,
    account_locator: OrganizationServiceAccountLocator,
    team_id: OrganizationTeamId,
}

#[tokio::test]
async fn approved_status_change_is_replayable_and_keeps_keys_unchanged()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("status-change").await?;
    let keys_before = key_rows(&fixture).await?;
    let request_id = create_request_and_decision(
        &fixture,
        11,
        OrganizationApprovalResourceType::ServiceAccount,
        status_summary(&fixture, OrganizationServiceAccountStatus::Suspended, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let write = status_write(
        &fixture,
        request_id,
        fixture.account_locator.department_id(),
        OrganizationServiceAccountStatus::Suspended,
        1,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;

    let record = match fixture
        .execution_repository
        .execute_service_account(write)
        .await?
    {
        OrganizationApprovalServiceAccountExecutionOutcome::Applied(record) => record,
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(_) => {
            panic!("首次执行必须应用")
        }
    };
    assert_eq!(record.applied_account_version(), 2);
    let account = fixture
        .account_repository
        .get(fixture.account_locator)
        .await?;
    assert_eq!(account.version(), 2);
    assert_eq!(
        account.account().status(),
        OrganizationServiceAccountStatus::Suspended
    );
    assert!(matches!(
        fixture
            .execution_repository
            .execute_service_account(write)
            .await?,
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(_)
    ));
    assert_eq!(key_rows(&fixture).await?, keys_before);

    let changed = status_write(
        &fixture,
        request_id,
        fixture.account_locator.department_id(),
        OrganizationServiceAccountStatus::Active,
        1,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_service_account(changed)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approved_status_noop_records_the_unchanged_account_version() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture("status-noop").await?;
    let keys_before = key_rows(&fixture).await?;
    let request_id = create_request_and_decision(
        &fixture,
        21,
        OrganizationApprovalResourceType::ServiceAccount,
        status_summary(&fixture, OrganizationServiceAccountStatus::Active, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let result = fixture
        .execution_repository
        .execute_service_account(status_write(
            &fixture,
            request_id,
            fixture.account_locator.department_id(),
            OrganizationServiceAccountStatus::Active,
            1,
            fixture.owner_membership_id,
            fixture.owner_user_id,
        )?)
        .await?;
    let record = match result {
        OrganizationApprovalServiceAccountExecutionOutcome::Applied(record) => record,
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(_) => {
            panic!("首次执行必须应用")
        }
    };
    assert_eq!(record.applied_account_version(), 1);
    assert_eq!(
        fixture
            .account_repository
            .get(fixture.account_locator)
            .await?
            .version(),
        1
    );
    assert_eq!(key_rows(&fixture).await?, keys_before);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn status_execution_rechecks_decision_resource_permission_and_enterprise_scope()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("status-boundaries").await?;
    let summary = status_summary(&fixture, OrganizationServiceAccountStatus::Suspended, 1);
    let rejected_id = create_request_and_decision(
        &fixture,
        31,
        OrganizationApprovalResourceType::ServiceAccount,
        summary.clone(),
        OrganizationApprovalDecisionKind::Rejected,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        execute_owner_status(
            &fixture,
            rejected_id,
            OrganizationServiceAccountStatus::Suspended,
            1
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotApproved)
    );

    let wrong_resource_id = create_request_and_decision(
        &fixture,
        41,
        OrganizationApprovalResourceType::Member,
        summary.clone(),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        execute_owner_status(
            &fixture,
            wrong_resource_id,
            OrganizationServiceAccountStatus::Suspended,
            1,
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch)
    );

    let permission_id = create_request_and_decision(
        &fixture,
        51,
        OrganizationApprovalResourceType::ServiceAccount,
        summary,
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let unauthorized = status_write(
        &fixture,
        permission_id,
        fixture.account_locator.department_id(),
        OrganizationServiceAccountStatus::Suspended,
        1,
        fixture.member_membership_id,
        fixture.member_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_service_account(unauthorized)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Forbidden)
    );

    let other_organization_id = create_organization(&fixture, "status-other").await?;
    let cross_organization = OrganizationApprovalServiceAccountExecutionWrite::new(
        other_organization_id,
        permission_id,
        fixture.account_locator.service_account_id(),
        fixture.account_locator.department_id(),
        OrganizationApprovalServiceAccountExecutionAction::SetStatus {
            target_status: OrganizationServiceAccountStatus::Suspended,
            expected_account_version: 1,
        },
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_service_account(cross_organization)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotFound)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn status_execution_rechecks_expiration_account_version_and_department_snapshot()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("status-freshness").await?;
    let expired_id = create_request_and_decision(
        &fixture,
        61,
        OrganizationApprovalResourceType::ServiceAccount,
        status_summary(&fixture, OrganizationServiceAccountStatus::Suspended, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 1,
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        execute_owner_status(
            &fixture,
            expired_id,
            OrganizationServiceAccountStatus::Suspended,
            1
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::Expired)
    );

    let version_id = create_request_and_decision(
        &fixture,
        71,
        OrganizationApprovalResourceType::ServiceAccount,
        status_summary(&fixture, OrganizationServiceAccountStatus::Suspended, 2),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        execute_owner_status(
            &fixture,
            version_id,
            OrganizationServiceAccountStatus::Suspended,
            2
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let department_id = create_request_and_decision(
        &fixture,
        81,
        OrganizationApprovalResourceType::ServiceAccount,
        organization_approval_service_account_status_resource_summary(
            fixture.organization_id,
            fixture.account_locator.service_account_id(),
            None,
            OrganizationServiceAccountStatus::Suspended,
            1,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let wrong_department = status_write(
        &fixture,
        department_id,
        None,
        OrganizationServiceAccountStatus::Suspended,
        1,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_service_account(wrong_department)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::InvalidTransition)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approved_budget_binding_create_and_update_are_cas_protected() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture("budget-binding").await?;
    let keys_before = key_rows(&fixture).await?;
    let organization_policy =
        create_policy(&fixture, OrganizationBudgetScope::Organization, -30).await?;
    let team_policy = create_policy(
        &fixture,
        OrganizationBudgetScope::Team(fixture.team_id),
        -20,
    )
    .await?;

    let create_request_id = create_binding_request(
        &fixture,
        91,
        organization_policy.id(),
        organization_policy.version(),
        None,
        1,
    )
    .await?;
    let created = execute_owner_binding(
        &fixture,
        create_request_id,
        organization_policy.id(),
        organization_policy.version(),
        None,
        1,
    )
    .await?;
    let created = match created {
        OrganizationApprovalServiceAccountExecutionOutcome::Applied(record) => record,
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(_) => {
            panic!("首次绑定必须应用")
        }
    };
    assert_eq!(created.applied_account_version(), 1);
    assert_eq!(
        created.applied_policy_version(),
        Some(organization_policy.version())
    );
    assert_eq!(created.applied_binding_version(), Some(1));

    let update_request_id = create_binding_request(
        &fixture,
        101,
        team_policy.id(),
        team_policy.version(),
        Some(1),
        1,
    )
    .await?;
    let updated = execute_owner_binding(
        &fixture,
        update_request_id,
        team_policy.id(),
        team_policy.version(),
        Some(1),
        1,
    )
    .await?;
    let updated = match updated {
        OrganizationApprovalServiceAccountExecutionOutcome::Applied(record) => record,
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(_) => {
            panic!("首次更新必须应用")
        }
    };
    assert_eq!(updated.applied_binding_version(), Some(2));
    assert!(matches!(
        execute_owner_binding(
            &fixture,
            update_request_id,
            team_policy.id(),
            team_policy.version(),
            Some(1),
            1,
        )
        .await?,
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(_)
    ));
    let binding = fixture
        .binding_repository
        .resolve_binding(
            fixture.organization_id,
            fixture.account_locator.service_account_id(),
        )
        .await?;
    assert_eq!(binding.policy_id(), team_policy.id());
    assert_eq!(binding.version(), 2);
    assert_eq!(key_rows(&fixture).await?, keys_before);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn budget_binding_rechecks_policy_binding_account_and_department_versions()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("budget-conflicts").await?;
    let first_policy = create_policy(&fixture, OrganizationBudgetScope::Organization, -30).await?;
    fixture
        .binding_repository
        .set_binding(
            fixture.organization_id,
            fixture.account_locator.service_account_id(),
            first_policy.id(),
            None,
        )
        .await?;
    let team_policy = create_policy(
        &fixture,
        OrganizationBudgetScope::Team(fixture.team_id),
        -20,
    )
    .await?;

    let policy_version_id = create_binding_request(
        &fixture,
        111,
        team_policy.id(),
        team_policy.version() + 1,
        Some(1),
        1,
    )
    .await?;
    assert_eq!(
        execute_owner_binding(
            &fixture,
            policy_version_id,
            team_policy.id(),
            team_policy.version() + 1,
            Some(1),
            1,
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let binding_version_id = create_binding_request(
        &fixture,
        121,
        team_policy.id(),
        team_policy.version(),
        Some(2),
        1,
    )
    .await?;
    assert_eq!(
        execute_owner_binding(
            &fixture,
            binding_version_id,
            team_policy.id(),
            team_policy.version(),
            Some(2),
            1,
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let account_version_id = create_binding_request(
        &fixture,
        131,
        team_policy.id(),
        team_policy.version(),
        Some(1),
        2,
    )
    .await?;
    assert_eq!(
        execute_owner_binding(
            &fixture,
            account_version_id,
            team_policy.id(),
            team_policy.version(),
            Some(1),
            2,
        )
        .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let department_id = create_request_and_decision(
        &fixture,
        141,
        OrganizationApprovalResourceType::ServiceAccount,
        organization_approval_service_account_budget_binding_resource_summary(
            fixture.organization_id,
            fixture.account_locator.service_account_id(),
            None,
            team_policy.id(),
            team_policy.version(),
            Some(1),
            1,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let wrong_department = binding_write(
        &fixture,
        department_id,
        None,
        team_policy.id(),
        team_policy.version(),
        Some(1),
        1,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_service_account(wrong_department)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );
    let binding = fixture
        .binding_repository
        .resolve_binding(
            fixture.organization_id,
            fixture.account_locator.service_account_id(),
        )
        .await?;
    assert_eq!(binding.policy_id(), first_policy.id());
    assert_eq!(binding.version(), 1);
    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn summaries_cover_all_snapshots_and_actions_exclude_key_issuance() {
    let organization_id = OrganizationId::new(1).unwrap();
    let account_id = af_domain::OrganizationServiceAccountId::new(2).unwrap();
    let department_id = OrganizationDepartmentId::new(3).unwrap();
    let status = organization_approval_service_account_status_resource_summary(
        organization_id,
        account_id,
        Some(department_id),
        OrganizationServiceAccountStatus::Suspended,
        1,
    );
    assert!(status.starts_with("service-account-status-sha256:"));
    assert_ne!(
        status,
        organization_approval_service_account_status_resource_summary(
            organization_id,
            account_id,
            None,
            OrganizationServiceAccountStatus::Suspended,
            1,
        )
    );
    assert_eq!(
        OrganizationApprovalServiceAccountExecutionAction::SetStatus {
            target_status: OrganizationServiceAccountStatus::Active,
            expected_account_version: 1,
        }
        .kind(),
        1
    );
    assert_eq!(
        OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
            policy_id: af_domain::OrganizationBudgetPolicyId::new(4).unwrap(),
            expected_policy_version: 1,
            expected_binding_version: Some(1),
            expected_account_version: 1,
        }
        .kind(),
        2
    );
}

async fn execute_owner_status(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    target_status: OrganizationServiceAccountStatus,
    expected_account_version: i64,
) -> Result<
    OrganizationApprovalServiceAccountExecutionOutcome,
    OrganizationApprovalExecutionRepositoryError,
> {
    fixture
        .execution_repository
        .execute_service_account(status_write(
            fixture,
            request_id,
            fixture.account_locator.department_id(),
            target_status,
            expected_account_version,
            fixture.owner_membership_id,
            fixture.owner_user_id,
        )?)
        .await
}

#[allow(clippy::too_many_arguments)]
fn status_write(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    department_id: Option<OrganizationDepartmentId>,
    target_status: OrganizationServiceAccountStatus,
    expected_account_version: i64,
    actor_membership_id: OrganizationMembershipId,
    actor_user_id: UserId,
) -> Result<
    OrganizationApprovalServiceAccountExecutionWrite,
    OrganizationApprovalExecutionRepositoryError,
> {
    OrganizationApprovalServiceAccountExecutionWrite::new(
        fixture.organization_id,
        request_id,
        fixture.account_locator.service_account_id(),
        department_id,
        OrganizationApprovalServiceAccountExecutionAction::SetStatus {
            target_status,
            expected_account_version,
        },
        actor_membership_id,
        actor_user_id,
    )
}

fn status_summary(
    fixture: &Fixture,
    target_status: OrganizationServiceAccountStatus,
    expected_account_version: i64,
) -> String {
    organization_approval_service_account_status_resource_summary(
        fixture.organization_id,
        fixture.account_locator.service_account_id(),
        fixture.account_locator.department_id(),
        target_status,
        expected_account_version,
    )
}

#[allow(clippy::too_many_arguments)]
async fn execute_owner_binding(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    policy_id: af_domain::OrganizationBudgetPolicyId,
    expected_policy_version: i64,
    expected_binding_version: Option<i64>,
    expected_account_version: i64,
) -> Result<
    OrganizationApprovalServiceAccountExecutionOutcome,
    OrganizationApprovalExecutionRepositoryError,
> {
    fixture
        .execution_repository
        .execute_service_account(binding_write(
            fixture,
            request_id,
            fixture.account_locator.department_id(),
            policy_id,
            expected_policy_version,
            expected_binding_version,
            expected_account_version,
        )?)
        .await
}

#[allow(clippy::too_many_arguments)]
fn binding_write(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    department_id: Option<OrganizationDepartmentId>,
    policy_id: af_domain::OrganizationBudgetPolicyId,
    expected_policy_version: i64,
    expected_binding_version: Option<i64>,
    expected_account_version: i64,
) -> Result<
    OrganizationApprovalServiceAccountExecutionWrite,
    OrganizationApprovalExecutionRepositoryError,
> {
    OrganizationApprovalServiceAccountExecutionWrite::new(
        fixture.organization_id,
        request_id,
        fixture.account_locator.service_account_id(),
        department_id,
        OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
            policy_id,
            expected_policy_version,
            expected_binding_version,
            expected_account_version,
        },
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )
}

async fn create_binding_request(
    fixture: &Fixture,
    seed: u8,
    policy_id: af_domain::OrganizationBudgetPolicyId,
    expected_policy_version: i64,
    expected_binding_version: Option<i64>,
    expected_account_version: i64,
) -> Result<OrganizationApprovalRequestId, Box<dyn Error>> {
    create_request_and_decision(
        fixture,
        seed,
        OrganizationApprovalResourceType::ServiceAccount,
        organization_approval_service_account_budget_binding_resource_summary(
            fixture.organization_id,
            fixture.account_locator.service_account_id(),
            fixture.account_locator.department_id(),
            policy_id,
            expected_policy_version,
            expected_binding_version,
            expected_account_version,
        ),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await
}

async fn create_policy(
    fixture: &Fixture,
    scope: OrganizationBudgetScope,
    offset_seconds: i64,
) -> Result<crate::OrganizationBudgetPolicyRecord, Box<dyn Error>> {
    let effective_from = now()
        .checked_add(offset_seconds)
        .ok_or("预算策略生效时间溢出")?;
    Ok(fixture
        .budget_repository
        .create_policy(OrganizationBudgetPolicyWrite::new(
            fixture.organization_id,
            scope,
            OrganizationBudgetPeriod::UtcDay,
            OrganizationBudgetLimit::new(Some(Quota::new(100)?))?,
            TimeDateTimeWithTimeZone::from_unix_timestamp(effective_from)?,
            fixture.owner_user_id,
        ))
        .await?)
}

async fn key_rows(
    fixture: &Fixture,
) -> Result<Vec<organization_service_account_keys::Model>, sea_orm::DbErr> {
    organization_service_account_keys::Entity::find()
        .filter(
            organization_service_account_keys::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .filter(
            organization_service_account_keys::Column::ServiceAccountId
                .eq(fixture.account_locator.service_account_id().get()),
        )
        .order_by_asc(organization_service_account_keys::Column::Id)
        .all(fixture.pool.connection())
        .await
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
            OrganizationApprovalTemplateKey::new(format!("service-account-execution-{seed}"))?,
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
        name: Set(format!("approval-service-account-{suffix}")),
        display_name: Set(format!("Approval Service Account {suffix}")),
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
            format!("Approval Service Account {suffix}"),
            format!("approval-service-account-{suffix}"),
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
    let team = organization_repository
        .create_team(
            organization.id(),
            OrganizationTeamCreate::new("审批执行团队".to_owned(), None),
        )
        .await?;
    let department_repository =
        OrganizationDepartmentRepository::new(pool.clone(), Duration::from_secs(5))?;
    let department = department_repository
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("审批执行部门".to_owned(), None, 1),
        )
        .await?;
    let account_repository =
        OrganizationServiceAccountRepository::new(pool.clone(), Duration::from_secs(5))?;
    let account = account_repository
        .create_with_department(
            organization.id(),
            department.id(),
            OrganizationServiceAccountName::new(format!("automation-{suffix}"))?,
        )
        .await?
        .into_parts()
        .0;
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
            Duration::from_secs(3_600),
        )?,
        binding_repository: OrganizationServiceAccountBudgetBindingRepository::new(
            pool.clone(),
            Duration::from_secs(5),
        )?,
        account_repository,
        execution_repository: OrganizationApprovalExecutionRepository::new(
            pool,
            Duration::from_secs(5),
        )?,
        organization_id: organization.id(),
        owner_user_id,
        owner_membership_id: owner.id(),
        member_user_id,
        member_membership_id: member.id(),
        account_locator: account.locator(),
        team_id: team.id(),
    })
}

async fn create_organization(
    fixture: &Fixture,
    suffix: &str,
) -> Result<OrganizationId, Box<dyn Error>> {
    Ok(fixture
        .organization_repository
        .create(OrganizationCreate::new(
            format!("Approval Service Account {suffix}"),
            format!("approval-service-account-{suffix}"),
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
        "approval-service-account-execution-test".to_owned(),
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
        username: Set(format!("approval-service-account-{suffix}")),
        email: Set(Some(format!(
            "approval-service-account-{suffix}@example.com"
        ))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("approval-service-account-{suffix}-aff")),
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
