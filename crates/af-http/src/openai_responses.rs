use af_admin::{GatewayModelAuthentication, PluginNativeTaskProtocol};
use af_domain::{AfError, Protocol};
use af_protocol::openai_responses::parse_request_envelope;
use af_relay::RelayDiagnosticInput;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, MatchedPath, State},
    response::Response,
};
use http::HeaderMap;

use crate::{
    chat_completions::{HttpState, execute_chat_response, prepare_model_request},
    error_response::OpenAiResponsesHttpError,
    plugin_native_tasks::{self, NativeTaskService},
};

/// 解析 OpenAI Responses 请求并经原生 Responses 生产链路转发。
pub(crate) async fn openai_responses(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<GatewayModelAuthentication>,
    plugin_tasks: Option<Extension<NativeTaskService>>,
    path: MatchedPath,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, OpenAiResponsesHttpError> {
    if path.as_str() == "/v1/responses"
        && let (Some(Extension(service)), GatewayModelAuthentication::User(auth)) =
            (plugin_tasks, &authentication)
        && let Some(response) = plugin_native_tasks::submit(
            &service,
            auth,
            PluginNativeTaskProtocol::Responses,
            &headers,
            &body,
            &request_id,
        )
        .await?
    {
        return Ok(response);
    }
    let diagnostic = RelayDiagnosticInput::capture("POST", "/v1/responses", &headers, &body);
    let request = parse_request_envelope(body)
        .map_err(|_| OpenAiResponsesHttpError::from(AfError::InvalidRequest))?;
    let request =
        prepare_model_request(request, &authentication).map_err(OpenAiResponsesHttpError::from)?;
    execute_chat_response(
        &state,
        authentication,
        request,
        Protocol::OpenAiResponses,
        request_id,
        diagnostic,
    )
    .await
    .map_err(OpenAiResponsesHttpError::from)
}
