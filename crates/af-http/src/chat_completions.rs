use std::sync::Arc;

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

/// 接收原始受限字节并交给协议感知 relay，保留重复键与结构预算检查。
pub(crate) async fn chat_completions(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let response = state
        .relay
        .chat_completions(body, request_id.as_str())
        .await
        .map_err(OpenAiHttpError::from)?;
    Ok((
        StatusCode::OK,
        [(CONTENT_TYPE, "application/json")],
        response.into_body(),
    )
        .into_response())
}
