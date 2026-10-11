use std::fmt;

use crate::{GatewayPrincipal, OrganizationServiceAccountRuntimeIdentity};

/// Authenticated request identity, with separate namespaces for people and services.
///
/// Service identities do not contain a user, member token or routing group. A caller
/// must handle them explicitly before using infrastructure that requires those fields.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum GatewayIdentity {
    User(GatewayPrincipal),
    ServiceAccount(OrganizationServiceAccountRuntimeIdentity),
}

impl From<GatewayPrincipal> for GatewayIdentity {
    fn from(principal: GatewayPrincipal) -> Self {
        Self::User(principal)
    }
}

impl From<OrganizationServiceAccountRuntimeIdentity> for GatewayIdentity {
    fn from(identity: OrganizationServiceAccountRuntimeIdentity) -> Self {
        Self::ServiceAccount(identity)
    }
}

impl fmt::Debug for GatewayIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GatewayIdentity(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GroupId, OrganizationId, OrganizationServiceAccountId, OrganizationServiceAccountKey,
        OrganizationServiceAccountLocator, TokenId, UserId,
    };
    use std::collections::HashSet;

    #[test]
    fn identity_keeps_user_organization_account_and_credential_namespaces_separate() {
        let user = GatewayIdentity::from(GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(1).unwrap(),
            GroupId::new(1).unwrap(),
        ));
        let service = |organization, account, key| {
            GatewayIdentity::from(OrganizationServiceAccountRuntimeIdentity::new(
                OrganizationServiceAccountLocator::new(
                    OrganizationId::new(organization).unwrap(),
                    OrganizationServiceAccountId::new(account).unwrap(),
                ),
                OrganizationServiceAccountKey::new([1; 16]).unwrap(),
                OrganizationServiceAccountKey::new([key; 16]).unwrap(),
            ))
        };
        let identities = [
            user,
            service(1, 1, 1),
            service(2, 1, 1),
            service(1, 2, 1),
            service(1, 1, 2),
        ];
        assert_eq!(identities.into_iter().collect::<HashSet<_>>().len(), 5);
        assert_eq!(
            format!("{:?}", service(1, 1, 1)),
            "GatewayIdentity(<redacted>)"
        );
    }
}
