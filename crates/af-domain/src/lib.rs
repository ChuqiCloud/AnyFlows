//! 无 IO 的领域类型、值对象与错误。

mod auto_ban;
mod billing;
mod channel;
mod concurrency;
mod enums;
mod error;
mod network;
mod organization;
mod organization_budget;
mod organization_entitlement;
mod organization_sso;
mod organization_sso_domain_verification;
mod organization_sso_identity;
mod payment;
mod principal;
mod quota;
mod redemption;
mod refund;
mod routing;
mod subscription;
mod subscription_window;
mod task;
mod token_policy;
mod wallet;

/// 试炼场自动申请的内部 Token 名称；普通用户不能通过界面主动复用该名称。
pub const PLAYGROUND_TOKEN_NAME: &str = "__anyflows_playground__";

pub use auto_ban::{
    ChannelAutoBanRules, ChannelAutoBanRulesError, MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES,
    MAX_CHANNEL_AUTO_BAN_KEYWORD_TOTAL_BYTES, MAX_CHANNEL_AUTO_BAN_RULES,
};
pub use concurrency::{ConcurrencyLimit, ConcurrencyLimitError};
pub use enums::{
    ChannelType, ClientSimulationProfile, ClientSimulationResult, CredentialKind,
    CredentialQuotaDimension, Operation, ParseEnumError, Protocol, PublicErrorCode,
    ResponsesCompactMode, ResponsesCompactProbeResult, Role, Status,
};
pub use error::{
    AfError, NetworkFailureKind, QuotaWindowRetryAfter, RateLimitScope, UpstreamError,
    UpstreamRetryAfter, UpstreamServerStatus,
};
pub use network::{IpCidr, IpCidrParseError, TrustedClientIp};
pub use organization::{
    OrganizationCustomRoleKey, OrganizationCustomRoleStatus, OrganizationIdentifierError,
    OrganizationInvitationKey, OrganizationInvitationStatus, OrganizationKey,
    OrganizationMembershipStatus, OrganizationPermission, OrganizationPermissionCodeError,
    OrganizationRole, OrganizationStateCodeError, OrganizationStatus, OrganizationTeamStatus,
    PlatformPermission,
};
pub use organization_budget::{
    MAX_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS, MIN_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS,
    OrganizationBudgetLimit, OrganizationBudgetPeriod, OrganizationBudgetPolicyError,
    OrganizationBudgetScopeKind,
};
pub use organization_entitlement::{
    MAX_ORGANIZATION_AUDIT_RETENTION_DAYS, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationEntitlementStateError,
    OrganizationEntitlementStatus,
};
pub use organization_sso::{
    OrganizationSsoDomain, OrganizationSsoDomainError, OrganizationSsoDomainStatus,
    OrganizationSsoEnforcementFacts, OrganizationSsoEnforcementPolicy,
    OrganizationSsoIdentifierError, OrganizationSsoProtocol, OrganizationSsoProviderKey,
    OrganizationSsoProviderStatus, OrganizationSsoStateCodeError,
};
pub use organization_sso_domain_verification::{
    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS, MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS,
    MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS, ORGANIZATION_SSO_DNS_PROOF_LABEL,
    OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsVerification,
    OrganizationSsoDnsVerificationError, OrganizationSsoDomainProof,
    OrganizationSsoDomainProofError, OrganizationSsoDomainProofToken,
    OrganizationSsoDomainSuffixKind, classify_organization_sso_domain_suffix,
};
pub use organization_sso_identity::{
    OrganizationSsoIdentityBindingMethod, OrganizationSsoIdentityLoginFacts,
    OrganizationSsoIdentityLoginRejection, OrganizationSsoIdentityStateCodeError,
    OrganizationSsoIdentityStatus, authorize_organization_sso_identity_login,
};
pub use payment::{
    TopupIdentifierError, TopupOrderId, TopupOrderStatus, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId, TopupStateCodeError,
};
pub use principal::{
    ChannelId, CredentialId, GatewayPrincipal, GroupId, ModelId, OrganizationBudgetPolicyId,
    OrganizationBudgetWindowId, OrganizationCustomRoleId, OrganizationGatewayPrincipal,
    OrganizationId, OrganizationMembershipId, OrganizationTeamId, PrincipalIdError, ProxyId,
    RouteChannelId, RouteId, TokenId, UserId,
};
pub use quota::{Quota, QuotaDelta, QuotaError};
pub use redemption::{
    RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, RedemptionCodeStatus,
    RedemptionIdentifierError, RedemptionStateCodeError,
};
pub use refund::{
    MAX_REFUND_APPROVAL_REASON_BYTES, MAX_REFUND_ORDER_KEY_BYTES,
    MAX_REFUND_PAYMENT_REFERENCE_BYTES, MAX_REFUND_PROVIDER_BYTES,
    MAX_REFUND_PROVIDER_REFUND_ID_BYTES, RefundApprovalStatus, RefundIdentifierError,
    RefundOrderKind, RefundRequestCreate, RefundRequestCreateOutcome, RefundRequestId,
    RefundRequestInputError, RefundRequestKey, RefundRequestRecord, RefundRequestStatus,
    RefundStateCodeError,
};
pub use routing::{
    MAX_ROUTE_MODEL_PATTERN_BYTES, RouteMode, RoutePatternError, RouteStrategy,
    route_model_pattern_matches, validate_route_model_pattern,
};
pub use subscription::{
    SubscriptionCycle, SubscriptionIdentifierError, SubscriptionOrderId,
    SubscriptionOrderRequestId, SubscriptionOrderStatus, SubscriptionPaymentEventId,
    SubscriptionPaymentEventType, SubscriptionPlanId, SubscriptionPlanStatus,
    SubscriptionStateCodeError, UserSubscriptionId, UserSubscriptionStatus,
};
pub use subscription_window::{
    MAX_SUBSCRIPTION_WINDOW_ADVANCES, SubscriptionWindow, SubscriptionWindowAdvance,
    SubscriptionWindowError,
};
pub use task::{
    AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskIdentifierError,
    AsyncTaskRequestFingerprint, AsyncTaskRequestId, MAX_TASK_FAILURE_REASON_BYTES,
    MAX_UPSTREAM_TASK_ID_BYTES, TaskFailure, TaskFailureError, TaskFailureKind,
    TaskIdentifierError, TaskProgress, TaskProgressError, TaskState, TaskStateError, TaskStatus,
    TaskSubmission, UpstreamTaskId,
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
mod organization_budget_tests;
#[cfg(test)]
mod organization_sso_domain_verification_tests;
#[cfg(test)]
mod organization_sso_tests;
#[cfg(test)]
mod organization_tests;
#[cfg(test)]
mod payment_tests;
#[cfg(test)]
mod principal_tests;
#[cfg(test)]
mod quota_tests;
#[cfg(test)]
mod redemption_tests;
#[cfg(test)]
mod refund_tests;
#[cfg(test)]
mod subscription_tests;
#[cfg(test)]
mod task_tests;
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
