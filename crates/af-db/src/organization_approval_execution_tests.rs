use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationRole, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};

use crate::entity::{groups, organization_approval_requests, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationApprovalDecisionWrite,
    OrganizationApprovalExecutionRepository, OrganizationApprovalExecutionRepositoryError,
    OrganizationApprovalMemberExecutionOutcome, OrganizationApprovalMemberExecutionWrite,
    OrganizationApprovalRepository, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestWrite, OrganizationApprovalTemplateWrite, OrganizationCreate,
    OrganizationEntitlementWrite, OrganizationMembershipCreate, OrganizationRepository,
    PoolOptions, organization_approval_member_resource_summary,
};

struct Fixture {
    pool: crate::DatabasePool,
    organization_repository: OrganizationRepository,
    approval_repository: OrganizationApprovalRepository,
    execution_repository: OrganizationApprovalExecutionRepository,
    organization_id: OrganizationId,
    owner_user_id: UserId,
    owner_membership_id: OrganizationMembershipId,
    member_user_id: UserId,
    member_membership_id: OrganizationMembershipId,
}

#[tokio::test]
async fn approved_member_change_is_applied_once_and_replayed_safely() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture("apply").await?;
    let summary = member_summary(&fixture, OrganizationRole::Viewer, 1);
    let request_id = create_request_and_decision(
        &fixture,
        11,
        OrganizationApprovalResourceType::Member,
        summary,
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let write = member_write(&fixture, request_id, OrganizationRole::Viewer, 1)?;

    let applied = fixture
        .execution_repository
        .execute_member_change(write)
        .await?;
    let record = match applied {
        OrganizationApprovalMemberExecutionOutcome::Applied(record) => record,
        OrganizationApprovalMemberExecutionOutcome::Existing(_) => panic!("首次执行必须应用"),
    };
    assert_eq!(record.applied_membership_version(), 2);
    assert_eq!(
        fixture
            .organization_repository
            .membership(fixture.organization_id, fixture.member_membership_id)
            .await?
            .role(),
        OrganizationRole::Viewer
    );

    assert!(matches!(
        fixture
            .execution_repository
            .execute_member_change(write)
            .await?,
        OrganizationApprovalMemberExecutionOutcome::Existing(_)
    ));
    assert_eq!(
        fixture
            .organization_repository
            .membership(fixture.organization_id, fixture.member_membership_id)
            .await?
            .version(),
        2
    );
    let changed = member_write(&fixture, request_id, OrganizationRole::Member, 1)?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(changed)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn decision_type_resource_scope_and_current_permission_fail_closed()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("boundaries").await?;
    let rejected_id = create_request_and_decision(
        &fixture,
        21,
        OrganizationApprovalResourceType::Member,
        member_summary(&fixture, OrganizationRole::Viewer, 1),
        OrganizationApprovalDecisionKind::Rejected,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(member_write(
                &fixture,
                rejected_id,
                OrganizationRole::Viewer,
                1,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotApproved)
    );

    let budget_id = create_request_and_decision(
        &fixture,
        31,
        OrganizationApprovalResourceType::Budget,
        member_summary(&fixture, OrganizationRole::Viewer, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(member_write(
                &fixture,
                budget_id,
                OrganizationRole::Viewer,
                1,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch)
    );

    let permission_id = create_request_and_decision(
        &fixture,
        41,
        OrganizationApprovalResourceType::Member,
        member_summary(&fixture, OrganizationRole::Viewer, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    let unauthorized = OrganizationApprovalMemberExecutionWrite::new(
        fixture.organization_id,
        permission_id,
        fixture.member_membership_id,
        OrganizationRole::Viewer,
        OrganizationMembershipStatus::Active,
        None,
        1,
        fixture.member_membership_id,
        fixture.member_user_id,
    )?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(unauthorized)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Forbidden)
    );

    let other = create_organization(&fixture, "boundaries-other").await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(OrganizationApprovalMemberExecutionWrite::new(
                other,
                permission_id,
                fixture.member_membership_id,
                OrganizationRole::Viewer,
                OrganizationMembershipStatus::Active,
                None,
                1,
                fixture.owner_membership_id,
                fixture.owner_user_id,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::NotFound)
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn expiration_and_member_version_are_rechecked_at_execution_time()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture("freshness").await?;
    let version_request_id = create_request_and_decision(
        &fixture,
        51,
        OrganizationApprovalResourceType::Member,
        member_summary(&fixture, OrganizationRole::Viewer, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    fixture
        .organization_repository
        .change_membership(
            fixture.organization_id,
            fixture.member_membership_id,
            crate::OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Suspended,
                None,
                1,
            ),
        )
        .await?;
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(member_write(
                &fixture,
                version_request_id,
                OrganizationRole::Viewer,
                9,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Conflict)
    );

    let expired_request_id = create_request_and_decision(
        &fixture,
        61,
        OrganizationApprovalResourceType::Member,
        member_summary(&fixture, OrganizationRole::Viewer, 1),
        OrganizationApprovalDecisionKind::Approved,
        now() + 3_600,
    )
    .await?;
    // 直接构造已过期持久化事实，避免依赖并发门禁中的调度时延。
    let expired_at = TimeDateTimeWithTimeZone::now_utc() - Duration::from_secs(1);
    let update = organization_approval_requests::Entity::update_many()
        .filter(organization_approval_requests::Column::Id.eq(expired_request_id.get()))
        .col_expr(
            organization_approval_requests::Column::ExpiresAt,
            Expr::value(expired_at),
        )
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(update.rows_affected, 1);
    assert_eq!(
        fixture
            .execution_repository
            .execute_member_change(member_write(
                &fixture,
                expired_request_id,
                OrganizationRole::Viewer,
                1,
            )?)
            .await,
        Err(OrganizationApprovalExecutionRepositoryError::Expired)
    );

    fixture.pool.close().await?;
    Ok(())
}

fn member_summary(fixture: &Fixture, role: OrganizationRole, version: i64) -> String {
    organization_approval_member_resource_summary(
        fixture.organization_id,
        fixture.member_membership_id,
        role,
        OrganizationMembershipStatus::Active,
        None,
        version,
    )
}

fn member_write(
    fixture: &Fixture,
    request_id: OrganizationApprovalRequestId,
    role: OrganizationRole,
    version: i64,
) -> Result<OrganizationApprovalMemberExecutionWrite, OrganizationApprovalExecutionRepositoryError>
{
    OrganizationApprovalMemberExecutionWrite::new(
        fixture.organization_id,
        request_id,
        fixture.member_membership_id,
        role,
        OrganizationMembershipStatus::Active,
        None,
        version,
        fixture.owner_membership_id,
        fixture.owner_user_id,
    )
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
            OrganizationApprovalTemplateKey::new(format!("execution-{seed}"))?,
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
        name: Set(format!("approval-execution-{suffix}")),
        display_name: Set(format!("Approval Execution {suffix}")),
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
            format!("Approval Execution {suffix}"),
            format!("approval-execution-{suffix}"),
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
            format!("Approval Execution {suffix}"),
            format!("approval-execution-{suffix}"),
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
        "approval-execution-test".to_owned(),
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
        username: Set(format!("approval-execution-{suffix}")),
        email: Set(Some(format!("approval-execution-{suffix}@example.com"))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("approval-execution-{suffix}-aff")),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(UserId::new(user.id)?)
}

fn now() -> i64 {
    TimeDateTimeWithTimeZone::now_utc().unix_timestamp()
}
