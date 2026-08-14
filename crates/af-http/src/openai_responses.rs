use af_admin::TokenAuthentication;
use af_domain::{AfError, Protocol};
use af_protocol::openai_responses::parse_request_envelope;
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
    error_response::OpenAiResponsesHttpError,
};

/// 解析 OpenAI Responses 请求并经原生 Responses 生产链路转发。
pub(crate) async fn openai_responses(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, OpenAiResponsesHttpError> {
    let diagnostic = RelayDiagnosticInput::capture("POST", "/v1/responses", &headers, &body);
    let request = parse_request_envelope(body)
        .map_err(|_| OpenAiResponsesHttpError::from(AfError::InvalidRequest))?;
    let request = prepare_canonical_request(request, authentication.model_policy())
        .map_err(OpenAiResponsesHttpError::from)?;
    let principal = authentication.principal();
    let response = state
        .chat_service()
        .chat_completions(
            &principal,
            authentication.user_concurrency(),
            request,
            Protocol::OpenAiResponses,
            request_id.as_str(),
            diagnostic,
        )
        .await
        .map_err(OpenAiResponsesHttpError::from)?;
    Ok(render_chat_response(response, state.stream_delivery()))
}
