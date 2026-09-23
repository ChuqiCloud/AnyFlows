use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipStatus, OrganizationScimGroupResource, OrganizationScimResourceId,
    OrganizationScimResourceSet, OrganizationScimResourceVersion, OrganizationScimTokenStatus,
    OrganizationScimUserResource, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationScimGroupRepository, OrganizationScimGroupRepositoryError,
    OrganizationScimGroupStatus, OrganizationScimGroupSyncOutcome,
    OrganizationScimTokenCreateRecord, OrganizationScimTokenRepository,
    OrganizationScimTokenRepositoryError, OrganizationScimUserRepository,
    OrganizationScimUserRepositoryError, OrganizationScimUserSyncOutcome,
    entity::{
        groups, organization_entitlements, organization_memberships,
        organization_scim_group_resources, organization_scim_tokens,
        organization_scim_user_resources, organization_teams, organizations, users,
    },
};

#[tokio::test]
async fn scim_token_is_digest_only_and_organization_scoped() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("scim-token-test-group".to_owned()),
        display_name: Set("SCIM Token Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("scim-token-test-user".to_owned()),
        email: Set(Some("scim-token-test@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("scim-token-test-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = UserId::new(user.id)?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM Token Test Organization".to_owned(),
            "scim-token-test".to_owned(),
            owner,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let scim = OrganizationScimTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = scim
        .create(OrganizationScimTokenCreateRecord::new(
            organization.id(),
            "Directory sync".to_owned(),
            OrganizationScimResourceSet::users_and_groups(),
        ))
        .await?;
    let (record, token) = issued.into_parts();
    let token_text = token.as_str().to_owned();
    let stored = organization_scim_tokens::Entity::find_by_id(record.id().get())
        .one(pool.connection())
        .await?
        .expect("令牌实体必须存在");
    assert_eq!(stored.token_digest.as_str(), token.digest().as_str());
    assert!(!format!("{stored:?}").contains(&token_text));
    assert_eq!(record.status(), OrganizationScimTokenStatus::Active);

    let authenticated = scim
        .authenticate_for_organization(organization.id(), token.clone())
        .await?;
    assert_eq!(authenticated.id(), record.id());

    let organization_model = organizations::Entity::find_by_id(organization.id().get())
        .one(pool.connection())
        .await?
        .expect("企业实体必须存在")
        .into_active_model();
    let mut suspended_organization = organization_model;
    suspended_organization.status = Set(2);
    suspended_organization.update(pool.connection()).await?;
    assert!(matches!(
        scim.authenticate_for_organization(organization.id(), token.clone())
            .await,
        Err(OrganizationScimTokenRepositoryError::Restricted)
    ));
    assert!(matches!(
        scim.authenticate_for_organization(
            af_domain::OrganizationId::new(organization.id().get() + 1)?,
            token.clone(),
        )
        .await,
        Err(OrganizationScimTokenRepositoryError::NotFound)
    ));

    let revoked = scim
        .revoke(organization.id(), record.id(), record.version())
        .await?;
    assert_eq!(revoked.status(), OrganizationScimTokenStatus::Revoked);
    assert_eq!(revoked.version(), 2);
    assert!(matches!(
        scim.revoke(organization.id(), record.id(), record.version())
            .await,
        Err(OrganizationScimTokenRepositoryError::Conflict)
    ));
    assert!(matches!(
        scim.authenticate_for_organization(organization.id(), token)
            .await,
        Err(OrganizationScimTokenRepositoryError::NotFound)
    ));
    pool.close().await?;
    Ok(())
}

struct ScimUserFixture {
    pool: crate::DatabasePool,
    organization_id: OrganizationId,
    token: crate::OrganizationScimTokenRecord,
    repository: OrganizationScimUserRepository,
    user_id: UserId,
}

async fn scim_user_fixture() -> Result<ScimUserFixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("scim-user-test-group".to_owned()),
        display_name: Set("SCIM User Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = insert_scim_user(&pool, group.id, "scim-user-owner").await?;
    let target = insert_scim_user(&pool, group.id, "scim-user-target").await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM User Test Organization".to_owned(),
            "scim-user-test-org".to_owned(),
            UserId::new(owner.id)?,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let token_repository =
        OrganizationScimTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = token_repository
        .create(OrganizationScimTokenCreateRecord::new(
            organization.id(),
            "User directory".to_owned(),
            OrganizationScimResourceSet::users(),
        ))
        .await?;
    let (token, _) = issued.into_parts();
    let repository = OrganizationScimUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok(ScimUserFixture {
        pool,
        organization_id: organization.id(),
        token,
        repository,
        user_id: UserId::new(target.id)?,
    })
}

async fn insert_scim_user(
    pool: &crate::DatabasePool,
    group_id: i64,
    username: &str,
) -> Result<users::Model, Box<dyn Error>> {
    Ok(users::ActiveModel {
        username: Set(username.to_owned()),
        email: Set(Some(format!("{username}@example.com"))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("{username}-aff")),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?)
}

fn scim_user_resource(
    organization_id: OrganizationId,
    resource_id: &str,
    version: u64,
    username: &str,
    active: bool,
) -> Result<OrganizationScimUserResource, Box<dyn Error>> {
    Ok(OrganizationScimUserResource::new(
        organization_id,
        OrganizationScimResourceId::new(resource_id.to_owned())?,
        OrganizationScimResourceVersion::new(version)?,
        username.to_owned(),
        Some("SCIM target".to_owned()),
        active,
    )?)
}

#[tokio::test]
async fn scim_user_sync_is_idempotent_and_projects_member_lifecycle() -> Result<(), Box<dyn Error>>
{
    let fixture = scim_user_fixture().await?;
    let first = fixture
        .repository
        .synchronize(
            fixture.token.clone(),
            scim_user_resource(
                fixture.organization_id,
                "directory-user-1",
                1,
                "scim-user-target",
                true,
            )?,
        )
        .await?;
    assert_eq!(first.outcome(), OrganizationScimUserSyncOutcome::Applied);
    assert_eq!(
        first.record().membership_status(),
        OrganizationMembershipStatus::Active
    );
    assert_eq!(first.record().user_id(), fixture.user_id);

    let noop = fixture
        .repository
        .synchronize(
            fixture.token.clone(),
            scim_user_resource(
                fixture.organization_id,
                "directory-user-1",
                1,
                "scim-user-target",
                false,
            )?,
        )
        .await?;
    assert_eq!(noop.outcome(), OrganizationScimUserSyncOutcome::Noop);
    assert_eq!(
        noop.record().membership_status(),
        OrganizationMembershipStatus::Active
    );

    let suspended = fixture
        .repository
        .synchronize(
            fixture.token.clone(),
            scim_user_resource(
                fixture.organization_id,
                "directory-user-1",
                2,
                "scim-user-target",
                false,
            )?,
        )
        .await?;
    assert_eq!(
        suspended.record().membership_status(),
        OrganizationMembershipStatus::Suspended
    );
    let restored = fixture
        .repository
        .synchronize(
            fixture.token.clone(),
            scim_user_resource(
                fixture.organization_id,
                "directory-user-1",
                3,
                "scim-user-target",
                true,
            )?,
        )
        .await?;
    assert_eq!(
        restored.record().membership_status(),
        OrganizationMembershipStatus::Active
    );

    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token.clone(),
                scim_user_resource(
                    fixture.organization_id,
                    "directory-user-1",
                    2,
                    "scim-user-target",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::Conflict)
    ));
    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token.clone(),
                scim_user_resource(
                    fixture.organization_id,
                    "directory-user-1",
                    4,
                    "scim-user-owner",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::Conflict)
    ));

    let binding = organization_scim_user_resources::Entity::find()
        .filter(
            organization_scim_user_resources::Column::OrganizationId
                .eq(fixture.organization_id.get()),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("SCIM 资源绑定必须存在");
    assert_eq!(binding.user_id, fixture.user_id.get());
    assert_eq!(binding.resource_version, 3);
    let membership = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(fixture.organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(fixture.user_id.get()))
        .one(fixture.pool.connection())
        .await?
        .expect("SCIM 成员关系必须存在");
    assert_eq!(
        membership.status,
        OrganizationMembershipStatus::Active.code()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn scim_user_sync_rejects_missing_users_and_cross_organization_resources()
-> Result<(), Box<dyn Error>> {
    let fixture = scim_user_fixture().await?;
    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token.clone(),
                scim_user_resource(
                    fixture.organization_id,
                    "directory-missing",
                    1,
                    "missing-user",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::UserUnavailable)
    ));
    let other_organization = OrganizationId::new(fixture.organization_id.get() + 1)?;
    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token,
                scim_user_resource(
                    other_organization,
                    "directory-cross-org",
                    1,
                    "scim-user-target",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::Restricted)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn scim_user_sync_rejects_seat_and_entitlement_boundaries() -> Result<(), Box<dyn Error>> {
    let fixture = scim_user_fixture().await?;
    replace_scim_entitlement(
        &fixture,
        OrganizationEntitlementCapacity::new(1, 10, 10)?,
        4_102_444_800,
    )
    .await?;
    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token.clone(),
                scim_user_resource(
                    fixture.organization_id,
                    "directory-seat-full",
                    1,
                    "scim-user-target",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::LifecycleRejected)
    ));

    replace_scim_entitlement(
        &fixture,
        OrganizationEntitlementCapacity::new(1, 10, 10)?,
        946_684_801,
    )
    .await?;
    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token.clone(),
                scim_user_resource(
                    fixture.organization_id,
                    "directory-expired-entitlement",
                    1,
                    "scim-user-target",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::LifecycleRejected)
    ));
    fixture.pool.close().await?;
    Ok(())
}

