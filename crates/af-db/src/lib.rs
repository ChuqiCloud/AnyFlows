//! 数据库连接池、迁移、实体与仓储边界。

// 公共核心保留部分供发行版注入的数据库契约；在核心仓库中暂未全部连接到路由。
#![allow(dead_code)]

/// 数据库存储使用的 UTC 时间类型，供应用任务传递一致的扫描时刻。
mod account_verification;
mod account_verification_settings;
pub use account_verification::{
    AccountVerificationError, AccountVerificationMaterial, AccountVerificationProviderRequest,
    AccountVerificationProviderResult, AccountVerificationProviderStart, AccountVerificationRecord,
    AccountVerificationRepository, AccountVerificationSubmit, VerificationMaterialWrite,
    validate_account_verification_submit, validate_verification_material,
};
pub use account_verification_settings::{
    AccountVerificationSettingsError, AccountVerificationSettingsRecord,
    AccountVerificationSettingsRepository,
};

pub type DatabaseTimestamp = sea_orm::entity::prelude::TimeDateTimeWithTimeZone;

mod ability_write;
#[cfg(test)]
mod ability_write_tests;
mod admin_channel;
#[cfg(test)]
mod admin_channel_tests;
mod admin_channel_write;
#[cfg(test)]
mod admin_channel_write_tests;
mod admin_credential;
mod admin_dashboard;
mod admin_dashboard_sla;
pub use admin_dashboard_sla::{
    DashboardServiceLevelPoint, DashboardServiceLevelQuery, DashboardServiceLevelReport,
    DashboardServiceLevelRow,
};
mod admin_dashboard_observability;
mod admin_dashboard_outcomes;
#[cfg(test)]
mod admin_dashboard_tests;
mod admin_group;
#[cfg(test)]
mod admin_group_tests;
mod admin_group_write;
#[cfg(test)]
mod admin_group_write_tests;
mod admin_model;
#[cfg(test)]
mod admin_model_tests;
mod admin_model_write;
mod admin_route;
mod admin_token;
#[cfg(test)]
mod admin_token_tests;
mod admin_token_write;
#[cfg(test)]
mod admin_token_write_tests;
mod admin_usage_log;
#[cfg(test)]
mod admin_usage_log_tests;
mod admin_user;
#[cfg(test)]
mod admin_user_tests;
mod admin_user_write;
mod analytics_export;
#[cfg(test)]
mod analytics_export_tests;
mod announcement;
mod async_task;
mod auth;
mod auth_challenge;
mod auth_challenge_rate_limit;
#[cfg(test)]
mod auth_challenge_tests;
#[cfg(test)]
mod auth_tests;
mod balance_alert;
mod billing_batch;
mod billing_contract;
pub use billing_contract::{BillingContractPriceSnapshotError, OrganizationContractPriceSnapshot};
#[cfg(test)]
mod billing_batch_tests;
mod channel_settings;
mod channel_state;
#[cfg(test)]
mod channel_state_tests;
mod compact_probe_state;
#[cfg(test)]
mod compact_probe_state_tests;
mod connection;
mod credential_probe;
#[cfg(test)]
mod credential_probe_tests;
mod credential_proxy;
#[cfg(test)]
mod credential_proxy_tests;
mod credential_state;
#[cfg(test)]
mod credential_state_tests;
mod custom_oauth2;
mod custom_oauth2_login;
#[cfg(test)]
mod custom_oauth2_login_tests;
#[cfg(test)]
mod custom_oauth2_tests;
mod debug_trace;
#[cfg(test)]
mod debug_trace_tests;
mod email_settings;
#[cfg(test)]
mod email_settings_tests;
mod entity;
mod error;
mod group_pricing;
#[cfg(test)]
mod group_pricing_tests;
mod identity_secret;
mod initial_setup;
#[cfg(test)]
mod initial_setup_tests;
mod invitation;
#[cfg(test)]
mod invitation_tests;
mod invite_rebate;
mod migration;
mod model_catalog_metadata;
mod model_price;
mod model_price_admin;
#[cfg(test)]
mod model_price_admin_tests;
#[cfg(test)]
mod model_price_tests;
mod model_provider_catalog;
mod model_sync;
#[cfg(test)]
mod model_sync_tests;
mod network_settings;
#[cfg(test)]
mod network_settings_tests;
mod notification;
mod oauth_credential;
#[cfg(test)]
mod oauth_credential_tests;
mod oauth_login;
#[cfg(test)]
mod oauth_login_tests;
mod options;
mod passkey;
#[cfg(test)]
mod passkey_tests;
mod password_reset;
mod payment_settings;
#[cfg(test)]
mod payment_settings_tests;
mod platform_audit;
mod playground_conversation;
#[cfg(test)]
mod playground_conversation_tests;
mod playground_share;
#[cfg(test)]
mod playground_share_tests;
mod quota;
#[cfg(test)]
mod quota_group_window_tests;
#[cfg(test)]
mod quota_tests;
#[cfg(test)]
mod quota_token_window_tests;
mod redemption;
mod refund;
#[cfg(test)]
mod refund_reconciliation_tests;
mod registration;
#[cfg(test)]
mod registration_tests;
mod request_outcome;
#[cfg(test)]
mod request_outcome_tests;
mod scheduler_ability;
#[cfg(test)]
mod scheduler_ability_tests;
mod scheduler_outbox;
#[cfg(test)]
mod scheduler_outbox_tests;
mod scheduler_runtime;
mod site_settings;
#[cfg(test)]
mod site_settings_tests;
mod smart_route_runtime;
#[cfg(test)]
mod smart_route_runtime_tests;
mod subscription;
mod subscription_alert;
mod token_owner_guard;
mod token_request_admission;
#[cfg(test)]
mod token_request_admission_tests;
mod topup;
mod usage_log;
#[cfg(test)]
mod usage_log_tests;
mod user_profile;
mod user_session;
#[cfg(test)]
mod user_session_tests;
mod user_token;
#[cfg(test)]
mod user_token_tests;
mod wallet_ledger;

