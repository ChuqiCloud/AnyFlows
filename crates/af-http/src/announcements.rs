use std::sync::Arc;

use af_admin::{
    AnnouncementAudience, AnnouncementService, AnnouncementServiceError, AnnouncementStatus,
    AnnouncementView, AnnouncementWriteCommand, SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Router,
    extract::{Extension, Json, Path, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct AnnouncementHttpState {
    service: Arc<dyn AnnouncementService>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = Announcement)]
pub(crate) struct AnnouncementResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(pattern = "^(draft|published|revoked)$")]
    status: &'static str,
    audience: AnnouncementAudienceResponse,
    #[schema(min_length = 1, max_length = 160)]
    title_zh: String,
    #[schema(min_length = 1, max_length = 160)]
    title_en: String,
    #[schema(min_length = 1, max_length = 8192)]
    body_zh: String,
    #[schema(min_length = 1, max_length = 8192)]
    body_en: String,
    visible_from: Option<i64>,
    visible_until: Option<i64>,
    #[schema(minimum = 1)]
    created_by: i64,
    published_at: Option<i64>,
    revoked_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AnnouncementListResponse)]
pub(crate) struct AnnouncementListResponse {
    entries: Vec<AnnouncementResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AnnouncementWriteRequest)]
pub(crate) struct AnnouncementWriteRequest {
    #[serde(default)]
    audience: Option<AnnouncementAudienceRequest>,
    #[schema(min_length = 1, max_length = 160)]
    title_zh: String,
    #[schema(min_length = 1, max_length = 160)]
    title_en: String,
    #[schema(min_length = 1, max_length = 8192)]
    body_zh: String,
    #[schema(min_length = 1, max_length = 8192)]
    body_en: String,
    visible_from: Option<i64>,
    visible_until: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AnnouncementMutationRequest)]
pub(crate) struct AnnouncementMutationRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AnnouncementAudience)]
pub(crate) enum AnnouncementAudienceResponse {
    Public,
    Authenticated,
}

#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AnnouncementAudienceRequest)]
pub(crate) enum AnnouncementAudienceRequest {
    Public,
    Authenticated,
}

impl AnnouncementAudienceRequest {
    fn into_domain(self) -> AnnouncementAudience {
        match self {
            Self::Public => AnnouncementAudience::Public,
            Self::Authenticated => AnnouncementAudience::Authenticated,
        }
    }
}

/// 构建公告公开读取与管理员版本化管理路由。
pub(crate) fn build_announcement_router(
    service: Arc<dyn AnnouncementService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/announcements", get(list_public_announcements))
        .route(
            "/api/admin/announcements",
            get(list_admin_announcements)
                .post(create_admin_announcement)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication.clone()),
        )
        .route(
            "/api/admin/announcements/{id}",
            put(update_admin_announcement)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication.clone()),
        )
        .route(
            "/api/admin/announcements/{id}/publish",
            post(publish_admin_announcement)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication.clone()),
        )
        .route(
            "/api/admin/announcements/{id}/revoke",
            post(revoke_admin_announcement)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication),
        )
        .with_state(AnnouncementHttpState { service })
}

async fn list_public_announcements(
    State(state): State<AnnouncementHttpState>,
) -> Result<Response, ManagementError> {
    let entries = state.service.list_public().await.map_err(map_error)?;
    Ok(no_store_json(AnnouncementListResponse::from_views(entries)))
}

async fn list_admin_announcements(
    State(state): State<AnnouncementHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let entries = state
        .service
        .list_admin(authentication.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AnnouncementListResponse::from_views(entries)))
}

async fn create_admin_announcement(
    State(state): State<AnnouncementHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AnnouncementWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = request.into_command()?;
    let result = state
        .service
        .create(authentication.principal(), command)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AnnouncementResponse::from_view(result)))
}

