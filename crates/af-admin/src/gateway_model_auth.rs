use std::fmt;

use af_domain::GroupId;

use crate::{ServiceAccountModelAuthentication, TokenAuthentication};

/// Model authorization snapshots retain the original credential namespace.
#[derive(Clone, Eq, PartialEq)]
pub enum GatewayModelAuthentication {
    User(TokenAuthentication),
    ServiceAccount(ServiceAccountModelAuthentication),
}

impl GatewayModelAuthentication {
    #[must_use]
    pub fn allows_model(&self, model: &str) -> bool {
        match self {
            Self::User(value) => value.model_policy().allows(model),
            Self::ServiceAccount(value) => value.model_policy().allows(model),
        }
    }

    #[must_use]
    pub fn group_id(&self) -> GroupId {
        match self {
            Self::User(value) => value.principal().group_id(),
            Self::ServiceAccount(value) => value.group_id(),
        }
    }
}

impl fmt::Debug for GatewayModelAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GatewayModelAuthentication(<redacted>)")
    }
}
