//! 无 IO 的领域类型、值对象与错误。

mod enums;
mod error;
mod network;
mod principal;
mod quota;

pub use enums::{
    ChannelType, CredentialKind, Operation, ParseEnumError, Protocol, PublicErrorCode, Role, Status,
};
pub use error::{AfError, UpstreamError, UpstreamServerStatus};
pub use network::{IpCidr, IpCidrParseError, TrustedClientIp};
pub use principal::{GatewayPrincipal, GroupId, PrincipalIdError, TokenId, UserId};
pub use quota::{Quota, QuotaDelta, QuotaError};

#[cfg(test)]
mod enums_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod principal_tests;
#[cfg(test)]
mod quota_tests;
