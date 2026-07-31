//! 登录用户与管理域应用服务。

mod api_key;
mod auth_challenge_security;
mod channel_read;
mod channel_write;
mod credential_read;
mod email_settings;
#[cfg(test)]
mod email_settings_tests;
mod group_read;
mod group_write;
mod initial_setup;
mod model_catalog;
mod password_reset;
mod playground_conversation;
mod playground_share;
mod playground_share_snapshot;
mod playground_share_token;
mod registration;
#[cfg(test)]
mod registration_tests;
mod registration_verification;
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
mod user_write;

pub use api_key::{
    ApiKeyDigest, ApiKeyGenerationError, ApiKeyParseError, ApiKeyPrefix, IssuedApiKey,
    PresentedApiKey,
};
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
    DatabaseAdminGroupWriter,
};
pub use initial_setup::{
    DatabaseInitialSetup, InitialSetup, InitialSetupCommand, InitialSetupError, InitialSetupFuture,
    InitialSetupStatus, InitialSetupStatusFuture, MAX_INITIAL_ADMIN_PASSWORD_BYTES,
    MIN_INITIAL_ADMIN_PASSWORD_BYTES,
};
pub use model_catalog::{
    DEFAULT_MODEL_CATALOG_PAGE_SIZE, MAX_MODEL_CATALOG_PAGE_SIZE, ModelCatalogBillingMode,
    ModelCatalogItem, ModelCatalogPage, ModelCatalogPricingScope, ModelCatalogQuery,
    ModelCatalogRatios, ModelCatalogReadError, ModelCatalogReader, ModelCatalogTokenPrices,
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
pub use registration::{
    DatabaseRegistrationService, MAX_REGISTRATION_PASSWORD_BYTES, MIN_REGISTRATION_PASSWORD_BYTES,
    RegistrationCommand, RegistrationEmailVerificationCommand, RegistrationEmailVerificationFuture,
    RegistrationEmailVerificationResult, RegistrationError, RegistrationFuture, RegistrationPolicy,
    RegistrationPolicyCommand, RegistrationPolicyFuture, RegistrationPolicyUpdateFuture,
    RegistrationResult, RegistrationService, RegistrationServiceConfigError, RegistrationStatus,
    RegistrationStatusFuture,
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
pub use user_write::{
    AdminUserCreateCommand, AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserUpdateCommand,
    AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter, DatabaseAdminUserWriter,
    MAX_ADMIN_USER_PASSWORD_BYTES,
};

#[cfg(test)]
mod api_key_tests;
#[cfg(test)]
mod playground_conversation_tests;
#[cfg(test)]
mod playground_share_tests;
#[cfg(test)]
mod token_auth_tests;
