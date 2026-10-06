use af_admin::TokenAuthentication;
use af_domain::AfError;
use af_protocol::openai_responses_compact::parse_request;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use http::{StatusCode, header::CONTENT_TYPE};

use crate::{chat_completions::HttpState, error_response::OpenAiResponsesHttpError};

/// 解析独立 Responses Compact 请求并经专用调度与计费链路转发。
pub(crate) async fn openai_responses_compact(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    body: Bytes,
) -> Result<Response, OpenAiResponsesHttpError> {
    let request = parse_request(&body)
        .map_err(|_| OpenAiResponsesHttpError::from(AfError::InvalidRequest))?;
    if !authentication.model_policy().allows(request.model()) {
        return Err(OpenAiResponsesHttpError::from(AfError::ModelNotAllowed));
    }
    let service = state
        .responses_compact_service()
        .ok_or_else(|| OpenAiResponsesHttpError::from(AfError::Internal))?;
    let principal = authentication.principal();
    let response = service
        .compact(
            &principal,
            authentication.user_concurrency(),
            request,
            request_id.as_str(),
        )
        .await
        .map_err(OpenAiResponsesHttpError::from)?;
    let (body, _usage) = response.into_parts();
    Ok((StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response())
}
