//! HTTP 路由、中间件、提取器与响应映射。

mod anthropic_messages;
mod authentication;
mod channel_probe;
mod chat_completions;
mod chat_service;
mod client_ip;
mod credentials;
mod email_settings;
mod error_response;
mod frontend_assets;
mod gemini_generate_content;
mod management_auth;
mod management_authorization;
mod management_channel_probe;
mod management_channel_writes;
mod management_channels;
mod management_credential_writes;
mod management_credentials;
mod management_dashboard;
mod management_error;
mod management_group_writes;
mod management_groups;
mod management_session;
mod management_setup;
mod management_token_writes;
mod management_tokens;
mod management_usage_logs;
mod management_users;
mod middleware;
mod model_catalog;
mod openai_responses;
mod openapi;
mod operations;
mod password_reset;
mod playground_conversations;
mod playground_shares;
mod registration;
mod router;
mod serve;
mod site_settings;
mod streaming;
mod user_tokens;

pub use channel_probe::{AdminChannelProbe, AdminChannelProbeFuture, AdminChannelProbeOutcome};
pub use chat_service::{ChatService, ChatServiceFuture};
pub use credentials::{ApiKeyExtractionError, QueryApiKeyPolicy, extract_presented_api_key};
pub use error_response::OpenAiHttpError;
pub use frontend_assets::{FrontendAsset, FrontendAssetSource};
pub use middleware::REQUEST_ID_HEADER_NAME;
pub use openapi::openapi_document;
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
mod email_settings_tests;
#[cfg(test)]
mod error_response_tests;
#[cfg(test)]
mod frontend_assets_tests;
#[cfg(test)]
mod management_channels_tests;
#[cfg(test)]
mod management_group_writes_tests;
#[cfg(test)]
mod management_groups_tests;
#[cfg(test)]
mod management_session_tests;
#[cfg(test)]
mod management_tokens_tests;
#[cfg(test)]
mod management_users_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod password_reset_tests;
#[cfg(test)]
mod playground_conversations_tests;
#[cfg(test)]
mod playground_shares_tests;
#[cfg(test)]
mod registration_tests;
#[cfg(test)]
mod site_settings_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod user_tokens_tests;
