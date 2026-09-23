use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationProvisioningCreateOutcome, OrganizationProvisioningDecisionKind,
    OrganizationProvisioningDecisionOutcome, OrganizationProvisioningIdempotencyKey,
    OrganizationProvisioningRequest, OrganizationProvisioningRequestKey,
    OrganizationProvisioningStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, EntityTrait, IntoActiveModel, PaginatorTrait, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationEntitlementWrite,
    OrganizationProvisioningDecisionWrite, OrganizationProvisioningRepository,
    OrganizationProvisioningRepositoryError,
    entity::{
        SensitiveString, groups, organization_entitlements, organization_memberships,
        organization_provisioning_requests, organization_sso_policies, organization_wallets,
        organizations, platform_audit_logs, users,
    },
};

struct Fixture {
    pool: crate::DatabasePool,
    repository: OrganizationProvisioningRepository,
    users: [UserId; 3],
}

#[tokio::test]
async fn request_creation_is_idempotent_and_user_scoped() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let request = request(1, fixture.users[0], "Acme AI", "acme-ai", "private reason")?;
    let request_key = request.request_key();
    let idempotency_key = request.idempotency_key();

    let created = fixture.repository.create_request(request.clone()).await?;
    let record = match created {
        OrganizationProvisioningCreateOutcome::Created(record) => record,
        OrganizationProvisioningCreateOutcome::Existing(_) => panic!("首次申请必须创建事实"),
    };
    assert_eq!(record.version(), 1);
    assert_eq!(record.organization_slug(), "acme-ai");
    assert!(matches!(
        fixture.repository.create_request(request.clone()).await?,
        OrganizationProvisioningCreateOutcome::Existing(_)
    ));
    assert_eq!(
        fixture
            .repository
            .get_request(fixture.users[0], request_key)
            .await?
            .expect("申请人必须能读取原申请"),
        record
    );
    assert_eq!(
        fixture
            .repository
            .get_by_idempotency_key(fixture.users[0], idempotency_key)
            .await?
            .expect("幂等键必须恢复原申请"),
        record
    );
    assert_eq!(
        fixture
            .repository
            .get_active_request(fixture.users[0])
            .await?
            .expect("必须返回唯一活动申请"),
        record
    );
    assert!(
        fixture
            .repository
            .get_request(fixture.users[1], request_key)
            .await?
            .is_none()
    );
    let rendered = format!("{record:?}");
    assert!(!rendered.contains("private reason"));
    assert!(!rendered.contains("Acme AI"));
    assert!(
        users::Entity::delete_by_id(fixture.users[0].get())
            .exec(fixture.pool.connection())
            .await
            .is_err()
    );
    assert_eq!(
        organization_provisioning_requests::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn active_user_slug_and_idempotency_conflicts_are_distinct() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = request(10, fixture.users[0], "Alpha", "alpha", "first reason")?;
    fixture.repository.create_request(first.clone()).await?;

    let conflicting_idempotency = OrganizationProvisioningRequest::new(
        first.request_key(),
        first.idempotency_key(),
        fixture.users[0],
        "Changed".to_owned(),
        "changed".to_owned(),
        "changed reason".to_owned(),
        first.created_at(),
    )?;
    assert_eq!(
        fixture
            .repository
            .create_request(conflicting_idempotency)
            .await,
        Err(OrganizationProvisioningRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .repository
            .create_request(request(20, fixture.users[0], "Beta", "beta", "other")?)
            .await,
        Err(OrganizationProvisioningRepositoryError::ActiveApplicantConflict)
    );
    assert_eq!(
        fixture
            .repository
            .create_request(request(30, fixture.users[1], "Alpha 2", "alpha", "other")?)
            .await,
        Err(OrganizationProvisioningRepositoryError::ActiveSlugConflict)
    );

    let disabled = users::Entity::find_by_id(fixture.users[2].get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在");
    let mut disabled = disabled.into_active_model();
    disabled.status = Set(2);
    disabled.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .repository
            .create_request(request(
                40,
                fixture.users[2],
                "Disabled",
                "disabled",
                "other"
            )?)
            .await,
        Err(OrganizationProvisioningRepositoryError::ApplicantUnavailable)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn create_retry_recovers_the_same_request_after_it_reaches_a_terminal_state()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let request = request(
        45,
        fixture.users[0],
        "Terminal",
        "terminal",
        "private reason",
    )?;
    fixture.repository.create_request(request.clone()).await?;

    let model = organization_provisioning_requests::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .expect("申请事实必须存在");
    let decided_at =
        TimeDateTimeWithTimeZone::from_unix_timestamp(i64::try_from(request.created_at() + 1)?)?;
    let mut terminal = model.into_active_model();
    terminal.status = Set(OrganizationProvisioningStatus::Rejected.code());
    terminal.active_slot = Set(None);
    terminal.version = Set(2);
    terminal.reviewer_user_id = Set(Some(fixture.users[1].get()));
    terminal.decision_reason = Set(Some(SensitiveString::from("manual rejection")));
    terminal.updated_at = Set(decided_at);
    terminal.decided_at = Set(Some(decided_at));
    terminal.update(fixture.pool.connection()).await?;

    let existing = fixture.repository.create_request(request).await?;
    let OrganizationProvisioningCreateOutcome::Existing(existing) = existing else {
        panic!("原创建命令重放必须恢复已经进入终态的同一申请");
    };
    assert_eq!(existing.status(), OrganizationProvisioningStatus::Rejected);
    assert_eq!(existing.version(), 2);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_slug_claim_has_one_winner_and_constraints_fail_closed()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let first = request(50, fixture.users[0], "Shared One", "shared", "one")?;
    let second = request(60, fixture.users[1], "Shared Two", "shared", "two")?;
    let (first, second) = tokio::join!(
        first_repository.create_request(first),
        second_repository.create_request(second),
    );
    let outcomes = [first, second];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                Ok(OrganizationProvisioningCreateOutcome::Created(_))
            ))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    Err(OrganizationProvisioningRepositoryError::ActiveSlugConflict)
                )
            })
            .count(),
        1
    );

    let now = TimeDateTimeWithTimeZone::from_unix_timestamp(1_900_000_100)?;
    let invalid = organization_provisioning_requests::ActiveModel {
        request_key: Set("A".repeat(32)),
        idempotency_key: Set("b".repeat(32)),
        applicant_user_id: Set(fixture.users[2].get()),
        organization_name: Set("Invalid".to_owned()),
        organization_slug: Set("invalid".to_owned()),
        business_reason: Set(SensitiveString::from("invalid")),
        status: Set(1),
        active_slot: Set(Some(1)),
        version: Set(1),
        reviewer_user_id: Set(None),
        decision_reason: Set(None),
        organization_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        decided_at: Set(None),
        ..Default::default()
    };
    assert!(invalid.insert(fixture.pool.connection()).await.is_err());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn approval_atomically_provisions_and_recovers_the_same_organization()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let request = request(70, fixture.users[0], "Approved", "approved", "private")?;
    let request_key = request.request_key();
    fixture.repository.create_request(request).await?;
    let decision = approval(request_key, fixture.users[1])?;

    let OrganizationProvisioningDecisionOutcome::Applied(applied) =
        fixture.repository.decide_request(decision.clone()).await?
    else {
        panic!("首次审批必须应用决定");
    };
    assert_eq!(applied.status(), OrganizationProvisioningStatus::Approved);
    assert_eq!(applied.version(), 2);
    assert_eq!(applied.reviewer_user_id(), Some(fixture.users[1]));
    assert!(applied.organization_id().is_some());

    let OrganizationProvisioningDecisionOutcome::Existing(existing) =
        fixture.repository.decide_request(decision).await?
    else {
        panic!("相同审批重放必须恢复原决定");
    };
    assert_eq!(existing, applied);
    for count in [
        organizations::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        organization_memberships::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        organization_entitlements::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        organization_wallets::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        organization_sso_policies::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        platform_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
    ] {
        assert_eq!(count, 1);
    }
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn rejection_requires_admin_and_matching_version_then_releases_active_slots()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let initial_request = request(80, fixture.users[0], "Rejected", "rejected", "private")?;
    let request_key = initial_request.request_key();
    fixture.repository.create_request(initial_request).await?;

    assert_eq!(
        fixture
            .repository
            .decide_request(rejection(request_key, 1, fixture.users[2])?)
            .await,
        Err(OrganizationProvisioningRepositoryError::Forbidden)
    );
    assert_eq!(
        fixture
            .repository
            .decide_request(rejection(request_key, 2, fixture.users[1])?)
            .await,
        Err(OrganizationProvisioningRepositoryError::VersionConflict)
    );
    let OrganizationProvisioningDecisionOutcome::Applied(rejected) = fixture
        .repository
        .decide_request(rejection(request_key, 1, fixture.users[1])?)
        .await?
    else {
        panic!("首次拒绝必须应用决定");
    };
    assert_eq!(rejected.status(), OrganizationProvisioningStatus::Rejected);
    assert!(rejected.organization_id().is_none());
    assert!(matches!(
        fixture
            .repository
            .create_request(request(82, fixture.users[0], "Retry", "rejected", "again")?)
            .await?,
        OrganizationProvisioningCreateOutcome::Created(_)
    ));
    assert_eq!(
        organizations::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn occupied_slug_rolls_back_the_entire_approval() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = request(90, fixture.users[0], "First", "claimed", "first")?;
    let first_key = first.request_key();
    fixture.repository.create_request(first).await?;
    fixture
        .repository
        .decide_request(approval(first_key, fixture.users[1])?)
        .await?;

    let second = request(92, fixture.users[2], "Second", "claimed", "second")?;
    let second_key = second.request_key();
    fixture.repository.create_request(second).await?;
    assert_eq!(
        fixture
            .repository
            .decide_request(approval(second_key, fixture.users[1])?)
            .await,
        Err(OrganizationProvisioningRepositoryError::OrganizationSlugConflict)
    );
    let pending = fixture
        .repository
        .get_request(fixture.users[2], second_key)
        .await?
        .expect("冲突申请必须保持可审批");
    assert_eq!(pending.status(), OrganizationProvisioningStatus::Pending);
    assert_eq!(pending.version(), 1);
    assert_eq!(
        organizations::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        platform_audit_logs::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

fn approval(
    request_key: OrganizationProvisioningRequestKey,
    reviewer_user_id: UserId,
) -> Result<OrganizationProvisioningDecisionWrite, Box<dyn Error>> {
    Ok(OrganizationProvisioningDecisionWrite::new(
        request_key,
        1,
        reviewer_user_id,
        OrganizationProvisioningDecisionKind::Approve,
        Some("verified".to_owned()),
        Some(initial_entitlement()?),
    )?)
}

fn rejection(
    request_key: OrganizationProvisioningRequestKey,
    expected_version: u64,
    reviewer_user_id: UserId,
) -> Result<OrganizationProvisioningDecisionWrite, Box<dyn Error>> {
    Ok(OrganizationProvisioningDecisionWrite::new(
        request_key,
        expected_version,
        reviewer_user_id,
        OrganizationProvisioningDecisionKind::Reject,
        Some("requirements_not_met".to_owned()),
        None,
    )?)
}

fn initial_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(5, 2, 3)?,
            true,
            false,
            false,
            90,
            1_900_000_000,
            1_931_536_000,
            Some(1_934_128_000),
        )?,
        "provisioning_approval".to_owned(),
        None,
        None,
    )?)
}

