use super::api_key::issued_from_test_entropy;
use crate::{ApiKeyGenerationError, ApiKeyParseError, IssuedApiKey, PresentedApiKey};

const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
const TEST_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const TEST_PREFIX: &str = "sk-af-AAECAwQFBgcI";

#[test]
fn fixed_entropy_derives_stable_storage_material() {
    let issued = issued_from_test_entropy(std::array::from_fn(|index| index as u8));

    assert_eq!(issued.key().expose_secret(), TEST_KEY);
    assert_eq!(issued.digest().as_str(), TEST_DIGEST);
    assert_eq!(issued.display_prefix().as_str(), TEST_PREFIX);
    assert_eq!(issued.digest().as_str().len(), 64);
    assert!(
        issued
            .digest()
            .as_str()
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert!(issued.display_prefix().as_str().len() <= 32);
}

#[test]
fn base64url_vector_uses_url_safe_alphabet_without_padding() {
    let mut entropy = [0xff; 32];
    entropy[0] = 0xfb;
    let issued = issued_from_test_entropy(entropy);

    assert_eq!(
        issued.key().expose_secret(),
        "sk-af--_________________________________________8"
    );
    assert_eq!(
        issued.digest().as_str(),
        "66138458d2b4902ccce18904be845751d1505acc854c11ea47af86431529509e"
    );
    assert!(!issued.key().expose_secret().contains(['+', '/', '=']));
}

#[test]
fn presented_keys_require_one_canonical_format_without_normalization() {
    assert_eq!(
        PresentedApiKey::parse(TEST_KEY).unwrap().expose_secret(),
        TEST_KEY
    );

    let mut non_canonical_tail = TEST_KEY.to_owned();
    non_canonical_tail.pop();
    non_canonical_tail.push('9');
    for invalid in [
        "",
        "sk-af-short",
        "SK-AF-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh+",
        "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh=",
        "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8 ",
        " sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        non_canonical_tail.as_str(),
    ] {
        assert_eq!(
            PresentedApiKey::parse(invalid).unwrap_err(),
            ApiKeyParseError::InvalidFormat
        );
    }
}

#[test]
fn key_types_and_errors_never_render_secret_material() {
    let issued = issued_from_test_entropy([0x5a; 32]);
    let secret = issued.key().expose_secret().to_owned();
    let digest = issued.digest().as_str().to_owned();
    let prefix = issued.display_prefix().as_str().to_owned();

    for rendered in [
        format!("{issued:?}"),
        format!("{:?}", issued.key()),
        format!("{:?}", issued.digest()),
        format!("{:?}", issued.display_prefix()),
        format!("{:?}", PresentedApiKey::parse(&secret).unwrap()),
        format!("{:?}", ApiKeyParseError::InvalidFormat),
        format!("{:?}", ApiKeyGenerationError::EntropyUnavailable),
    ] {
        assert!(!rendered.contains(&secret));
        assert!(!rendered.contains(&digest));
        assert!(!rendered.contains(&prefix));
    }
}

#[test]
fn operating_system_rng_can_issue_a_canonical_key() {
    let issued = IssuedApiKey::generate().unwrap();
    let reparsed = PresentedApiKey::parse(issued.key().expose_secret()).unwrap();

    assert_eq!(reparsed.digest(), *issued.digest());
    assert_eq!(reparsed.display_prefix(), *issued.display_prefix());
}
