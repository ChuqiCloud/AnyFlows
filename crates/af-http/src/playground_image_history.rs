use std::sync::Arc;

use af_admin::{
    ImageHistoryError, ImageHistoryService, ImageHistoryUpload, MAX_IMAGE_HISTORY_IMAGE_BYTES,
    MAX_IMAGE_HISTORY_THUMBNAIL_BYTES, SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Extension, Multipart, Path, State, multipart::MultipartRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

mod dto;

pub(crate) use dto::{ImageHistoryItemDto, ImageHistoryListResponse};

const IMAGE_HISTORY_REQUEST_LIMIT: usize =
    MAX_IMAGE_HISTORY_IMAGE_BYTES + MAX_IMAGE_HISTORY_THUMBNAIL_BYTES + 256 * 1024;

#[derive(Clone)]
struct ImageHistoryHttpState {
    service: Arc<dyn ImageHistoryService>,
}

/// Build private image-history routes with a dedicated bounded upload body.
pub(crate) fn build_playground_image_history_router(
    service: Arc<dyn ImageHistoryService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/playground/image-history",
            get(list_images).post(save_image).delete(clear_images),
        )
        .route(
            "/api/playground/image-history/{image_id}",
            delete(delete_image),
        )
        .route(
            "/api/playground/image-history/{image_id}/image",
            get(read_image),
        )
        .route(
            "/api/playground/image-history/{image_id}/thumbnail",
            get(read_thumbnail),
        )
        .layer(DefaultBodyLimit::max(IMAGE_HISTORY_REQUEST_LIMIT))
        .layer(RequestBodyLimitLayer::new(IMAGE_HISTORY_REQUEST_LIMIT))
        .layer(authentication)
        .with_state(ImageHistoryHttpState { service })
}

async fn list_images(
    State(state): State<ImageHistoryHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let entries = state
        .service
        .list(authentication.principal())
        .await
        .map_err(map_image_history_error)?;
    Ok(no_store_json(ImageHistoryListResponse::from_application(
        entries,
    )))
}

async fn save_image(
    State(state): State<ImageHistoryHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<Response, ManagementError> {
    let multipart = match multipart {
        Ok(multipart) => multipart,
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return Ok(rejection.into_response());
        }
        Err(_) => return Err(ManagementError::InvalidRequest),
    };
    let upload = parse_upload(multipart).await?;
    let entry = state
        .service
        .save(authentication.principal(), upload)
        .await
        .map_err(map_image_history_error)?;
    Ok(no_store_json(ImageHistoryItemDto::from_application(entry)))
}

async fn read_image(
    State(state): State<ImageHistoryHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(image_id): Path<String>,
) -> Result<Response, ManagementError> {
    read_image_content(state.service, authentication, image_id, false).await
}

async fn read_thumbnail(
    State(state): State<ImageHistoryHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(image_id): Path<String>,
) -> Result<Response, ManagementError> {
    read_image_content(state.service, authentication, image_id, true).await
}

async fn read_image_content(
    service: Arc<dyn ImageHistoryService>,
    authentication: SessionAuthentication,
    image_id: String,
    thumbnail: bool,
) -> Result<Response, ManagementError> {
    let content = service
        .read(authentication.principal(), image_id, thumbnail)
        .await
        .map_err(map_image_history_error)?;
    let (format, bytes) = content.into_parts();
    let content_type = image_content_type(&format).ok_or(ManagementError::Internal)?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

async fn delete_image(
    State(state): State<ImageHistoryHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(image_id): Path<String>,
) -> Result<Response, ManagementError> {
    state
        .service
        .delete(authentication.principal(), image_id)
        .await
        .map_err(map_image_history_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

async fn clear_images(
    State(state): State<ImageHistoryHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    state
        .service
        .clear(authentication.principal())
        .await
        .map_err(map_image_history_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn parse_upload(mut multipart: Multipart) -> Result<ImageHistoryUpload, ManagementError> {
    let mut model = None;
    let mut prompt = None;
    let mut revised_prompt = None;
    let mut request_id = None;
    let mut image_bytes = None;
    let mut image_url = None;
    let mut thumbnail_bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ManagementError::InvalidRequest)?
    {
        let name = field
            .name()
            .ok_or(ManagementError::InvalidRequest)?
            .to_owned();
        let bytes = field
            .bytes()
            .await
            .map_err(|_| ManagementError::InvalidRequest)?;
        match name.as_str() {
            "model" if model.is_none() => model = Some(read_text(&bytes)?),
            "prompt" if prompt.is_none() => prompt = Some(read_text(&bytes)?),
            "revised_prompt" if revised_prompt.is_none() => {
                revised_prompt = Some(read_text(&bytes)?)
            }
            "request_id" if request_id.is_none() => request_id = Some(read_text(&bytes)?),
            "image" if image_bytes.is_none() => image_bytes = Some(bytes.to_vec()),
            "image_url" if image_url.is_none() => image_url = Some(read_text(&bytes)?),
            "thumbnail" if thumbnail_bytes.is_none() => thumbnail_bytes = Some(bytes.to_vec()),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }

    ImageHistoryUpload::new(
        model.ok_or(ManagementError::InvalidRequest)?,
        prompt.ok_or(ManagementError::InvalidRequest)?,
        revised_prompt,
        request_id.filter(|value| !value.is_empty()),
        image_bytes,
        image_url,
        thumbnail_bytes,
    )
    .map_err(|_| ManagementError::InvalidRequest)
}

fn read_text(bytes: &[u8]) -> Result<String, ManagementError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| ManagementError::InvalidRequest)
}

fn image_content_type(format: &str) -> Option<&'static str> {
    match format {
        "png" => Some("image/png"),
        "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn map_image_history_error(error: ImageHistoryError) -> ManagementError {
    match error {
        ImageHistoryError::NotFound => ManagementError::ImageHistoryNotFound,
        ImageHistoryError::LimitReached => ManagementError::ImageHistoryLimitReached,
        ImageHistoryError::InvalidInput => ManagementError::InvalidRequest,
        ImageHistoryError::Unavailable => ManagementError::ImageHistoryUnavailable,
        ImageHistoryError::Internal => ManagementError::Internal,
    }
}
