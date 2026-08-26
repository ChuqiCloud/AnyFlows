use super::*;

#[test]
fn scim_token_is_high_entropy_and_redacted() {
    let token = OrganizationScimToken::generate().expect("系统随机源应可用");
    assert_eq!(token.as_str().len(), 69);
    assert!(token.as_str().starts_with("scim_"));
    assert_eq!(token.prefix(), "scim_");
    assert_eq!(format!("{token:?}"), "OrganizationScimToken(<redacted>)");
    assert_eq!(token.digest().as_str().len(), 64);
    assert_eq!(
        format!("{:?}", token.digest()),
        "OrganizationScimTokenDigest(<redacted>)"
    );
    let key = OrganizationScimTokenKey::generate().expect("系统随机源应可用");
    assert_eq!(key.as_str().len(), 32);
    assert_eq!(format!("{key:?}"), "OrganizationScimTokenKey(<redacted>)");
}

#[test]
fn scim_token_rejects_noncanonical_material() {
    assert!(
        OrganizationScimToken::new(
            "scim_0000000000000000000000000000000000000000000000000000000000000000"
        )
        .is_err()
    );
    assert!(
        OrganizationScimToken::new(
            "scim_ABCDEF00000000000000000000000000000000000000000000000000000000"
        )
        .is_err()
    );
    assert!(
        OrganizationScimToken::new(
            "token_1111111111111111111111111111111111111111111111111111111111111111"
        )
        .is_err()
    );
    assert!(OrganizationScimTokenDigest::from_persistence_key("A".repeat(64)).is_err());
}

#[test]
fn scim_resource_scope_is_closed() {
    let users = OrganizationScimResourceSet::users();
    assert!(users.contains(OrganizationScimResourceKind::Users));
    assert!(!users.contains(OrganizationScimResourceKind::Groups));
    let both = OrganizationScimResourceSet::users_and_groups();
    assert!(both.contains(OrganizationScimResourceKind::Users));
    assert!(both.contains(OrganizationScimResourceKind::Groups));
    assert!(OrganizationScimResourceSet::from_code(0).is_err());
    assert!(OrganizationScimResourceSet::from_code(4).is_err());
    assert_eq!(
        OrganizationScimTokenStatus::from_code(1).unwrap(),
        OrganizationScimTokenStatus::Active
    );
    assert!(OrganizationScimTokenStatus::from_code(3).is_err());
}
