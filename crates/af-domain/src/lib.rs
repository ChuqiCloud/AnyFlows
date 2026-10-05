//! 无 IO 的领域类型、值对象与错误。

mod auto_ban;
mod billing;
mod billing_contract;
mod billing_runtime;
mod channel;
mod concurrency;
mod enums;
mod error;
mod network;
mod payment;
mod platform_permission;
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
    ChannelType, ClientSimulationBodyPatchResult, ClientSimulationBodyProfile,
    ClientSimulationProfile, ClientSimulationResult, CredentialKind, CredentialQuotaDimension,
    Operation, ParseEnumError, Protocol, PublicErrorCode, ResponsesCompactMode,
    ResponsesCompactProbeResult, Role, Status,
};
pub use error::{
    AfError, NetworkFailureKind, QuotaWindowRetryAfter, RateLimitScope, UpstreamError,
    UpstreamRetryAfter, UpstreamServerStatus,
};
pub use network::{IpCidr, IpCidrParseError, TrustedClientIp};
pub use payment::{
    TopupIdentifierError, TopupOrderId, TopupOrderStatus, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId, TopupStateCodeError,
};
pub use platform_permission::PlatformPermission;
pub use principal::{
    BillingContractPriceId, ChannelId, CredentialId, GatewayPrincipal, GroupId, ModelId,
    OrganizationApprovalDecisionId, OrganizationApprovalRequestId, OrganizationApprovalTemplateId,
    OrganizationBudgetPolicyId, OrganizationBudgetWindowId, OrganizationContractPriceId,
    OrganizationCreditInvoiceId, OrganizationCreditRepaymentAllocationId,
    OrganizationCreditRepaymentId, OrganizationCreditTermId, OrganizationCustomRoleId,
    OrganizationDepartmentId, OrganizationGatewayPrincipal, OrganizationId,
    OrganizationMembershipId, OrganizationProvisioningRequestId, OrganizationScimTokenId,
    OrganizationServiceAccountId, OrganizationTeamId, OrganizationVerificationCaseId,
    OrganizationVerificationMaterialId, PrincipalIdError, ProxyId, RouteChannelId, RouteId,
    TokenId, UserId,
};
pub use quota::{Quota, QuotaDelta, QuotaError};
pub use redemption::{
    RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, RedemptionCodeStatus,
    RedemptionIdentifierError, RedemptionStateCodeError,
};
pub use refund::{
    MAX_REFUND_APPROVAL_REASON_BYTES, MAX_REFUND_MANUAL_REFERENCE_BYTES,
    MAX_REFUND_ORDER_KEY_BYTES, MAX_REFUND_PAYMENT_REFERENCE_BYTES, MAX_REFUND_PROVIDER_BYTES,
    MAX_REFUND_PROVIDER_REFUND_ID_BYTES, RefundApprovalStatus, RefundIdentifierError,
    RefundManualCompletion, RefundManualResult, RefundOrderKind, RefundRequestCreate,
    RefundRequestCreateOutcome, RefundRequestId, RefundRequestInputError, RefundRequestKey,
    RefundRequestRecord, RefundRequestStatus, RefundStateCodeError,
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
pub use billing_contract::{BillingContractPriceSnapshot, BillingContractPriceSnapshotError};
pub use billing_runtime::{
    OrganizationServiceAccountKey, OrganizationServiceAccountKeyError,
    OrganizationServiceAccountLocator, OrganizationServiceAccountRuntimeIdentity,
};
pub use channel::{
    ChannelTimeout, ChannelTimeoutError, MAX_CHANNEL_TIMEOUT_SECS, MIN_CHANNEL_TIMEOUT_SECS,
};

#[cfg(test)]
mod channel_tests;
