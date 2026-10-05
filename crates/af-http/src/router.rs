use std::sync::Arc;

use af_admin::{
    AdminBalanceAlertSettingsService, AdminChannelReader, AdminChannelWriter,
    AdminCredentialProxyService, AdminCustomOAuth2ProviderService, AdminDebugTraceService,
    AdminEmailSettingsService, AdminGroupReader, AdminGroupWriter, AdminModelPriceService,
    AdminModelReader, AdminModelSyncService, AdminModelWriter, AdminNetworkSettingsService,
    AdminPaymentSettingsService, AdminRouteReader, AdminRouteWriter, AdminTokenReader,
    AdminTokenWriter, AdminUsageLogReader, AdminUserReader, AdminUserWriter, AdminWalletService,
    InitialSetup, ModelCatalogReader, OAuthLoginService, PasskeyAuthenticationService,
    PasswordResetService, PlaygroundConversationService, PlaygroundShareService, RedemptionService,
    RegistrationService, SessionAuthenticator, SiteSettingsService, SubscriptionService,
    TokenAuthenticator, UserInvitationService, UserNotificationService, UserTopupService,
    UserWalletService,
};
use af_analytics::AdminDashboardReader;
use af_config::ServerConfig;
use af_domain::Protocol;
use af_protocol::openai_chat::MAX_BODY_BYTES;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
use http::{
    HeaderName, HeaderValue, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::announcements::build_announcement_router;
use crate::anthropic_messages::anthropic_messages;
use crate::api_explorer::build_api_explorer_router;
use crate::authentication::{
    AuthenticationState, authenticate_api_key, authenticate_playground_session,
};
use crate::balance_alert_settings::build_balance_alert_settings_router;
use crate::credential_proxies::build_credential_proxy_router;
use crate::credential_usage::get_admin_credential_usage;
use crate::custom_oauth2::build_custom_oauth2_router;
use crate::debug_traces::build_debug_trace_router;
use crate::email_settings::build_email_settings_router;
use crate::frontend_assets::{FrontendAssetSource, frontend_fallback};
use crate::gemini_generate_content::gemini_generate_content;
use crate::http_extensions::{HttpExtensions, build_extension_catalog_router};
use crate::invitations::build_user_invitation_router;
use crate::management_analytics_export::{
    get_admin_analytics_export_status, replay_admin_analytics_export,
};
use crate::management_auth::{
    ManagementAuthenticationState, authenticate_management_session,
    authenticate_optional_management_session,
};
use crate::management_authorization::authorize_management_admin;
use crate::management_channel_probe::probe_admin_channel;
use crate::management_channel_writes::{
    create_admin_channel, delete_admin_channel, update_admin_channel,
};
use crate::management_channels::{get_admin_channel, list_admin_channels};
use crate::management_credential_writes::{
    create_admin_credential, delete_admin_credential, export_admin_credentials,
    import_admin_credentials, update_admin_credential,
};
use crate::management_credentials::{get_admin_credential, list_admin_credentials};
use crate::management_dashboard::get_admin_dashboard;
use crate::management_group_writes::{create_admin_group, delete_admin_group, update_admin_group};
use crate::management_groups::{get_admin_group, list_admin_groups};
use crate::management_model_prices::{
    apply_admin_model_prices, list_admin_model_prices, preview_admin_litellm_model_prices,
    preview_admin_model_price_expression, preview_admin_model_prices,
};
use crate::management_model_sync::{
    apply_admin_model_sync_preview, create_admin_model_sync_preview, import_missing_admin_models,
    list_missing_admin_models,
};
use crate::management_model_writes::{create_admin_model, delete_admin_model, update_admin_model};
use crate::management_models::{get_admin_model, list_admin_models};
use crate::management_refunds::{
    approve_admin_refund, list_account_refund_reconciliations, list_admin_refund_reconciliations,
    list_admin_refunds, manual_complete_admin_refund, reject_admin_refund, submit_admin_refund,
};
use crate::management_route_writes::{create_admin_route, delete_admin_route, update_admin_route};
use crate::management_routes::{get_admin_route, list_admin_routes};
use crate::management_session::current_session;
use crate::management_setup::{initialize_setup, setup_status};
use crate::management_token_writes::{create_admin_token, delete_admin_token, update_admin_token};
use crate::management_tokens::{get_admin_token, list_admin_tokens};
use crate::management_usage_logs::{list_admin_usage_logs, list_user_usage_logs};
use crate::management_users::{
    create_admin_user, delete_admin_user, get_admin_user, list_admin_users, update_admin_user,
};
use crate::management_wallet::{adjust_admin_wallet, list_admin_wallet_entries};
use crate::middleware::{
    AllowedOrigins, LogHttpResponse, MakeHttpSpan, assign_request_id, enforce_allowed_origin,
    ensure_request_id, request_id_header,
};
use crate::model_catalog::{list_model_providers, list_models};
use crate::model_provider_catalog::{
    delete_admin_model_provider, get_admin_model_provider, list_admin_model_provider_catalog,
    list_public_model_provider_catalog, update_admin_model_provider,
};
use crate::network_settings::build_network_settings_router;
use crate::oauth_connections::{AdminOAuthConnectionService, build_admin_oauth_connection_router};
use crate::oauth_login::build_oauth_login_router;
use crate::openai_audio::openai_audio_transcription;
use crate::openai_embeddings::openai_embeddings;
use crate::openai_images::openai_images;
use crate::openai_responses::openai_responses;
use crate::openai_responses_compact::openai_responses_compact;
use crate::openai_speech::openai_speech;
use crate::operations::{ReadinessHandle, operations_router};
use crate::passkey_auth::build_passkey_authentication_router;
use crate::password_reset::build_password_reset_router;
use crate::payment_settings::build_payment_settings_router;
use crate::platform_audit::build_platform_audit_router;
use crate::playground_conversations::build_playground_conversation_router;
use crate::playground_shares::build_playground_share_router;
use crate::redemption::build_redemption_router;
use crate::registration::build_registration_router;
use crate::rerank::rerank as rerank_request;
use crate::site_settings::build_site_settings_router;
use crate::subscriptions::build_subscription_router;
use crate::user_profile::build_user_profile_router;
use crate::user_tokens::build_user_token_router;
use crate::user_topups::build_user_topup_router;
use crate::user_wallet::build_user_wallet_router;
use crate::xai_video::{list_video_tasks, poll_video_task, submit_video_task};
use crate::{
    AudioService, ChatService, EmbeddingService, ImageService, QueryApiKeyPolicy, RerankService,
    ResponsesCompactService, SpeechService, VideoTaskService, chat_completions::HttpState,
    chat_completions::chat_completions,
};

/// OpenAI Chat 业务路由的默认请求正文上限，与协议解析预算共享单一常量。
pub const DEFAULT_REQUEST_BODY_LIMIT_BYTES: usize = MAX_BODY_BYTES;

/// 已完成中间件装配的 HTTP 路由。
///
/// Axum 类型只在 `af-http` 内部流转，服务编排层通过本 crate 提供的监听入口使用它，
/// 避免框架依赖向 `af-server` 泄漏。
#[derive(Clone)]
pub struct HttpRouter(Router);

impl HttpRouter {
    pub(crate) fn new(router: Router) -> Self {
        Self(router)
    }

    pub(crate) fn into_router(self) -> Router {
        self.0
    }

    /// 在组合根追加一个已经自带认证边界的业务路由。
    pub fn merge(self, router: Router) -> Self {
        Self(self.0.merge(router))
    }
}

/// 构建包含已鉴权模型网关入口、管理端接口及健康检查的 HTTP Router。
///
/// 后续业务与运维路由应分别加入 `public_routes` 和 `operations_routes`，确保
/// 鉴权与 CORS 永远不会扩散到 `/metrics`、健康检查等运维端点。
#[allow(
    clippy::too_many_arguments,
    reason = "组合根显式注入各独立服务，避免隐藏运行时依赖"
)]
pub fn build_router(
    config: &ServerConfig,
    chat: Arc<dyn ChatService>,
    audio: Arc<dyn AudioService>,
    embedding: Arc<dyn EmbeddingService>,
    image: Arc<dyn ImageService>,
    rerank: Arc<dyn RerankService>,
    video_task: Option<Arc<dyn VideoTaskService>>,
    speech: Arc<dyn SpeechService>,
    responses_compact: Arc<dyn ResponsesCompactService>,
    readiness: ReadinessHandle,
    authenticator: Arc<dyn TokenAuthenticator>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    model_catalog_reader: Arc<dyn ModelCatalogReader>,
    playground_share_service: Arc<dyn PlaygroundShareService>,
    playground_conversation_service: Arc<dyn PlaygroundConversationService>,
    user_token_service: Arc<dyn af_admin::UserTokenService>,
    initial_setup: Arc<dyn InitialSetup>,
    registration_service: Arc<dyn RegistrationService>,
    passkey_authentication_service: Option<Arc<dyn PasskeyAuthenticationService>>,
    password_reset_service: Arc<dyn PasswordResetService>,
    user_profile_service: Arc<dyn af_admin::UserProfileService>,
    user_wallet_service: Arc<dyn UserWalletService>,
    user_notification_service: Arc<dyn UserNotificationService>,
    platform_audit_service: Option<Arc<dyn af_admin::PlatformAuditService>>,
    user_topup_service: Option<Arc<dyn UserTopupService>>,
    redemption_service: Arc<dyn RedemptionService>,
    subscription_service: Arc<dyn SubscriptionService>,
    user_invitation_service: Arc<dyn UserInvitationService>,
    site_settings_service: Arc<dyn SiteSettingsService>,
    announcement_service: Arc<dyn af_admin::AnnouncementService>,
    admin_custom_oauth2_provider_service: Option<Arc<dyn AdminCustomOAuth2ProviderService>>,
    admin_model_provider_catalog_service: Option<
        Arc<dyn af_admin::AdminModelProviderCatalogService>,
    >,
    oauth_login_service: Option<Arc<dyn OAuthLoginService>>,
    turnstile_verifier: Option<Arc<dyn crate::TurnstileVerifier>>,
    turnstile_site_key: Option<&str>,
    admin_email_settings_service: Arc<dyn AdminEmailSettingsService>,
    admin_network_settings_service: Arc<dyn AdminNetworkSettingsService>,
    admin_payment_settings_service: Arc<dyn AdminPaymentSettingsService>,
    admin_credential_proxy_service: Arc<dyn AdminCredentialProxyService>,
    admin_oauth_connection_service: Option<Arc<dyn AdminOAuthConnectionService>>,
    admin_balance_alert_settings_service: Arc<dyn AdminBalanceAlertSettingsService>,
    admin_debug_trace_service: Arc<dyn AdminDebugTraceService>,
    admin_channel_reader: Arc<dyn AdminChannelReader>,
    admin_channel_writer: Arc<dyn AdminChannelWriter>,
    admin_channel_probe: Option<Arc<dyn crate::AdminChannelProbe>>,
    admin_credential_usage_probe: Option<Arc<dyn crate::AdminCredentialUsageProbe>>,
    admin_group_reader: Arc<dyn AdminGroupReader>,
    admin_group_writer: Arc<dyn AdminGroupWriter>,
    admin_model_reader: Option<Arc<dyn AdminModelReader>>,
    admin_model_writer: Option<Arc<dyn AdminModelWriter>>,
    admin_model_sync_service: Option<Arc<dyn AdminModelSyncService>>,
    admin_model_price_service: Option<Arc<dyn AdminModelPriceService>>,
    admin_route_reader: Option<Arc<dyn AdminRouteReader>>,
    admin_route_writer: Option<Arc<dyn AdminRouteWriter>>,
    admin_token_reader: Arc<dyn AdminTokenReader>,
    admin_token_writer: Arc<dyn AdminTokenWriter>,
    admin_dashboard_reader: Arc<dyn AdminDashboardReader>,
    analytics_export_control: Option<Arc<dyn af_analytics::AnalyticsExportControl>>,
    admin_usage_log_reader: Arc<dyn AdminUsageLogReader>,
    admin_user_reader: Arc<dyn AdminUserReader>,
    admin_user_writer: Arc<dyn AdminUserWriter>,
    admin_wallet_service: Option<Arc<dyn AdminWalletService>>,
    admin_refund_service: Option<Arc<dyn af_admin::AdminRefundService>>,
    query_api_key_policy: QueryApiKeyPolicy,
    frontend_assets: Option<Arc<dyn FrontendAssetSource>>,
    http_extensions: Option<HttpExtensions>,
    payment_webhook_routes: Option<Router>,
) -> HttpRouter {
    let openai_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config),
        authenticate_api_key,
    );
    let responses_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::OpenAiResponses),
        authenticate_api_key,
    );
    let rerank_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::JinaRerank),
        authenticate_api_key,
    );
    let video_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::XaiVideo),
        authenticate_api_key,
    );
    let anthropic_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::Anthropic),
        authenticate_api_key,
    );
    let gemini_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::Gemini),
        authenticate_api_key,
    );
    let playground_openai_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config),
        authenticate_playground_session,
    );
    let playground_responses_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::OpenAiResponses),
        authenticate_playground_session,
    );
    let playground_anthropic_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(Arc::clone(&authenticator), query_api_key_policy, config)
            .with_error_protocol(Protocol::Anthropic),
        authenticate_playground_session,
    );
    let playground_video_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authenticator, query_api_key_policy, config)
            .with_error_protocol(Protocol::XaiVideo),
        authenticate_playground_session,
    );
    let http_state = HttpState::new(
        chat,
        Arc::clone(&session_authenticator),
        admin_group_reader,
        admin_group_writer,
        admin_token_reader,
        admin_token_writer,
        admin_user_reader,
        admin_user_writer,
    )
    .with_audio_service(audio)
    .with_embedding_service(embedding)
    .with_image_service(image)
    .with_rerank_service(rerank)
    .with_video_task_service(video_task)
    .with_speech_service(speech)
    .with_responses_compact_service(responses_compact)
    .with_initial_setup(initial_setup)
    .with_model_catalog_reader(model_catalog_reader)
    .with_model_provider_catalog_service(admin_model_provider_catalog_service)
    .with_admin_model_reader(admin_model_reader)
    .with_admin_model_writer(admin_model_writer)
    .with_admin_model_sync_service(admin_model_sync_service)
    .with_admin_model_price_service(admin_model_price_service)
    .with_admin_route_reader(admin_route_reader)
    .with_admin_route_writer(admin_route_writer)
    .with_admin_channel_reader(admin_channel_reader)
    .with_admin_channel_writer(admin_channel_writer)
    .with_admin_channel_probe(admin_channel_probe)
    .with_admin_credential_usage_probe(admin_credential_usage_probe)
    .with_admin_dashboard_reader(admin_dashboard_reader)
    .with_analytics_export_control(analytics_export_control)
    .with_admin_usage_log_reader(admin_usage_log_reader)
    .with_admin_wallet_service(admin_wallet_service)
    .with_admin_refund_service(admin_refund_service)
    .with_platform_audit_service(platform_audit_service.clone());
    let platform_audit_routes = platform_audit_service
        .map(|service| {
            build_platform_audit_router(service, Arc::clone(&http_state.session_authenticator))
        })
        .unwrap_or_default();
    let api_explorer_routes = build_api_explorer_router(
        Arc::clone(&http_state.session_authenticator),
        http_state.clone(),
    );
    let public_routes = Router::new()
        .route(
            "/v1/models",
            get(crate::openai_models::list_openai_models)
                .route_layer(openai_authentication.clone()),
        )
        .route(
            "/v1/chat/completions",
            // 只包裹已注册的 POST endpoint，不把路径级 405 改写为 401。
            post(chat_completions).route_layer(openai_authentication.clone()),
        )
        .route(
            "/v1/embeddings",
            // Embeddings 与 Chat 共用 OpenAI 错误 wire，但使用独立非流式业务端口。
            post(openai_embeddings).route_layer(openai_authentication.clone()),
        )
        .route(
            "/v1/images/generations",
            // Images 共用 OpenAI 错误 wire，但使用独立的大响应非流式业务端口。
            post(openai_images).route_layer(openai_authentication.clone()),
        )
        .route(
            "/v1/rerank",
            // Rerank 使用独立非流式协议、调度与计费端口，不进入 Chat 参数链路。
            post(rerank_request).route_layer(rerank_authentication),
        )
        .route(
            "/v1/videos/generations",
            // 视频提交必须携带客户端幂等键，服务端只公开稳定本地任务标识。
            post(submit_video_task).route_layer(video_authentication.clone()),
        )
        .route(
            "/v1/videos",
            // 历史列表只读取当前用户持久化摘要，不触发上游轮询或返回签名地址。
            get(list_video_tasks).route_layer(video_authentication.clone()),
        )
        .route(
            "/v1/videos/{task_id}",
            // 查询只在认证用户范围内恢复原绑定，成功结果不会触发第二次付费提交。
            get(poll_video_task).route_layer(video_authentication),
        )
        .route(
            "/v1/audio/transcriptions",
            // Audio 使用独立 multipart 入口和非流式计费端口，不进入通用 Chat 参数链路。
            post(openai_audio_transcription).route_layer(openai_authentication.clone()),
        )
        .route(
            "/v1/audio/speech",
            // Speech 使用独立 JSON 入站与二进制响应链路，不进入转录或 Chat 参数路径。
            post(openai_speech).route_layer(openai_authentication),
        )
        .route(
            "/v1/responses",
            // Responses 使用独立协议标记，OpenAI 错误 wire 与 Chat 保持兼容。
            post(openai_responses).route_layer(responses_authentication.clone()),
        )
        .route(
            "/v1/responses/compact",
            // Compact 使用专用调度与单次计费端口，不回退普通 Responses。
            post(openai_responses_compact).route_layer(responses_authentication),
        )
        .route(
            "/v1/messages",
            // Anthropic 入口独立选择错误 wire，框架级 404/405 仍保持原始边界。
            post(anthropic_messages).route_layer(anthropic_authentication),
        )
        .route(
            "/v1beta/models/{model_action}",
            // Google 风格动作与模型共享一个路径段，由 handler 严格拆分两个已实现动作。
            post(gemini_generate_content).route_layer(gemini_authentication),
        )
        .with_state(http_state.clone());
    let playground_session_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let playground_routes = Router::new()
        .route(
            "/api/playground/v1/chat/completions",
            post(chat_completions).route_layer(playground_openai_authentication),
        )
        .route(
            "/api/playground/v1/responses",
            post(openai_responses).route_layer(playground_responses_authentication),
        )
        .route(
            "/api/playground/v1/messages",
            post(anthropic_messages).route_layer(playground_anthropic_authentication),
        )
        .route(
            "/api/playground/v1/videos/generations",
            post(submit_video_task).route_layer(playground_video_authentication.clone()),
        )
        .route(
            "/api/playground/v1/videos",
            get(list_video_tasks).route_layer(playground_video_authentication.clone()),
        )
        .route(
            "/api/playground/v1/videos/{task_id}",
            get(poll_video_task).route_layer(playground_video_authentication),
        )
        // 最外层先校验 JWT，内层才能把会话用户解析为试炼场主体。
        .layer(playground_session_authentication)
        .with_state(http_state.clone());
    let session_route_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    let model_catalog_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_optional_management_session,
    );
    let user_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let user_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let wallet_entries_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let wallet_adjustment_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let group_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let group_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let route_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let route_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let model_management_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let model_management_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let token_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let token_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let usage_log_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let user_usage_log_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let dashboard_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let channel_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let channel_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let credential_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let credential_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let playground_share_routes = build_playground_share_router(
        playground_share_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let playground_conversation_routes = build_playground_conversation_router(
        playground_conversation_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let user_token_routes = build_user_token_router(
        user_token_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let registration_routes = build_registration_router(
        Arc::clone(&registration_service),
        Arc::clone(&http_state.session_authenticator),
        config,
        turnstile_verifier,
    );
    let passkey_authentication_routes = build_passkey_authentication_router(
        passkey_authentication_service,
        Arc::clone(&http_state.session_authenticator),
        config,
    );
    let password_reset_routes = build_password_reset_router(password_reset_service, config);
    let user_profile_routes = build_user_profile_router(
        user_profile_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let user_wallet_routes = build_user_wallet_router(
        user_wallet_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let user_notification_routes = crate::user_notifications::build_user_notification_router(
        user_notification_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let user_topup_routes = build_user_topup_router(
        user_topup_service.clone(),
        Arc::clone(&http_state.session_authenticator),
    );
    let redemption_routes = build_redemption_router(
        redemption_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let subscription_routes = build_subscription_router(
        subscription_service,
        Arc::clone(&http_state.session_authenticator),
        user_topup_service.clone(),
    );
    let user_invitation_routes = build_user_invitation_router(
        user_invitation_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let site_settings_routes = build_site_settings_router(
        site_settings_service,
        registration_service,
        oauth_login_service.clone(),
        turnstile_site_key,
        Arc::clone(&http_state.session_authenticator),
    );
    let announcement_routes = build_announcement_router(
        announcement_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let custom_oauth2_routes = admin_custom_oauth2_provider_service
        .map(|service| {
            build_custom_oauth2_router(service, Arc::clone(&http_state.session_authenticator))
        })
        .unwrap_or_default();
    let oauth_login_routes = oauth_login_service.map(|service| {
        build_oauth_login_router(service, Arc::clone(&http_state.session_authenticator))
    });
    let email_settings_routes = build_email_settings_router(
        admin_email_settings_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let network_settings_routes = build_network_settings_router(
        admin_network_settings_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let payment_settings_routes = build_payment_settings_router(
        admin_payment_settings_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let credential_proxy_routes = build_credential_proxy_router(
        admin_credential_proxy_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let balance_alert_settings_routes = build_balance_alert_settings_router(
        admin_balance_alert_settings_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let debug_trace_routes = build_debug_trace_router(
        admin_debug_trace_service,
        Arc::clone(&http_state.session_authenticator),
    );
    let oauth_connection_routes = admin_oauth_connection_service.map(|service| {
        build_admin_oauth_connection_router(service, Arc::clone(&http_state.session_authenticator))
    });
    let admin_user_collection_routes = get(list_admin_users)
        .route_layer(middleware::from_fn(ensure_request_id))
        .route_layer(user_collection_authentication.clone())
        .merge(
            post(create_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(user_collection_authentication.clone()),
        );
    let management_routes = Router::new()
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup", post(initialize_setup))
        .route(
            "/api/auth/session",
            get(current_session).route_layer(session_route_authentication),
        )
        .route(
            "/api/models",
            get(list_models).route_layer(model_catalog_authentication.clone()),
        )
        .route(
            "/api/model-providers",
            get(list_model_providers).route_layer(model_catalog_authentication),
        )
        .route(
            "/api/model-provider-catalog",
            get(list_public_model_provider_catalog),
        )
        .route(
            "/api/admin/model-provider-catalog",
            get(list_admin_model_provider_catalog)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(user_collection_authentication.clone()),
        )
        .route(
            "/api/admin/model-provider-catalog/{provider_key}",
            get(get_admin_model_provider)
                .put(update_admin_model_provider)
                .delete(delete_admin_model_provider)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(user_item_authentication.clone()),
        )
        .route("/api/admin/users", admin_user_collection_routes)
        .route(
            "/api/admin/users/{id}",
            get(get_admin_user)
                .put(update_admin_user)
                .delete(delete_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(user_item_authentication),
        )
        .route(
            "/api/admin/users/{id}/wallet/entries",
            get(list_admin_wallet_entries)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(wallet_entries_authentication),
        )
        .route(
            "/api/admin/users/{id}/wallet/adjustments",
            post(adjust_admin_wallet)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(wallet_adjustment_authentication),
        )
        .route(
            "/api/admin/refunds",
            get(list_admin_refunds)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/account/refund-reconciliations",
            get(list_account_refund_reconciliations)
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/admin/refund-reconciliations",
            get(list_admin_refund_reconciliations)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/admin/refunds/{request_id}/approve",
            post(approve_admin_refund)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/admin/refunds/{request_id}/reject",
            post(reject_admin_refund)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/admin/refunds/{request_id}/submit",
            post(submit_admin_refund)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/admin/refunds/{request_id}/manual-complete",
            post(manual_complete_admin_refund)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication.clone()),
        )
        .route(
            "/api/admin/groups",
            get(list_admin_groups)
                .post(create_admin_group)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(group_collection_authentication),
        )
        .route(
            "/api/admin/groups/{id}",
            get(get_admin_group)
                .put(update_admin_group)
                .delete(delete_admin_group)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(group_item_authentication),
        )
        .route(
            "/api/admin/routes",
            get(list_admin_routes)
                .post(create_admin_route)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(route_collection_authentication),
        )
        .route(
            "/api/admin/routes/{id}",
            get(get_admin_route)
                .put(update_admin_route)
                .delete(delete_admin_route)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(route_item_authentication),
        )
        .route(
            "/api/admin/models",
            get(list_admin_models)
                .post(create_admin_model)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/models/{id}",
            get(get_admin_model)
                .put(update_admin_model)
                .delete(delete_admin_model)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_item_authentication.clone()),
        )
        .route(
            "/api/admin/models/missing",
            get(list_missing_admin_models)
                .post(import_missing_admin_models)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/models/sync-previews",
            post(create_admin_model_sync_preview)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/models/sync-previews/{preview_id}/apply",
            post(apply_admin_model_sync_preview)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_item_authentication),
        )
        .route(
            "/api/admin/model-prices",
            get(list_admin_model_prices)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/model-prices/models-dev-preview",
            post(preview_admin_model_prices)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/model-prices/litellm-preview",
            post(preview_admin_litellm_model_prices)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/model-prices/expression-preview",
            post(preview_admin_model_price_expression)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication.clone()),
        )
        .route(
            "/api/admin/model-prices/batch",
            post(apply_admin_model_prices)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(model_management_collection_authentication),
        )
        .route(
            "/api/admin/tokens",
            get(list_admin_tokens)
                .post(create_admin_token)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(token_collection_authentication),
        )
        .route(
            "/api/admin/tokens/{id}",
            get(get_admin_token)
                .put(update_admin_token)
                .delete(delete_admin_token)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(token_item_authentication),
        )
        .route(
            "/api/admin/dashboard",
            get(get_admin_dashboard)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(dashboard_authentication.clone()),
        )
        .route(
            "/api/admin/dashboard/service-levels",
            get(crate::management_service_levels::get_service_levels)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(dashboard_authentication.clone()),
        )
        .route(
            "/api/admin/analytics/export-status",
            get(get_admin_analytics_export_status)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(dashboard_authentication.clone()),
        )
        .route(
            "/api/admin/analytics/export-replay",
            post(replay_admin_analytics_export)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(dashboard_authentication),
        )
        .route(
            "/api/admin/usage-logs",
            get(list_admin_usage_logs)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(usage_log_collection_authentication),
        )
        .route(
            "/api/account/usage-logs",
            get(list_user_usage_logs).route_layer(user_usage_log_authentication),
        )
        .route(
            "/api/admin/channels",
            get(list_admin_channels)
                .post(create_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_collection_authentication),
        )
        .route(
            "/api/admin/channels/{id}",
            get(get_admin_channel)
                .put(update_admin_channel)
                .delete(delete_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_item_authentication.clone()),
        )
        .route(
            "/api/admin/channels/{id}/probe",
            post(probe_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(channel_item_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials",
            get(list_admin_credentials)
                .post(create_admin_credential)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_collection_authentication.clone()),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/import",
            axum::routing::post(import_admin_credentials)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_collection_authentication.clone()),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/export",
            axum::routing::get(export_admin_credentials)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_collection_authentication.clone()),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}",
            get(get_admin_credential)
                .put(update_admin_credential)
                .delete(delete_admin_credential)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_item_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage",
            get(get_admin_credential_usage)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_collection_authentication.clone()),
        )
        .with_state(http_state.clone())
        .merge(playground_share_routes)
        .merge(playground_conversation_routes)
        .merge(user_token_routes)
        .merge(registration_routes)
        .merge(passkey_authentication_routes)
        .merge(password_reset_routes)
        .merge(user_profile_routes)
        .merge(user_wallet_routes)
        .merge(user_notification_routes)
        .merge(user_topup_routes)
        .merge(redemption_routes)
        .merge(subscription_routes)
        .merge(user_invitation_routes)
        .merge(site_settings_routes)
        .merge(announcement_routes)
        .merge(custom_oauth2_routes)
        .merge(email_settings_routes)
        .merge(network_settings_routes)
        .merge(payment_settings_routes)
        .merge(credential_proxy_routes)
        .merge(balance_alert_settings_routes);
    let management_routes = match oauth_login_routes {
        Some(routes) => management_routes.merge(routes),
        None => management_routes,
    };
    let management_routes = management_routes.merge(debug_trace_routes);
    let management_routes = match oauth_connection_routes {
        Some(routes) => management_routes.merge(routes),
        None => management_routes,
    };
    let extension_catalog_routes = build_extension_catalog_router(
        http_extensions
            .as_ref()
            .map(HttpExtensions::descriptors)
            .unwrap_or_default(),
    );
    let extension_routes = http_extensions
        .map(HttpExtensions::into_routes)
        .unwrap_or_default();
    HttpRouter::new(build_router_with_routes_and_frontend(
        public_routes
            .merge(api_explorer_routes)
            .merge(platform_audit_routes)
            .merge(extension_catalog_routes)
            .merge(playground_routes)
            .merge(management_routes)
            .merge(extension_routes.public)
            .merge(extension_routes.management)
            .merge(extension_routes.webhook)
            .merge(payment_webhook_routes.unwrap_or_else(Router::new)),
        operations_router(readiness),
        config,
        DEFAULT_REQUEST_BODY_LIMIT_BYTES,
        frontend_assets,
    ))
}

#[cfg(test)]
pub(crate) fn build_router_with_routes(
    public_routes: Router,
    operations_routes: Router,
    config: &ServerConfig,
    body_limit: usize,
) -> Router {
    build_router_with_routes_and_frontend(
        public_routes,
        operations_routes,
        config,
        body_limit,
        None,
    )
}

pub(crate) fn build_router_with_routes_and_frontend(
    public_routes: Router,
    operations_routes: Router,
    config: &ServerConfig,
    body_limit: usize,
    frontend_assets: Option<Arc<dyn FrontendAssetSource>>,
) -> Router {
    let public_routes = public_routes
        // 显式覆盖 Axum 隐含的 2 MiB，确保 extractor 与流量层共享同一预算。
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(RequestBodyLimitLayer::new(body_limit));
    let public_routes = match cors_layer(config) {
        Some(cors) => public_routes.layer(cors),
        None => public_routes,
    }
    .layer(middleware::from_fn_with_state(
        AllowedOrigins::new(config.cors_allowed_origins()),
        enforce_allowed_origin,
    ));

    let operations_routes = operations_routes
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(RequestBodyLimitLayer::new(body_limit));

    let trace = TraceLayer::new_for_http()
        .make_span_with(MakeHttpSpan)
        .on_request(())
        .on_response(LogHttpResponse)
        .on_body_chunk(())
        .on_eos(())
        // 原始失败对象可能携带敏感上下文，错误分类由后续响应映射显式记录。
        .on_failure(());

    let router = Router::new().merge(public_routes).merge(operations_routes);
    let router = match frontend_assets {
        Some(source) => router.fallback(frontend_fallback(source)),
        None => router,
    };
    router
        .layer(trace)
        // request-id 最外层覆盖 404、预检、Origin 拒绝和 413。
        .layer(middleware::from_fn(assign_request_id))
}

fn cors_layer(config: &ServerConfig) -> Option<CorsLayer> {
    if config.cors_allowed_origins().is_empty() {
        return None;
    }
    let origins = config
        .cors_allowed_origins()
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin.as_str())
                .expect("CorsOrigin 规范文本必须始终满足 HTTP HeaderValue 约束")
        })
        .collect::<Vec<_>>();

    Some(
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ])
            .allow_headers([
                ACCEPT,
                AUTHORIZATION,
                CONTENT_TYPE,
                HeaderName::from_static("x-api-key"),
                HeaderName::from_static("x-goog-api-key"),
                HeaderName::from_static("idempotency-key"),
            ])
            .expose_headers([request_id_header()]),
    )
}