pub use ability_write::{
    MAX_ADMIN_CHANNEL_ABILITIES, MAX_ADMIN_CHANNEL_GROUPS, MAX_ADMIN_CHANNEL_MODEL_BYTES,
    MAX_ADMIN_CHANNEL_MODELS,
};
pub use admin_channel::{
    AdminChannelLookupOutcome, AdminChannelPageRecord, AdminChannelRecord, AdminChannelRepository,
    AdminChannelRepositoryConfigError, AdminChannelRepositoryError, MAX_ADMIN_CHANNEL_JSON_BYTES,
    MAX_ADMIN_CHANNEL_PAGE_SIZE,
};
pub use admin_channel_write::{
    AdminChannelDeleteOutcome, AdminChannelMutationOutcome, AdminChannelWriteRecord,
    AdminChannelWriteRepositoryError, AdminCredentialCreateOutcome, AdminCredentialDeleteOutcome,
    AdminCredentialMutationOutcome, AdminCredentialWriteRecord,
};
pub use admin_credential::{
    AdminCredentialLookupOutcome, AdminCredentialPageOutcome, AdminCredentialPageRecord,
    AdminCredentialRecord, AdminCredentialSecretRecord,
};
pub use admin_dashboard::{
    AdminDashboardChannelRecord, AdminDashboardRecord, AdminDashboardRepository,
    AdminDashboardRepositoryConfigError, AdminDashboardRepositoryError,
};
pub use admin_dashboard_observability::{
    ADMIN_DASHBOARD_BUCKET_COUNT, ADMIN_DASHBOARD_BUCKET_SECONDS, AdminDashboardHourlyRecord,
    AdminDashboardPerformanceRecord, SLOW_FIRST_TOKEN_THRESHOLD_MS, SLOW_REQUEST_THRESHOLD_MS,
};
pub use admin_dashboard_outcomes::{
    AdminDashboardChannelFlowRecord, AdminDashboardFailureRecord, AdminDashboardFlowPathRecord,
    MAX_DASHBOARD_CHANNEL_FLOWS, MAX_DASHBOARD_FLOW_PATHS,
};
pub use admin_group::{
    AdminGroupLookupOutcome, AdminGroupPageRecord, AdminGroupPeakRecord, AdminGroupRecord,
    AdminGroupRepository, AdminGroupRepositoryConfigError, AdminGroupRepositoryError,
    AdminGroupWindowRecord, MAX_ADMIN_GROUP_FLAGS_BYTES, MAX_ADMIN_GROUP_PAGE_SIZE,
};
pub use admin_group_write::{
    AdminGroupDeleteOutcome, AdminGroupMutationOutcome, AdminGroupPeakWriteRecord,
    AdminGroupWriteRecord,
};
pub use admin_model::{
    AdminModelLifecycleRecord, AdminModelLookupOutcome, AdminModelModalitiesRecord,
    AdminModelPageRecord, AdminModelRecord, AdminModelRepository, AdminModelRepositoryConfigError,
    AdminModelRepositoryError, AdminModelVisibilityRecord, MAX_ADMIN_MODEL_CONTEXT_WINDOW,
    MAX_ADMIN_MODEL_DESCRIPTION_BYTES, MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES,
    MAX_ADMIN_MODEL_ICON_URL_BYTES, MAX_ADMIN_MODEL_PAGE_SIZE, MAX_ADMIN_MODEL_PROVIDER_BYTES,
    MAX_ADMIN_MODEL_TAG_BYTES, MAX_ADMIN_MODEL_TAGS, MAX_ADMIN_MODEL_TAGS_BYTES,
};
pub use admin_model_write::{
    AdminModelCreateRecord, AdminModelDeleteOutcome, AdminModelMutationOutcome,
    AdminModelWriteRecord,
};
pub use admin_route::{
    AdminRouteChannelRecord, AdminRouteChannelWriteRecord, AdminRouteDeleteOutcome,
    AdminRouteLookupOutcome, AdminRouteMutationOutcome, AdminRoutePageRecord, AdminRouteRecord,
    AdminRouteRepository, AdminRouteRepositoryConfigError, AdminRouteRepositoryError,
    AdminRouteWriteRecord, MAX_ADMIN_ROUTE_CHANNELS, MAX_ADMIN_ROUTE_MAPPING_BYTES,
    MAX_ADMIN_ROUTE_NAME_BYTES, MAX_ADMIN_ROUTE_PAGE_SIZE,
};
pub use admin_token::{
    AdminTokenLookupOutcome, AdminTokenPageRecord, AdminTokenRecord, AdminTokenRepository,
    AdminTokenRepositoryConfigError, AdminTokenRepositoryError, MAX_ADMIN_TOKEN_PAGE_SIZE,
};
pub use admin_token_write::{
    AdminTokenCreateRecord, AdminTokenDeleteOutcome, AdminTokenMutationOutcome,
    AdminTokenWriteRecord, AdminTokenWriteRepositoryError,
};
pub use admin_usage_log::{
    AdminUsageLogPageRecord, AdminUsageLogRecord, AdminUsageLogRepository,
    AdminUsageLogRepositoryConfigError, AdminUsageLogRepositoryError, AdminUsageLogUsageRecord,
    MAX_ADMIN_USAGE_LOG_PAGE_SIZE,
};
pub use admin_user::{
    AdminUserLookupOutcome, AdminUserPageRecord, AdminUserRecord, AdminUserRepository,
    AdminUserRepositoryConfigError, AdminUserRepositoryError, MAX_ADMIN_USER_PAGE_SIZE,
};
pub use admin_user_write::{
    AdminUserCreateRecord, AdminUserDeleteOutcome, AdminUserMutationOutcome,
    AdminUserRegistrationCreateOutcome, AdminUserUpdateRecord, AdminUserVerifiedCreateOutcome,
};
pub use analytics_export::{
    AnalyticsExportCompletionOutcome, AnalyticsExportFact, AnalyticsExportFactKind,
    AnalyticsExportLease, AnalyticsExportQueueCounts, AnalyticsExportRepository,
    AnalyticsExportRepositoryError,
};
pub use announcement::{
    AnnouncementAudienceRecord, AnnouncementRecord, AnnouncementRepository,
    AnnouncementRepositoryConfigError, AnnouncementRepositoryError, AnnouncementStatusRecord,
    AnnouncementWriteRecord, MAX_ANNOUNCEMENT_BODY_BYTES, MAX_ANNOUNCEMENT_PAGE_SIZE,
    MAX_ANNOUNCEMENT_TITLE_BYTES,
};
pub use async_task::{
    AsyncTaskBillingAccept, AsyncTaskBillingClear, AsyncTaskBillingMark,
    AsyncTaskBillingMutationOutcome, AsyncTaskBillingPlan, AsyncTaskBillingPlanOutcome,
    AsyncTaskBillingRecord, AsyncTaskBillingRepository, AsyncTaskBillingResolution,
    AsyncTaskBillingSettlement, AsyncTaskBillingState, AsyncTaskCreate, AsyncTaskCreateOutcome,
    AsyncTaskInputError, AsyncTaskPageCursor, AsyncTaskPageRecord, AsyncTaskRecord,
    AsyncTaskRepository, AsyncTaskRepositoryConfigError, AsyncTaskRepositoryError,
    AsyncTaskSubmissionAccept, AsyncTaskSubmissionBegin, AsyncTaskSubmissionClaim,
    AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionMutationOutcome, AsyncTaskSubmissionRecord,
    AsyncTaskSubmissionRelease, AsyncTaskSubmissionRepository, AsyncTaskSubmissionState,
    AsyncTaskTransition, AsyncTaskTransitionOutcome, AsyncTaskVideoResolution,
};
pub use auth::{
    OrganizationTokenAuthContext, OrganizationTokenAuthValidation, OrganizationTokenAuthValidator,
    TokenAuthLookup, TokenAuthLookupError, TokenAuthLookupOutcome, TokenAuthRepository,
    TokenAuthRepositoryConfigError, TokenAuthRepositoryError,
};
pub use auth_challenge::{
    AuthChallengeConsume, AuthChallengeConsumeOutcome, AuthChallengeConsumption,
    AuthChallengeInputError, AuthChallengeIssue, AuthChallengeIssueOutcome, AuthChallengeIssued,
    AuthChallengePurpose, AuthChallengeRepository, AuthChallengeRepositoryConfigError,
    AuthChallengeRepositoryError, MAX_AUTH_CHALLENGE_ATTEMPTS,
    MAX_AUTH_CHALLENGE_RESEND_COOLDOWN_SECONDS, MAX_AUTH_CHALLENGE_TTL_SECONDS,
    MIN_AUTH_CHALLENGE_RESEND_COOLDOWN_SECONDS, MIN_AUTH_CHALLENGE_TTL_SECONDS,
};
pub use auth_challenge_rate_limit::{
    AuthChallengeRateLimitClaim, AuthChallengeRateLimitInputError, AuthChallengeRateLimitOutcome,
    AuthChallengeRateLimitRepository, AuthChallengeRateLimitRepositoryConfigError,
    AuthChallengeRateLimitRepositoryError, MAX_AUTH_CHALLENGE_RATE_LIMIT_ATTEMPTS,
    MAX_AUTH_CHALLENGE_RATE_LIMIT_WINDOW_SECONDS, MIN_AUTH_CHALLENGE_RATE_LIMIT_WINDOW_SECONDS,
};
pub use balance_alert::{
    BalanceAlertClaimOutcome, BalanceAlertCompletionOutcome, BalanceAlertDeliveryFailureKind,
    BalanceAlertDeliveryLease, BalanceAlertEnqueueReport, BalanceAlertRepository,
    BalanceAlertRepositoryConfigError, BalanceAlertRepositoryError, BalanceAlertSettingsRecord,
    BalanceAlertSettingsRepository, BalanceAlertSettingsRepositoryConfigError,
    BalanceAlertSettingsRepositoryError, BalanceAlertSettingsWriteRecord,
    DEFAULT_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS, DEFAULT_BALANCE_ALERT_THRESHOLD_QUOTA,
    DEFAULT_SUBSCRIPTION_REMAINING_PERCENT, MAX_BALANCE_ALERT_ATTEMPTS,
    MAX_BALANCE_ALERT_BATCH_SIZE, MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS,
    MAX_SUBSCRIPTION_REMAINING_PERCENT, MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS,
    MIN_SUBSCRIPTION_REMAINING_PERCENT,
};
pub use billing_batch::{
    BillingBatchRepository, BillingBatchRepositoryError, BillingBatchWrite, BillingBatchWriteError,
    BillingBatchWriteOutcome, ChannelBillingWrite, TokenBillingWrite, UserBillingWrite,
};
pub use channel_state::{
    ChannelAutoDisableOutcome, ChannelProbeLease, ChannelProbeRecoveryOutcome,
    ChannelStateRepository, ChannelStateRepositoryError, MAX_CHANNEL_PROBE_BATCH,
};
pub use compact_probe_state::{
    CompactProbeStateRepository, CompactProbeStateRepositoryError, CompactProbeStateWriteOutcome,
};
pub use connection::DatabasePool;
pub(crate) use connection::connect;
pub use credential_probe::{
    CREDENTIAL_ENVELOPE_NONCE_BYTES, CREDENTIAL_ENVELOPE_TAG_BYTES, ChannelProbeHeader,
    ChannelProbeTargetRecord, ChannelProbeTargetRepository, ChannelProbeTargetRepositoryError,
    ChannelProbeTargetRevision, EncryptedCredentialEnvelope, EncryptedCredentialEnvelopeError,
    MAX_CREDENTIAL_ENVELOPE_CIPHERTEXT_BYTES, MAX_CREDENTIAL_ENVELOPE_KEY_ID_BYTES,
};
pub use credential_proxy::{
    CredentialProxyCreateRecord, CredentialProxyDeleteOutcome, CredentialProxyLookupOutcome,
    CredentialProxyMutationOutcome, CredentialProxyPageRecord, CredentialProxyPasswordUpdate,
    CredentialProxyRecord, CredentialProxyRepository, CredentialProxyRepositoryConfigError,
    CredentialProxyRepositoryError, CredentialProxyScheme, CredentialProxyUpdateRecord,
    MAX_CREDENTIAL_PROXY_HOST_BYTES, MAX_CREDENTIAL_PROXY_NAME_BYTES,
    MAX_CREDENTIAL_PROXY_PAGE_SIZE, MAX_CREDENTIAL_PROXY_USERNAME_BYTES,
};
pub use credential_state::{
    CredentialStateChange, CredentialStateEvent, CredentialStateRepository,
    CredentialStateRepositoryError, CredentialStateWriteReport, MAX_CREDENTIAL_STATE_BATCH,
};
pub use custom_oauth2::{
    CustomOAuth2ProviderRecord, CustomOAuth2ProviderRepository,
    CustomOAuth2ProviderRepositoryError, CustomOAuth2ProviderSecretUpdate,
    CustomOAuth2ProviderWriteRecord,
};
pub use custom_oauth2_login::{
    CustomOAuth2IdentityCompletion, CustomOAuth2LoginRepository, CustomOAuth2LoginRepositoryError,
    CustomOAuth2LoginStateClaim,
};
pub use debug_trace::{
    DEFAULT_DEBUG_TRACE_BODY_BYTES, DebugTraceAttemptDiagnosticWrite, DebugTraceAttemptOutcome,
    DebugTraceAttemptRecord, DebugTraceAttemptSnapshotRecord, DebugTraceAttemptWrite,
    DebugTraceDetailRecord, DebugTraceFailureKind, DebugTraceListQuery, DebugTraceOperation,
    DebugTraceOutcome, DebugTracePageRecord, DebugTraceProtocol, DebugTraceRepository,
    DebugTraceRepositoryConfigError, DebugTraceRepositoryError, DebugTraceRequestDiagnosticWrite,
    DebugTraceSettingsRecord, DebugTraceSettingsWrite, DebugTraceSnapshotCipher,
    DebugTraceSnapshotCipherError, DebugTraceSnapshotContext, DebugTraceSnapshotKind,
    DebugTraceSnapshotPlaintext, DebugTraceSnapshotRecord, DebugTraceSnapshotScope,
    DebugTraceSummaryRecord, DebugTraceWrite, DebugTraceWriteError, MAX_DEBUG_TRACE_BODY_BYTES,
    MAX_DEBUG_TRACE_PAGE_SIZE, MAX_DEBUG_TRACE_RETENTION_HOURS, MAX_DEBUG_TRACE_SAMPLE_PER_MILLION,
    MIN_DEBUG_TRACE_BODY_BYTES,
};
pub use email_settings::{
    EmailPasswordUpdate, EmailSettingsRecord, EmailSettingsRepository,
    EmailSettingsRepositoryConfigError, EmailSettingsRepositoryError, EmailSettingsWriteRecord,
    EmailTlsMode,
};
pub use error::{DatabaseError, DatabaseOptionsError};
pub use group_pricing::{
    GroupModelRatioRecord, GroupPeakPricingRecord, GroupPricingCatalog, GroupPricingRecord,
    GroupPricingRepository, GroupPricingRepositoryError, MAX_GROUP_MODEL_RATIO_ENTRIES,
    MAX_GROUP_PRICING_ENTRIES,
};
pub use initial_setup::{
    InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    InitialSetupRepositoryConfigError, InitialSetupRepositoryError, InitialSetupStatus,
};
pub use invitation::{
    UserInvitationLookupOutcome, UserInvitationRebateRecord, UserInvitationRepository,
    UserInvitationRepositoryConfigError, UserInvitationRepositoryError,
    UserInvitationSummaryRecord,
};
pub use invite_rebate::{
    InviteRebateGrant, InviteRebateGrantOutcome, InviteRebateInputError, InviteRebateRecord,
    InviteRebateRejection, InviteRebateRepository, InviteRebateRepositoryConfigError,
    InviteRebateRepositoryError,
};
pub use migration::{
    DatabaseMigrationExtension, EXTENSION_MIGRATION_TABLE_NAME, MigrationHistoryAdoption,
    MigrationOptions, MigratorExtension, PUBLIC_MIGRATION_TABLE_NAME, run_pending_migrations,
    run_pending_migrations_with,
};
pub use model_catalog_metadata::ModelCatalogMetadataPageRecord;
pub use model_price::{
    MAX_MODEL_PRICE_ENTRIES, MAX_MODEL_PRICE_EXPRESSION_BYTES, ModelPriceBillingMode,
    ModelPriceRecord, ModelPriceRepository, ModelPriceRepositoryError,
};
pub use model_price_admin::{
    MAX_MODEL_PRICE_PAGE_SIZE, MAX_MODEL_PRICE_WRITE_BATCH, ModelPricePageRecord,
    ModelPriceWriteError, ModelPriceWriteRecord,
};
pub use model_provider_catalog::{
    MAX_MODEL_PROVIDER_ALIAS_BYTES, MAX_MODEL_PROVIDER_ALIASES,
    MAX_MODEL_PROVIDER_DISPLAY_NAME_BYTES, MAX_MODEL_PROVIDER_KEY_BYTES,
    MAX_MODEL_PROVIDER_LOGO_BYTES, ModelProviderCatalogRecord, ModelProviderCatalogRepository,
    ModelProviderCatalogRepositoryError, ModelProviderCatalogWriteRecord,
};
pub use model_sync::{
    DiscoveredModelRecord, MAX_MISSING_MODEL_CHANNELS, MAX_MISSING_MODEL_PAGE_SIZE,
    MAX_MODEL_SYNC_APPLY_ITEMS, MAX_MODEL_SYNC_CANDIDATES, MAX_MODEL_SYNC_METHOD_BYTES,
    MAX_MODEL_SYNC_METHODS, MODEL_SYNC_PREVIEW_ID_BYTES, MODEL_SYNC_PREVIEW_TTL_SECONDS,
    MissingModelChannelRecord, MissingModelImportItemRecord, MissingModelPageRecord,
    MissingModelRecord, ModelDiscoveryHeaderRecord, ModelDiscoveryMappingRecord,
    ModelDiscoveryTargetLookup, ModelDiscoveryTargetRecord, ModelSyncApplyItemRecord,
    ModelSyncItemRecord, ModelSyncPreviewRecord, ModelSyncPreviewWrite, ModelSyncRelationRecord,
    ModelSyncRepository, ModelSyncRepositoryConfigError, ModelSyncRepositoryError,
};
pub use network_settings::{
    NetworkSettingsMode, NetworkSettingsRecord, NetworkSettingsRepository,
    NetworkSettingsRepositoryConfigError, NetworkSettingsRepositoryError,
    NetworkSettingsWriteRecord, ProxyPasswordUpdate,
};
pub use notification::{
    MAX_USER_NOTIFICATION_PAGE_SIZE, MAX_USER_NOTIFICATION_READ_BATCH, NotificationChannel,
    NotificationDeliveryState, NotificationKind, UserNotificationCursor,
    UserNotificationListOutcome, UserNotificationListQuery, UserNotificationMarkReadOutcome,
    UserNotificationPageRecord, UserNotificationRecord, UserNotificationRepository,
    UserNotificationRepositoryConfigError, UserNotificationRepositoryError,
    UserNotificationSnapshot, UserNotificationWrite,
};
pub use oauth_credential::{
    MAX_OAUTH_REFRESH_CANDIDATES, OAuthCredentialExpirationProjectionUpdateOutcome,
    OAuthCredentialIdentityPatch, OAuthCredentialRefreshFailureKind,
    OAuthCredentialRefreshFailureUpdateOutcome, OAuthCredentialRefreshUpdateOutcome,
    OAuthCredentialRepository, OAuthCredentialRepositoryError, OAuthCredentialTokenUpdateOutcome,
    OAuthExpirationProjectionCandidateRecord, OAuthRefreshCandidateRecord,
};
pub use oauth_login::{
    OAUTH_LOGIN_DISCORD_PROVIDER, OAUTH_LOGIN_GITHUB_PROVIDER, OAUTH_LOGIN_GOOGLE_PROVIDER,
    OAUTH_LOGIN_LINUXDO_PROVIDER, OAUTH_LOGIN_OIDC_PROVIDER, OAUTH_LOGIN_TELEGRAM_PROVIDER,
    OAUTH_LOGIN_WECHAT_PROVIDER, OAuthIdentityCompletion, OAuthLoginProviderRecord,
    OAuthLoginProviderWriteRecord, OAuthLoginRepository, OAuthLoginRepositoryError,
    OAuthLoginSecretUpdate, OAuthLoginStateClaim,
};
pub use options::{DatabaseDialect, DatabaseOptions, PoolOptions};
pub use passkey::{
    PasskeyAuthenticationChallenge, PasskeyAuthenticationOutcome, PasskeyAuthenticationTarget,
    PasskeyRecord, PasskeyRegistrationChallenge, PasskeyRepository, PasskeyRepositoryError,
    PasskeyRevokeOutcome, PasskeySecurityFactors,
};
pub use password_reset::{
    PasswordResetOutcome, PasswordResetRepository, PasswordResetRepositoryConfigError,
    PasswordResetRepositoryError, PasswordResetTargetOutcome,
};
pub use payment_settings::{
    PaymentSecretUpdate, PaymentSettingsRecord, PaymentSettingsRepository,
    PaymentSettingsRepositoryConfigError, PaymentSettingsRepositoryError,
    PaymentSettingsWriteRecord,
};
pub use platform_audit::{
    MAX_PLATFORM_AUDIT_PAGE_SIZE, PlatformAuditOutcome, PlatformAuditPageRecord,
    PlatformAuditQuery, PlatformAuditRecord, PlatformAuditRepository, PlatformAuditRepositoryError,
    PlatformAuditWrite, PlatformAuditWriteError, PlatformAuditWriteOutcome,
};
pub use playground_conversation::{
    MAX_PLAYGROUND_CONVERSATIONS_PER_USER, PlaygroundConversationDeleteOutcome,
    PlaygroundConversationRecord, PlaygroundConversationRepository,
    PlaygroundConversationRepositoryConfigError, PlaygroundConversationRepositoryError,
    PlaygroundConversationSummaryRecord, PlaygroundConversationWrite,
};
pub use playground_share::{
    MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER, MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES,
    PlaygroundShareCreatedRecord, PlaygroundShareRecord, PlaygroundShareRepository,
    PlaygroundShareRepositoryConfigError, PlaygroundShareRepositoryError,
    PlaygroundShareRevokeOutcome, PlaygroundShareWrite,
};
pub use quota::{
    QuotaExtension, QuotaExtensionFuture, QuotaFundingContext, QuotaFundingExtension,
    QuotaFundingFuture, QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError,
    QuotaReservationKind, QuotaReservationStatus,
};
pub use redemption::{
    IssuedRedemptionBatch, IssuedRedemptionCode, MAX_REDEMPTION_AUDIT_PAGE_SIZE,
    MAX_REDEMPTION_BATCH_CODES, MAX_REDEMPTION_BATCH_NAME_BYTES, MAX_REDEMPTION_BATCH_PAGE_SIZE,
    PresentedRedemptionCode, RedemptionAttempt, RedemptionAuditBatchRecord,
    RedemptionAuditPageRecord, RedemptionAuditQuery, RedemptionAuditQueryError,
    RedemptionAuditStatus, RedemptionAuditSummaryRecord, RedemptionBatchCreateOutcome,
    RedemptionBatchDisable, RedemptionBatchDisableOutcome, RedemptionBatchListRecord,
    RedemptionBatchPageRecord, RedemptionBatchRecord, RedemptionBatchWrite,
    RedemptionCodeDefinition, RedemptionCodeDigest, RedemptionInputError, RedemptionMaterialError,
    RedemptionOutcome, RedemptionRecord, RedemptionRejection, RedemptionRepository,
    RedemptionRepositoryConfigError, RedemptionRepositoryError,
};
pub use refund::{
    DEFAULT_REFUND_ADMIN_PAGE_SIZE, MAX_REFUND_ADMIN_PAGE_SIZE, RefundAdminListQuery,
    RefundAdminPage, RefundApprovalOutcome, RefundReceiptOutcome, RefundReceiptWrite,
    RefundReconciliationPage, RefundReconciliationQuery, RefundReconciliationRecord,
    RefundRepository, RefundRepositoryConfigError, RefundRepositoryError, RefundSubmissionOutcome,
};
pub use registration::{
    DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS, DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
    MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS, MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
    MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS, RegistrationAttemptOutcome,
    RegistrationPolicyRecord, RegistrationPolicyWriteRecord, RegistrationRepository,
    RegistrationRepositoryConfigError, RegistrationRepositoryError, RegistrationStatusRecord,
};
pub use request_outcome::{
    RequestFailureKind, RequestOutcomePageRecord, RequestOutcomeRecord, RequestOutcomeRepository,
    RequestOutcomeRepositoryError, RequestOutcomeSubject, RequestOutcomeWrite,
    RequestOutcomeWriteError, RequestOutcomeWriteOutcome,
};
pub use scheduler_ability::{
    MAX_SCHEDULER_ABILITY_ENTRIES, MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES, SchedulerAbilityRecord,
    SchedulerAbilityRepository, SchedulerAbilityRepositoryError,
};
pub use scheduler_outbox::{
    SchedulerCatalogSubject, SchedulerCatalogSubjectError, SchedulerOutboxClaimOutcome,
    SchedulerOutboxCompletionOutcome, SchedulerOutboxLease, SchedulerOutboxRepository,
    SchedulerOutboxRepositoryConfigError, SchedulerOutboxRepositoryError,
};
pub use scheduler_runtime::{
    ChannelModelMappings, ChannelParameterOverrides, ChannelRequestPolicyError,
    MAX_CHANNEL_MODEL_MAPPINGS, MAX_CHANNEL_OUTPUT_TOKENS, MAX_CHANNEL_PARAMETER_OVERRIDES,
    MAX_CHANNEL_REQUEST_POLICY_BYTES, MAX_CHANNEL_STOP_SEQUENCE_BYTES, MAX_CHANNEL_STOP_SEQUENCES,
    MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES, MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL,
    MAX_SCHEDULER_RUNTIME_PROJECTION_BYTES, SchedulerRuntimeCredentialRecord,
    SchedulerRuntimeHeader, SchedulerRuntimeProjection, SchedulerRuntimeProjectionError,
    SchedulerRuntimeProxyRecord, SchedulerRuntimeRecord, SchedulerRuntimeRepository,
    SchedulerRuntimeRepositoryError, SchedulerRuntimeTargetRecord,
    SchedulerRuntimeTargetRecordError, validate_channel_header_overrides,
};
pub use sea_orm_migration::{MigrationTrait, MigratorTrait};
pub use site_settings::{
    BalanceDisplayModeRecord, BalanceDisplayPolicyRecord, BalanceSymbolPositionRecord,
    SiteNavigationGroupRecord, SiteNavigationLinkRecord, SiteNavigationRecord, SiteSettingsRecord,
    SiteSettingsRepository, SiteSettingsRepositoryConfigError, SiteSettingsRepositoryError,
    SiteSettingsWriteRecord, SiteSidebarLinkRecord,
};
pub use smart_route_runtime::{
    MAX_SMART_ROUTE_RUNTIME_RULES, SmartRouteAttemptFeedback, SmartRouteRuntimeCandidate,
    SmartRouteRuntimeRepository, SmartRouteRuntimeRepositoryConfigError,
    SmartRouteRuntimeRepositoryError, SmartRouteRuntimeRule, validate_smart_route_model_mapping,
};
pub use subscription::{
    MAX_SUBSCRIPTION_CURRENCY_BYTES, MAX_SUBSCRIPTION_IDEMPOTENCY_KEY_BYTES,
    MAX_SUBSCRIPTION_PAGE_SIZE, MAX_SUBSCRIPTION_PAYMENT_METHOD_BYTES,
    MAX_SUBSCRIPTION_PLAN_NAME_BYTES, MAX_SUBSCRIPTION_PROVIDER_BYTES,
    MAX_SUBSCRIPTION_PROVIDER_EVENT_ID_BYTES, MAX_SUBSCRIPTION_TRADE_NO_BYTES,
    SubscriptionExpirationDueCursor, SubscriptionInputError, SubscriptionOrderCreate,
    SubscriptionOrderCreateOutcome, SubscriptionOrderRecord, SubscriptionOrderSubmission,
    SubscriptionOrderSubmitOutcome, SubscriptionPaymentEventOutcome,
    SubscriptionPaymentEventRejection, SubscriptionPaymentEventWrite,
    SubscriptionPlanCreateOutcome, SubscriptionPlanDisable, SubscriptionPlanDisableOutcome,
    SubscriptionPlanPageRecord, SubscriptionPlanPriceRecord, SubscriptionPlanRecord,
    SubscriptionPlanWrite, SubscriptionRepository, SubscriptionRepositoryConfigError,
    SubscriptionRepositoryError, SubscriptionResetDueCursor, UserSubscriptionBind,
    UserSubscriptionBindOutcome, UserSubscriptionExpirationDuePageRecord,
    UserSubscriptionLifecycleTransition, UserSubscriptionLifecycleTransitionOutcome,
    UserSubscriptionLifecycleTransitionRecord, UserSubscriptionPageRecord, UserSubscriptionRecord,
    UserSubscriptionResetDuePageRecord, UserSubscriptionWindowAdvance,
    UserSubscriptionWindowAdvanceOutcome, UserSubscriptionWindowAdvanceRecord,
};
pub use subscription_alert::{
    SubscriptionBalanceAlertClaimOutcome, SubscriptionBalanceAlertDeliveryLease,
    SubscriptionBalanceAlertEnqueueReport, SubscriptionBalanceAlertRepository,
    SubscriptionBalanceAlertRepositoryConfigError, SubscriptionBalanceAlertRepositoryError,
};
pub use token_request_admission::{
    TokenRequestAdmissionOutcome, TokenRequestAdmissionRepository,
    TokenRequestAdmissionRepositoryConfigError, TokenRequestAdmissionRepositoryError,
};
pub use topup::{
    MAX_PAYMENT_METHOD_BYTES, MAX_PAYMENT_PROVIDER_BYTES, MAX_PROVIDER_EVENT_ID_BYTES,
    MAX_PROVIDER_ORDER_ID_BYTES, MAX_PROVIDER_TRADE_NO_BYTES, TopupCreditOutcome, TopupExtension,
    TopupExtensionFuture, TopupInputError, TopupOrderCreate, TopupOrderCreateOutcome,
    TopupOrderRecord, TopupOrderSubmission, TopupOrderSubmitOutcome, TopupPaymentEventOutcome,
    TopupPaymentEventRejection, TopupPaymentEventWrite, TopupRepository,
    TopupRepositoryConfigError, TopupRepositoryError,
};
pub use usage_log::{
    UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics,
    UsageLogSource, UsageLogUsage, UsageLogVideoResolution, UsageLogWrite, UsageLogWriteError,
    UsageLogWriteOutcome,
};
pub use user_profile::{
    MAX_USER_PROFILE_USERNAME_BYTES, UserNotificationPreferencesRecord, UserPasswordChangeOutcome,
    UserProfileLookupOutcome, UserProfileMutationOutcome, UserProfileRecord, UserProfileRepository,
    UserProfileRepositoryConfigError, UserProfileRepositoryError, UserTwoFactorMutationOutcome,
};
pub use user_session::{
    UserSessionLookupByIdOutcome, UserSessionLookupOutcome, UserSessionRepository,
    UserSessionRepositoryConfigError, UserSessionRepositoryError,
};
pub use user_token::{
    MAX_USER_TOKEN_PAGE_SIZE, MAX_USER_TOKENS_PER_USER, UserTokenCreateRecord,
    UserTokenDeleteOutcome, UserTokenLookupOutcome, UserTokenMutationOutcome, UserTokenPageRecord,
    UserTokenRecord, UserTokenRepository, UserTokenRepositoryConfigError, UserTokenRepositoryError,
    UserTokenWriteRecord,
};
pub use wallet_ledger::{
    MAX_WALLET_ADJUSTMENT_REASON_BYTES, MAX_WALLET_LEDGER_PAGE_SIZE, WalletAdjustmentOutcome,
    WalletAdjustmentWrite, WalletBalanceLookupOutcome, WalletBalanceRecord,
    WalletLedgerEntryRecord, WalletLedgerEntryType, WalletLedgerError, WalletLedgerListOutcome,
    WalletLedgerPageRecord, WalletLedgerRepository, WalletLedgerRepositoryConfigError,
};

