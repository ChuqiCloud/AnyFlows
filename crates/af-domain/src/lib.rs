//! 无 IO 的领域类型、值对象与错误。

mod billing;
mod channel;
mod enums;
mod error;
mod network;
mod principal;
mod quota;
mod token_policy;
mod wallet;

pub use enums::{
    ChannelType, CredentialKind, Operation, ParseEnumError, Protocol, PublicErrorCode, Role, Status,
};
pub use error::{AfError, UpstreamError, UpstreamServerStatus};
pub use network::{IpCidr, IpCidrParseError, TrustedClientIp};
pub use principal::{
    ChannelId, CredentialId, GatewayPrincipal, GroupId, ModelId, PrincipalIdError, TokenId, UserId,
};
pub use quota::{Quota, QuotaDelta, QuotaError};
pub use token_policy::{
    MAX_MODEL_NAME_BYTES, MAX_TOKEN_MODEL_ALLOWLIST_COUNT, MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES,
    TokenModelPolicy, TokenModelPolicyError,
};
pub use wallet::{WalletEventId, WalletEventIdError};

#[cfg(test)]
mod billing_tests;
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
#[cfg(test)]
mod token_policy_tests;
#[cfg(test)]
mod wallet_tests;
pub use billing::{BillingReservationId, BillingReservationIdError};
pub use channel::{
    ChannelTimeout, ChannelTimeoutError, MAX_CHANNEL_TIMEOUT_SECS, MIN_CHANNEL_TIMEOUT_SECS,
};

#[cfg(test)]
mod channel_tests;