async fn update_admin_announcement(
    State(state): State<AnnouncementHttpState>,
    Path(id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AnnouncementUpdateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AnnouncementWriteCommand::new_with_audience(
        request
            .audience
            .map(AnnouncementAudienceRequest::into_domain)
            .unwrap_or(AnnouncementAudience::Public),
        request.title_zh.clone(),
        request.title_en.clone(),
        request.body_zh.clone(),
        request.body_en.clone(),
        request.visible_from,
        request.visible_until,
    )
    .map_err(map_error)?;
    let result = state
        .service
        .update_draft(
            authentication.principal(),
            id,
            request.expected_version,
            command,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AnnouncementResponse::from_view(result)))
}

async fn publish_admin_announcement(
    State(state): State<AnnouncementHttpState>,
    Path(id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AnnouncementMutationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let result = state
        .service
        .publish(authentication.principal(), id, request.expected_version)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AnnouncementResponse::from_view(result)))
}

async fn revoke_admin_announcement(
    State(state): State<AnnouncementHttpState>,
    Path(id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AnnouncementMutationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let result = state
        .service
        .revoke(authentication.principal(), id, request.expected_version)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AnnouncementResponse::from_view(result)))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnnouncementUpdateRequest {
    #[serde(default)]
    audience: Option<AnnouncementAudienceRequest>,
    #[schema(min_length = 1, max_length = 160)]
    title_zh: String,
    #[schema(min_length = 1, max_length = 160)]
    title_en: String,
    #[schema(min_length = 1, max_length = 8192)]
    body_zh: String,
    #[schema(min_length = 1, max_length = 8192)]
    body_en: String,
    visible_from: Option<i64>,
    visible_until: Option<i64>,
    #[schema(minimum = 1)]
    expected_version: i64,
}

impl AnnouncementWriteRequest {
    fn into_command(self) -> Result<AnnouncementWriteCommand, ManagementError> {
        AnnouncementWriteCommand::new_with_audience(
            self.audience
                .map(AnnouncementAudienceRequest::into_domain)
                .unwrap_or(AnnouncementAudience::Public),
            self.title_zh,
            self.title_en,
            self.body_zh,
            self.body_en,
            self.visible_from,
            self.visible_until,
        )
        .map_err(map_error)
    }
}

impl AnnouncementResponse {
    fn from_view(view: AnnouncementView) -> Self {
        let status = match view.status() {
            AnnouncementStatus::Draft => "draft",
            AnnouncementStatus::Published => "published",
            AnnouncementStatus::Revoked => "revoked",
        };
        let audience = match view.audience() {
            AnnouncementAudience::Public => AnnouncementAudienceResponse::Public,
            AnnouncementAudience::Authenticated => AnnouncementAudienceResponse::Authenticated,
        };
        Self {
            id: view.id(),
            version: view.version(),
            status,
            audience,
            title_zh: view.title_zh().to_owned(),
            title_en: view.title_en().to_owned(),
            body_zh: view.body_zh().to_owned(),
            body_en: view.body_en().to_owned(),
            visible_from: view.visible_from(),
            visible_until: view.visible_until(),
            created_by: view.created_by(),
            published_at: view.published_at(),
            revoked_at: view.revoked_at(),
            created_at: view.created_at(),
            updated_at: view.updated_at(),
        }
    }
}

impl AnnouncementListResponse {
    fn from_views(views: Vec<AnnouncementView>) -> Self {
        Self {
            entries: views
                .into_iter()
                .map(AnnouncementResponse::from_view)
                .collect(),
        }
    }
}

fn map_error(error: AnnouncementServiceError) -> ManagementError {
    match error {
        AnnouncementServiceError::InvalidRequest => ManagementError::AnnouncementInvalidRequest,
        AnnouncementServiceError::Forbidden => ManagementError::Forbidden,
        AnnouncementServiceError::NotFound => ManagementError::AnnouncementNotFound,
        AnnouncementServiceError::Conflict => ManagementError::AnnouncementConflict,
        AnnouncementServiceError::Internal => ManagementError::Internal,
    }
}
