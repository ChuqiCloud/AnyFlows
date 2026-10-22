use af_admin::GatewayModelAuthentication;
use af_domain::{AfError, Protocol};
use af_protocol::anthropic::parse_request_envelope;
use af_relay::RelayDiagnosticInput;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::Response,
};
use http::HeaderMap;

use crate::{
    chat_completions::{HttpState, execute_chat_response, prepare_model_request},
    error_response::AnthropicHttpError,
};

/// 解析 Anthropic Messages 请求并经 Canonical Chat 执行边界转发。
pub(crate) async fn anthropic_messages(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<GatewayModelAuthentication>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AnthropicHttpError> {
    let diagnostic = RelayDiagnosticInput::capture("POST", "/v1/messages", &headers, &body);
    let request = parse_request_envelope(body)
        .map_err(|_| AnthropicHttpError::from(AfError::InvalidRequest))?;
    let request =
        prepare_model_request(request, &authentication).map_err(AnthropicHttpError::from)?;
    execute_chat_response(
        &state,
        authentication,
        request,
        Protocol::Anthropic,
        request_id,
        diagnostic,
    )
    .await
    .map_err(AnthropicHttpError::from)
}
