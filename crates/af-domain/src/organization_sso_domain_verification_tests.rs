use super::{
    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS, OrganizationSsoDnsReverification,
    OrganizationSsoDnsReverificationError, OrganizationSsoDnsTxtAnswers,
    OrganizationSsoDnsVerification, OrganizationSsoDnsVerificationError, OrganizationSsoDomain,
    OrganizationSsoDomainProof, OrganizationSsoDomainProofError, OrganizationSsoDomainProofToken,
    OrganizationSsoDomainStatus, OrganizationSsoDomainSuffixKind,
    classify_organization_sso_domain_suffix,
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde1";

fn proof(domain: &str) -> OrganizationSsoDomainProof {
    OrganizationSsoDomainProof::from_token(
        OrganizationSsoDomain::new(domain).unwrap(),
        OrganizationSsoDomainProofToken::new(TOKEN).unwrap(),
        1_000,
        3_600,
    )
    .unwrap()
}

#[test]
fn suffix_policy_rejects_public_and_private_suffixes() {
    assert_eq!(
        classify_organization_sso_domain_suffix(&OrganizationSsoDomain::new("co.uk").unwrap()),
        OrganizationSsoDomainSuffixKind::PublicSuffix
    );
    assert_eq!(
        classify_organization_sso_domain_suffix(&OrganizationSsoDomain::new("github.io").unwrap()),
        OrganizationSsoDomainSuffixKind::PrivateSuffix
    );
    assert_eq!(
        classify_organization_sso_domain_suffix(
            &OrganizationSsoDomain::new("engineering.github.io").unwrap()
        ),
        OrganizationSsoDomainSuffixKind::Registrable
    );
}

#[test]
fn proof_uses_fixed_record_name_and_redacted_debug() {
    let proof = proof("example.com");
    assert_eq!(proof.txt_record_name(), "_anyflows-sso.example.com");
    assert_eq!(proof.digest().len(), 64);
    let rendered = format!("{proof:?}");
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains(TOKEN));
}

#[test]
fn proof_digest_is_domain_separated() {
    assert_ne!(proof("example.com").digest(), proof("example.net").digest());
}

#[test]
fn verification_accepts_only_exact_server_txt_answer() {
    let proof = proof("example.com");
    let answers = OrganizationSsoDnsTxtAnswers::new(vec!["unrelated".to_owned(), TOKEN.to_owned()]);
    let result = OrganizationSsoDnsVerification::verify(
        &proof,
        OrganizationSsoDomainStatus::Pending,
        &answers,
        1_100,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )
    .unwrap();
    assert_eq!(result.verified_at(), 1_100);
    assert_eq!(
        result.recheck_at(),
        1_100 + DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS
    );

    let mismatch = OrganizationSsoDnsTxtAnswers::new(vec![format!("{TOKEN}extra")]);
    assert_eq!(
        OrganizationSsoDnsVerification::verify(
            &proof,
            OrganizationSsoDomainStatus::Pending,
            &mismatch,
            1_100,
            DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
        ),
        Err(OrganizationSsoDnsVerificationError::ProofMismatch)
    );
}

#[test]
fn verification_rejects_expired_and_revoked_proofs() {
    let proof = proof("example.com");
    let answers = OrganizationSsoDnsTxtAnswers::new(vec![TOKEN.to_owned()]);
    assert_eq!(
        OrganizationSsoDnsVerification::verify(
            &proof,
            OrganizationSsoDomainStatus::Pending,
            &answers,
            4_600,
            DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
        ),
        Err(OrganizationSsoDnsVerificationError::ProofExpired)
    );
    assert_eq!(
        OrganizationSsoDnsVerification::verify(
            &proof,
            OrganizationSsoDomainStatus::Revoked,
            &answers,
            1_100,
            DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
        ),
        Err(OrganizationSsoDnsVerificationError::Revoked)
    );
}

#[test]
fn proof_constructor_rejects_invalid_token_and_ttl() {
    assert_eq!(
        OrganizationSsoDomainProofToken::new("0".repeat(64)),
        Err(OrganizationSsoDomainProofError::InvalidToken)
    );
    assert_eq!(
        OrganizationSsoDomainProof::from_token(
            OrganizationSsoDomain::new("example.com").unwrap(),
            OrganizationSsoDomainProofToken::new(TOKEN).unwrap(),
            1_000,
            1,
        ),
        Err(OrganizationSsoDomainProofError::InvalidTtl)
    );
}

#[test]
fn reverification_accepts_expired_proof_when_txt_digest_still_matches() {
    let proof = proof("example.com");
    let answers = OrganizationSsoDnsTxtAnswers::new(vec![TOKEN.to_owned()]);
    let result = OrganizationSsoDnsReverification::verify(
        proof.domain(),
        OrganizationSsoDomainStatus::Verified,
        &proof.digest(),
        &answers,
        10_000,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )
    .unwrap();
    assert_eq!(
        result.recheck_at(),
        10_000 + DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS
    );
}

#[test]
fn reverification_rejects_mismatch_and_non_verified_status_without_leaking_material() {
    let proof = proof("example.com");
    let mismatch = OrganizationSsoDnsTxtAnswers::new(vec!["not-a-token".to_owned()]);
    assert_eq!(
        OrganizationSsoDnsReverification::verify(
            proof.domain(),
            OrganizationSsoDomainStatus::Verified,
            &proof.digest(),
            &mismatch,
            10_000,
            DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
        ),
        Err(OrganizationSsoDnsReverificationError::ProofMismatch)
    );
    assert_eq!(
        OrganizationSsoDnsReverification::verify(
            proof.domain(),
            OrganizationSsoDomainStatus::Revoked,
            &proof.digest(),
            &OrganizationSsoDnsTxtAnswers::new(vec![TOKEN.to_owned()]),
            10_000,
            DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
        ),
        Err(OrganizationSsoDnsReverificationError::InvalidStatus)
    );
    assert_eq!(
        OrganizationSsoDnsReverification::verify(
            proof.domain(),
            OrganizationSsoDomainStatus::Verified,
            &proof.digest().to_ascii_uppercase(),
            &OrganizationSsoDnsTxtAnswers::new(vec![TOKEN.to_owned()]),
            10_000,
            DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
        ),
        Err(OrganizationSsoDnsReverificationError::InvalidDigest)
    );
}
