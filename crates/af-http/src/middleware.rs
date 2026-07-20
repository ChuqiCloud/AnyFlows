use std::{sync::Arc, time::Duration};

use af_config::CorsOrigin;
use af_telemetry::{RequestId, request_span};
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::{
    HeaderName, HeaderValue, Request as HttpRequest, Response as HttpResponse, StatusCode,
    header::ORIGIN,
};
use tower_http::trace::{MakeSpan, OnResponse};
use tracing::Span;
use uuid::Uuid;

/// AnyFlows 对外返回 canonical 请求标识所用的响应头。
pub const REQUEST_ID_HEADER_NAME: &str = "x-request-id";

/// 业务路由允许的精确 Origin 集合；空集合拒绝所有携带 Origin 的请求。
#[derive(Clone)]
pub(crate) struct AllowedOrigins(Arc<[CorsOrigin]>);

impl AllowedOrigins {
    pub(crate) fn new(origins: &[CorsOrigin]) -> Self {
        Self(Arc::from(origins))
    }

    fn contains(&self, origin: &CorsOrigin) -> bool {
        self.0.contains(origin)
    }
}

pub(crate) fn request_id_header() -> HeaderName {
    HeaderName::from_static(REQUEST_ID_HEADER_NAME)
}

/// 为每个请求生成服务端可信 ID，并覆盖请求与响应中的同名不受信值。
pub(crate) async fn assign_request_id(mut request: Request, next: Next) -> Response {
    let request_id = server_request_id();
    request.headers_mut().remove(request_id_header());
    request.extensions_mut().insert(request_id.clone());

    let mut response = next.run(request).await;
    let header_value = HeaderValue::from_str(request_id.as_str())
        .expect("RequestId 契约必须始终满足 HTTP HeaderValue 约束");
    response
        .headers_mut()
        .insert(request_id_header(), header_value);
    response
}

/// 在 CORS 层执行前拒绝不受信 Origin，避免跨域副作用请求进入 handler。
pub(crate) async fn enforce_allowed_origin(
    State(allowed): State<AllowedOrigins>,
    request: Request,
    next: Next,
) -> Response {
    if !request_origin_is_allowed(&request, &allowed) {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

fn request_origin_is_allowed(request: &Request, allowed: &AllowedOrigins) -> bool {
    let mut values = request.headers().get_all(ORIGIN).iter();
    let Some(value) = values.next() else {
        return true;
    };
    if values.next().is_some() {
        return false;
    }
    value
        .to_str()
        .ok()
        .and_then(|value| value.parse::<CorsOrigin>().ok())
        .is_some_and(|origin| allowed.contains(&origin))
}

fn server_request_id() -> RequestId {
    RequestId::new(Uuid::new_v4().to_string())
        .expect("UUID v4 文本必须始终满足 canonical RequestId 约束")
}

/// 为 TraceLayer 创建 canonical 请求根 span 与安全 HTTP 子 span。
#[derive(Clone, Copy, Debug)]
pub(crate) struct MakeHttpSpan;

impl<B> MakeSpan<B> for MakeHttpSpan {
    fn make_span(&mut self, request: &HttpRequest<B>) -> Span {
        let Some(request_id) = request.extensions().get::<RequestId>() else {
            return Span::none();
        };
        let root = request_span(request_id);
        let route = request
            .extensions()
            .get::<MatchedPath>()
            .map_or("<unmatched>", MatchedPath::as_str);
        tracing::info_span!(
            target: "af_http::middleware",
            parent: &root,
            "http_request",
            http_method = request.method().as_str(),
            route
        )
    }
}

/// 只记录稳定响应元数据；TraceLayer 会让该 span 继续覆盖响应 body 轮询。
#[derive(Clone, Copy, Debug)]
pub(crate) struct LogHttpResponse;

impl<B> OnResponse<B> for LogHttpResponse {
    fn on_response(self, response: &HttpResponse<B>, latency: Duration, span: &Span) {
        tracing::info!(
            target: "af_http::middleware",
            parent: span,
            status_code = u64::from(response.status().as_u16()),
            response_latency_ms = duration_millis(latency),
            "HTTP 请求完成"
        );
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
