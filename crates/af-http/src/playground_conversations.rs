use std::sync::Arc;

use af_admin::{
    PlaygroundConversationError, PlaygroundConversationService, SessionAuthentication,
    SessionAuthenticator,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

mod dto;

pub(crate) use dto::{
    PlaygroundConversationListResponse, PlaygroundConversationResponse,
    PlaygroundConversationSaveRequest, PlaygroundConversationSummaryDto,
};

/// Playground 私有会话路由独立持有的应用服务状态。
#[derive(Clone)]
pub(crate) struct PlaygroundConversationHttpState {
    service: Arc<dyn PlaygroundConversationService>,
}

impl PlaygroundConversationHttpState {
    /// 绑定启动期装配的私有历史应用服务。
    pub(crate) fn new(service: Arc<dyn PlaygroundConversationService>) -> Self {
        Self { service }
    }
}

/// 构建统一登录鉴权且使用专属正文上限的私有历史路由。
pub(crate) fn build_playground_conversation_router(
    service: Arc<dyn PlaygroundConversationService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/playground/conversations", get(list_conversations))
        .route(
            "/api/playground/conversations/{conversation_id}",
            get(get_conversation)
                .put(save_conversation)
                .delete(delete_conversation),
        )
        // JSON 结构开销高于 512 KiB 快照上限，但不允许使用全局 32 MiB 预算。
        .layer(DefaultBodyLimit::max(640 * 1024))
        .layer(RequestBodyLimitLayer::new(640 * 1024))
        .layer(authentication)
        .with_state(PlaygroundConversationHttpState::new(service))
}

/// 列出当前登录用户的全部有界摘要。
pub(crate) async fn list_conversations(
    State(state): State<PlaygroundConversationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let summaries = state
        .service
        .list(authentication.principal())
        .await
        .map_err(map_conversation_error)?;
    Ok(no_store_json(
        PlaygroundConversationListResponse::from_application(summaries),
    ))
}

/// 幂等创建或按 revision 更新当前用户的会话。
pub(crate) async fn save_conversation(
    State(state): State<PlaygroundConversationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(conversation_id): Path<String>,
    request: Result<Json<PlaygroundConversationSaveRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return Ok(rejection.into_response());
        }
        Err(_) => return Err(ManagementError::InvalidRequest),
    };
    let command = request
        .into_command(conversation_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let conversation = state
        .service
        .save(authentication.principal(), command)
        .await
        .map_err(map_conversation_error)?;
    Ok(no_store_json(
        PlaygroundConversationResponse::from_application(conversation),
    ))
}

/// 读取当前用户拥有的一份完整会话。
pub(crate) async fn get_conversation(
    State(state): State<PlaygroundConversationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(conversation_id): Path<String>,
) -> Result<Response, ManagementError> {
    let conversation = state
        .service
        .read(authentication.principal(), conversation_id)
        .await
        .map_err(map_conversation_error)?;
    Ok(no_store_json(
        PlaygroundConversationResponse::from_application(conversation),
    ))
}

/// 删除当前用户拥有的一份会话。
pub(crate) async fn delete_conversation(
    State(state): State<PlaygroundConversationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(conversation_id): Path<String>,
) -> Result<Response, ManagementError> {
    state
        .service
        .delete(authentication.principal(), conversation_id)
        .await
        .map_err(map_conversation_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn map_conversation_error(error: PlaygroundConversationError) -> ManagementError {
    match error {
        PlaygroundConversationError::InvalidInput => ManagementError::InvalidRequest,
        PlaygroundConversationError::InvalidSession => ManagementError::InvalidSession,
        PlaygroundConversationError::LimitReached => {
            ManagementError::PlaygroundConversationLimitReached
        }
        PlaygroundConversationError::NotFound => ManagementError::PlaygroundConversationNotFound,
        PlaygroundConversationError::Conflict => ManagementError::PlaygroundConversationConflict,
        PlaygroundConversationError::Internal => ManagementError::Internal,
    }
}
