use crate::{
    OrganizationSsoDomain, OrganizationSsoDomainError, OrganizationSsoDomainStatus,
    OrganizationSsoEnforcementFacts, OrganizationSsoEnforcementPolicy,
    OrganizationSsoIdentifierError, OrganizationSsoProtocol, OrganizationSsoProviderKey,
    OrganizationSsoProviderStatus, OrganizationSsoStateCodeError,
};

#[test]
fn provider_key_is_nonzero_canonical_and_redacted() {
    let key = OrganizationSsoProviderKey::new([0x6d; 16]).unwrap();
    let persisted = key.persistence_key();

    assert_eq!(persisted, "6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d");
    assert_eq!(
        OrganizationSsoProviderKey::from_persistence_key(&persisted),
        Ok(key)
    );
    assert_eq!(format!("{key:?}"), "OrganizationSsoProviderKey(<redacted>)");
    assert!(!format!("{key:?}").contains(&persisted));
    assert_eq!(
        OrganizationSsoProviderKey::new([0; 16]),
        Err(OrganizationSsoIdentifierError::AllZero)
    );
}

#[test]
fn provider_key_rejects_noncanonical_persistence_text() {
    for invalid in [
        "",
        "6D6D6D6D6D6D6D6D6D6D6D6D6D6D6D6D",
        "00000000000000000000000000000000",
        "not-an-organization-sso-key",
    ] {
        assert!(OrganizationSsoProviderKey::from_persistence_key(invalid).is_err());
    }
}

#[test]
fn organization_sso_domain_accepts_canonical_ascii_names() {
    for domain in [
        "example.com",
        "login.example.com",
        "xn--fsqu00a.xn--0zwm56d",
        "sso-1.example.com",
    ] {
        let parsed = OrganizationSsoDomain::new(domain).unwrap();
        assert_eq!(parsed.as_str(), domain);
        assert_eq!(parsed.as_ref(), domain);
    }
}

#[test]
fn organization_sso_domain_rejects_noncanonical_or_unsafe_names() {
    let invalid_cases = [
        ("", OrganizationSsoDomainError::InvalidLength),
        ("localhost", OrganizationSsoDomainError::SingleLabel),
        ("127.0.0.1", OrganizationSsoDomainError::IpLiteral),
        ("::1", OrganizationSsoDomainError::IpLiteral),
        ("Example.com", OrganizationSsoDomainError::NonCanonical),
        ("例子.测试", OrganizationSsoDomainError::NonCanonical),
        (" example.com", OrganizationSsoDomainError::InvalidLabel),
        ("example.com ", OrganizationSsoDomainError::InvalidLabel),
        (".example.com", OrganizationSsoDomainError::InvalidLabel),
        ("example.com.", OrganizationSsoDomainError::InvalidLabel),
        ("example..com", OrganizationSsoDomainError::InvalidLabel),
        (
            "sso_test.example.com",
            OrganizationSsoDomainError::InvalidLabel,
        ),
        ("-sso.example.com", OrganizationSsoDomainError::InvalidLabel),
        ("sso-.example.com", OrganizationSsoDomainError::InvalidLabel),
    ];

    for (domain, expected) in invalid_cases {
        assert_eq!(OrganizationSsoDomain::new(domain), Err(expected));
    }

    let long_label = format!("{}.example.com", "a".repeat(64));
    assert_eq!(
        OrganizationSsoDomain::new(long_label),
        Err(OrganizationSsoDomainError::InvalidLabel)
    );
    let long_name = format!(
        "{}.{}.{}.com",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63)
    );
    let long_name = format!("{long_name}.{}", "d".repeat(63));
    assert_eq!(
        OrganizationSsoDomain::new(long_name),
        Err(OrganizationSsoDomainError::InvalidLength)
    );
}

#[test]
fn organization_sso_state_codes_are_stable_and_closed() {
    for (code, protocol) in [
        (1, OrganizationSsoProtocol::Oidc),
        (2, OrganizationSsoProtocol::Saml),
    ] {
        assert_eq!(protocol.code(), code);
        assert_eq!(OrganizationSsoProtocol::try_from(code), Ok(protocol));
    }
    for (code, status) in [
        (1, OrganizationSsoProviderStatus::Draft),
        (2, OrganizationSsoProviderStatus::Active),
        (3, OrganizationSsoProviderStatus::Disabled),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(OrganizationSsoProviderStatus::try_from(code), Ok(status));
    }
    for (code, status) in [
        (1, OrganizationSsoDomainStatus::Pending),
        (2, OrganizationSsoDomainStatus::Verified),
        (3, OrganizationSsoDomainStatus::Revoked),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(OrganizationSsoDomainStatus::try_from(code), Ok(status));
    }
    for (code, policy) in [
        (1, OrganizationSsoEnforcementPolicy::Optional),
        (2, OrganizationSsoEnforcementPolicy::Required),
    ] {
        assert_eq!(policy.code(), code);
        assert_eq!(OrganizationSsoEnforcementPolicy::try_from(code), Ok(policy));
    }

    assert_eq!(
        OrganizationSsoProtocol::try_from(0),
        Err(OrganizationSsoStateCodeError::InvalidProtocol)
    );
    assert_eq!(
        OrganizationSsoProviderStatus::try_from(4),
        Err(OrganizationSsoStateCodeError::InvalidProviderStatus)
    );
    assert_eq!(
        OrganizationSsoDomainStatus::try_from(-1),
        Err(OrganizationSsoStateCodeError::InvalidDomainStatus)
    );
    assert_eq!(
        OrganizationSsoEnforcementPolicy::try_from(3),
        Err(OrganizationSsoStateCodeError::InvalidEnforcementPolicy)
    );
}

#[test]
fn required_enforcement_facts_fail_closed_when_any_dependency_is_missing() {
    let complete = OrganizationSsoEnforcementFacts::new(true, true, true, true, true, true);
    assert!(complete.allows_required());

    for index in 0..6 {
        let mut facts = [true; 6];
        facts[index] = false;
        let facts = OrganizationSsoEnforcementFacts::new(
            facts[0], facts[1], facts[2], facts[3], facts[4], facts[5],
        );
        assert!(!facts.allows_required());
    }
    assert!(OrganizationSsoEnforcementPolicy::Required.is_required());
    assert!(!OrganizationSsoEnforcementPolicy::Optional.is_required());
}
