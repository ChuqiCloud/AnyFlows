use super::*;

#[test]
fn service_account_public_key_is_canonical_and_redacted() {
    let key = OrganizationServiceAccountKey::new([0x11; 16]).unwrap();
    assert_eq!(key.persistence_key(), "11".repeat(16));
    assert_eq!(
        OrganizationServiceAccountKey::from_persistence_key(&key.persistence_key()).unwrap(),
        key
    );
    assert_eq!(
        format!("{key:?}"),
        "OrganizationServiceAccountKey(<redacted>)"
    );
    assert!(OrganizationServiceAccountKey::new([0; 16]).is_err());
    assert!(OrganizationServiceAccountKey::from_persistence_key(&"A".repeat(32)).is_err());
}

#[test]
fn service_account_name_is_bounded_and_debug_redacted() {
    let name = OrganizationServiceAccountName::new("billing-worker").unwrap();
    assert_eq!(name.as_str(), "billing-worker");
    assert_eq!(
        format!("{name:?}"),
        "OrganizationServiceAccountName(<redacted>)"
    );
    for invalid in ["", " billing", "billing ", "billing\nworker"] {
        assert!(OrganizationServiceAccountName::new(invalid).is_err());
    }
    assert!(OrganizationServiceAccountName::new("x".repeat(129)).is_err());
}

#[test]
fn service_account_locator_keeps_enterprise_boundary() {
    let organization_id = OrganizationId::new(7).unwrap();
    let other_organization_id = OrganizationId::new(8).unwrap();
    let service_account_id = OrganizationServiceAccountId::new(12).unwrap();
    let locator = OrganizationServiceAccountLocator::new(organization_id, service_account_id);
    assert!(locator.belongs_to(organization_id));
    assert!(!locator.belongs_to(other_organization_id));

    let account = OrganizationServiceAccount::new(
        locator,
        OrganizationServiceAccountKey::new([1; 16]).unwrap(),
        OrganizationServiceAccountName::new("automation").unwrap(),
    );
    assert_eq!(account.locator(), locator);
    assert_eq!(account.status(), OrganizationServiceAccountStatus::Active);
    assert!(account.status().is_usable());
    assert!(!format!("{account:?}").contains("automation"));
}

#[test]
fn service_account_lifecycle_is_closed_and_revocation_is_terminal() {
    use OrganizationServiceAccountLifecycleDisposition as D;
    use OrganizationServiceAccountStatus as S;

    assert_eq!(S::from_code(1).unwrap(), S::Active);
    assert_eq!(S::from_code(3).unwrap(), S::Revoked);
    assert!(S::from_code(4).is_err());
    assert_eq!(
        classify_organization_service_account_lifecycle(S::Active, S::Suspended),
        D::Suspend
    );
    assert_eq!(
        classify_organization_service_account_lifecycle(S::Suspended, S::Active),
        D::Resume
    );
    assert_eq!(
        classify_organization_service_account_lifecycle(S::Active, S::Revoked),
        D::Revoke
    );
    assert_eq!(
        classify_organization_service_account_lifecycle(S::Revoked, S::Revoked),
        D::Noop
    );
    assert_eq!(
        classify_organization_service_account_lifecycle(S::Revoked, S::Active),
        D::Reject(OrganizationServiceAccountLifecycleError::RevokedImmutable)
    );
}
