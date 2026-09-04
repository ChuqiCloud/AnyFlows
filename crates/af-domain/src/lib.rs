//! 无 IO 的领域类型、值对象与错误。

mod auto_ban;
mod billing;
mod channel;
mod concurrency;
mod enums;
mod error;
mod network;
mod organization;
mod organization_approvals;
mod organization_budget;
mod organization_credit_invoices;
mod organization_credit_repayments;
mod organization_credit_terms;
mod organization_department;
mod organization_department_budget;
mod organization_entitlement;
mod organization_provisioning;
mod organization_scim;
mod organization_scim_audit;
mod organization_scim_rate_limit;
mod organization_service_account;
mod organization_service_account_budget;
mod organization_sso;
mod organization_sso_domain_verification;
mod organization_sso_identity;
mod organization_sso_public_login_rate_limit;
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
pub use organization::{
    OrganizationCustomRoleKey, OrganizationCustomRoleStatus, OrganizationIdentifierError,
    OrganizationInvitationKey, OrganizationInvitationStatus, OrganizationKey,
    OrganizationMembershipStatus, OrganizationPermission, OrganizationPermissionCodeError,
    OrganizationRole, OrganizationStateCodeError, OrganizationStatus, OrganizationTeamStatus,
    PlatformPermission,
};
pub use organization_approvals::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalError, OrganizationApprovalRequest,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateKey,
    OrganizationApprovalTemplateSnapshot,
};
pub use organization_budget::{
    MAX_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS, MIN_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS,
    OrganizationBudgetLimit, OrganizationBudgetPeriod, OrganizationBudgetPolicyError,
    OrganizationBudgetScopeKind,
};
pub use organization_credit_invoices::{
    OrganizationCreditInvoiceAmount, OrganizationCreditInvoiceError, OrganizationCreditInvoiceKey,
    OrganizationCreditInvoiceKeyError, OrganizationCreditInvoiceSnapshot,
    OrganizationCreditInvoiceSourceKey, OrganizationCreditInvoiceSourceKind,
    OrganizationCreditInvoiceStatus,
};
pub use organization_credit_repayments::{
    OrganizationCreditRepaymentAllocation, OrganizationCreditRepaymentAllocationKey,
    OrganizationCreditRepaymentAmount, OrganizationCreditRepaymentError,
    OrganizationCreditRepaymentKey, OrganizationCreditRepaymentKeyError,
    OrganizationCreditRepaymentSnapshot, OrganizationCreditRepaymentSourceKey,
    OrganizationCreditRepaymentSourceKind,
};
pub use organization_credit_terms::{
    OrganizationCreditCurrency, OrganizationCreditLimit, OrganizationCreditTermPeriod,
    OrganizationCreditTermStatus, OrganizationCreditTermsError, OrganizationCreditTermsSnapshot,
};
pub use organization_department::{
    MAX_ORGANIZATION_DEPARTMENT_DEPTH, OrganizationDepartmentDisableFacts,
    OrganizationDepartmentDisableRejection, OrganizationDepartmentKey,
    OrganizationDepartmentKeyError, OrganizationDepartmentMembership,
    OrganizationDepartmentMembershipError, OrganizationDepartmentMembershipStatus,
    OrganizationDepartmentNode, OrganizationDepartmentStatus, OrganizationDepartmentTreeError,
    validate_organization_department_move, validate_organization_department_tree,
};
pub use organization_department_budget::{
    MAX_ORGANIZATION_DEPARTMENT_BUDGET_PATH, OrganizationDepartmentBudgetPath,
    OrganizationDepartmentBudgetPathError,
};
pub use organization_entitlement::{
    MAX_ORGANIZATION_AUDIT_RETENTION_DAYS, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationEntitlementStateError,
    OrganizationEntitlementStatus,
};
pub use organization_provisioning::{
    MAX_ORGANIZATION_PROVISIONING_DECISION_REASON_BYTES, MAX_ORGANIZATION_PROVISIONING_NAME_BYTES,
    MAX_ORGANIZATION_PROVISIONING_REASON_BYTES, MAX_ORGANIZATION_PROVISIONING_SLUG_BYTES,
    OrganizationProvisioningCreateOutcome, OrganizationProvisioningDecisionKind,
    OrganizationProvisioningDecisionOutcome, OrganizationProvisioningError,
    OrganizationProvisioningIdempotencyKey, OrganizationProvisioningRecord,
    OrganizationProvisioningRequest, OrganizationProvisioningRequestKey,
    OrganizationProvisioningStatus,
};
pub use organization_scim::{
    MAX_ORGANIZATION_SCIM_DISPLAY_NAME_BYTES, MAX_ORGANIZATION_SCIM_GROUP_MEMBER_COUNT,
    MAX_ORGANIZATION_SCIM_RESOURCE_ID_BYTES, OrganizationScimGroupResource,
    OrganizationScimLifecycleDisposition, OrganizationScimLifecycleFacts,
    OrganizationScimLifecycleFactsError, OrganizationScimLifecycleIntent,
    OrganizationScimLifecycleRejection, OrganizationScimResourceError, OrganizationScimResourceId,
    OrganizationScimResourceKey, OrganizationScimResourceKind, OrganizationScimResourceSet,
    OrganizationScimResourceVersion, OrganizationScimSyncDisposition, OrganizationScimToken,
    OrganizationScimTokenDigest, OrganizationScimTokenError, OrganizationScimTokenKey,
    OrganizationScimTokenStatus, OrganizationScimUserResource,
    classify_organization_scim_lifecycle, classify_organization_scim_sync,
};
pub use organization_scim_audit::{
    OrganizationScimAuditEventType, OrganizationScimAuditFacts, OrganizationScimAuditFactsError,
    OrganizationScimAuditReason, OrganizationScimAuditResource, OrganizationScimAuditResult,
    OrganizationScimRequestFingerprint, OrganizationScimResourceDigest,
};
pub use organization_scim_rate_limit::{
    DEFAULT_ORGANIZATION_SCIM_RATE_LIMIT, DEFAULT_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS,
    DEFAULT_TOKEN_SCIM_RATE_LIMIT, MAX_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS,
    OrganizationScimRateLimitDecision, OrganizationScimRateLimitError,
    OrganizationScimRateLimitFuture, OrganizationScimRateLimitRejection,
    OrganizationScimRateLimitRule, OrganizationScimRateLimitSnapshot,
    OrganizationScimRateLimitState, OrganizationScimRateLimitStore,
    OrganizationScimRateLimitSubject, OrganizationScimRateLimitWindow,
    classify_organization_scim_rate_limit,
};
pub use organization_service_account::{
    OrganizationServiceAccount, OrganizationServiceAccountIdentifierError,
    OrganizationServiceAccountKey, OrganizationServiceAccountLifecycleDisposition,
    OrganizationServiceAccountLifecycleError, OrganizationServiceAccountLocator,
    OrganizationServiceAccountName, OrganizationServiceAccountNameError,
    OrganizationServiceAccountRuntimeIdentity, OrganizationServiceAccountSecret,
    OrganizationServiceAccountSecretDigest, OrganizationServiceAccountStatus,
    classify_organization_service_account_lifecycle,
};
pub use organization_service_account_budget::{
    OrganizationServiceAccountBudgetBinding, OrganizationServiceAccountBudgetBindingError,
    OrganizationServiceAccountBudgetScope,
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
    OrganizationSsoDnsReverification, OrganizationSsoDnsReverificationError,
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
pub use organization_sso_public_login_rate_limit::{
    ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IDENTIFIER,
    ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IP,
    ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_WINDOW, OrganizationSsoPublicLoginRateLimitDecision,
    OrganizationSsoPublicLoginRateLimitFuture, OrganizationSsoPublicLoginRateLimitKey,
    OrganizationSsoPublicLoginRateLimitStore,
};
pub use payment::{
    TopupIdentifierError, TopupOrderId, TopupOrderStatus, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId, TopupStateCodeError,
};
pub use principal::{
    ChannelId, CredentialId, GatewayPrincipal, GroupId, ModelId, OrganizationApprovalDecisionId,
    OrganizationApprovalRequestId, OrganizationApprovalTemplateId, OrganizationBudgetPolicyId,
    OrganizationBudgetWindowId, OrganizationContractPriceId, OrganizationCreditInvoiceId,
    OrganizationCreditRepaymentAllocationId, OrganizationCreditRepaymentId,
    OrganizationCreditTermId, OrganizationCustomRoleId, OrganizationDepartmentId,
    OrganizationGatewayPrincipal, OrganizationId, OrganizationMembershipId,
    OrganizationProvisioningRequestId, OrganizationScimTokenId, OrganizationServiceAccountId,
    OrganizationTeamId, PrincipalIdError, ProxyId, RouteChannelId, RouteId, TokenId, UserId,
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
mod organization_budget_tests;
#[cfg(test)]
mod organization_department_budget_tests;
#[cfg(test)]
mod organization_department_tests;
#[cfg(test)]
mod organization_scim_audit_tests;
#[cfg(test)]
mod organization_scim_rate_limit_tests;
#[cfg(test)]
mod organization_scim_tests;
#[cfg(test)]
mod organization_service_account_tests;
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