/// 连接数据库并执行全部待处理迁移。
///
/// 这是服务启动的标准入口。迁移失败时不会返回连接池，调用方必须拒绝启动。
pub async fn connect_and_migrate(
    database_options: &DatabaseOptions,
    migration_options: MigrationOptions,
) -> Result<DatabasePool, DatabaseError> {
    connect_and_migrate_with_extension(database_options, migration_options, None).await
}

/// 连接数据库并依次执行公共核心与可选扩展迁移。
///
/// 扩展迁移接收与公共迁移相同的迁移连接，并在公共迁移完成后执行。扩展必须使用
/// 独立迁移表；迁移失败时不会返回连接池，调用方必须拒绝启动。
pub async fn connect_and_migrate_with_extension(
    database_options: &DatabaseOptions,
    migration_options: MigrationOptions,
    extension: Option<&dyn DatabaseMigrationExtension>,
) -> Result<DatabasePool, DatabaseError> {
    if let Some(dedicated_options) = database_options.dedicated_migration_options() {
        let migration_pool = connect(&dedicated_options).await?;
        let public_result = match extension {
            Some(extension) => {
                extension
                    .run_public_migrations(&migration_pool, migration_options)
                    .await
            }
            None => run_pending_migrations(&migration_pool, migration_options).await,
        };
        if let Err(error) = public_result {
            let _ = migration_pool.close().await;
            return Err(error);
        }
        if let Some(extension) = extension
            && let Err(error) = extension
                .run_pending(&migration_pool, migration_options)
                .await
        {
            let _ = migration_pool.close().await;
            return Err(error);
        }
        migration_pool.close().await?;
        return connect(database_options).await;
    }
    let pool = connect(database_options).await?;
    let public_result = match extension {
        Some(extension) => {
            extension
                .run_public_migrations(&pool, migration_options)
                .await
        }
        None => run_pending_migrations(&pool, migration_options).await,
    };
    if let Err(error) = public_result {
        // 迁移失败时主动执行有界关闭，同时保留原始迁移错误供启动层判定。
        let _ = pool.close().await;
        return Err(error);
    }
    if let Some(extension) = extension
        && let Err(error) = extension.run_pending(&pool, migration_options).await
    {
        let _ = pool.close().await;
        return Err(error);
    }
    Ok(pool)
}

/// 连接数据库并依次执行公共核心与扩展迁移。
///
/// 扩展迁移由调用方通过 [`MigratorTrait`] 提供，并且必须使用独立的迁移表。
/// 公共迁移和扩展迁移会在同一个迁移连接上按顺序执行；对于文件 SQLite，这一点
/// 可以避免两套迁移分别获取连接后产生写锁竞争。迁移全部成功后才返回业务连接池。
pub async fn connect_and_migrate_with<M>(
    database_options: &DatabaseOptions,
    migration_options: MigrationOptions,
) -> Result<DatabasePool, DatabaseError>
where
    M: MigratorTrait + 'static,
{
    let extension = MigratorExtension::<M>::new();
    connect_and_migrate_with_extension(database_options, migration_options, Some(&extension)).await
}
