use std::sync::Arc;

use af_admin::TokenAuthentication;
use af_domain::AfError;
use af_protocol::openai_chat::parse_request;
use af_relay::RelayService;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use http::{StatusCode, header::CONTENT_TYPE};

use crate::OpenAiHttpError;

/// HTTP 层持有的框架私有服务状态。
#[derive(Clone)]
pub(crate) struct HttpState {
    relay: Arc<RelayService>,
}

impl HttpState {
    pub(crate) fn new(relay: Arc<RelayService>) -> Self {
        Self { relay }
    }
}

/// 解析受限正文、执行请求级令牌策略，再把 Canonical 请求交给 Relay。
pub(crate) async fn chat_completions(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let canonical =
        parse_request(&body).map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    if !authentication.model_policy().allows(&canonical.model) {
        return Err(OpenAiHttpError::from(AfError::ModelNotAllowed));
    }
    let principal = authentication.principal();
    let response = state
        .relay
        .chat_completions(&principal, canonical, request_id.as_str())
        .await
        .map_err(OpenAiHttpError::from)?;
    Ok((
        StatusCode::OK,
        [(CONTENT_TYPE, "application/json")],
        response.into_body(),
    )
        .into_response())
}
