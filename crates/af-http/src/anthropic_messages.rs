use af_admin::TokenAuthentication;
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
    chat_completions::{HttpState, prepare_canonical_request, render_chat_response},
    error_response::AnthropicHttpError,
};

/// 解析 Anthropic Messages 请求并经 Canonical Chat 执行边界转发。
pub(crate) async fn anthropic_messages(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AnthropicHttpError> {
    let diagnostic = RelayDiagnosticInput::capture("POST", "/v1/messages", &headers, &body);
    let request = parse_request_envelope(body)
        .map_err(|_| AnthropicHttpError::from(AfError::InvalidRequest))?;
    let request = prepare_canonical_request(request, authentication.model_policy())
        .map_err(AnthropicHttpError::from)?;
    let principal = authentication.principal();
    let response = state
        .chat_service()
        .chat_completions(
            &principal,
            authentication.user_concurrency(),
            request,
            Protocol::Anthropic,
            request_id.as_str(),
            diagnostic,
        )
        .await
        .map_err(AnthropicHttpError::from)?;
    Ok(render_chat_response(response, state.stream_delivery()))
}
