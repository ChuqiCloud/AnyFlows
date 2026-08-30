use std::error::Error;
use std::time::Duration;

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestKey,
    OrganizationApprovalResourceType, OrganizationApprovalTemplateDigest,
    OrganizationApprovalTemplateKey, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationId, UserId,
};
use sea_orm::{ActiveModelTrait, Set, entity::prelude::TimeDateTimeWithTimeZone};

use crate::entity::{groups, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationApprovalDecisionOutcome,
    OrganizationApprovalDecisionWrite, OrganizationApprovalRepository,
    OrganizationApprovalRepositoryError, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestWrite, OrganizationApprovalTemplateWrite, OrganizationCreate,
    OrganizationEntitlementWrite, OrganizationRepository, PoolOptions,
};

struct Fixture {
    pool: crate::DatabasePool,
    repository: OrganizationApprovalRepository,
    organization_id: OrganizationId,
    owner_id: UserId,
}

#[tokio::test]
async fn template_request_and_decision_facts_are_scoped_and_idempotent()
-> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let template_key = OrganizationApprovalTemplateKey::new("budget-change")?;
    let template = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            template_key.clone(),
            OrganizationApprovalResourceType::Budget,
            OrganizationApprovalTemplateDigest::new([1; 32])?,
            fixture.owner_id,
            None,
        ))
        .await?;
    assert_eq!(template.version(), 1);

    let second = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            template_key.clone(),
            OrganizationApprovalResourceType::Budget,
            OrganizationApprovalTemplateDigest::new([2; 32])?,
            fixture.owner_id,
            Some(1),
        ))
        .await?;
    assert_eq!(second.version(), 2);
    assert_eq!(
        fixture
            .repository
            .create_template(OrganizationApprovalTemplateWrite::new(
                fixture.organization_id,
                template_key,
                OrganizationApprovalResourceType::Budget,
                OrganizationApprovalTemplateDigest::new([3; 32])?,
                fixture.owner_id,
                Some(1),
            ))
            .await,
        Err(OrganizationApprovalRepositoryError::Conflict)
    );

    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([4; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Budget,
        "monthly limit 100".to_owned(),
        second.id(),
        second.version(),
        fixture.owner_id,
        now + 3_600,
    )?;
    let created = fixture
        .repository
        .create_request(OrganizationApprovalRequestWrite::new(request.clone()))
        .await?;
    let request_id = match created {
        OrganizationApprovalRequestCreateOutcome::Created(record) => record.id(),
        OrganizationApprovalRequestCreateOutcome::Existing(_) => panic!("首次申请必须创建"),
    };
    assert!(matches!(
        fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(request.clone()))
            .await?,
        OrganizationApprovalRequestCreateOutcome::Existing(_)
    ));
    let conflict = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([4; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Budget,
        "monthly limit 200".to_owned(),
        second.id(),
        second.version(),
        fixture.owner_id,
        now + 3_600,
    )?;
    assert_eq!(
        fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(conflict))
            .await,
        Err(OrganizationApprovalRepositoryError::Conflict)
    );

    let decision = OrganizationApprovalDecision::new(
        OrganizationApprovalDecisionKey::new([5; 16])?,
        fixture.organization_id,
        request_id,
        OrganizationApprovalDecisionKind::Approved,
        Some(fixture.owner_id),
        Some("approved by owner".to_owned()),
        now,
    )?;
    assert!(matches!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision.clone()))
            .await?,
        OrganizationApprovalDecisionOutcome::Created(_)
    ));
    assert!(matches!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(decision))
            .await?,
        OrganizationApprovalDecisionOutcome::Existing(_)
    ));
    let second_decision = OrganizationApprovalDecision::new(
        OrganizationApprovalDecisionKey::new([6; 16])?,
        fixture.organization_id,
        request_id,
        OrganizationApprovalDecisionKind::Rejected,
        Some(fixture.owner_id),
        None,
        now,
    )?;
    assert_eq!(
        fixture
            .repository
            .record_decision(OrganizationApprovalDecisionWrite::new(second_decision))
            .await,
        Err(OrganizationApprovalRepositoryError::AlreadyDecided)
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approval_request_requires_active_member() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture().await?;
    let template = fixture
        .repository
        .create_template(OrganizationApprovalTemplateWrite::new(
            fixture.organization_id,
            OrganizationApprovalTemplateKey::new("member-change")?,
            OrganizationApprovalResourceType::Member,
            OrganizationApprovalTemplateDigest::new([7; 32])?,
            fixture.owner_id,
            None,
        ))
        .await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::new([8; 16])?,
        fixture.organization_id,
        OrganizationApprovalResourceType::Member,
        "invite user".to_owned(),
        template.id(),
        template.version(),
        UserId::new(999)?,
        now + 3_600,
    )?;
    assert_eq!(
        fixture
            .repository
            .create_request(OrganizationApprovalRequestWrite::new(request))
            .await,
        Err(OrganizationApprovalRepositoryError::MemberUnavailable)
    );
    fixture.pool.close().await?;
    Ok(())
}

async fn new_fixture() -> Result<Fixture, Box<dyn Error>> {
    let options = DatabaseOptions::new("sqlite::memory:")?.with_pool_options(PoolOptions {
        max_connections: 1,
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set("approval-test-group".to_owned()),
        display_name: Set("Approval Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = users::ActiveModel {
        username: Set("approval-test-owner".to_owned()),
        email: Set(Some("approval-test-owner@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("approval-test-owner-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner_id = UserId::new(owner.id)?;
    let organization = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?
        .create(OrganizationCreate::new(
            "Approval Test Organization".to_owned(),
            "approval-test-organization".to_owned(),
            owner_id,
            OrganizationEntitlementWrite::new(
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
                "approval-test".to_owned(),
                None,
                None,
            )?,
        ))
        .await?
        .into_parts()
        .0;
    let repository = OrganizationApprovalRepository::new(pool.clone(), Duration::from_secs(10))?;
    Ok(Fixture {
        pool,
        repository,
        organization_id: organization.id(),
        owner_id,
    })
}
