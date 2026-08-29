use super::organization::{
    OrganizationCustomRoleKey, OrganizationCustomRoleStatus, OrganizationIdentifierError,
    OrganizationKey, OrganizationMembershipStatus, OrganizationPermission,
    OrganizationPermissionCodeError, OrganizationRole, OrganizationStateCodeError,
    OrganizationStatus, OrganizationTeamStatus, PlatformPermission,
};

#[test]
fn organization_key_round_trips_without_exposing_database_order() {
    let key = OrganizationKey::new([0x4a; 16]).unwrap();
    let persisted = key.persistence_key();

    assert_eq!(persisted, "4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a");
    assert_eq!(OrganizationKey::from_persistence_key(&persisted), Ok(key));
    assert_eq!(format!("{key:?}"), "OrganizationKey(<redacted>)");
}

#[test]
fn organization_key_rejects_zero_and_noncanonical_text() {
    assert_eq!(
        OrganizationKey::new([0; 16]),
        Err(OrganizationIdentifierError::AllZero)
    );
    for invalid in [
        "",
        "4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A",
        "00000000000000000000000000000000",
        "not-an-organization-key",
    ] {
        assert!(OrganizationKey::from_persistence_key(invalid).is_err());
    }
}

#[test]
fn organization_custom_role_key_is_nonzero_and_canonical() {
    let key = OrganizationCustomRoleKey::new([0x5c; 16]).unwrap();
    let persisted = key.persistence_key();

    assert_eq!(persisted, "5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c");
    assert_eq!(
        OrganizationCustomRoleKey::from_persistence_key(&persisted),
        Ok(key)
    );
    assert_eq!(format!("{key:?}"), "OrganizationCustomRoleKey(<redacted>)");
    for invalid in [
        "",
        "5C5C5C5C5C5C5C5C5C5C5C5C5C5C5C5C",
        "00000000000000000000000000000000",
        "not-a-custom-role-key",
    ] {
        assert!(OrganizationCustomRoleKey::from_persistence_key(invalid).is_err());
    }
}

#[test]
fn organization_state_codes_are_closed() {
    assert_eq!(
        OrganizationStatus::try_from(1),
        Ok(OrganizationStatus::Active)
    );
    assert_eq!(OrganizationRole::try_from(1), Ok(OrganizationRole::Owner));
    assert_eq!(
        OrganizationCustomRoleStatus::try_from(2),
        Ok(OrganizationCustomRoleStatus::Disabled)
    );
    assert_eq!(
        OrganizationMembershipStatus::try_from(3),
        Ok(OrganizationMembershipStatus::Removed)
    );
    assert_eq!(
        OrganizationTeamStatus::try_from(2),
        Ok(OrganizationTeamStatus::Disabled)
    );
    assert_eq!(
        OrganizationRole::try_from(0),
        Err(OrganizationStateCodeError::InvalidRole)
    );
}

#[test]
fn organization_roles_use_the_fixed_permission_matrix() {
    assert!(OrganizationRole::Owner.allows(OrganizationPermission::OwnershipTransfer));
    assert!(OrganizationRole::Admin.allows(OrganizationPermission::MembersManage));
    assert!(!OrganizationRole::Admin.allows(OrganizationPermission::OwnershipTransfer));
    assert!(OrganizationRole::Billing.allows(OrganizationPermission::WalletManage));
    assert!(OrganizationRole::Billing.allows(OrganizationPermission::BudgetsManage));
    assert!(!OrganizationRole::Billing.allows(OrganizationPermission::KeysManageSelf));
    assert!(OrganizationRole::Member.allows(OrganizationPermission::KeysManageSelf));
    assert!(!OrganizationRole::Member.allows(OrganizationPermission::AuditRead));
    assert!(OrganizationRole::Viewer.allows(OrganizationPermission::AuditRead));
    assert!(OrganizationRole::Owner.allows(OrganizationPermission::RolesAssign));
    assert!(OrganizationRole::Admin.allows(OrganizationPermission::RolesManage));
    assert!(OrganizationRole::Owner.allows(OrganizationPermission::ServiceAccountsRead));
    assert!(OrganizationRole::Owner.allows(OrganizationPermission::ServiceAccountsManage));
    assert!(OrganizationRole::Admin.allows(OrganizationPermission::ServiceAccountsRead));
    assert!(OrganizationRole::Admin.allows(OrganizationPermission::ServiceAccountsManage));
    assert!(!OrganizationRole::Billing.allows(OrganizationPermission::ServiceAccountsRead));
    assert!(!OrganizationRole::Billing.allows(OrganizationPermission::RolesRead));
    assert_eq!(
        OrganizationPermission::MembersManage.code(),
        "organization.members.manage"
    );
    assert_eq!(
        OrganizationPermission::BudgetsManage.code(),
        "organization.budgets.manage"
    );
    assert_eq!(
        PlatformPermission::DashboardReadAll.code(),
        "platform.dashboard.read_all"
    );
    assert_eq!(
        PlatformPermission::OrganizationsReadAll.code(),
        "platform.organizations.read_all"
    );
    assert_eq!(
        PlatformPermission::OrganizationUsageLogsReadAll.code(),
        "platform.organization_usage_logs.read_all"
    );
    assert_eq!(
        PlatformPermission::OrganizationAuditReadAll.code(),
        "platform.organization_audit.read_all"
    );
}

#[test]
fn custom_role_permissions_use_a_closed_server_allowlist() {
    for permission in OrganizationPermission::custom_role_grantable_permissions() {
        assert!(permission.is_custom_role_grantable());
        assert_eq!(
            OrganizationPermission::from_code(permission.code()),
            Ok(*permission)
        );
    }
    for reserved in [
        OrganizationPermission::OwnershipTransfer,
        OrganizationPermission::Close,
        OrganizationPermission::RolesManage,
        OrganizationPermission::RolesAssign,
        OrganizationPermission::DepartmentsManage,
        OrganizationPermission::ServiceAccountsRead,
        OrganizationPermission::ServiceAccountsManage,
    ] {
        assert!(!reserved.is_custom_role_grantable());
        assert_eq!(
            OrganizationPermission::from_code(reserved.code()),
            Ok(reserved)
        );
    }
    assert_eq!(
        OrganizationPermission::from_code("platform.organizations.read_all"),
        Err(OrganizationPermissionCodeError::Unknown)
    );
    assert_eq!(
        OrganizationPermission::from_code("organization.unknown.read"),
        Err(OrganizationPermissionCodeError::Unknown)
    );
}
