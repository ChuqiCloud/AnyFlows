use std::sync::Arc;

use af_admin::{
    PlaygroundShareError, PlaygroundShareService, SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::{Extension, Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use http::{HeaderName, HeaderValue, StatusCode, header::CACHE_CONTROL};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

mod dto;

pub(crate) use dto::{
    PlaygroundShareCreateRequest, PlaygroundShareCreateResponse, PlaygroundShareMessageDto,
    PlaygroundShareMessageRoleDto, PlaygroundShareReadResponse, PlaygroundShareSessionDto,
};

const REFERRER_POLICY: HeaderName = HeaderName::from_static("referrer-policy");
const X_ROBOTS_TAG: HeaderName = HeaderName::from_static("x-robots-tag");

/// Playground 分享路由独立持有的应用服务状态。
#[derive(Clone)]
pub(crate) struct PlaygroundShareHttpState {
    service: Arc<dyn PlaygroundShareService>,
}

impl PlaygroundShareHttpState {
    /// 绑定启动期装配的分享应用服务。
    pub(crate) fn new(service: Arc<dyn PlaygroundShareService>) -> Self {
        Self { service }
    }
}

/// 构建分享专属路由，使公开读取不会继承登录中间件。
pub(crate) fn build_playground_share_router(
    service: Arc<dyn PlaygroundShareService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let create_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let revoke_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    let create_routes = Router::new()
        .route("/api/playground/shares", post(create_playground_share))
        // JSON 结构开销高于 512 KiB 快照上限，但不允许退回全局 32 MiB 预算。
        .layer(DefaultBodyLimit::max(640 * 1024))
        .layer(RequestBodyLimitLayer::new(640 * 1024))
        .layer(create_authentication);
    let revoke_routes = Router::new()
        .route(
            "/api/playground/shares/{token}",
            delete(revoke_playground_share),
        )
        .layer(revoke_authentication);
    Router::new()
        .route("/api/playground/shares/{token}", get(get_playground_share))
        .merge(create_routes)
        .merge(revoke_routes)
        .with_state(PlaygroundShareHttpState::new(service))
}

/// 为当前登录用户创建只返回一次令牌的只读分享。
pub(crate) async fn create_playground_share(
    State(state): State<PlaygroundShareHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<PlaygroundShareCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return Ok(rejection.into_response());
        }
        Err(_) => return Err(ManagementError::InvalidRequest),
    };
    let command = request
        .into_command()
        .map_err(|_| ManagementError::InvalidRequest)?;
    let issued = state
        .service
        .create(authentication.principal(), command)
        .await
        .map_err(map_share_error)?;
    Ok(no_store_json(PlaygroundShareCreateResponse::from_issued(
        issued,
    )))
}

/// 公开读取有效分享，并为成功与错误响应统一加固缓存和索引策略。
pub(crate) async fn get_playground_share(
    State(state): State<PlaygroundShareHttpState>,
    Path(token): Path<String>,
) -> Response {
    let response = match state.service.read(token).await {
        Ok(view) => no_store_json(PlaygroundShareReadResponse::from_view(view)),
        Err(error) => map_share_error(error).into_response(),
    };
    public_share_response(response)
}

/// 仅允许创建者撤销仍有效的分享。
pub(crate) async fn revoke_playground_share(
    State(state): State<PlaygroundShareHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(token): Path<String>,
) -> Result<Response, ManagementError> {
    state
        .service
        .revoke(authentication.principal(), token)
        .await
        .map_err(map_share_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn map_share_error(error: PlaygroundShareError) -> ManagementError {
    match error {
        PlaygroundShareError::InvalidInput => ManagementError::InvalidRequest,
        PlaygroundShareError::InvalidSession => ManagementError::InvalidSession,
        PlaygroundShareError::LimitReached => ManagementError::PlaygroundShareLimitReached,
        PlaygroundShareError::NotFound => ManagementError::PlaygroundShareNotFound,
        PlaygroundShareError::Internal => ManagementError::Internal,
    }
}

fn public_share_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
        .headers_mut()
        .insert(X_ROBOTS_TAG, HeaderValue::from_static("noindex"));
    response
}
