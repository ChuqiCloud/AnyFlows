//! 登录用户与管理域应用服务。

mod api_key;
mod auth_challenge_security;
mod balance_alert_settings;
mod balance_alert_task;
#[cfg(test)]
mod balance_alert_tests;
mod channel_read;
mod channel_write;
mod credential_proxy;
mod credential_read;
mod email_settings;
#[cfg(test)]
mod email_settings_tests;
mod group_read;
mod group_write;
mod initial_setup;
mod invitation;
mod model_catalog;
mod model_metadata_read;
mod model_metadata_write;
mod model_price;
mod model_sync;
mod network_settings;
mod password_reset;
mod playground_conversation;
mod playground_share;
mod playground_share_snapshot;
mod playground_share_token;
mod redemption;
mod registration;
#[cfg(test)]
mod registration_tests;
mod registration_verification;
mod route_read;
mod route_write;
mod routing_read;
mod session;
mod site_settings;
mod token_auth;
mod token_read;
mod token_write;
mod usage_log_read;
mod user_profile;
mod user_read;
mod user_token;
mod user_wallet;
mod user_write;
mod wallet;

pub use af_account::PlainCredentialSecret as AdminCredentialSecret;
pub use af_db::{CredentialProxyScheme, NetworkSettingsMode};
pub use api_key::{
    ApiKeyDigest, ApiKeyGenerationError, ApiKeyParseError, ApiKeyPrefix, IssuedApiKey,
    PresentedApiKey,
};
pub use balance_alert_settings::{
    AdminBalanceAlertSettings, AdminBalanceAlertSettingsCommand, AdminBalanceAlertSettingsError,
    AdminBalanceAlertSettingsReadFuture, AdminBalanceAlertSettingsService,
    AdminBalanceAlertSettingsUpdateFuture, DatabaseAdminBalanceAlertSettingsService,
};
pub use balance_alert_task::{BalanceAlertTask, BalanceAlertTaskError, BalanceAlertTaskReport};
pub use channel_read::{
    AdminChannel, AdminChannelListQuery, AdminChannelPage, AdminRoutingStatus,
    DEFAULT_ADMIN_CHANNEL_PAGE_SIZE,
};
pub use channel_write::{
    AdminChannelCreateCommand, AdminChannelCreateFuture, AdminChannelDeleteFuture,
    AdminChannelUpdateCommand, AdminChannelUpdateFuture, AdminChannelWriteError,
    AdminChannelWriter, AdminCredentialCreateCommand, AdminCredentialCreateFuture,
    AdminCredentialDeleteFuture, AdminCredentialUpdateCommand, AdminCredentialUpdateFuture,
    AdminRoutingWriteStatus, DatabaseAdminChannelWriter,
};
pub use credential_proxy::{
    AdminCredentialProxy, AdminCredentialProxyCreateCommand, AdminCredentialProxyDeleteFuture,
    AdminCredentialProxyError, AdminCredentialProxyFuture, AdminCredentialProxyListQuery,
    AdminCredentialProxyPage, AdminCredentialProxyPageFuture, AdminCredentialProxyService,
    AdminCredentialProxyUpdateCommand, DatabaseAdminCredentialProxyService,
};
pub use credential_read::{
    AdminCredential, AdminCredentialListQuery, AdminCredentialMultiKeyMode, AdminCredentialPage,
    AdminCredentialQuotaDimension,
};
pub use email_settings::{
    AdminEmailSettings, AdminEmailSettingsCommand, AdminEmailSettingsError,
    AdminEmailSettingsReadFuture, AdminEmailSettingsService, AdminEmailSettingsUpdateFuture,
    AdminEmailTestCommand, AdminEmailTestFuture, AdminEmailTlsMode,
    DatabaseAdminEmailSettingsService, EmailDelivery, EmailDeliveryError, EmailDeliveryFuture,
    EmailDeliveryRequest,
};
pub use group_read::{
    AdminGroup, AdminGroupGetFuture, AdminGroupListFuture, AdminGroupListQuery, AdminGroupPage,
    AdminGroupPeak, AdminGroupReadError, AdminGroupReader, DEFAULT_ADMIN_GROUP_PAGE_SIZE,
    DatabaseAdminGroupReader,
};
pub use group_write::{
    AdminGroupCreateCommand, AdminGroupCreateFuture, AdminGroupDeleteFuture, AdminGroupPeakCommand,
    AdminGroupUpdateCommand, AdminGroupUpdateFuture, AdminGroupWriteError, AdminGroupWriter,
    DatabaseAdminGroupWriter, GroupPricingRuntimeRefreshError, GroupPricingRuntimeRefreshFuture,
    GroupPricingRuntimeRefresher, RuntimeRefreshingAdminGroupWriter,
};
pub use initial_setup::{
    DatabaseInitialSetup, InitialSetup, InitialSetupCommand, InitialSetupError, InitialSetupFuture,
    InitialSetupStatus, InitialSetupStatusFuture, MAX_INITIAL_ADMIN_PASSWORD_BYTES,
    MIN_INITIAL_ADMIN_PASSWORD_BYTES,
};
pub use invitation::{
    DatabaseUserInvitationService, UserInvitationError, UserInvitationReadFuture,
    UserInvitationRebate, UserInvitationService, UserInvitationSummary,
};
pub use model_catalog::{
    DEFAULT_MODEL_CATALOG_PAGE_SIZE, MAX_MODEL_CATALOG_PAGE_SIZE, ModelCatalogBillingMode,
    ModelCatalogItem, ModelCatalogLifecycle, ModelCatalogListFuture, ModelCatalogMetadata,
    ModelCatalogPage, ModelCatalogPricingScope, ModelCatalogQuery, ModelCatalogRatios,
    ModelCatalogReadError, ModelCatalogReader, ModelCatalogRuntimeStatus, ModelCatalogTokenPrices,
};
pub use model_metadata_read::{
    AdminModel, AdminModelGetFuture, AdminModelLifecycle, AdminModelListFuture,
    AdminModelListQuery, AdminModelModalities, AdminModelPage, AdminModelReadError,
    AdminModelReader, AdminModelVisibility, DEFAULT_ADMIN_MODEL_PAGE_SIZE,
    DatabaseAdminModelReader,
};
pub use model_metadata_write::{
    AdminModelCreateCommand, AdminModelCreateFuture, AdminModelDeleteFuture,
    AdminModelUpdateCommand, AdminModelUpdateFuture, AdminModelWriteError, AdminModelWriter,
    DatabaseAdminModelWriter,
};
pub use model_price::{
    AdminModelPrice, AdminModelPriceApplyCommand, AdminModelPriceBillingMode, AdminModelPriceError,
    AdminModelPriceListFuture, AdminModelPriceListQuery, AdminModelPricePage,
    AdminModelPricePreviewFuture, AdminModelPriceService, AdminModelPriceWriteCommand,
    AdminModelPriceWriteFuture, DEFAULT_ADMIN_MODEL_PRICE_PAGE_SIZE,
    DatabaseAdminModelPriceService, MAX_MODEL_PRICE_SOURCE_TARGETS, ModelPriceRuntimeRefreshError,
    ModelPriceRuntimeRefreshFuture, ModelPriceRuntimeRefresher, ModelPriceSourceCandidate,
    ModelPriceSourceDiscoverer, ModelPriceSourceDiscoveryError, ModelPriceSourceDiscoveryFuture,
    ModelPriceSourcePreview, ModelPriceSourceTarget,
};
pub use model_sync::{
    AdminModelSyncApplyCommand, AdminModelSyncApplyItemCommand, AdminModelSyncError,
    AdminModelSyncService, DEFAULT_MISSING_MODEL_PAGE_SIZE, DatabaseAdminModelSyncService,
    MAX_MISSING_MODEL_PAGE_SIZE, MAX_MODEL_SYNC_APPLY_ITEMS, MissingModelChannelRecord,
    MissingModelListFuture, MissingModelPageRecord, MissingModelQuery, MissingModelRecord,
    ModelSyncApplyFuture, ModelSyncItemRecord, ModelSyncPreviewFuture, ModelSyncPreviewRecord,
    ModelSyncRelationRecord, UpstreamModelDiscoverer, UpstreamModelDiscovery,
    UpstreamModelDiscoveryError, UpstreamModelDiscoveryFuture,
};
pub use network_settings::{
    AdminNetworkSettings, AdminNetworkSettingsCommand, AdminNetworkSettingsError,
    AdminNetworkSettingsReadFuture, AdminNetworkSettingsService, AdminNetworkSettingsUpdateFuture,
    DatabaseAdminNetworkSettingsService, NetworkSettingsApplyFuture, NetworkSettingsRuntimeApplier,
    NetworkSettingsRuntimeError,
};
pub use password_reset::{
    DatabasePasswordResetService, MAX_PASSWORD_RESET_PASSWORD_BYTES,
    MIN_PASSWORD_RESET_PASSWORD_BYTES, PasswordResetConfirmCommand, PasswordResetConfirmFuture,
    PasswordResetConfirmResult, PasswordResetError, PasswordResetRequestCommand,
    PasswordResetRequestFuture, PasswordResetRequestResult, PasswordResetService,
    PasswordResetServiceConfigError,
};
pub use playground_conversation::{
    DatabasePlaygroundConversationService, PlaygroundConversation,
    PlaygroundConversationDeleteFuture, PlaygroundConversationError, PlaygroundConversationId,
    PlaygroundConversationInputError, PlaygroundConversationListFuture,
    PlaygroundConversationReadFuture, PlaygroundConversationSaveCommand,
    PlaygroundConversationSaveFuture, PlaygroundConversationService, PlaygroundConversationSummary,
};
pub use playground_share::{
    DatabasePlaygroundShareService, IssuedPlaygroundShare, PlaygroundShareCreateFuture,
    PlaygroundShareError, PlaygroundShareReadFuture, PlaygroundShareRevokeFuture,
    PlaygroundShareService, PlaygroundShareView,
};
pub use playground_share_snapshot::{
    MAX_PLAYGROUND_SHARE_MESSAGE_BYTES, MAX_PLAYGROUND_SHARE_MESSAGES,
    MAX_PLAYGROUND_SHARE_SESSIONS, PlaygroundShareCreateCommand, PlaygroundShareInputError,
    PlaygroundShareMessage, PlaygroundShareMessageRole, PlaygroundShareSession, PlaygroundShareTtl,
};
pub use playground_share_token::{
    IssuedPlaygroundShareToken, PlaygroundShareTokenDigest, PlaygroundShareTokenError,
    PresentedPlaygroundShareToken,
};
pub use redemption::{
    AdminRedemptionBatch, AdminRedemptionBatchCreateCommand, AdminRedemptionBatchDisableCommand,
    AdminRedemptionBatchDisableResult, AdminRedemptionBatchListQuery, AdminRedemptionBatchPage,
    DEFAULT_ADMIN_REDEMPTION_BATCH_PAGE_SIZE, DatabaseRedemptionService,
    IssuedAdminRedemptionBatch, RedemptionCreateFuture, RedemptionDisableFuture,
    RedemptionListFuture, RedemptionRedeemFuture, RedemptionService, RedemptionServiceError,
    UserRedemptionCommand, UserRedemptionResult,
};
pub use registration::{
    DatabaseRegistrationService, MAX_REGISTRATION_PASSWORD_BYTES, MIN_REGISTRATION_PASSWORD_BYTES,
    RegistrationCommand, RegistrationEmailVerificationCommand, RegistrationEmailVerificationFuture,
    RegistrationEmailVerificationResult, RegistrationError, RegistrationFuture, RegistrationPolicy,
    RegistrationPolicyCommand, RegistrationPolicyFuture, RegistrationPolicyUpdateFuture,
    RegistrationResult, RegistrationService, RegistrationServiceConfigError, RegistrationStatus,
    RegistrationStatusFuture,
};
pub use route_read::{
    AdminRoute, AdminRouteChannel, AdminRouteGetFuture, AdminRouteListFuture, AdminRouteListQuery,
    AdminRoutePage, AdminRouteReadError, AdminRouteReader, DEFAULT_ADMIN_ROUTE_PAGE_SIZE,
    DatabaseAdminRouteReader,
};
pub use route_write::{
    AdminRouteChannelCommand, AdminRouteCreateCommand, AdminRouteCreateFuture,
    AdminRouteDeleteFuture, AdminRouteUpdateCommand, AdminRouteUpdateFuture, AdminRouteWriteError,
    AdminRouteWriter, DatabaseAdminRouteWriter,
};
pub use routing_read::{
    AdminChannelGetFuture, AdminChannelListFuture, AdminChannelReadError, AdminChannelReader,
    AdminCredentialGetFuture, AdminCredentialListFuture, DatabaseAdminChannelReader,
};
pub use session::{
    DatabaseSessionAuthenticator, IssuedSession, LoginCredentials, SessionAuthentication,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionAuthenticatorConfigError, SessionLoginFuture, SessionPrincipal, SessionRole,
    SessionToken,
};
pub use site_settings::{
    AdminSiteSettings, AdminSiteSettingsReadFuture, AdminSiteSettingsUpdateFuture,
    DatabaseSiteSettingsService, PublicSiteSettings, PublicSiteSettingsFuture, SiteSettingsCommand,
    SiteSettingsError, SiteSettingsService,
};
pub use token_auth::{
    DatabaseTokenAuthenticator, TokenAuthentication, TokenAuthenticationError,
    TokenAuthenticationFuture, TokenAuthenticator,
};
pub use token_read::{
    AdminToken, AdminTokenGetFuture, AdminTokenListFuture, AdminTokenListQuery, AdminTokenPage,
    AdminTokenReadError, AdminTokenReader, AdminTokenStatus, DEFAULT_ADMIN_TOKEN_PAGE_SIZE,
    DatabaseAdminTokenReader,
};
pub use token_write::{
    AdminTokenCreateCommand, AdminTokenCreateFuture, AdminTokenDeleteFuture,
    AdminTokenUpdateCommand, AdminTokenUpdateFuture, AdminTokenWriteError, AdminTokenWriter,
    DatabaseAdminTokenWriter, IssuedAdminToken, MAX_ADMIN_TOKEN_IP_ALLOWLIST_COUNT,
    MAX_ADMIN_TOKEN_IP_ALLOWLIST_ITEM_BYTES, MAX_ADMIN_TOKEN_IP_ALLOWLIST_TEXT_BYTES,
};
pub use usage_log_read::{
    AdminUsageLog, AdminUsageLogBillingMode, AdminUsageLogListFuture, AdminUsageLogListQuery,
    AdminUsageLogPage, AdminUsageLogReadError, AdminUsageLogReader, AdminUsageLogSemantics,
    AdminUsageLogSource, AdminUsageLogUsage, DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE,
    DatabaseAdminUsageLogReader,
};
pub use user_profile::{
    DatabaseUserProfileService, MAX_USER_PROFILE_PASSWORD_BYTES, MAX_USER_PROFILE_USERNAME_BYTES,
    MIN_USER_PROFILE_PASSWORD_BYTES, UserNotificationPreferences,
    UserNotificationPreferencesCommand, UserNotificationUpdateFuture, UserPasswordChangeCommand,
    UserPasswordChangeFuture, UserProfile, UserProfileError, UserProfileReadFuture,
    UserProfileService, UserProfileUpdateCommand, UserProfileUpdateFuture,
};
pub use user_read::{
    AdminUser, AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery, AdminUserPage,
    AdminUserReadError, AdminUserReader, AdminUserStatus, DEFAULT_ADMIN_USER_PAGE_SIZE,
    DatabaseAdminUserReader,
};
pub use user_token::{
    DEFAULT_USER_TOKEN_PAGE_SIZE, DatabaseUserTokenService, IssuedUserToken,
    MAX_USER_TOKENS_PER_USER, UserToken, UserTokenCreateFuture, UserTokenDeleteFuture,
    UserTokenError, UserTokenGetFuture, UserTokenListFuture, UserTokenListQuery, UserTokenPage,
    UserTokenService, UserTokenStatus, UserTokenUpdateFuture, UserTokenWriteCommand,
};
pub use user_wallet::{
    DEFAULT_USER_WALLET_PAGE_SIZE, DatabaseUserWalletService, UserWalletEntry, UserWalletEntryType,
    UserWalletError, UserWalletListFuture, UserWalletListQuery, UserWalletPage, UserWalletService,
    UserWalletSummary, UserWalletSummaryFuture,
};
pub use user_write::{
    AdminUserCreateCommand, AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserUpdateCommand,
    AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter, DatabaseAdminUserWriter,
    MAX_ADMIN_USER_PASSWORD_BYTES,
};
pub use wallet::{
    AdminWalletAdjustmentCommand, AdminWalletAdjustmentFuture, AdminWalletAdjustmentResult,
    AdminWalletEntry, AdminWalletEntryType, AdminWalletError, AdminWalletListFuture,
    AdminWalletListQuery, AdminWalletPage, AdminWalletService, DEFAULT_ADMIN_WALLET_PAGE_SIZE,
    DatabaseAdminWalletService,
};

#[cfg(test)]
mod api_key_tests;
#[cfg(test)]
mod playground_conversation_tests;
#[cfg(test)]
mod playground_share_tests;
#[cfg(test)]
mod token_auth_tests;
