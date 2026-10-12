use std::{fmt, future::Future, num::NonZeroU32, pin::Pin};

use af_domain::{ConcurrencyLimit, GroupId, ServiceModelPolicy};
use thiserror::Error;

use crate::{ServiceAccountAuthentication, ServiceAccountKeyDigest};

/// A request snapshot validated together with the current service credential.
/// The routing group is a platform grant, never the account creator's group.
#[derive(Clone, Eq, PartialEq)]
pub struct ServiceAccountModelAuthentication {
    authentication: ServiceAccountAuthentication,
    group_id: GroupId,
    model_policy: ServiceModelPolicy,
    concurrency: ConcurrencyLimit,
    rpm_limit: NonZeroU32,
}

impl ServiceAccountModelAuthentication {
    #[must_use]
    pub const fn new(
        authentication: ServiceAccountAuthentication,
        group_id: GroupId,
        model_policy: ServiceModelPolicy,
        concurrency: ConcurrencyLimit,
        rpm_limit: NonZeroU32,
    ) -> Self {
        Self {
            authentication,
            group_id,
            model_policy,
            concurrency,
            rpm_limit,
        }
    }
    #[must_use]
    pub const fn authentication(&self) -> ServiceAccountAuthentication {
        self.authentication
    }
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }
    #[must_use]
    pub const fn model_policy(&self) -> &ServiceModelPolicy {
        &self.model_policy
    }
    #[must_use]
    pub const fn concurrency(&self) -> ConcurrencyLimit {
        self.concurrency
    }
    #[must_use]
    pub const fn rpm_limit(&self) -> NonZeroU32 {
        self.rpm_limit
    }
}

impl fmt::Debug for ServiceAccountModelAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ServiceAccountModelAuthentication(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ServiceAccountModelAuthenticationError {
    #[error("Invalid service account credential")]
    InvalidKey,
    #[error("Service account model access denied")]
    Forbidden,
    #[error("Service account model authentication unavailable")]
    Unavailable,
}

pub type ServiceAccountModelAuthenticationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    ServiceAccountModelAuthentication,
                    ServiceAccountModelAuthenticationError,
                >,
            > + Send
            + 'a,
    >,
>;

/// Independent authorization port. Resolves identity, routing grant and scope
/// restrictions in one snapshot; registering it alone does not enable execution.
pub trait ServiceAccountModelAuthenticator: Send + Sync {
    fn authenticate_models<'a>(
        &'a self,
        digest: &'a ServiceAccountKeyDigest,
    ) -> ServiceAccountModelAuthenticationFuture<'a>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::{
        GatewayIdentity, OrganizationId, OrganizationServiceAccountId,
        OrganizationServiceAccountKey, OrganizationServiceAccountLocator,
        OrganizationServiceAccountRuntimeIdentity,
    };

    #[test]
    fn model_snapshot_preserves_independent_identity_and_explicit_limits() {
        let identity = OrganizationServiceAccountRuntimeIdentity::new(
            OrganizationServiceAccountLocator::new(
                OrganizationId::new(1).unwrap(),
                OrganizationServiceAccountId::new(2).unwrap(),
            ),
            OrganizationServiceAccountKey::new([3; 16]).unwrap(),
            OrganizationServiceAccountKey::new([4; 16]).unwrap(),
        );
        let snapshot = ServiceAccountModelAuthentication::new(
            ServiceAccountAuthentication::new(identity, None, None),
            GroupId::new(5).unwrap(),
            ServiceModelPolicy::new(vec!["gpt-5".into()]).unwrap(),
            ConcurrencyLimit::new(2).unwrap(),
            NonZeroU32::new(60).unwrap(),
        );
        assert_eq!(
            snapshot.authentication().gateway_identity(),
            GatewayIdentity::ServiceAccount(identity)
        );
        assert_eq!(snapshot.group_id().get(), 5);
        assert_eq!(snapshot.concurrency().get(), 2);
        assert_eq!(snapshot.rpm_limit().get(), 60);
        assert!(snapshot.model_policy().allows("gpt-5"));
        assert!(!snapshot.model_policy().allows("another-model"));
        assert_eq!(
            format!("{snapshot:?}"),
            "ServiceAccountModelAuthentication(<redacted>)"
        );
    }
}
