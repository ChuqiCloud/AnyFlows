use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::PersonalAccessTokenError;

pub const PERSONAL_ACCESS_TOKEN_PREFIX: &str = "af_pat_";
const ENTROPY_BYTES: usize = 32;
const RANDOM_CHARS: usize = 43;

/// An owned, zeroizing secret; never logged, cloned or implicitly serialized.
pub struct PresentedPersonalAccessToken(Zeroizing<String>);

impl PresentedPersonalAccessToken {
    pub fn parse(value: &str) -> Result<Self, PersonalAccessTokenError> {
        if value.len() != PERSONAL_ACCESS_TOKEN_PREFIX.len() + RANDOM_CHARS
            || !value.starts_with(PERSONAL_ACCESS_TOKEN_PREFIX)
        {
            return Err(PersonalAccessTokenError::InvalidToken);
        }
        let random = &value[PERSONAL_ACCESS_TOKEN_PREFIX.len()..];
        let mut decoded = Zeroizing::new([0_u8; ENTROPY_BYTES]);
        let length = URL_SAFE_NO_PAD
            .decode_slice(random, &mut *decoded)
            .map_err(|_| PersonalAccessTokenError::InvalidToken)?;
        if length != ENTROPY_BYTES || URL_SAFE_NO_PAD.encode(*decoded) != random {
            return Err(PersonalAccessTokenError::InvalidToken);
        }
        Ok(Self(Zeroizing::new(value.to_owned())))
    }

    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn digest(&self) -> PersonalAccessTokenDigest {
        PersonalAccessTokenDigest(domain_hash(
            b"anyflows:personal-access-token:credential:v1\0",
            self.0.as_bytes(),
        ))
    }

    #[must_use]
    pub fn display_hint(&self) -> String {
        self.0[..PERSONAL_ACCESS_TOKEN_PREFIX.len() + 12].to_owned()
    }
}

impl fmt::Debug for PresentedPersonalAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PresentedPersonalAccessToken(<redacted>)")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PersonalAccessTokenDigest(String);

impl PersonalAccessTokenDigest {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PersonalAccessTokenDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PersonalAccessTokenDigest(<redacted>)")
    }
}

/// A secret that can only be serialized at its one-time issuance boundary.
pub struct PersonalAccessTokenSecret(PresentedPersonalAccessToken);

impl PersonalAccessTokenSecret {
    pub(super) fn generate() -> Result<Self, PersonalAccessTokenError> {
        let mut bytes = Zeroizing::new([0_u8; ENTROPY_BYTES]);
        getrandom::fill(&mut *bytes).map_err(|_| PersonalAccessTokenError::Internal)?;
        let mut text = Zeroizing::new(String::with_capacity(
            PERSONAL_ACCESS_TOKEN_PREFIX.len() + RANDOM_CHARS,
        ));
        text.push_str(PERSONAL_ACCESS_TOKEN_PREFIX);
        URL_SAFE_NO_PAD.encode_string(*bytes, &mut text);
        Ok(Self(PresentedPersonalAccessToken(text)))
    }

    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }

    pub(super) fn presented(&self) -> &PresentedPersonalAccessToken {
        &self.0
    }
}

impl fmt::Debug for PersonalAccessTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PersonalAccessTokenSecret(<redacted>)")
    }
}

impl Serialize for PersonalAccessTokenSecret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose_secret())
    }
}

#[must_use]
pub fn digest_personal_access_token_proof(secret: &str) -> String {
    domain_hash(
        b"anyflows:personal-access-token:proof:v1\0",
        secret.as_bytes(),
    )
}

fn domain_hash(domain: &[u8], value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}
