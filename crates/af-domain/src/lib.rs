//! 无 IO 的领域类型、值对象与错误。

mod auto_ban;
mod billing;
mod channel;
mod concurrency;
mod enums;
mod error;
mod network;
mod payment;
mod principal;
mod quota;
mod redemption;
mod subscription;
mod token_policy;
mod wallet;

pub use auto_ban::{
    ChannelAutoBanRules, ChannelAutoBanRulesError, MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES,
    MAX_CHANNEL_AUTO_BAN_KEYWORD_TOTAL_BYTES, MAX_CHANNEL_AUTO_BAN_RULES,
};
pub use concurrency::{ConcurrencyLimit, ConcurrencyLimitError};
pub use enums::{
    ChannelType, CredentialKind, Operation, ParseEnumError, Protocol, PublicErrorCode, Role, Status,
};
pub use error::{
    AfError, NetworkFailureKind, RateLimitScope, UpstreamError, UpstreamRetryAfter,
    UpstreamServerStatus,
};
pub use network::{IpCidr, IpCidrParseError, TrustedClientIp};
pub use payment::{
    TopupIdentifierError, TopupOrderId, TopupOrderStatus, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId, TopupStateCodeError,
};
pub use principal::{
    ChannelId, CredentialId, GatewayPrincipal, GroupId, ModelId, PrincipalIdError, TokenId, UserId,
};
pub use quota::{Quota, QuotaDelta, QuotaError};
pub use redemption::{
    RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, RedemptionCodeStatus,
    RedemptionIdentifierError, RedemptionStateCodeError,
};
pub use subscription::{
    SubscriptionCycle, SubscriptionIdentifierError, SubscriptionPlanId, SubscriptionPlanStatus,
    SubscriptionStateCodeError, UserSubscriptionId, UserSubscriptionStatus,
};
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
mod payment_tests;
#[cfg(test)]
mod principal_tests;
#[cfg(test)]
mod quota_tests;
#[cfg(test)]
mod redemption_tests;
#[cfg(test)]
mod subscription_tests;
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
