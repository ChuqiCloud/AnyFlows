use std::sync::Arc;

use af_admin::TokenAuthenticator;
use af_config::ServerConfig;
use af_protocol::openai_chat::MAX_BODY_BYTES;
use af_relay::RelayService;
use axum::{Router, extract::DefaultBodyLimit, middleware, routing::post};
use http::{
    HeaderName, HeaderValue, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::authentication::{AuthenticationState, authenticate_api_key};
use crate::middleware::{
    AllowedOrigins, LogHttpResponse, MakeHttpSpan, assign_request_id, enforce_allowed_origin,
    request_id_header,
};
use crate::operations::{ReadinessHandle, operations_router};
use crate::{QueryApiKeyPolicy, chat_completions::HttpState, chat_completions::chat_completions};

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
pub fn build_router(
    config: &ServerConfig,
    relay: Arc<RelayService>,
    readiness: ReadinessHandle,
    authenticator: Arc<dyn TokenAuthenticator>,
    query_api_key_policy: QueryApiKeyPolicy,
) -> HttpRouter {
    let authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authenticator, query_api_key_policy),
        authenticate_api_key,
    );
    let public_routes = Router::new()
        .route(
            "/v1/chat/completions",
            // 只包裹已注册的 POST endpoint，不把路径级 405 改写为 401。
            post(chat_completions).route_layer(authentication),
        )
        .with_state(HttpState::new(relay));
    HttpRouter::new(build_router_with_routes(
        public_routes,
        operations_router(readiness),
        config,
        DEFAULT_REQUEST_BODY_LIMIT_BYTES,
    ))
}

pub(crate) fn build_router_with_routes(
    public_routes: Router,
    operations_routes: Router,
    config: &ServerConfig,
    body_limit: usize,
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

    Router::new()
        .merge(public_routes)
        .merge(operations_routes)
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
