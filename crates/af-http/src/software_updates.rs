//! Administrator-only software identity and explicitly requested update actions.

use std::{future::Future, pin::Pin, sync::Arc};

use af_admin::SessionAuthenticator;
use axum::{
    Json, Router,
    extract::State,
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_session::no_store_json,
};

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SoftwareRelease {
    pub version: String,
    pub tag: String,
    pub url: String,
    pub notes: String,
    pub published_at: Option<String>,
    pub package_name: Option<String>,
    pub package_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SoftwareStatus {
    pub edition: String,
    pub version: String,
    pub core_version: String,
    pub commit: Option<String>,
    pub build_time: Option<String>,
    pub target: String,
    pub deployment: String,
    pub node_id: String,
    /// direct or launcher; no launcher is required for direct binary deployment.
    pub launch_mode: String,
    pub launcher_protocol: Option<u32>,
    pub launcher_version: Option<String>,
    pub update_phase: String,
    pub launcher_error: Option<String>,
    pub check_enabled: bool,
    pub install_enabled: bool,
    pub install_blocked_reason: Option<String>,
    pub restart_required: bool,
    pub checked_at: Option<i64>,
    pub latest: Option<SoftwareRelease>,
    pub update_available: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallSoftwareUpdate {
    pub tag: String,
    pub expected_node_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SoftwareUpdateErrorBody {
    pub code: String,
}

#[derive(Clone, Copy, Debug)]
pub enum SoftwareUpdateError {
    Disabled,
    Unavailable,
    InvalidRelease,
    Conflict,
    InstallFailed,
}

impl IntoResponse for SoftwareUpdateError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Disabled => (StatusCode::FORBIDDEN, "update_disabled"),
            Self::Unavailable => (StatusCode::BAD_GATEWAY, "release_unavailable"),
            Self::InvalidRelease => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_release"),
            Self::Conflict => (StatusCode::CONFLICT, "update_conflict"),
            Self::InstallFailed => (StatusCode::INTERNAL_SERVER_ERROR, "install_failed"),
        };
        (
            status,
            no_store_json(SoftwareUpdateErrorBody {
                code: code.to_owned(),
            }),
        )
            .into_response()
    }
}

pub type SoftwareUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SoftwareStatus, SoftwareUpdateError>> + Send + 'a>>;

/// Hosts may supply their own identity and disable online access completely.
pub trait SoftwareUpdateService: Send + Sync {
    fn status(&self) -> SoftwareUpdateFuture<'_>;
    fn check(&self) -> SoftwareUpdateFuture<'_>;
    fn install(&self, tag: String, expected_node_id: String) -> SoftwareUpdateFuture<'_>;
}

pub fn build_software_update_router(
    service: Arc<dyn SoftwareUpdateService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route("/api/admin/software", get(status))
        .route("/api/admin/software/check", post(check))
        .route("/api/admin/software/install", post(install))
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(service)
}

async fn status(
    State(service): State<Arc<dyn SoftwareUpdateService>>,
) -> Result<Response, SoftwareUpdateError> {
    Ok(no_store_json(service.status().await?))
}

async fn check(
    State(service): State<Arc<dyn SoftwareUpdateService>>,
) -> Result<Response, SoftwareUpdateError> {
    Ok(no_store_json(service.check().await?))
}

async fn install(
    State(service): State<Arc<dyn SoftwareUpdateService>>,
    Json(request): Json<InstallSoftwareUpdate>,
) -> Result<Response, SoftwareUpdateError> {
    Ok(no_store_json(
        service
            .install(request.tag, request.expected_node_id)
            .await?,
    ))
}

#[cfg(test)]
mod tests;
