use af_db::PersonalAccessTokenRepositoryError;

mod secret;
mod service;
#[cfg(test)]
mod tests;
mod types;

pub use secret::{
    PERSONAL_ACCESS_TOKEN_PREFIX, PersonalAccessTokenDigest, PersonalAccessTokenSecret,
    PresentedPersonalAccessToken, digest_personal_access_token_proof,
};
pub use service::{DatabasePersonalAccessTokenService, PersonalAccessTokenService};
pub use types::{
    CreatePersonalAccessTokenCommand, IssuedPersonalAccessToken, PersonalAccessToken,
    PersonalAccessTokenAccessRecord, PersonalAccessTokenAuditIdentity,
    PersonalAccessTokenAuthentication, PersonalAccessTokenError, PersonalAccessTokenFuture,
    PersonalAccessTokenLookup, PersonalAccessTokenMutationContext, PersonalAccessTokenPage,
    PersonalAccessTokenPolicy, PersonalAccessTokenProofClaim, PersonalAccessTokenRejectionReason,
    UpdatePersonalAccessTokenCommand,
};

impl PresentedPersonalAccessToken {
    pub fn generate() -> Result<PersonalAccessTokenSecret, PersonalAccessTokenError> {
        PersonalAccessTokenSecret::generate()
    }
}

impl PersonalAccessTokenDigest {
    #[must_use]
    pub fn from_secret(secret: &PresentedPersonalAccessToken) -> Self {
        secret.digest()
    }
}

pub fn personal_access_token_payload_digest(
    name: &str,
    scopes: &[String],
    expiry: Option<i64>,
    ips: Option<&[String]>,
    enabled: bool,
    version: Option<i64>,
) -> Result<String, PersonalAccessTokenError> {
    af_db::personal_access_token_payload_digest(name, scopes, expiry, ips, enabled, version)
        .map_err(map_error)
}

#[derive(serde::Serialize)]
pub struct IssuedStepUpProof {
    pub secret: PersonalAccessTokenProofSecret,
    pub expires_at: i64,
}

pub struct PersonalAccessTokenProofSecret(zeroize::Zeroizing<String>);

impl PersonalAccessTokenProofSecret {
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for PersonalAccessTokenProofSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalAccessTokenProofSecret(<redacted>)")
    }
}

impl serde::Serialize for PersonalAccessTokenProofSecret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose_secret())
    }
}

pub struct PersonalAccessTokenStepUpEvidence {
    pub session_version: i64,
    pub session_binding: String,
    pub issued_at: i64,
}

pub(crate) fn map_error(error: PersonalAccessTokenRepositoryError) -> PersonalAccessTokenError {
    match error {
        PersonalAccessTokenRepositoryError::InvalidInput => PersonalAccessTokenError::InvalidInput,
        PersonalAccessTokenRepositoryError::OwnerUnavailable => PersonalAccessTokenError::Forbidden,
        PersonalAccessTokenRepositoryError::IssuanceDisabled => {
            PersonalAccessTokenError::IssuanceDisabled
        }
        PersonalAccessTokenRepositoryError::Disabled => PersonalAccessTokenError::Disabled,
        PersonalAccessTokenRepositoryError::LimitReached => PersonalAccessTokenError::LimitReached,
        PersonalAccessTokenRepositoryError::NotFound => PersonalAccessTokenError::NotFound,
        PersonalAccessTokenRepositoryError::Conflict => PersonalAccessTokenError::Conflict,
        PersonalAccessTokenRepositoryError::Forbidden => PersonalAccessTokenError::Forbidden,
        PersonalAccessTokenRepositoryError::InvalidProof => PersonalAccessTokenError::InvalidProof,
        PersonalAccessTokenRepositoryError::RateLimited { retry_after } => {
            PersonalAccessTokenError::RateLimited { retry_after }
        }
        PersonalAccessTokenRepositoryError::Query
        | PersonalAccessTokenRepositoryError::Timeout
        | PersonalAccessTokenRepositoryError::Invariant => PersonalAccessTokenError::Internal,
    }
}

fn validate_proof_evidence(
    evidence: &PersonalAccessTokenStepUpEvidence,
) -> Result<i64, PersonalAccessTokenError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if evidence.session_version <= 0
        || evidence.session_binding.len() != 64
        || !evidence
            .session_binding
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || evidence.issued_at < now.saturating_sub(300)
        || evidence.issued_at > now + 30
    {
        return Err(PersonalAccessTokenError::InvalidProof);
    }
    Ok(now)
}
