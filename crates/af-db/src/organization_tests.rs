use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationEntitlementStatus, OrganizationMembershipStatus, OrganizationRole,
    OrganizationTeamStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, EntityTrait, PaginatorTrait, Set, entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationMembershipChange, OrganizationMembershipCreate, OrganizationOwnershipTransfer,
    OrganizationRepository, OrganizationRepositoryError, OrganizationTeamCreate,
};
use crate::entity::{groups, organization_memberships, organization_teams, organizations, users};

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
