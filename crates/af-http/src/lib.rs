//! HTTP 路由、中间件、提取器与响应映射。

mod authentication;
mod chat_completions;
mod chat_service;
mod client_ip;
mod credentials;
mod error_response;
mod middleware;
mod operations;
mod router;
mod serve;
mod streaming;

pub use chat_service::{ChatService, ChatServiceFuture};
pub use credentials::{ApiKeyExtractionError, QueryApiKeyPolicy, extract_presented_api_key};
pub use error_response::OpenAiHttpError;
pub use middleware::REQUEST_ID_HEADER_NAME;
pub use operations::{
    HEALTH_PATH, READINESS_PATH, ReadinessFuture, ReadinessHandle, ReadinessProbe,
};
pub use router::{DEFAULT_REQUEST_BODY_LIMIT_BYTES, HttpRouter, build_router};
pub use serve::{HttpListener, ServeError, ServeOutcome, serve_with_graceful_shutdown};

#[cfg(test)]
mod authentication_tests;
#[cfg(test)]
mod client_ip_tests;
#[cfg(test)]
mod credentials_tests;
#[cfg(test)]
mod error_response_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod tests;
