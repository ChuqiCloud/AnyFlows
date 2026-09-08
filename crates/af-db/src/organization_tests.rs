use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementChangePlan,
    OrganizationEntitlementPreflightError, OrganizationEntitlementSnapshot,
    OrganizationEntitlementStatus, OrganizationId, OrganizationInvitationKey,
    OrganizationInvitationStatus, OrganizationMembershipStatus, OrganizationPlanFeatures,
    OrganizationPlanSnapshot, OrganizationRole, OrganizationTeamId, OrganizationTeamStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementPreflightOutcome,
    OrganizationEntitlementWrite, OrganizationInvitationCreate, OrganizationInvitationPreviewItem,
    OrganizationMembershipChange, OrganizationMembershipCreate, OrganizationOwnershipTransfer,
    OrganizationRepository, OrganizationRepositoryError, OrganizationTeamCreate,
};
use crate::{
    EncryptedCredentialEnvelope,
    entity::{
        groups, organization_invitations, organization_memberships, organization_teams,
        organizations, users,
    },
};

struct Fixture {
    pool: crate::DatabasePool,
    repository: OrganizationRepository,
    users: [UserId; 3],
}

#[tokio::test]
async fn create_is_atomic_and_installs_one_active_owner() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let created = fixture
        .repository
        .create(OrganizationCreate::new(
            "Acme Research".to_owned(),
            "acme-research".to_owned(),
            fixture.users[0],
            active_entitlement(10, 10, None)?,
        ))
        .await?;
    let (organization, owner) = created.into_parts();

    assert_eq!(organization.slug(), "acme-research");
    assert_eq!(organization.version(), 1);
    assert_eq!(organization.organization_key().persistence_key().len(), 32);
    assert_eq!(owner.user_id(), fixture.users[0]);
    assert_eq!(owner.role(), OrganizationRole::Owner);
    assert_eq!(owner.status(), OrganizationMembershipStatus::Active);
    assert_eq!(
        organizations::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        organization_memberships::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );

    // 成员关系使用 RESTRICT，禁止直接删除企业后静默抹去 Owner 事实。
    assert!(
        organizations::Entity::delete_by_id(organization.id().get())
            .exec(fixture.pool.connection())
            .await
            .is_err()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn platform_directory_and_user_memberships_return_only_scoped_summaries()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (first, first_owner) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Alpha Company".to_owned(),
            "alpha-company".to_owned(),
            fixture.users[0],
            active_entitlement(3, 2, None)?,
        ))
        .await?
        .into_parts();
    let (second, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Beta Company".to_owned(),
            "beta-company".to_owned(),
            fixture.users[0],
            active_entitlement(4, 2, None)?,
        ))
        .await?
        .into_parts();
    let member = fixture
        .repository
        .add_membership(
            first.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Admin,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;
    fixture
        .repository
        .add_membership(
            second.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Viewer,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;

    let first_page = fixture.repository.list_platform_directory(None, 1).await?;
    let (records, cursor) = first_page.into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].organization().id(), first.id());
    assert_eq!(records[0].owner_membership().user_id(), fixture.users[0]);
    assert_eq!(records[0].owner_username(), "organization-user-0");
    assert_eq!(records[0].active_member_count(), 2);
    assert_eq!(cursor, Some(first.id()));

    let second_page = fixture
        .repository
        .list_platform_directory(cursor, 1)
        .await?;
    let (records, cursor) = second_page.into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].organization().id(), second.id());
    assert_eq!(records[0].owner_membership().user_id(), fixture.users[0]);
    assert_eq!(records[0].owner_username(), "organization-user-0");
    assert_eq!(cursor, None);

    let memberships = fixture
        .repository
        .list_user_organizations(fixture.users[1], None, 100)
        .await?;
    let (records, cursor) = memberships.into_parts();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].membership().id(), member.id());
    assert_eq!(records[0].organization().id(), first.id());
    assert_eq!(records[0].membership().role(), OrganizationRole::Admin);
    assert_eq!(records[1].organization().id(), second.id());
    assert_eq!(cursor, None);

    let first_member_page = fixture
        .repository
        .list_platform_members(first.id(), None, 1)
        .await?;
    let (records, cursor) = first_member_page.into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].membership().id(), first_owner.id());
    assert_eq!(records[0].username(), "organization-user-0");
    assert_eq!(records[0].email(), Some("organization-user-0@example.com"));
    assert_eq!(cursor, Some(first_owner.id()));
    let (records, cursor) = fixture
        .repository
        .list_platform_members(first.id(), cursor, 1)
        .await?
        .into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].membership().id(), member.id());
    assert_eq!(records[0].username(), "organization-user-1");
    assert_eq!(cursor, None);
    assert!(matches!(
        fixture
            .repository
            .list_platform_members(OrganizationId::new(9_999)?, None, 100)
            .await,
        Err(OrganizationRepositoryError::NotFound)
    ));

    assert!(matches!(
        fixture
            .repository
            .list_user_organizations(UserId::new(9_999)?, None, 100)
            .await,
        Err(OrganizationRepositoryError::NotFound)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn last_owner_is_protected_and_transfer_is_atomic() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, owner) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Ownership Lab".to_owned(),
            "ownership-lab".to_owned(),
            fixture.users[0],
            active_entitlement(10, 10, None)?,
        ))
        .await?
        .into_parts();
    assert_eq!(
        fixture
            .repository
            .change_membership(
                organization.id(),
                owner.id(),
                OrganizationMembershipChange::new(
                    OrganizationRole::Admin,
                    OrganizationMembershipStatus::Active,
                    None,
                    owner.version(),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::LastOwner)
    );

    let target = fixture
        .repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Member,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;
    let transferred = fixture
        .repository
        .transfer_ownership(
            organization.id(),
            OrganizationOwnershipTransfer::new(
                owner.id(),
                owner.version(),
                target.id(),
                target.version(),
            ),
        )
        .await?;
    let (previous_owner, current_owner) = transferred.into_parts();
    assert_eq!(previous_owner.role(), OrganizationRole::Admin);
    assert_eq!(current_owner.role(), OrganizationRole::Owner);
    assert_eq!(previous_owner.version(), 2);
    assert_eq!(current_owner.version(), 2);
    assert_eq!(
        fixture
            .repository
            .change_membership(
                organization.id(),
                current_owner.id(),
                OrganizationMembershipChange::new(
                    OrganizationRole::Owner,
                    OrganizationMembershipStatus::Suspended,
                    None,
                    current_owner.version(),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::LastOwner)
    );

    let stored_previous = organization_memberships::Entity::find_by_id(previous_owner.id().get())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    let stored_current = organization_memberships::Entity::find_by_id(current_owner.id().get())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(stored_previous.role, OrganizationRole::Admin.code());
    assert_eq!(stored_current.role, OrganizationRole::Owner.code());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn team_scope_uniqueness_and_active_members_block_disable() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (first, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "First Company".to_owned(),
            "first-company".to_owned(),
            fixture.users[0],
            active_entitlement(10, 10, None)?,
        ))
        .await?
        .into_parts();
    let (second, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Second Company".to_owned(),
            "second-company".to_owned(),
            fixture.users[2],
            active_entitlement(10, 10, None)?,
        ))
        .await?
        .into_parts();
    let first_team = fixture
        .repository
        .create_team(
            first.id(),
            OrganizationTeamCreate::new("Platform".to_owned(), Some("API team".to_owned())),
        )
        .await?;
    let second_team = fixture
        .repository
        .create_team(
            second.id(),
            OrganizationTeamCreate::new("Platform".to_owned(), None),
        )
        .await?;

    assert_eq!(
        fixture
            .repository
            .add_membership(
                first.id(),
                OrganizationMembershipCreate::new(
                    fixture.users[1],
                    OrganizationRole::Member,
                    Some(second_team.id()),
                    Some(fixture.users[0]),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::NotFound)
    );
    let member = fixture
        .repository
        .add_membership(
            first.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Member,
                Some(first_team.id()),
                Some(fixture.users[0]),
            ),
        )
        .await?;
    assert_eq!(member.team_id(), Some(first_team.id()));
    assert_eq!(
        fixture
            .repository
            .add_membership(
                first.id(),
                OrganizationMembershipCreate::new(
                    fixture.users[1],
                    OrganizationRole::Viewer,
                    None,
                    Some(fixture.users[0]),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .repository
            .disable_team(first.id(), first_team.id(), first_team.version())
            .await,
        Err(OrganizationRepositoryError::TeamHasActiveMembers)
    );

    let moved = fixture
        .repository
        .change_membership(
            first.id(),
            member.id(),
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Active,
                None,
                member.version(),
            ),
        )
        .await?;
    assert_eq!(moved.team_id(), None);
    let disabled = fixture
        .repository
        .disable_team(first.id(), first_team.id(), first_team.version())
        .await?;
    assert_eq!(disabled.status(), OrganizationTeamStatus::Disabled);
    assert_eq!(disabled.version(), 2);

    let stored = organization_teams::Entity::find_by_id(first_team.id().get())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert!(stored.deleted_at.is_some());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    assert!(OrganizationRepository::new(pool.clone(), Duration::ZERO).is_err());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn authorized_access_requires_same_user_and_active_enterprise_path()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, owner) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Authorization Lab".to_owned(),
            "authorization-lab".to_owned(),
            fixture.users[0],
            active_entitlement(10, 10, None)?,
        ))
        .await?
        .into_parts();
    let member = fixture
        .repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Viewer,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;

    let owner_access = fixture
        .repository
        .authorized_access(organization.id(), fixture.users[0])
        .await?;
    assert_eq!(owner_access.membership().id(), owner.id());
    assert_eq!(owner_access.organization().id(), organization.id());
    assert_eq!(
        fixture
            .repository
            .authorized_access(organization.id(), fixture.users[2])
            .await,
        Err(OrganizationRepositoryError::NotFound)
    );

    let member_access = fixture
        .repository
        .authorized_access(organization.id(), fixture.users[1])
        .await?;
    assert_eq!(member_access.membership().id(), member.id());

    fixture
        .repository
        .change_membership(
            organization.id(),
            member.id(),
            OrganizationMembershipChange::new(
                OrganizationRole::Viewer,
                OrganizationMembershipStatus::Suspended,
                None,
                member.version(),
            ),
        )
        .await?;
    assert_eq!(
        fixture
            .repository
            .authorized_access(organization.id(), fixture.users[1])
            .await,
        Err(OrganizationRepositoryError::NotFound)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn entitlement_enforces_capacity_and_cas() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Capacity Lab".to_owned(),
            "capacity-lab".to_owned(),
            fixture.users[0],
            active_entitlement(1, 1, None)?,
        ))
        .await?
        .into_parts();

    assert_eq!(
        fixture
            .repository
            .add_membership(
                organization.id(),
                OrganizationMembershipCreate::new(
                    fixture.users[1],
                    OrganizationRole::Member,
                    None,
                    Some(fixture.users[0]),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::SeatLimitReached)
    );
    fixture
        .repository
        .create_team(
            organization.id(),
            OrganizationTeamCreate::new("First Team".to_owned(), None),
        )
        .await?;
    assert_eq!(
        fixture
            .repository
            .create_team(
                organization.id(),
                OrganizationTeamCreate::new("Second Team".to_owned(), None),
            )
            .await,
        Err(OrganizationRepositoryError::TeamLimitReached)
    );

    let read_only = OrganizationEntitlementSnapshot::new(
        OrganizationEntitlementCapacity::new(10, 10, 2)?,
        true,
        false,
        false,
        90,
        946_684_800,
        946_771_200,
        Some(946_771_200),
    )?;
    let replaced = fixture
        .repository
        .replace_entitlement(
            organization.id(),
            OrganizationEntitlementWrite::new(
                read_only,
                "contract".to_owned(),
                Some("agreement-redacted".to_owned()),
                Some(1),
            )?,
        )
        .await?;
    assert_eq!(replaced.version(), 2);
    let projection = fixture.repository.entitlement(organization.id()).await?;
    assert_eq!(
        projection.organization_status(),
        af_domain::OrganizationStatus::Active
    );
    assert_eq!(projection.entitlement().source(), "contract");
    assert_eq!(
        projection.entitlement().external_reference(),
        Some("agreement-redacted")
    );
    assert_eq!(projection.entitlement().version(), 2);
    assert!(projection.entitlement().updated_at() > 0);
    assert_eq!(
        replaced.snapshot().status(
            af_domain::OrganizationStatus::Active,
            TimeDateTimeWithTimeZone::now_utc().unix_timestamp(),
        ),
        OrganizationEntitlementStatus::ReadOnly
    );
    assert_eq!(
        fixture
            .repository
            .replace_entitlement(organization.id(), active_entitlement(10, 10, Some(1))?,)
            .await,
        Err(OrganizationRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .repository
            .add_membership(
                organization.id(),
                OrganizationMembershipCreate::new(
                    fixture.users[1],
                    OrganizationRole::Member,
                    None,
                    Some(fixture.users[0]),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::EntitlementRestricted)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn entitlement_change_preflight_is_atomic_and_keeps_resources() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Entitlement Change Lab".to_owned(),
            "entitlement-change-lab".to_owned(),
            fixture.users[0],
            active_entitlement(2, 1, None)?,
        ))
        .await?
        .into_parts();
    fixture
        .repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Member,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;
    fixture
        .repository
        .create_team(
            organization.id(),
            OrganizationTeamCreate::new("Capacity Team".to_owned(), None),
        )
        .await?;

    let downgrade_plan = OrganizationEntitlementChangePlan::from_plan(
        OrganizationPlanSnapshot::new(
            OrganizationPlanFeatures::new(true, false, false),
            1,
            1,
            1,
            90,
            30,
        )?,
        2_000,
        0,
    )?;
    assert_eq!(
        fixture
            .repository
            .preflight_entitlement_change(organization.id(), downgrade_plan)
            .await?,
        OrganizationEntitlementPreflightOutcome::Rejected(
            OrganizationEntitlementPreflightError::SeatCapacityExceeded
        )
    );
    let entitlement = fixture.repository.entitlement(organization.id()).await?;
    assert_eq!(entitlement.entitlement().version(), 1);
    assert_eq!(
        fixture
            .repository
            .list_teams(organization.id(), None, 10)
            .await?
            .into_parts()
            .0
            .len(),
        1
    );
    assert_eq!(
        fixture
            .repository
            .list_memberships(organization.id(), None, 10)
            .await?
            .into_parts()
            .0
            .len(),
        2
    );

    let valid_plan = OrganizationEntitlementChangePlan::from_plan(
        OrganizationPlanSnapshot::new(
            OrganizationPlanFeatures::new(true, false, false),
            2,
            1,
            1,
            90,
            30,
        )?,
        2_000,
        0,
    )?;
    assert_eq!(
        fixture
            .repository
            .preflight_entitlement_change(organization.id(), valid_plan)
            .await?,
        OrganizationEntitlementPreflightOutcome::Passed
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invitation_resend_invalidates_old_token_and_accepts_once() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Invitation Lab".to_owned(),
            "invitation-lab".to_owned(),
            fixture.users[0],
            active_entitlement(10, 10, None)?,
        ))
        .await?
        .into_parts();
    let email = "organization-user-1@example.com";
    let old_key = OrganizationInvitationKey::new([1; 16])?;
    let old_digest = [65; 32];
    let fingerprint = [1; 32];
    let old = invitation_write(1, email, fixture.users[0], None)?;
    fixture
        .repository
        .create_invitation(organization.id(), old)
        .await?;

    let replacement_key = OrganizationInvitationKey::new([2; 16])?;
    let replacement_digest = [66; 32];
    let replacement = invitation_write_with_fingerprint(2, 1, email, fixture.users[0], None)?;
    fixture
        .repository
        .resend_invitation(organization.id(), old_key, replacement)
        .await?;
    assert_eq!(
        fixture
            .repository
            .accept_invitation(
                organization.id(),
                old_key,
                old_digest,
                fingerprint,
                fixture.users[1],
                email.to_owned(),
            )
            .await,
        Err(OrganizationRepositoryError::InvitationNotPending)
    );
    let membership = fixture
        .repository
        .accept_invitation(
            organization.id(),
            replacement_key,
            replacement_digest,
            fingerprint,
            fixture.users[1],
            email.to_owned(),
        )
        .await?;
    assert_eq!(membership.user_id(), fixture.users[1]);
    assert_eq!(
        fixture
            .repository
            .accept_invitation(
                organization.id(),
                replacement_key,
                replacement_digest,
                fingerprint,
                fixture.users[1],
                email.to_owned(),
            )
            .await,
        Err(OrganizationRepositoryError::InvitationNotPending)
    );
    let invitations = fixture
        .repository
        .list_invitations(organization.id())
        .await?;
    assert_eq!(invitations.len(), 2);
    assert!(invitations.iter().any(|record| {
        record.invitation_key() == old_key
            && record.status() == OrganizationInvitationStatus::Revoked
    }));
    assert!(invitations.iter().any(|record| {
        record.invitation_key() == replacement_key
            && record.status() == OrganizationInvitationStatus::Accepted
    }));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invitation_batch_is_idempotent_and_binds_payload() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Batch Lab".to_owned(),
            "batch-lab".to_owned(),
            fixture.users[0],
            active_entitlement(10, 10, None)?,
        ))
        .await?
        .into_parts();
    let batch_key = OrganizationInvitationKey::new([0x31; 16])?;
    let batch = Some((batch_key, [0x41; 32], 2));
    let first = fixture
        .repository
        .create_invitation_batch(
            organization.id(),
            vec![
                invitation_write(3, "batch-one@example.com", fixture.users[0], batch)?,
                invitation_write(4, "batch-two@example.com", fixture.users[0], batch)?,
            ],
        )
        .await?;
    assert!(!first.replayed());
    let replay = fixture
        .repository
        .create_invitation_batch(
            organization.id(),
            vec![
                invitation_write(5, "batch-one@example.com", fixture.users[0], batch)?,
                invitation_write(6, "batch-two@example.com", fixture.users[0], batch)?,
            ],
        )
        .await?;
    assert!(replay.replayed());
    assert_eq!(first.records(), replay.records());
    let conflicting_batch = Some((batch_key, [0x42; 32], 2));
    assert_eq!(
        fixture
            .repository
            .create_invitation_batch(
                organization.id(),
                vec![
                    invitation_write(
                        7,
                        "batch-one@example.com",
                        fixture.users[0],
                        conflicting_batch,
                    )?,
                    invitation_write(
                        8,
                        "batch-two@example.com",
                        fixture.users[0],
                        conflicting_batch,
                    )?,
                ],
            )
            .await,
        Err(OrganizationRepositoryError::InvitationBatchConflict)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invitation_preview_reports_members_pending_teams_and_seats() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Preview Lab".to_owned(),
            "preview-lab".to_owned(),
            fixture.users[0],
            active_entitlement(3, 3, None)?,
        ))
        .await?
        .into_parts();
    let team = fixture
        .repository
        .create_team(
            organization.id(),
            OrganizationTeamCreate::new("Preview Team".to_owned(), None),
        )
        .await?;
    let pending_fingerprint = [9; 32];
    let pending = invitation_write(9, "pending@example.com", fixture.users[0], None)?;
    fixture
        .repository
        .create_invitation(organization.id(), pending)
        .await?;
    let snapshot = fixture
        .repository
        .preview_invitations(
            organization.id(),
            vec![
                OrganizationInvitationPreviewItem::new(
                    "organization-user-0@example.com".to_owned(),
                    [0xa1; 32],
                    None,
                ),
                OrganizationInvitationPreviewItem::new(
                    "pending@example.com".to_owned(),
                    pending_fingerprint,
                    None,
                ),
                OrganizationInvitationPreviewItem::new(
                    "fresh@example.com".to_owned(),
                    [0xa3; 32],
                    Some(team.id()),
                ),
                OrganizationInvitationPreviewItem::new(
                    "wrong-team@example.com".to_owned(),
                    [0xa4; 32],
                    Some(OrganizationTeamId::new(9_999)?),
                ),
            ],
        )
        .await?;
    assert_eq!(snapshot.available_seats(), 1);
    assert!(snapshot.rows()[0].existing_member());
    assert!(snapshot.rows()[1].pending_invitation());
    assert!(snapshot.rows()[2].valid_team());
    assert!(!snapshot.rows()[3].valid_team());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invitations_reject_existing_members_and_serialize_last_seat() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .repository
        .create(OrganizationCreate::new(
            "Seat Lab".to_owned(),
            "seat-lab".to_owned(),
            fixture.users[0],
            active_entitlement(2, 2, None)?,
        ))
        .await?
        .into_parts();
    assert_eq!(
        fixture
            .repository
            .create_invitation(
                organization.id(),
                invitation_write(
                    10,
                    "organization-user-0@example.com",
                    fixture.users[0],
                    None,
                )?,
            )
            .await,
        Err(OrganizationRepositoryError::Conflict)
    );

    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let first = invitation_write(11, "seat-one@example.com", fixture.users[0], None)?;
    let second = invitation_write(12, "seat-two@example.com", fixture.users[0], None)?;
    let (first, second) = tokio::join!(
        first_repository.create_invitation(organization.id(), first),
        second_repository.create_invitation(organization.id(), second),
    );
    let results = [first, second];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                matches!(result, Err(OrganizationRepositoryError::SeatLimitReached))
            })
            .count(),
        1
    );
    assert_eq!(
        organization_invitations::Entity::find()
            .filter(organization_invitations::Column::OrganizationId.eq(organization.id().get()),)
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

fn active_entitlement(
    seat_limit: i64,
    team_limit: i64,
    expected_version: Option<u64>,
) -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(seat_limit, team_limit, 2)?,
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )?,
        "contract".to_owned(),
        None,
        expected_version,
    )?)
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("organization-test-group".to_owned()),
        display_name: Set("企业测试分组".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("organization-user-{index}")),
            email: Set(Some(format!("organization-user-{index}@example.com"))),
            password_hash: Set(None),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("organization-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    let repository = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok(Fixture {
        pool,
        repository,
        users: user_ids.try_into().expect("测试用户数量固定为三"),
    })
}

