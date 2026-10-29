use af_domain::{
    ConcurrencyLimit, GatewayIdentity, GatewayPrincipal, GroupId, OrganizationId,
    OrganizationServiceAccountRuntimeIdentity,
};

/// Routing facts do not change the authenticated account or credential.
#[derive(Clone, Copy)]
pub(crate) enum ModelSubject {
    User(GatewayPrincipal),
    ServiceAccount {
        identity: OrganizationServiceAccountRuntimeIdentity,
        group_id: GroupId,
        concurrency: ConcurrencyLimit,
    },
}

impl ModelSubject {
    pub(crate) const fn group_id(self) -> GroupId {
        match self {
            Self::User(value) => value.group_id(),
            Self::ServiceAccount { group_id, .. } => group_id,
        }
    }

    pub(crate) const fn billing_source_group_id(self) -> GroupId {
        match self {
            Self::User(value) => value.billing_source_group_id(),
            Self::ServiceAccount { group_id, .. } => group_id,
        }
    }

    pub(crate) const fn organization_id(self) -> Option<OrganizationId> {
        match self {
            Self::User(value) => match value.organization_principal() {
                Some(organization) => Some(organization.organization_id()),
                None => None,
            },
            Self::ServiceAccount { identity, .. } => Some(identity.locator().organization_id()),
        }
    }

    pub(crate) const fn is_playground(self) -> bool {
        match self {
            Self::User(value) => value.is_playground(),
            Self::ServiceAccount { .. } => false,
        }
    }

    pub(crate) const fn identity(self) -> GatewayIdentity {
        match self {
            Self::User(value) => GatewayIdentity::User(value),
            Self::ServiceAccount { identity, .. } => GatewayIdentity::ServiceAccount(identity),
        }
    }
}

impl From<GatewayPrincipal> for ModelSubject {
    fn from(value: GatewayPrincipal) -> Self {
        Self::User(value)
    }
}
