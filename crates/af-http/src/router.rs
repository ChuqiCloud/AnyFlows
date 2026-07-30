use std::sync::Arc;

use af_admin::{
    AdminChannelReader, AdminChannelWriter, AdminGroupReader, AdminGroupWriter, AdminTokenReader,
    AdminTokenWriter, AdminUsageLogReader, AdminUserReader, AdminUserWriter, InitialSetup,
    ModelCatalogReader, PlaygroundConversationService, PlaygroundShareService, RegistrationService,
    SessionAuthenticator, TokenAuthenticator,
};
use af_analytics::AdminDashboardReader;
use af_config::ServerConfig;
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

use crate::authentication::{AuthenticationState, authenticate_api_key};
use crate::frontend_assets::{FrontendAssetSource, frontend_fallback};
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
    create_admin_credential, delete_admin_credential, update_admin_credential,
};
use crate::management_credentials::{get_admin_credential, list_admin_credentials};
use crate::management_dashboard::get_admin_dashboard;
use crate::management_group_writes::{create_admin_group, delete_admin_group, update_admin_group};
use crate::management_groups::{get_admin_group, list_admin_groups};
use crate::management_session::{current_session, login};
use crate::management_setup::{initialize_setup, setup_status};
use crate::management_token_writes::{create_admin_token, delete_admin_token, update_admin_token};
use crate::management_tokens::{get_admin_token, list_admin_tokens};
use crate::management_usage_logs::list_admin_usage_logs;
use crate::management_users::{
    create_admin_user, delete_admin_user, get_admin_user, list_admin_users, update_admin_user,
};
use crate::middleware::{
    AllowedOrigins, LogHttpResponse, MakeHttpSpan, assign_request_id, enforce_allowed_origin,
    request_id_header,
};
use crate::model_catalog::list_models;
use crate::operations::{ReadinessHandle, operations_router};
use crate::playground_conversations::build_playground_conversation_router;
use crate::playground_shares::build_playground_share_router;
use crate::registration::build_registration_router;
use crate::user_tokens::build_user_token_router;
use crate::{
    ChatService, QueryApiKeyPolicy, chat_completions::HttpState, chat_completions::chat_completions,
};

/// OpenAI Chat 业务路由的默认请求正文上限，与协议解析预算共享单一常量。
pub const DEFAULT_REQUEST_BODY_LIMIT_BYTES: usize = MAX_BODY_BYTES;

/// 已完成中间件装配的 HTTP 路由。
///
/// Axum 类型只在 `af-http` 内部流转，服务编排层通过本 crate 提供的监听入口使用它，
/// 避免框架依赖向 `af-server` 泄漏。
pub struct HttpRouter(Router);

impl HttpRouter {
    pub(crate) fn new(router: Router) -> Self {
        Self(router)
    }

    pub(crate) fn into_router(self) -> Router {
        self.0
    }
}

/// 构建包含已鉴权 OpenAI Chat 非流式入口及健康检查的 HTTP Router。
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
    readiness: ReadinessHandle,
    authenticator: Arc<dyn TokenAuthenticator>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    model_catalog_reader: Arc<dyn ModelCatalogReader>,
    playground_share_service: Arc<dyn PlaygroundShareService>,
    playground_conversation_service: Arc<dyn PlaygroundConversationService>,
    user_token_service: Arc<dyn af_admin::UserTokenService>,
    initial_setup: Arc<dyn InitialSetup>,
    registration_service: Arc<dyn RegistrationService>,
    admin_channel_reader: Arc<dyn AdminChannelReader>,
    admin_channel_writer: Arc<dyn AdminChannelWriter>,
    admin_channel_probe: Option<Arc<dyn crate::AdminChannelProbe>>,
    admin_group_reader: Arc<dyn AdminGroupReader>,
    admin_group_writer: Arc<dyn AdminGroupWriter>,
    admin_token_reader: Arc<dyn AdminTokenReader>,
    admin_token_writer: Arc<dyn AdminTokenWriter>,
    admin_dashboard_reader: Arc<dyn AdminDashboardReader>,
    admin_usage_log_reader: Arc<dyn AdminUsageLogReader>,
    admin_user_reader: Arc<dyn AdminUserReader>,
    admin_user_writer: Arc<dyn AdminUserWriter>,
    query_api_key_policy: QueryApiKeyPolicy,
    frontend_assets: Option<Arc<dyn FrontendAssetSource>>,
) -> HttpRouter {
    let authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authenticator, query_api_key_policy, config),
        authenticate_api_key,
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
    .with_initial_setup(initial_setup)
    .with_model_catalog_reader(model_catalog_reader)
    .with_admin_channel_reader(admin_channel_reader)
    .with_admin_channel_writer(admin_channel_writer)
    .with_admin_channel_probe(admin_channel_probe)
    .with_admin_dashboard_reader(admin_dashboard_reader)
    .with_admin_usage_log_reader(admin_usage_log_reader);
    let public_routes = Router::new()
        .route(
            "/v1/chat/completions",
            // 只包裹已注册的 POST endpoint，不把路径级 405 改写为 401。
            post(chat_completions).route_layer(authentication),
        )
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
    let group_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&http_state.session_authenticator)),
        authenticate_management_session,
    );
    let group_item_authentication = middleware::from_fn_with_state(
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
        registration_service,
        Arc::clone(&http_state.session_authenticator),
        config,
    );
    let management_routes = Router::new()
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup", post(initialize_setup))
        .route("/api/auth/login", post(login))
        .route(
            "/api/auth/session",
            get(current_session).route_layer(session_route_authentication),
        )
        .route(
            "/api/models",
            get(list_models).route_layer(model_catalog_authentication),
        )
        .route(
            "/api/admin/users",
            get(list_admin_users)
                .post(create_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(user_collection_authentication),
        )
        .route(
            "/api/admin/users/{id}",
            get(get_admin_user)
                .put(update_admin_user)
                .delete(delete_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(user_item_authentication),
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
                .route_layer(dashboard_authentication),
        )
        .route(
            "/api/admin/usage-logs",
            get(list_admin_usage_logs)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(usage_log_collection_authentication),
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
                .route_layer(credential_collection_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}",
            get(get_admin_credential)
                .put(update_admin_credential)
                .delete(delete_admin_credential)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_item_authentication),
        )
        .with_state(http_state)
        .merge(playground_share_routes)
        .merge(playground_conversation_routes)
        .merge(user_token_routes)
        .merge(registration_routes);
    HttpRouter::new(build_router_with_routes_and_frontend(
        public_routes.merge(management_routes),
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
            ])
            .expose_headers([request_id_header()]),
    )
}