fn invitation_write(
    marker: u8,
    normalized_email: &str,
    invited_by_user_id: UserId,
    batch: Option<(OrganizationInvitationKey, [u8; 32], usize)>,
) -> Result<OrganizationInvitationCreate, Box<dyn Error>> {
    invitation_write_with_fingerprint(marker, marker, normalized_email, invited_by_user_id, batch)
}

fn invitation_write_with_fingerprint(
    marker: u8,
    fingerprint_marker: u8,
    normalized_email: &str,
    invited_by_user_id: UserId,
    batch: Option<(OrganizationInvitationKey, [u8; 32], usize)>,
) -> Result<OrganizationInvitationCreate, Box<dyn Error>> {
    let (batch_key, batch_digest, batch_size) = batch
        .map(|(key, digest, size)| (Some(key), Some(digest), Some(size)))
        .unwrap_or((None, None, None));
    Ok(OrganizationInvitationCreate::new(
        OrganizationInvitationKey::new([marker; 16])?,
        normalized_email.to_owned(),
        [fingerprint_marker; 32],
        EncryptedCredentialEnvelope::new(
            "organization-test-key".to_owned(),
            [marker; 24],
            vec![marker; 16],
        )?,
        [marker.wrapping_add(64); 32],
        OrganizationRole::Member,
        None,
        invited_by_user_id,
        batch_key,
        batch_digest,
        batch_size,
        TimeDateTimeWithTimeZone::now_utc()
            .unix_timestamp()
            .saturating_add(86_400),
    )?)
}
