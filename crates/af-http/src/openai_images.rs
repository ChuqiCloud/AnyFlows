use af_admin::TokenAuthentication;
use af_domain::AfError;
use af_protocol::openai_images::{
    self, OpenAiImageEditForm, OpenAiImageEditFormPart, parse_request,
};
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, Multipart, State, multipart::MultipartRejection},
    response::{IntoResponse, Response},
};
use http::{StatusCode, header::CONTENT_TYPE};

use crate::{OpenAiHttpError, chat_completions::HttpState};

/// 解析 OpenAI Images 请求并经独立生产链路生成图片。
pub(crate) async fn openai_images(
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
        .image_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let principal = authentication.principal();
    let response = service
        .generate(
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

/// 解析 OpenAI Images multipart 编辑请求并经独立生产链路编辑图片。
pub(crate) async fn openai_image_edit(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<Response, OpenAiHttpError> {
    let multipart = multipart.map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    let request = parse_edit_multipart(multipart).await?;
    if !authentication.model_policy().allows(request.model()) {
        return Err(OpenAiHttpError::from(AfError::ModelNotAllowed));
    }
    let service = state
        .image_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let principal = authentication.principal();
    let response = service
        .edit(
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

async fn parse_edit_multipart(
    mut multipart: Multipart,
) -> Result<af_protocol::CanonicalImageEditRequest, OpenAiHttpError> {
    let mut parts = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?
    {
        let name = field
            .name()
            .ok_or_else(|| OpenAiHttpError::from(AfError::InvalidRequest))?
            .to_owned();
        let file_name = field.file_name().map(str::to_owned);
        let content_type = field.content_type().map(str::to_owned);
        let bytes = field
            .bytes()
            .await
            .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
        let part = if let Some(file_name) = file_name {
            OpenAiImageEditFormPart::file(name, file_name, content_type, bytes)
        } else {
            let value = std::str::from_utf8(&bytes)
                .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?
                .to_owned();
            OpenAiImageEditFormPart::text(name, value)
        }
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
        parts.push(part);
    }
    let form = OpenAiImageEditForm::new(parts)
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    openai_images::parse_edit_request(form)
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))
}
