//! HTTP 路由、中间件、提取器与响应映射。

// 公共核心保留部分供发行版扩展的 HTTP 契约；企业发行版会连接这些路由。
#![allow(dead_code)]

mod announcements;
mod anthropic_messages;
mod api_explorer;
mod audio_service;
mod authentication;
mod balance_alert_settings;
mod channel_probe;
mod chat_completions;
mod chat_service;
mod client_ip;
mod credential_proxies;
mod credential_usage;
mod credentials;
mod custom_oauth2;
mod debug_traces;
mod email_settings;
mod embedding_service;
mod error_response;
mod frontend_assets;
mod frontend_templates;
mod gemini_generate_content;
mod http_extensions;
mod image_service;
mod invitations;
#[cfg(test)]
mod invitations_tests;
mod management_analytics_export;
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
mod management_model_prices;
mod management_model_sync;
mod management_model_writes;
mod management_models;
mod management_refunds;
mod management_route_writes;
mod management_routes;
mod management_service_levels;
mod management_session;
mod management_setup;
mod management_token_writes;
mod management_tokens;
mod management_usage_logs;
mod management_users;
mod management_wallet;
mod middleware;
mod model_catalog;
mod model_provider_catalog;
mod network_settings;
mod oauth_connections;
mod oauth_login;
mod openai_audio;
mod openai_embeddings;
mod openai_images;
mod openai_models;
mod openai_responses;
mod openai_responses_compact;
mod openai_speech;
mod openapi;
mod operations;
mod passkey_auth;
mod password_reset;
mod payment_settings;
mod verification_settings;
pub use verification_settings::build_verification_settings_router;
mod payment_webhook;
mod platform_audit;
mod playground_conversations;
mod playground_shares;
mod redemption;
mod refund_webhook;
mod registration;
mod rerank;
mod rerank_service;
mod responses_compact_service;
mod router;
mod serve;
mod site_settings;
mod speech_service;
mod streaming;
mod subscriptions;
mod turnstile;
mod user_notifications;
mod user_profile;
mod user_tokens;
mod user_topups;
mod user_wallet;
mod video_task_service;
mod wallet_query;
mod xai_video;

pub use audio_service::{AudioService, AudioServiceFuture};
pub use axum::{
    Router,
    body::{Body, to_bytes},
    http,
    response::Response,
};
pub use channel_probe::{AdminChannelProbe, AdminChannelProbeFuture, AdminChannelProbeOutcome};
pub use chat_service::{ChatService, ChatServiceFuture};
pub use credential_usage::{
    AdminCredentialUsageFuture, AdminCredentialUsageProbe, CredentialUsageSnapshot,
    CredentialUsageStatus, CredentialUsageWindow,
};
pub use credentials::{ApiKeyExtractionError, QueryApiKeyPolicy, extract_presented_api_key};
pub use embedding_service::{EmbeddingService, EmbeddingServiceFuture};
pub use error_response::OpenAiHttpError;
pub use frontend_assets::{FrontendAsset, FrontendAssetSource};
pub use frontend_templates::{
    FrontendTemplateCatalog, FrontendTemplateError, FrontendTemplateFuture,
    FrontendTemplatePreview, FrontendTemplateService, FrontendTemplateSummary,
    build_frontend_template_router,
};
pub use http_extensions::{
    HttpExtension, HttpExtensionDescriptor, HttpExtensionRoutes, HttpExtensions,
    build_extension_catalog_router,
};
pub use image_service::{ImageService, ImageServiceFuture};
pub use middleware::REQUEST_ID_HEADER_NAME;
pub use oauth_connections::{
    AdminOAuthAuthorizationStart, AdminOAuthBeginFuture, AdminOAuthCompleteFuture,
    AdminOAuthCompletionOutcome, AdminOAuthConnectionError, AdminOAuthConnectionService,
    AdminOAuthProvider, AdminOAuthProviderStatus, MAX_ADMIN_OAUTH_CALLBACK_BODY_BYTES,
    MAX_OAUTH_LOOPBACK_QUERY_BYTES, OAUTH_CALLBACK_COMPLETION_TIMEOUT, OAuthLoopbackBinding,
    OAuthLoopbackBindingError, build_admin_oauth_connection_router,
    build_oauth_loopback_callback_router,
};
pub use openapi::{openapi_document, openapi_document_with_extensions};
pub use operations::{
    HEALTH_PATH, READINESS_PATH, ReadinessFuture, ReadinessHandle, ReadinessProbe,
};
pub use payment_webhook::{PaymentWebhookProcessorRegistry, build_payment_webhook_router};
pub use refund_webhook::{RefundReceiptProcessorRegistry, build_refund_receipt_webhook_router};
pub use rerank_service::{RerankService, RerankServiceFuture};
pub use responses_compact_service::{ResponsesCompactService, ResponsesCompactServiceFuture};
pub use router::{DEFAULT_REQUEST_BODY_LIMIT_BYTES, HttpRouter, build_router};
pub use serve::{HttpListener, ServeError, ServeOutcome, serve_with_graceful_shutdown};
pub use speech_service::{SpeechService, SpeechServiceFuture};
pub use turnstile::{TurnstileVerification, TurnstileVerificationFuture, TurnstileVerifier};
pub use video_task_service::{
    VideoTaskListCursor, VideoTaskListFuture, VideoTaskListItem, VideoTaskPage, VideoTaskService,
    VideoTaskServiceFuture, VideoTaskSnapshot,
};

#[cfg(test)]
mod authentication_tests;
#[cfg(test)]
mod client_ip_tests;
#[cfg(test)]
mod credentials_tests;
#[cfg(test)]
mod custom_oauth2_tests;
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
mod management_wallet_tests;
#[cfg(test)]
mod network_settings_tests;
#[cfg(test)]
mod oauth_connections_tests;
#[cfg(test)]
mod oauth_login_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod passkey_auth_tests;
#[cfg(test)]
mod password_reset_tests;
#[cfg(test)]
mod payment_settings_tests;
#[cfg(test)]
mod payment_webhook_tests;
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
#[cfg(test)]
mod user_topups_tests;
#[cfg(test)]
mod user_wallet_tests;

mod account_verification;
pub use account_verification::build_account_verification_router;
