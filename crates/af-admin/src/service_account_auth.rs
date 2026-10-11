use std::{fmt, future::Future, pin::Pin};

use af_domain::{
    GatewayIdentity, OrganizationDepartmentId, OrganizationServiceAccountRuntimeIdentity,
    OrganizationTeamId,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

const PREFIX: &str = "sa_";
// Retain the existing digest domain so previously issued keys remain valid.
const DIGEST_CONTEXT: &[u8] = b"anyflows-enterprise-service-account-secret-v1";

/// A canonical non-human credential. The owned plaintext is cleared on drop.
pub struct PresentedServiceAccountKey(Zeroizing<String>);

impl PresentedServiceAccountKey {
    pub fn parse(value: &str) -> Result<Self, ServiceAccountAuthenticationError> {
        let entropy = value
            .strip_prefix(PREFIX)
            .ok_or(ServiceAccountAuthenticationError::InvalidKey)?;
        if entropy.len() != 64
            || !entropy
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || entropy.bytes().all(|byte| byte == b'0')
        {
            return Err(ServiceAccountAuthenticationError::InvalidKey);
        }
        Ok(Self(Zeroizing::new(value.to_owned())))
    }

    #[must_use]
    pub fn digest(&self) -> ServiceAccountKeyDigest {
        let mut hasher = Sha256::new();
        hasher.update(DIGEST_CONTEXT);
        hasher.update([0]);
        hasher.update(self.0.as_bytes());
        let hash = hasher.finalize();
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(64);
        for byte in hash {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        ServiceAccountKeyDigest(Zeroizing::new(encoded))
    }
}

impl fmt::Debug for PresentedServiceAccountKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PresentedServiceAccountKey(<redacted>)")
    }
}

/// Domain-separated digest, deliberately distinct from a personal API Key digest.
pub struct ServiceAccountKeyDigest(Zeroizing<String>);

impl ServiceAccountKeyDigest {
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ServiceAccountKeyDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ServiceAccountKeyDigest(<redacted>)")
    }
}

/// Identity and organizational scope validated in a single storage snapshot.
/// This value grants neither a management session nor permission to charge a wallet.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ServiceAccountAuthentication {
    identity: OrganizationServiceAccountRuntimeIdentity,
    team_id: Option<OrganizationTeamId>,
    department_id: Option<OrganizationDepartmentId>,
}

impl ServiceAccountAuthentication {
    #[must_use]
    pub const fn new(
        identity: OrganizationServiceAccountRuntimeIdentity,
        team_id: Option<OrganizationTeamId>,
        department_id: Option<OrganizationDepartmentId>,
    ) -> Self {
        Self {
            identity,
            team_id,
            department_id,
        }
    }

    #[must_use]
    pub const fn identity(self) -> OrganizationServiceAccountRuntimeIdentity {
        self.identity
    }

    #[must_use]
    pub const fn gateway_identity(self) -> GatewayIdentity {
        GatewayIdentity::ServiceAccount(self.identity)
    }

    #[must_use]
    pub const fn team_id(self) -> Option<OrganizationTeamId> {
        self.team_id
    }

    #[must_use]
    pub const fn department_id(self) -> Option<OrganizationDepartmentId> {
        self.department_id
    }
}

impl fmt::Debug for ServiceAccountAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ServiceAccountAuthentication(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ServiceAccountAuthenticationError {
    #[error("Service account credential is invalid")]
    InvalidKey,
    #[error("Service account authentication is unavailable")]
    Unavailable,
}

pub type ServiceAccountAuthenticationFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ServiceAccountAuthentication, ServiceAccountAuthenticationError>>
            + Send
            + 'a,
    >,
>;

/// Distribution-supplied authentication for independently stored service identities.
/// Implementations must revalidate lifecycle and authorization on every call.
pub trait ServiceAccountAuthenticator: Send + Sync {
    fn authenticate<'a>(
        &'a self,
        digest: &'a ServiceAccountKeyDigest,
    ) -> ServiceAccountAuthenticationFuture<'a>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_matches_previously_issued_credentials_and_redacts_input() {
        let key = PresentedServiceAccountKey::parse(&format!("sa_{}", "1".repeat(64))).unwrap();
        let digest = key.digest();
        assert_eq!(
            digest.as_str(),
            "0454234391ad750cd568dee8274962d94d9010366be2f12b9d22aac7ab9f31ad"
        );
        assert_eq!(
            format!("{key:?}{digest:?}"),
            "PresentedServiceAccountKey(<redacted>)ServiceAccountKeyDigest(<redacted>)"
        );
    }

    #[test]
    fn key_format_does_not_accept_personal_keys_or_noncanonical_values() {
        for value in [
            format!("sa_{}", "0".repeat(64)),
            format!("sa_{}", "A".repeat(64)),
            format!("sa_{}", "1".repeat(63)),
            format!("sa_{} ", "1".repeat(64)),
            format!("sa_{}", "é".repeat(32)),
            format!("sk-af-{}", "1".repeat(43)),
        ] {
            assert_eq!(
                PresentedServiceAccountKey::parse(&value).unwrap_err(),
                ServiceAccountAuthenticationError::InvalidKey
            );
        }
        assert!(PresentedServiceAccountKey::parse(&format!("sa_{}a", "0".repeat(63))).is_ok());
    }
}
