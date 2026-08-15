use super::organization::{
    OrganizationIdentifierError, OrganizationKey, OrganizationMembershipStatus,
    OrganizationPermission, OrganizationRole, OrganizationStateCodeError, OrganizationStatus,
    OrganizationTeamStatus, PlatformPermission,
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
fn organization_state_codes_are_closed() {
    assert_eq!(
        OrganizationStatus::try_from(1),
        Ok(OrganizationStatus::Active)
    );
    assert_eq!(OrganizationRole::try_from(1), Ok(OrganizationRole::Owner));
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
    assert!(!OrganizationRole::Billing.allows(OrganizationPermission::KeysManageSelf));
    assert!(OrganizationRole::Member.allows(OrganizationPermission::KeysManageSelf));
    assert!(!OrganizationRole::Member.allows(OrganizationPermission::AuditRead));
    assert!(OrganizationRole::Viewer.allows(OrganizationPermission::AuditRead));
    assert_eq!(
        OrganizationPermission::MembersManage.code(),
        "organization.members.manage"
    );
    assert_eq!(
        PlatformPermission::OrganizationsReadAll.code(),
        "platform.organizations.read_all"
    );
}
