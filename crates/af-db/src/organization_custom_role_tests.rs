use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationCustomRoleStatus, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationPermission, UserId,
};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationCustomRoleCreate,
    OrganizationCustomRoleRepository, OrganizationCustomRoleRepositoryError,
    OrganizationCustomRoleUpdate, OrganizationEntitlementWrite, OrganizationRepository,
    entity::{
        groups, organization_custom_role_permissions, organization_custom_roles, organizations,
        users,
    },
};

struct Fixture {
    pool: crate::DatabasePool,
    organizations: OrganizationRepository,
    roles: OrganizationCustomRoleRepository,
    users: [UserId; 3],
}

#[tokio::test]
async fn custom_role_repository_preserves_cas_permissions_and_disable_facts()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (organization, _) = fixture
        .organizations
        .create(OrganizationCreate::new(
            "Role Lab".to_owned(),
            "role-lab".to_owned(),
            fixture.users[0],
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let created = fixture
        .roles
        .create_role(
            organization.id(),
            OrganizationCustomRoleCreate::new(
                "Support".to_owned(),
                Some("Customer operations".to_owned()),
                vec![
                    OrganizationPermission::RolesRead,
                    OrganizationPermission::OverviewRead,
                ],
                fixture.users[0],
            ),
        )
        .await?;

    assert_eq!(created.status(), OrganizationCustomRoleStatus::Active);
    assert_eq!(created.version(), 1);
    assert_eq!(created.role_key().persistence_key().len(), 32);
    assert_eq!(
        created.permissions(),
        &[
            OrganizationPermission::OverviewRead,
            OrganizationPermission::RolesRead,
        ]
    );
    assert_eq!(
        fixture
            .roles
            .role(organization.id(), created.role_key())
            .await?,
        created
    );
    let debug = format!("{created:?}");
    assert!(!debug.contains("Support"));
    assert!(!debug.contains("Customer operations"));

    let updated = fixture
        .roles
        .update_role(
            organization.id(),
            created.role_key(),
            OrganizationCustomRoleUpdate::new(
                "Support Advanced".to_owned(),
                None,
                vec![
                    OrganizationPermission::AuditRead,
                    OrganizationPermission::UsageLogsReadSelf,
                ],
                created.version(),
            ),
        )
        .await?;
    assert_eq!(updated.version(), 2);
    assert_eq!(updated.description(), None);
    assert_eq!(
        fixture
            .roles
            .update_role(
                organization.id(),
                created.role_key(),
                OrganizationCustomRoleUpdate::new(
                    "Stale".to_owned(),
                    None,
                    vec![],
                    created.version(),
                ),
            )
            .await,
        Err(OrganizationCustomRoleRepositoryError::Conflict)
    );

    let disabled = fixture
        .roles
        .disable_role(organization.id(), created.role_key(), updated.version())
        .await?;
    assert_eq!(disabled.status(), OrganizationCustomRoleStatus::Disabled);
    assert_eq!(disabled.version(), 3);
    assert_eq!(disabled.permissions(), updated.permissions());
    assert_eq!(
        fixture
            .roles
            .update_role(
                organization.id(),
                created.role_key(),
                OrganizationCustomRoleUpdate::new(
                    "Cannot Restore".to_owned(),
                    None,
                    vec![],
                    disabled.version(),
                ),
            )
            .await,
        Err(OrganizationCustomRoleRepositoryError::InvalidTransition)
    );
    assert_eq!(
        fixture
            .roles
            .disable_role(organization.id(), created.role_key(), disabled.version())
            .await,
        Err(OrganizationCustomRoleRepositoryError::InvalidTransition)
    );

    assert_eq!(
        organization_custom_role_permissions::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        2
    );
    assert!(
        organization_custom_roles::Entity::delete_by_id(created.id().get())
            .exec(fixture.pool.connection())
            .await
            .is_err(),
        "权限事实必须以 RESTRICT 阻止角色定义被物理删除"
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn custom_role_repository_closes_scope_and_permission_boundaries()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (first, _) = fixture
        .organizations
        .create(OrganizationCreate::new(
            "First Role Company".to_owned(),
            "first-role-company".to_owned(),
            fixture.users[0],
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let (second, _) = fixture
        .organizations
        .create(OrganizationCreate::new(
            "Second Role Company".to_owned(),
            "second-role-company".to_owned(),
            fixture.users[1],
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let first_role = fixture
        .roles
        .create_role(
            first.id(),
            OrganizationCustomRoleCreate::new(
                "Analyst".to_owned(),
                None,
                vec![OrganizationPermission::UsageLogsReadAll],
                fixture.users[0],
            ),
        )
        .await?;

    assert_eq!(
        fixture.roles.role(second.id(), first_role.role_key()).await,
        Err(OrganizationCustomRoleRepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .roles
            .create_role(
                first.id(),
                OrganizationCustomRoleCreate::new(
                    "Reserved".to_owned(),
                    None,
                    vec![OrganizationPermission::RolesAssign],
                    fixture.users[0],
                ),
            )
            .await,
        Err(OrganizationCustomRoleRepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .roles
            .create_role(
                first.id(),
                OrganizationCustomRoleCreate::new(
                    "Duplicate Permissions".to_owned(),
                    None,
                    vec![
                        OrganizationPermission::OverviewRead,
                        OrganizationPermission::OverviewRead,
                    ],
                    fixture.users[0],
                ),
            )
            .await,
        Err(OrganizationCustomRoleRepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .roles
            .create_role(
                first.id(),
                OrganizationCustomRoleCreate::new(
                    "Analyst".to_owned(),
                    None,
                    vec![],
                    fixture.users[0],
                ),
            )
            .await,
        Err(OrganizationCustomRoleRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .roles
            .create_role(
                first.id(),
                OrganizationCustomRoleCreate::new(
                    "Cross Member".to_owned(),
                    None,
                    vec![],
                    fixture.users[1],
                ),
            )
            .await,
        Err(OrganizationCustomRoleRepositoryError::CreatorUnavailable)
    );

    let second_role = fixture
        .roles
        .create_role(
            second.id(),
            OrganizationCustomRoleCreate::new("Analyst".to_owned(), None, vec![], fixture.users[1]),
        )
        .await?;
    assert_eq!(second_role.name(), first_role.name());

    for index in 0..2 {
        fixture
            .roles
            .create_role(
                first.id(),
                OrganizationCustomRoleCreate::new(
                    format!("Paged {index}"),
                    None,
                    vec![],
                    fixture.users[0],
                ),
            )
            .await?;
    }
    let first_page = fixture.roles.list_roles(first.id(), None, 2).await?;
    assert_eq!(first_page.items().len(), 2);
    let cursor = first_page.next_cursor().expect("目录必须存在下一页");
    let second_page = fixture
        .roles
        .list_roles(first.id(), Some(cursor), 2)
        .await?;
    assert_eq!(second_page.items().len(), 1);
    assert_eq!(second_page.next_cursor(), None);
    assert!(first_page.items().iter().all(|role| {
        second_page
            .items()
            .iter()
            .all(|next| next.id() != role.id())
    }));

    assert!(
        organizations::Entity::delete_by_id(first.id().get())
            .exec(fixture.pool.connection())
            .await
            .is_err(),
        "角色定义必须以 RESTRICT 阻止企业被物理删除"
    );
    fixture.pool.close().await?;
    Ok(())
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

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("custom-role-test-group".to_owned()),
        display_name: Set("自定义角色测试分组".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("custom-role-user-{index}")),
            email: Set(Some(format!("custom-role-user-{index}@example.com"))),
            password_hash: Set(None),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("custom-role-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let roles = OrganizationCustomRoleRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok(Fixture {
        pool,
        organizations,
        roles,
        users: user_ids.try_into().expect("测试用户数量固定为三"),
    })
}
