use af_admin::TokenAuthentication;
use af_domain::AfError;
use af_protocol::openai_audio_speech::parse_request;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use http::{StatusCode, header::CONTENT_TYPE};

use crate::{OpenAiHttpError, chat_completions::HttpState};

/// 解析 OpenAI Audio Speech 请求并经独立生产链路返回完整二进制音频。
pub(crate) async fn openai_speech(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let request =
        parse_request(&body).map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    if !authentication.model_policy().allows(request.model()) {
        return Err(OpenAiHttpError::from(AfError::ModelNotAllowed));
    }
    let service = state
        .speech_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let principal = authentication.principal();
    let response = service
        .synthesize(
            &principal,
            authentication.user_concurrency(),
            request,
            request_id.as_str(),
        )
        .await
        .map_err(OpenAiHttpError::from)?;
    let (body, output_format) = response.into_parts();
    Ok((
        StatusCode::OK,
        [(CONTENT_TYPE, output_format.content_type())],
        body,
    )
        .into_response())
}
