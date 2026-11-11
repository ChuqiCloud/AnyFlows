//! Admin configuration for explicitly opted-in channel health monitors.
use std::sync::Arc;

use af_admin::{
    ActiveChannelMonitorConfig, ActiveChannelMonitorEvent, ActiveChannelMonitorEventKind,
    ActiveChannelMonitorService, ActiveChannelMonitorServiceError, ActiveChannelMonitorTarget,
    ActiveChannelMonitorTargetInput, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{ChannelId, CredentialId};
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct ActiveChannelMonitorState {
    service: Arc<dyn ActiveChannelMonitorService>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActiveChannelMonitorSettingsRequest {
    pub enabled: bool,
    pub global_daily_budget: u32,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActiveChannelMonitorTargetRequest {
    pub channel_id: i64,
    pub credential_id: i64,
    pub model: String,
    pub enabled: bool,
    pub interval_secs: u32,
    pub daily_budget: u32,
    pub timeout_secs: u32,
    pub alert_after_secs: u32,
    pub muted_until_epoch_seconds: Option<i64>,
}

fn target_input(
    value: ActiveChannelMonitorTargetRequest,
) -> Result<ActiveChannelMonitorTargetInput, ManagementError> {
    Ok(ActiveChannelMonitorTargetInput {
        channel_id: ChannelId::new(value.channel_id)
            .map_err(|_| ManagementError::InvalidRequest)?,
        credential_id: CredentialId::new(value.credential_id)
            .map_err(|_| ManagementError::InvalidRequest)?,
        model: value.model,
        enabled: value.enabled,
        interval_secs: value.interval_secs,
        daily_budget: value.daily_budget,
        timeout_secs: value.timeout_secs,
        alert_after_secs: value.alert_after_secs,
        muted_until_epoch_seconds: value.muted_until_epoch_seconds,
    })
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorTargetResponse {
    pub id: i64,
    pub channel_id: i64,
    pub credential_id: i64,
    pub model: String,
    pub enabled: bool,
    pub interval_secs: u32,
    pub daily_budget: u32,
    pub timeout_secs: u32,
    pub alert_after_secs: u32,
    pub daily_used: u32,
    pub last_attempt_epoch_seconds: Option<i64>,
    pub last_success_epoch_seconds: Option<i64>,
    pub failure_since_epoch_seconds: Option<i64>,
    pub alert_active: bool,
    pub muted_until_epoch_seconds: Option<i64>,
}

impl From<ActiveChannelMonitorTarget> for ActiveChannelMonitorTargetResponse {
    fn from(v: ActiveChannelMonitorTarget) -> Self {
        Self {
            id: v.id,
            channel_id: v.channel_id.get(),
            credential_id: v.credential_id.get(),
            model: v.model,
            enabled: v.enabled,
            interval_secs: v.interval_secs,
            daily_budget: v.daily_budget,
            timeout_secs: v.timeout_secs,
            alert_after_secs: v.alert_after_secs,
            daily_used: v.daily_used,
            last_attempt_epoch_seconds: v.last_attempt_epoch_seconds,
            last_success_epoch_seconds: v.last_success_epoch_seconds,
            failure_since_epoch_seconds: v.failure_since_epoch_seconds,
            alert_active: v.alert_active,
            muted_until_epoch_seconds: v.muted_until_epoch_seconds,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActiveChannelMonitorEventKindResponse {
    Down,
    Recovered,
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorEventResponse {
    pub id: i64,
    pub target_id: i64,
    pub kind: ActiveChannelMonitorEventKindResponse,
    pub latency_ms: Option<i64>,
    pub occurred_epoch_seconds: i64,
}

impl From<ActiveChannelMonitorEvent> for ActiveChannelMonitorEventResponse {
    fn from(v: ActiveChannelMonitorEvent) -> Self {
        Self {
            id: v.id,
            target_id: v.target_id,
            kind: match v.kind {
                ActiveChannelMonitorEventKind::Down => ActiveChannelMonitorEventKindResponse::Down,
                ActiveChannelMonitorEventKind::Recovered => {
                    ActiveChannelMonitorEventKindResponse::Recovered
                }
            },
            latency_ms: v.latency_ms,
            occurred_epoch_seconds: v.occurred_epoch_seconds,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorSettingsResponse {
    pub enabled: bool,
    pub global_daily_budget: u32,
}

impl From<ActiveChannelMonitorConfig> for ActiveChannelMonitorSettingsResponse {
    fn from(v: ActiveChannelMonitorConfig) -> Self {
        Self {
            enabled: v.enabled,
            global_daily_budget: v.global_daily_budget,
        }
    }
}

/// Root registers this isolated router alongside its management APIs.
pub fn build_active_channel_monitor_router(
    auth: Arc<dyn SessionAuthenticator>,
    service: Arc<dyn ActiveChannelMonitorService>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(auth),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/channel-monitor/settings",
            get(get_settings).patch(set_settings),
        )
        .route(
            "/api/admin/channel-monitor/targets",
            get(list_targets).post(upsert_target),
        )
        .route(
            "/api/admin/channel-monitor/targets/{id}",
            delete(delete_target),
        )
        .route(
            "/api/admin/channel-monitor/targets/{id}/events",
            get(list_events),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(ActiveChannelMonitorState { service })
}

fn map_error(error: ActiveChannelMonitorServiceError) -> ManagementError {
    match error {
        ActiveChannelMonitorServiceError::InvalidInput => ManagementError::InvalidRequest,
        ActiveChannelMonitorServiceError::NotFound => ManagementError::ChannelNotFound,
        ActiveChannelMonitorServiceError::Unavailable => ManagementError::ProbeUnavailable,
    }
}

#[utoipa::path(get,path="/api/admin/channel-monitor/settings",responses((status=200,body=ActiveChannelMonitorSettingsResponse)),security(("bearerAuth" = [])))]
async fn get_settings(
    State(s): State<ActiveChannelMonitorState>,
    Extension(_): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(ActiveChannelMonitorSettingsResponse::from(
        s.service.config().await.map_err(map_error)?,
    )))
}

#[utoipa::path(patch,path="/api/admin/channel-monitor/settings",request_body=ActiveChannelMonitorSettingsRequest,responses((status=200,body=ActiveChannelMonitorSettingsResponse)),security(("bearerAuth" = [])))]
async fn set_settings(
    State(s): State<ActiveChannelMonitorState>,
    Extension(_): Extension<SessionAuthentication>,
    request: Result<Json<ActiveChannelMonitorSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(v) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let result = s
        .service
        .set_config(ActiveChannelMonitorConfig {
            enabled: v.enabled,
            global_daily_budget: v.global_daily_budget,
        })
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ActiveChannelMonitorSettingsResponse::from(
        result,
    )))
}

#[utoipa::path(get,path="/api/admin/channel-monitor/targets",responses((status=200,body=[ActiveChannelMonitorTargetResponse])),security(("bearerAuth" = [])))]
async fn list_targets(
    State(s): State<ActiveChannelMonitorState>,
    Extension(_): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let rows = s
        .service
        .list_targets()
        .await
        .map_err(map_error)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<ActiveChannelMonitorTargetResponse>>();
    Ok(no_store_json(rows))
}

#[utoipa::path(post,path="/api/admin/channel-monitor/targets",request_body=ActiveChannelMonitorTargetRequest,responses((status=200,body=ActiveChannelMonitorTargetResponse)),security(("bearerAuth" = [])))]
async fn upsert_target(
    State(s): State<ActiveChannelMonitorState>,
    Extension(_): Extension<SessionAuthentication>,
    request: Result<Json<ActiveChannelMonitorTargetRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(v) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let input = target_input(v)?;
    Ok(no_store_json(ActiveChannelMonitorTargetResponse::from(
        s.service.upsert_target(input).await.map_err(map_error)?,
    )))
}

#[utoipa::path(delete,path="/api/admin/channel-monitor/targets/{id}",params(("id"=i64,Path,minimum=1)),responses((status=204)),security(("bearerAuth" = [])))]
async fn delete_target(
    State(s): State<ActiveChannelMonitorState>,
    Path(id): Path<i64>,
    Extension(_): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    if id <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    s.service.delete_target(id).await.map_err(map_error)?;
    Ok(http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(get,path="/api/admin/channel-monitor/targets/{id}/events",params(("id"=i64,Path,minimum=1)),responses((status=200,body=Vec<ActiveChannelMonitorEventResponse>)),security(("bearerAuth" = [])))]
async fn list_events(
    State(s): State<ActiveChannelMonitorState>,
    Path(id): Path<i64>,
    Extension(_): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    if id <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let rows = s
        .service
        .events(id)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<ActiveChannelMonitorEventResponse>>();
    Ok(no_store_json(rows))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_settings,
        set_settings,
        list_targets,
        upsert_target,
        delete_target,
        list_events
    ),
    components(schemas(
        ActiveChannelMonitorSettingsRequest,
        ActiveChannelMonitorSettingsResponse,
        ActiveChannelMonitorTargetRequest,
        ActiveChannelMonitorTargetResponse,
        ActiveChannelMonitorEventResponse,
        ActiveChannelMonitorEventKindResponse
    ))
)]
pub struct ActiveChannelMonitorApiDoc;

pub fn active_channel_monitor_openapi_document() -> utoipa::openapi::OpenApi {
    ActiveChannelMonitorApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::active_channel_monitor_openapi_document;

    #[test]
    fn every_active_monitor_endpoint_declares_bearer_authentication() {
        let document = serde_json::to_value(active_channel_monitor_openapi_document()).unwrap();
        let paths = document["paths"].as_object().unwrap();
        assert_eq!(paths.len(), 4);
        for path in paths.values() {
            for operation in path.as_object().unwrap().values() {
                assert_eq!(
                    operation["security"],
                    serde_json::json!([{"bearerAuth": []}])
                );
            }
        }
    }
}
