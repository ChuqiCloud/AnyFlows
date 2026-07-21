//! 管理域应用服务。

mod api_key;
mod token_auth;

pub use api_key::{
    ApiKeyDigest, ApiKeyGenerationError, ApiKeyParseError, ApiKeyPrefix, IssuedApiKey,
    PresentedApiKey,
};
pub use token_auth::{
    DatabaseTokenAuthenticator, TokenAuthentication, TokenAuthenticationError,
    TokenAuthenticationFuture, TokenAuthenticator,
};

#[cfg(test)]
mod api_key_tests;
#[cfg(test)]
mod token_auth_tests;
