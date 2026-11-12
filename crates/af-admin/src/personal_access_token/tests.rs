use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

use super::*;

#[test]
fn personal_access_token_secret_has_independent_canonical_format_and_redacted_debug() {
    let secret = PresentedPersonalAccessToken::generate().unwrap();
    let parsed = PresentedPersonalAccessToken::parse(secret.expose_secret()).unwrap();
    assert_eq!(secret.expose_secret().len(), 50);
    assert!(secret.expose_secret().starts_with("af_pat_"));
    assert_eq!(parsed.display_hint().len(), 19);
    assert!(crate::PresentedApiKey::parse(secret.expose_secret()).is_err());
    assert!(!format!("{parsed:?}").contains(secret.expose_secret()));
    assert!(!format!("{secret:?}").contains(secret.expose_secret()));
    let digest = parsed.digest();
    assert_eq!(digest.as_str().len(), 64);
    assert!(!format!("{digest:?}").contains(digest.as_str()));
    assert_ne!(
        digest.as_str(),
        format!("{:x}", Sha256::digest(secret.expose_secret().as_bytes()))
    );
    assert_ne!(
        digest.as_str(),
        digest_personal_access_token_proof(secret.expose_secret())
    );
    for invalid in ["", "af_pat_short", "sk-af-invalid", "Bearer af_pat_invalid"] {
        assert!(PresentedPersonalAccessToken::parse(invalid).is_err());
    }
    let zero = format!("af_pat_{}", URL_SAFE_NO_PAD.encode([0_u8; 32]));
    assert!(PresentedPersonalAccessToken::parse(&zero).is_ok());
    // Padding, non-canonical trailing bits and Unicode are all invalid.
    assert!(PresentedPersonalAccessToken::parse(&(zero.clone() + "=")).is_err());
    let mut noncanonical = zero;
    noncanonical.pop();
    noncanonical.push('B');
    assert!(PresentedPersonalAccessToken::parse(&noncanonical).is_err());
    assert!(PresentedPersonalAccessToken::parse(&format!("af_pat_{}", "界".repeat(43))).is_err());
}

#[test]
fn personal_access_token_proof_payload_is_canonical_and_bound_to_actual_mutation() {
    let scopes = vec!["usage:read".to_owned(), "account:read".to_owned()];
    let reversed = vec!["account:read".to_owned(), "usage:read".to_owned()];
    let ips = vec!["10.0.0.1/24".to_owned(), "192.168.0.1/32".to_owned()];
    let canonical_ips = vec!["192.168.0.1".to_owned(), "10.0.0.0/24".to_owned()];
    let original = personal_access_token_payload_digest(
        "Automation",
        &scopes,
        Some(1000),
        Some(&ips),
        true,
        Some(2),
    )
    .unwrap();
    assert_eq!(
        original,
        personal_access_token_payload_digest(
            "Automation",
            &reversed,
            Some(1000),
            Some(&canonical_ips),
            true,
            Some(2)
        )
        .unwrap()
    );
    for changed in [
        personal_access_token_payload_digest(
            "Renamed",
            &scopes,
            Some(1000),
            Some(&ips),
            true,
            Some(2),
        )
        .unwrap(),
        personal_access_token_payload_digest(
            "Automation",
            &scopes,
            None,
            Some(&ips),
            true,
            Some(2),
        )
        .unwrap(),
        personal_access_token_payload_digest(
            "Automation",
            &scopes,
            Some(1000),
            None,
            true,
            Some(2),
        )
        .unwrap(),
        personal_access_token_payload_digest(
            "Automation",
            &scopes,
            Some(1000),
            Some(&ips),
            false,
            Some(2),
        )
        .unwrap(),
        personal_access_token_payload_digest(
            "Automation",
            &scopes,
            Some(1000),
            Some(&ips),
            true,
            Some(3),
        )
        .unwrap(),
    ] {
        assert_ne!(original, changed);
    }
}
