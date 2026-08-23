use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationMembershipStatus,
    OrganizationPermission, OrganizationRole, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, sea_query::Expr};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationCustomRoleBindingRepository,
    OrganizationCustomRoleBindingRepositoryError, OrganizationCustomRoleCreate,
    OrganizationCustomRoleRepository, OrganizationEntitlementWrite, OrganizationMembershipChange,
    OrganizationMembershipCreate, OrganizationRepository,
    entity::{groups, organization_memberships, users},
    organization_custom_role_binding::active_custom_role_permissions,
};

struct Fixture {
    pool: crate::DatabasePool,
    organizations: OrganizationRepository,
    roles: OrganizationCustomRoleRepository,
    bindings: OrganizationCustomRoleBindingRepository,
    users: [UserId; 3],
}

#[tokio::test]
async fn binding_repository_enforces_scope_member_status_and_cas() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let (first, first_owner) = fixture
        .organizations
        .create(OrganizationCreate::new(
            "Binding Company".to_owned(),
            "binding-company".to_owned(),
            fixture.users[0],
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let (second, _) = fixture
        .organizations
        .create(OrganizationCreate::new(
            "Other Binding Company".to_owned(),
            "other-binding-company".to_owned(),
            fixture.users[1],
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let member = fixture
        .organizations
        .add_membership(
            first.id(),
            OrganizationMembershipCreate::new(
                fixture.users[2],
                OrganizationRole::Member,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;
    let role_change_member = fixture
        .organizations
        .add_membership(
            first.id(),
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Member,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;
    let active_role = fixture
        .roles
        .create_role(
            first.id(),
            OrganizationCustomRoleCreate::new(
                "Operator".to_owned(),
                None,
                vec![OrganizationPermission::ProfileWrite],
                fixture.users[0],
            ),
        )
        .await?;
    let cross_scope_role = fixture
        .roles
        .create_role(
            second.id(),
            OrganizationCustomRoleCreate::new(
                "Other Operator".to_owned(),
                None,
                vec![],
                fixture.users[1],
            ),
        )
        .await?;

    // 迁移约束必须独立于仓储阻止跨企业和非 Member 绑定。
    for (membership_id, role_id) in [
        (member.id().get(), cross_scope_role.id().get()),
        (first_owner.id().get(), active_role.id().get()),
    ] {
        let result = organization_memberships::Entity::update_many()
            .filter(organization_memberships::Column::Id.eq(membership_id))
            .col_expr(
                organization_memberships::Column::CustomRoleId,
                Expr::value(Some(role_id)),
            )
            .exec(fixture.pool.connection())
            .await;
        assert!(result.is_err());
    }

    assert_eq!(
        fixture
            .bindings
            .set_binding(
                first.id(),
                member.id(),
                Some(cross_scope_role.id()),
                member.version(),
            )
            .await,
        Err(OrganizationCustomRoleBindingRepositoryError::RoleUnavailable)
    );
    assert_eq!(
        fixture
            .bindings
            .set_binding(
                first.id(),
                first_owner.id(),
                Some(active_role.id()),
                first_owner.version(),
            )
            .await,
        Err(OrganizationCustomRoleBindingRepositoryError::MembershipUnavailable)
    );

    let bound = fixture
        .bindings
        .set_binding(
            first.id(),
            member.id(),
            Some(active_role.id()),
            member.version(),
        )
        .await?;
    assert_eq!(bound.custom_role_id(), Some(active_role.id()));
    assert_eq!(bound.membership_version(), member.version() + 1);
    let unchanged = fixture
        .bindings
        .set_binding(
            first.id(),
            member.id(),
            Some(active_role.id()),
            bound.membership_version(),
        )
        .await?;
    assert_eq!(unchanged.membership_version(), bound.membership_version());
    assert_eq!(
        fixture
            .bindings
            .set_binding(first.id(), member.id(), None, member.version())
            .await,
        Err(OrganizationCustomRoleBindingRepositoryError::Conflict)
    );

    let membership_model = organization_memberships::Entity::find_by_id(member.id().get())
        .one(fixture.pool.connection())
        .await?
        .expect("成员绑定事实必须存在");
    assert_eq!(
        active_custom_role_permissions(fixture.pool.connection(), &membership_model, true).await?,
        Some(vec![OrganizationPermission::ProfileWrite])
    );
    assert_eq!(
        active_custom_role_permissions(fixture.pool.connection(), &membership_model, false).await?,
        None
    );

    let disabled_role = fixture
        .roles
        .disable_role(first.id(), active_role.role_key(), active_role.version())
        .await?;
    assert_eq!(
        active_custom_role_permissions(fixture.pool.connection(), &membership_model, true).await?,
        None
    );
    assert_eq!(
        fixture
            .bindings
            .set_binding(
                first.id(),
                member.id(),
                Some(disabled_role.id()),
                bound.membership_version(),
            )
            .await,
        Err(OrganizationCustomRoleBindingRepositoryError::RoleUnavailable)
    );

    let cleared = fixture
        .bindings
        .set_binding(first.id(), member.id(), None, bound.membership_version())
        .await?;
    assert_eq!(cleared.custom_role_id(), None);
    let replacement = fixture
        .roles
        .create_role(
            first.id(),
            OrganizationCustomRoleCreate::new(
                "Auditor".to_owned(),
                None,
                vec![OrganizationPermission::AuditRead],
                fixture.users[0],
            ),
        )
        .await?;
    let role_change_binding = fixture
        .bindings
        .set_binding(
            first.id(),
            role_change_member.id(),
            Some(replacement.id()),
            role_change_member.version(),
        )
        .await?;
    let viewer = fixture
        .organizations
        .change_membership(
            first.id(),
            role_change_member.id(),
            OrganizationMembershipChange::new(
                OrganizationRole::Viewer,
                OrganizationMembershipStatus::Active,
                None,
                role_change_binding.membership_version(),
            ),
        )
        .await?;
    assert_eq!(viewer.custom_role_id(), None);
    let rebound = fixture
        .bindings
        .set_binding(
            first.id(),
            member.id(),
            Some(replacement.id()),
            cleared.membership_version(),
        )
        .await?;
    let suspended = fixture
        .organizations
        .change_membership(
            first.id(),
            member.id(),
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Suspended,
                None,
                rebound.membership_version(),
            ),
        )
        .await?;
    assert_eq!(suspended.custom_role_id(), Some(replacement.id()));
    assert_eq!(
        fixture
            .bindings
            .set_binding(first.id(), member.id(), None, suspended.version())
            .await,
        Err(OrganizationCustomRoleBindingRepositoryError::MembershipUnavailable)
    );
    let removed = fixture
        .organizations
        .change_membership(
            first.id(),
            member.id(),
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Removed,
                None,
                suspended.version(),
            ),
        )
        .await?;
    assert_eq!(removed.custom_role_id(), Some(replacement.id()));
    assert_eq!(
        fixture
            .bindings
            .set_binding(first.id(), member.id(), None, removed.version())
            .await,
        Err(OrganizationCustomRoleBindingRepositoryError::MembershipUnavailable)
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
        name: Set("custom-role-binding-test-group".to_owned()),
        display_name: Set("自定义角色绑定测试分组".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("custom-role-binding-user-{index}")),
            email: Set(Some(format!(
                "custom-role-binding-user-{index}@example.com"
            ))),
            password_hash: Set(None),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("custom-role-binding-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let roles = OrganizationCustomRoleRepository::new(pool.clone(), Duration::from_secs(5))?;
    let bindings =
        OrganizationCustomRoleBindingRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok(Fixture {
        pool,
        organizations,
        roles,
        bindings,
        users: user_ids.try_into().expect("测试用户数量固定为三"),
    })
}
