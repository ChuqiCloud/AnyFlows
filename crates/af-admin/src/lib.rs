//! 管理域应用服务。

mod api_key;
mod group_read;
mod group_write;
mod session;
mod token_auth;
mod user_read;
mod user_write;

pub use api_key::{
    ApiKeyDigest, ApiKeyGenerationError, ApiKeyParseError, ApiKeyPrefix, IssuedApiKey,
    PresentedApiKey,
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
