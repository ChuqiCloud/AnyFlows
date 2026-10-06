use af_admin::TokenAuthentication;
use af_domain::AfError;
use af_protocol::rerank_v1::parse_request;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use http::{StatusCode, header::CONTENT_TYPE};

use crate::{OpenAiHttpError, chat_completions::HttpState};

/// 解析通用 Rerank 请求并经独立生产链路转发。
pub(crate) async fn rerank(
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
        .rerank_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let principal = authentication.principal();
    let response = service
        .rerank(
            &principal,
            authentication.user_concurrency(),
            request,
            request_id.as_str(),
        )
        .await
        .map_err(OpenAiHttpError::from)?;
    let (body, _usage) = response.into_parts();
    Ok((StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response())
}
