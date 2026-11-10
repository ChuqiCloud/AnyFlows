use std::sync::Arc;

use af_admin::{AdminUpstreamUsageService, SessionAuthenticator};
use af_domain::{
    ChannelId, CredentialId, UpstreamUsageConfig, UpstreamUsageError, UpstreamUsageProfileWrite,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::StatusCode;

use crate::{
    AdminCredentialUsageProbe,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_session::{no_store_empty, no_store_json},
};

#[derive(Clone)]
struct UsageState {
    service: Arc<dyn AdminUpstreamUsageService>,
    probe: Arc<dyn AdminCredentialUsageProbe>,
}

pub fn build_upstream_usage_router(
    service: Arc<dyn AdminUpstreamUsageService>,
    probe: Arc<dyn AdminCredentialUsageProbe>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route(
            "/api/admin/channels/{channel_id}/usage-config",
            get(channel_config).put(save_channel_config),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage-config",
            get(config).put(save_config).delete(reset_config),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage/refresh",
            axum::routing::post(refresh),
        )
        .route(
            "/api/admin/upstream-usage-profiles",
            get(profiles).post(create_profile),
        )
        .route(
            "/api/admin/upstream-usage-profiles/{id}",
            axum::routing::put(update_profile).delete(delete_profile),
        )
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn(
            crate::management_authorization::authorize_management_admin,
        ))
        .layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(UsageState { service, probe })
}

struct UsageError(UpstreamUsageError);
impl From<UpstreamUsageError> for UsageError {
    fn from(value: UpstreamUsageError) -> Self {
        Self(value)
    }
}
impl IntoResponse for UsageError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            UpstreamUsageError::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
            UpstreamUsageError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            UpstreamUsageError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            UpstreamUsageError::Conflict => (StatusCode::CONFLICT, "usage_config_conflict"),
            UpstreamUsageError::InUse => (StatusCode::CONFLICT, "usage_profile_in_use"),
            UpstreamUsageError::Unavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "usage_unavailable")
            }
        };
        let mut response = no_store_json(
            serde_json::json!({"code":code,"error":{"code":code,"message":self.0.to_string()}}),
        );
        *response.status_mut() = status;
        response
    }
}

fn ids(channel: i64, credential: i64) -> Result<(ChannelId, CredentialId), UsageError> {
    Ok((
        ChannelId::new(channel).map_err(|_| UpstreamUsageError::InvalidInput)?,
        CredentialId::new(credential).map_err(|_| UpstreamUsageError::InvalidInput)?,
    ))
}

async fn config(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path((channel, credential)): Path<(i64, i64)>,
) -> Result<Response, UsageError> {
    let (channel, credential) = ids(channel, credential)?;
    Ok(no_store_json(
        state
            .service
            .config(auth.principal(), channel, credential)
            .await?,
    ))
}

async fn save_config(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path((channel, credential)): Path<(i64, i64)>,
    body: Result<Json<UpstreamUsageConfig>, JsonRejection>,
) -> Result<Response, UsageError> {
    let (channel, credential) = ids(channel, credential)?;
    Ok(no_store_json(
        state
            .service
            .save_config(
                auth.principal(),
                channel,
                credential,
                body.map_err(|_| UpstreamUsageError::InvalidInput)?.0,
            )
            .await?,
    ))
}

async fn refresh(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path((channel, credential)): Path<(i64, i64)>,
) -> Result<Response, UsageError> {
    let (channel, credential) = ids(channel, credential)?;
    state
        .service
        .config(auth.principal(), channel, credential)
        .await?;
    Ok(no_store_json(
        state
            .probe
            .refresh(auth.principal(), channel, credential)
            .await,
    ))
}

async fn profiles(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, UsageError> {
    Ok(no_store_json(
        serde_json::json!({"items":state.service.profiles(auth.principal()).await?}),
    ))
}

async fn create_profile(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    body: Result<Json<UpstreamUsageProfileWrite>, JsonRejection>,
) -> Result<Response, UsageError> {
    let profile = state
        .service
        .save_profile(
            auth.principal(),
            None,
            body.map_err(|_| UpstreamUsageError::InvalidInput)?.0,
        )
        .await?;
    let mut response = no_store_json(profile);
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

async fn update_profile(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path(id): Path<i64>,
    body: Result<Json<UpstreamUsageProfileWrite>, JsonRejection>,
) -> Result<Response, UsageError> {
    if id <= 0 {
        return Err(UpstreamUsageError::InvalidInput.into());
    }
    Ok(no_store_json(
        state
            .service
            .save_profile(
                auth.principal(),
                Some(id),
                body.map_err(|_| UpstreamUsageError::InvalidInput)?.0,
            )
            .await?,
    ))
}

async fn channel_config(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path(channel): Path<i64>,
) -> Result<Response, UsageError> {
    let channel = ChannelId::new(channel).map_err(|_| UpstreamUsageError::InvalidInput)?;
    Ok(no_store_json(
        state
            .service
            .channel_config(auth.principal(), channel)
            .await?,
    ))
}

async fn save_channel_config(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path(channel): Path<i64>,
    body: Result<Json<UpstreamUsageConfig>, JsonRejection>,
) -> Result<Response, UsageError> {
    let channel = ChannelId::new(channel).map_err(|_| UpstreamUsageError::InvalidInput)?;
    Ok(no_store_json(
        state
            .service
            .save_channel_config(
                auth.principal(),
                channel,
                body.map_err(|_| UpstreamUsageError::InvalidInput)?.0,
            )
            .await?,
    ))
}

async fn reset_config(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path((channel, credential)): Path<(i64, i64)>,
) -> Result<Response, UsageError> {
    let (channel, credential) = ids(channel, credential)?;
    state
        .service
        .reset_config(auth.principal(), channel, credential)
        .await?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

async fn delete_profile(
    State(state): State<UsageState>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, UsageError> {
    if id <= 0 {
        return Err(UpstreamUsageError::InvalidInput.into());
    }
    state.service.delete_profile(auth.principal(), id).await?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}
