//! 登录用户与管理域应用服务。

mod api_key;
mod channel_read;
mod channel_write;
mod credential_read;
mod group_read;
mod group_write;
mod initial_setup;
mod model_catalog;
mod routing_read;
mod session;
mod token_auth;
mod token_read;
mod token_write;
mod usage_log_read;
mod user_read;
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
pub use user_read::{
    AdminUser, AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery, AdminUserPage,
    AdminUserReadError, AdminUserReader, AdminUserStatus, DEFAULT_ADMIN_USER_PAGE_SIZE,
    DatabaseAdminUserReader,
};
pub use user_write::{
    AdminUserCreateCommand, AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserUpdateCommand,
    AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter, DatabaseAdminUserWriter,
    MAX_ADMIN_USER_PASSWORD_BYTES,
};

#[cfg(test)]
mod api_key_tests;
#[cfg(test)]
mod token_auth_tests;
