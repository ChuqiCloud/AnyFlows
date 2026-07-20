//! 管理域应用服务。

mod api_key;

pub use api_key::{
    ApiKeyDigest, ApiKeyGenerationError, ApiKeyParseError, ApiKeyPrefix, IssuedApiKey,
    PresentedApiKey,
};

#[cfg(test)]
mod api_key_tests;