fn request(
    marker: u8,
    user_id: UserId,
    name: &str,
    slug: &str,
    reason: &str,
) -> Result<OrganizationProvisioningRequest, Box<dyn Error>> {
    Ok(OrganizationProvisioningRequest::new(
        OrganizationProvisioningRequestKey::new([marker; 16])?,
        OrganizationProvisioningIdempotencyKey::new([marker.wrapping_add(1); 16])?,
        user_id,
        name.to_owned(),
        slug.to_owned(),
        reason.to_owned(),
        1_900_000_000 + u64::from(marker),
    )?)
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("organization-provisioning-test".to_owned()),
        display_name: Set("企业开通申请测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("organization-provisioning-user-{index}")),
            email: Set(Some(format!(
                "organization-provisioning-{index}@example.com"
            ))),
            password_hash: Set(None),
            role: Set(if index == 1 { 1 } else { 0 }),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("organization-provisioning-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        crate::entity::account_verifications::ActiveModel {
            user_id: Set(user.id),
            kind: Set("enterprise".to_owned()),
            provider: Set("manual".to_owned()),
            document_country: Set("CN".to_owned()),
            document_type: Set("business_registration".to_owned()),
            document_number_masked: Set(None),
            subject_name: Set(SensitiveString::from("Verified fixture".to_owned())),
            summary: Set(SensitiveString::from("reviewed fixture".to_owned())),
            status: Set(4),
            version: Set(2),
            reviewer_user_id: Set(None),
            review_reason: Set(None),
            created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            updated_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    Ok(Fixture {
        repository: OrganizationProvisioningRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        users: user_ids.try_into().expect("测试用户数量固定为三"),
    })
}

#[tokio::test]
async fn enterprise_verification_is_required_only_for_new_provisioning_requests()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let original = request(
        92,
        fixture.users[0],
        "Verified company",
        "verified-company",
        "Workspace request",
    )?;
    fixture.repository.create_request(original.clone()).await?;
    crate::entity::account_verifications::Entity::delete_many()
        .exec(fixture.pool.connection())
        .await?;
    assert!(matches!(
        fixture.repository.create_request(original).await?,
        OrganizationProvisioningCreateOutcome::Existing(_)
    ));
    let unverified = request(
        93,
        fixture.users[2],
        "Unverified company",
        "unverified-company",
        "Workspace request",
    )?;
    assert_eq!(
        fixture.repository.create_request(unverified).await,
        Err(OrganizationProvisioningRepositoryError::Forbidden)
    );
    fixture.pool.close().await?;
    Ok(())
}