async fn replace_scim_entitlement(
    fixture: &ScimUserFixture,
    capacity: OrganizationEntitlementCapacity,
    valid_until: i64,
) -> Result<(), Box<dyn Error>> {
    let current = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(fixture.organization_id.get()))
        .one(fixture.pool.connection())
        .await?
        .expect("SCIM 测试授权必须存在");
    let snapshot = OrganizationEntitlementSnapshot::new(
        capacity,
        current.team_management_enabled,
        current.sso_enabled,
        current.custom_roles_enabled,
        current.audit_retention_days,
        current.valid_from.unix_timestamp(),
        valid_until,
        Some(valid_until),
    )?;
    let write = OrganizationEntitlementWrite::new(
        snapshot,
        "scim-test".to_owned(),
        None,
        Some(u64::try_from(current.version)?),
    )?;
    OrganizationRepository::new(fixture.pool.clone(), Duration::from_secs(5))?
        .replace_entitlement(fixture.organization_id, write)
        .await?;
    Ok(())
}

#[tokio::test]
async fn scim_user_sync_rechecks_token_status_and_version() -> Result<(), Box<dyn Error>> {
    let fixture = scim_user_fixture().await?;
    let token_repository =
        OrganizationScimTokenRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    token_repository
        .revoke(
            fixture.organization_id,
            fixture.token.id(),
            fixture.token.version(),
        )
        .await?;
    assert!(matches!(
        fixture
            .repository
            .synchronize(
                fixture.token,
                scim_user_resource(
                    fixture.organization_id,
                    "directory-revoked",
                    1,
                    "scim-user-target",
                    true,
                )?,
            )
            .await,
        Err(OrganizationScimUserRepositoryError::Restricted)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn scim_group_sync_projects_team_lifecycle_and_never_revives_removed_group()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("scim-group-test-group".to_owned()),
        display_name: Set("SCIM Group Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = insert_scim_user(&pool, group.id, "scim-group-owner").await?;
    let target = insert_scim_user(&pool, group.id, "scim-group-target").await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM Group Test Organization".to_owned(),
            "scim-group-test-org".to_owned(),
            UserId::new(owner.id)?,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let token_repository =
        OrganizationScimTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = token_repository
        .create(OrganizationScimTokenCreateRecord::new(
            organization.id(),
            "Group directory".to_owned(),
            OrganizationScimResourceSet::users_and_groups(),
        ))
        .await?;
    let (token, _) = issued.into_parts();
    let user_repository =
        OrganizationScimUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    user_repository
        .synchronize(
            token.clone(),
            scim_user_resource(
                organization.id(),
                "directory-group-user-1",
                1,
                "scim-group-target",
                true,
            )?,
        )
        .await?;
    let group_repository =
        OrganizationScimGroupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first = group_repository
        .synchronize(
            token.clone(),
            scim_group_resource(
                organization.id(),
                "directory-group-1",
                1,
                "Engineering",
                &["directory-group-user-1"],
            )?,
        )
        .await?;
    assert_eq!(first.outcome(), OrganizationScimGroupSyncOutcome::Applied);
    assert_eq!(first.record().status(), OrganizationScimGroupStatus::Active);
    let team_id = first.record().team_id();
    let membership = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization.id().get()))
        .filter(organization_memberships::Column::UserId.eq(target.id))
        .one(pool.connection())
        .await?
        .expect("SCIM 成员关系必须存在");
    assert_eq!(membership.team_id, Some(team_id.get()));

    let noop = group_repository
        .synchronize(
            token.clone(),
            scim_group_resource(
                organization.id(),
                "directory-group-1",
                1,
                "Changed but same version",
                &[],
            )?,
        )
        .await?;
    assert_eq!(noop.outcome(), OrganizationScimGroupSyncOutcome::Noop);
    assert!(matches!(
        group_repository
            .synchronize(
                token.clone(),
                scim_group_resource(
                    organization.id(),
                    "directory-group-1",
                    1,
                    "Engineering",
                    &[],
                )?,
            )
            .await,
        Err(OrganizationScimGroupRepositoryError::Conflict)
    ));

    let updated = group_repository
        .synchronize(
            token.clone(),
            scim_group_resource(
                organization.id(),
                "directory-group-1",
                2,
                "Engineering",
                &[],
            )?,
        )
        .await?;
    assert_eq!(updated.record().resource_version().get(), 2);
    let membership = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization.id().get()))
        .filter(organization_memberships::Column::UserId.eq(target.id))
        .one(pool.connection())
        .await?
        .expect("SCIM 成员关系必须保留");
    assert_eq!(membership.team_id, None);

    assert!(matches!(
        group_repository
            .delete(
                token.clone(),
                updated.record().resource_id().clone(),
                OrganizationScimResourceVersion::new(3)?,
            )
            .await,
        Err(OrganizationScimGroupRepositoryError::Conflict)
    ));

    let removed = group_repository
        .delete(
            token.clone(),
            updated.record().resource_id().clone(),
            updated.record().resource_version(),
        )
        .await?;
    assert_eq!(
        removed.record().status(),
        OrganizationScimGroupStatus::Removed
    );
    let team = organization_teams::Entity::find_by_id(team_id.get())
        .one(pool.connection())
        .await?
        .expect("SCIM 团队必须保留删除事实");
    assert_eq!(team.status, 2);
    assert!(team.deleted_at.is_some());
    let repeated = group_repository
        .delete(
            token.clone(),
            updated.record().resource_id().clone(),
            updated.record().resource_version(),
        )
        .await?;
    assert_eq!(repeated.outcome(), OrganizationScimGroupSyncOutcome::Noop);
    assert!(matches!(
        group_repository
            .synchronize(
                token,
                scim_group_resource(
                    organization.id(),
                    "directory-group-1",
                    3,
                    "Engineering",
                    &[],
                )?,
            )
            .await,
        Err(OrganizationScimGroupRepositoryError::LifecycleRejected)
    ));
    let binding = organization_scim_group_resources::Entity::find()
        .filter(
            organization_scim_group_resources::Column::OrganizationId.eq(organization.id().get()),
        )
        .one(pool.connection())
        .await?
        .expect("SCIM 组资源绑定必须存在");
    assert_eq!(binding.resource_version, 2);
    pool.close().await?;
    Ok(())
}

fn scim_group_resource(
    organization_id: OrganizationId,
    resource_id: &str,
    version: u64,
    display_name: &str,
    member_ids: &[&str],
) -> Result<OrganizationScimGroupResource, Box<dyn Error>> {
    Ok(OrganizationScimGroupResource::new(
        organization_id,
        OrganizationScimResourceId::new(resource_id.to_owned())?,
        OrganizationScimResourceVersion::new(version)?,
        display_name.to_owned(),
        member_ids
            .iter()
            .map(|value| OrganizationScimResourceId::new((*value).to_owned()))
            .collect::<Result<Vec<_>, _>>()?,
    )?)
}

fn active_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10)?,
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
        None,
    )?)
}
