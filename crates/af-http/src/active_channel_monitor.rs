//! Admin configuration for explicitly opted-in channel health monitors.
use std::sync::Arc;

use af_admin::{
    ActiveChannelMonitorConfig, ActiveChannelMonitorEvent, ActiveChannelMonitorEventKind,
    ActiveChannelMonitorPluginReviewCheck, ActiveChannelMonitorPluginReviewError,
    ActiveChannelMonitorPluginReviewService, ActiveChannelMonitorPluginReviewStatus,
    ActiveChannelMonitorService, ActiveChannelMonitorServiceError, ActiveChannelMonitorTarget,
    ActiveChannelMonitorTargetInput, HostActionProposal, PluginActionProposalRecord,
    PluginActionProposalStatus, SessionAuthentication, SessionAuthenticator,
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
    plugin_review: Arc<dyn ActiveChannelMonitorPluginReviewService>,
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

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActiveChannelMonitorPluginReviewRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActiveChannelMonitorPluginActionProposalRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    #[schema(minimum = 1)]
    pub expected_target_version: i64,
    #[schema(min_length = 1, max_length = 128)]
    pub idempotency_key: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActiveChannelMonitorPluginActionConfirmRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub confirmation_key: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginActionProposalStatusResponse {
    Pending,
    Confirmed,
    Expired,
    Stale,
    Revoked,
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorPluginActionProposalResponse {
    #[schema(min_length = 1, max_length = 36)]
    pub proposal_key: String,
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    pub target_id: i64,
    #[schema(min_length = 1, max_length = 96)]
    pub action_id: String,
    pub action_version: u16,
    pub expected_enabled: bool,
    pub requested_enabled: bool,
    pub expected_target_version: i64,
    pub status: PluginActionProposalStatusResponse,
    pub expires_at: i64,
    pub created_at: i64,
    pub confirmed_at: Option<i64>,
    pub outcome: Option<serde_json::Value>,
}

impl From<PluginActionProposalRecord> for ActiveChannelMonitorPluginActionProposalResponse {
    fn from(value: PluginActionProposalRecord) -> Self {
        Self {
            proposal_key: value.proposal_key,
            plugin_id: value.plugin_id,
            target_id: value.target_id,
            action_id: value.action_id,
            action_version: value.action_version,
            expected_enabled: value.expected_enabled,
            requested_enabled: value.requested_enabled,
            expected_target_version: value.expected_target_version,
            status: match value.status {
                PluginActionProposalStatus::Pending => PluginActionProposalStatusResponse::Pending,
                PluginActionProposalStatus::Confirmed => {
                    PluginActionProposalStatusResponse::Confirmed
                }
                PluginActionProposalStatus::Expired => PluginActionProposalStatusResponse::Expired,
                PluginActionProposalStatus::Stale => PluginActionProposalStatusResponse::Stale,
                PluginActionProposalStatus::Revoked => PluginActionProposalStatusResponse::Revoked,
            },
            expires_at: value.expires_at,
            created_at: value.created_at,
            confirmed_at: value.confirmed_at,
            outcome: value.outcome,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorPluginReviewerResponse {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    #[schema(min_length = 1, max_length = 128)]
    pub display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    pub plugin_version: String,
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorPluginReviewerListResponse {
    #[schema(max_items = 100)]
    pub plugins: Vec<ActiveChannelMonitorPluginReviewerResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActiveChannelMonitorPluginReviewStatusResponse {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorPluginReviewCheckResponse {
    #[schema(min_length = 1, max_length = 64)]
    pub code: String,
    #[schema(min_length = 1, max_length = 128)]
    pub title: String,
    pub status: ActiveChannelMonitorPluginReviewStatusResponse,
    #[schema(min_length = 1, max_length = 2048)]
    pub message: String,
    #[schema(required = true, min_length = 1, max_length = 2048)]
    pub recommendation: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveChannelMonitorPluginActionHintResponse {
    pub action_id: String,
    pub action_version: u16,
    pub input: serde_json::Value,
}

impl From<HostActionProposal> for ActiveChannelMonitorPluginActionHintResponse {
    fn from(value: HostActionProposal) -> Self {
        Self {
            action_id: value.action_id.as_str().to_owned(),
            action_version: value.action_version,
            input: value.input,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ActiveChannelMonitorPluginReviewResponse {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    #[schema(min_length = 1, max_length = 128)]
    pub display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    pub plugin_version: String,
    #[schema(minimum = 0)]
    pub event_count: u32,
    pub target_config_version: i64,
    #[schema(min_length = 1, max_length = 2048)]
    pub summary: String,
    #[schema(max_items = 64)]
    pub checks: Vec<ActiveChannelMonitorPluginReviewCheckResponse>,
    #[schema(value_type = Vec<serde_json::Value>, max_items = 8)]
    pub ui_views: Vec<af_plugin_contract::PluginUiView>,
    pub action_proposal: Option<ActiveChannelMonitorPluginActionHintResponse>,
}

impl From<ActiveChannelMonitorPluginReviewCheck> for ActiveChannelMonitorPluginReviewCheckResponse {
    fn from(value: ActiveChannelMonitorPluginReviewCheck) -> Self {
        Self {
            code: value.code,
            title: value.title,
            status: match value.status {
                ActiveChannelMonitorPluginReviewStatus::Pass => {
                    ActiveChannelMonitorPluginReviewStatusResponse::Pass
                }
                ActiveChannelMonitorPluginReviewStatus::Info => {
                    ActiveChannelMonitorPluginReviewStatusResponse::Info
                }
                ActiveChannelMonitorPluginReviewStatus::Warning => {
                    ActiveChannelMonitorPluginReviewStatusResponse::Warning
                }
                ActiveChannelMonitorPluginReviewStatus::Fail => {
                    ActiveChannelMonitorPluginReviewStatusResponse::Fail
                }
            },
            message: value.message,
            recommendation: value.recommendation,
        }
    }
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
    plugin_review: Arc<dyn ActiveChannelMonitorPluginReviewService>,
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
        .route(
            "/api/admin/channel-monitor/plugin-reviewers",
            get(list_plugin_reviewers),
        )
        .route(
            "/api/admin/channel-monitor/targets/{id}/plugin-review",
            axum::routing::post(review_monitor_events),
        )
        .route(
            "/api/admin/channel-monitor/targets/{id}/action-proposals",
            axum::routing::post(propose_monitor_action),
        )
        .route(
            "/api/admin/channel-monitor/action-proposals/{proposal_key}/confirm",
            axum::routing::post(confirm_monitor_action_proposal),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(ActiveChannelMonitorState {
            service,
            plugin_review,
        })
}

fn map_error(error: ActiveChannelMonitorServiceError) -> ManagementError {
    match error {
        ActiveChannelMonitorServiceError::InvalidInput => ManagementError::InvalidRequest,
        ActiveChannelMonitorServiceError::NotFound => ManagementError::ChannelNotFound,
        ActiveChannelMonitorServiceError::Unavailable => ManagementError::ProbeUnavailable,
    }
}

fn map_plugin_review_error(error: ActiveChannelMonitorPluginReviewError) -> ManagementError {
    match error {
        ActiveChannelMonitorPluginReviewError::InvalidInput => {
            ManagementError::PluginLifecycleInvalidRequest
        }
        ActiveChannelMonitorPluginReviewError::NotFound => ManagementError::PluginLifecycleNotFound,
        ActiveChannelMonitorPluginReviewError::Forbidden => ManagementError::Forbidden,
        ActiveChannelMonitorPluginReviewError::Conflict => ManagementError::PluginLifecycleConflict,
        ActiveChannelMonitorPluginReviewError::Unavailable => {
            ManagementError::PluginLifecycleUnavailable
        }
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

#[utoipa::path(get,path="/api/admin/channel-monitor/plugin-reviewers",operation_id="listAdminChannelMonitorPluginReviewers",summary="列出渠道监控分析插件",responses((status=200,body=ActiveChannelMonitorPluginReviewerListResponse)),security(("bearerAuth" = []))) ]
async fn list_plugin_reviewers(
    State(s): State<ActiveChannelMonitorState>,
    Extension(_): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let plugins = s
        .plugin_review
        .reviewers()
        .await
        .map_err(map_plugin_review_error)?
        .plugins
        .into_iter()
        .map(|value| ActiveChannelMonitorPluginReviewerResponse {
            plugin_id: value.plugin_id,
            display_name: value.display_name,
            plugin_version: value.plugin_version,
        })
        .collect();
    Ok(no_store_json(
        ActiveChannelMonitorPluginReviewerListResponse { plugins },
    ))
}

#[utoipa::path(post,path="/api/admin/channel-monitor/targets/{id}/plugin-review",operation_id="reviewAdminChannelMonitorTargetWithPlugin",summary="分析渠道监控事件",params(("id"=i64,Path,minimum=1)),request_body=ActiveChannelMonitorPluginReviewRequest,responses((status=200,body=ActiveChannelMonitorPluginReviewResponse)),security(("bearerAuth" = []))) ]
async fn review_monitor_events(
    State(s): State<ActiveChannelMonitorState>,
    Path(id): Path<i64>,
    Extension(request_id): Extension<af_telemetry::RequestId>,
    Extension(_): Extension<SessionAuthentication>,
    request: Result<Json<ActiveChannelMonitorPluginReviewRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    if id <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let review = s
        .plugin_review
        .review_events(id, request.plugin_id, request_id.as_str().to_owned())
        .await
        .map_err(map_plugin_review_error)?;
    Ok(no_store_json(ActiveChannelMonitorPluginReviewResponse {
        plugin_id: review.plugin_id,
        display_name: review.display_name,
        plugin_version: review.plugin_version,
        event_count: review.event_count,
        target_config_version: review.target_config_version,
        summary: review.summary,
        checks: review.checks.into_iter().map(Into::into).collect(),
        ui_views: review.ui_views,
        action_proposal: review.action_proposal.map(Into::into),
    }))
}

#[utoipa::path(post,path="/api/admin/channel-monitor/targets/{id}/action-proposals",operation_id="proposeAdminChannelMonitorTargetAction",summary="为监控目标创建插件动作提案",params(("id"=i64,Path,minimum=1)),request_body=ActiveChannelMonitorPluginActionProposalRequest,responses((status=200,body=ActiveChannelMonitorPluginActionProposalResponse)),security(("bearerAuth" = []))) ]
async fn propose_monitor_action(
    State(state): State<ActiveChannelMonitorState>,
    Path(target_id): Path<i64>,
    Extension(request_id): Extension<af_telemetry::RequestId>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<ActiveChannelMonitorPluginActionProposalRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    if target_id <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let proposal = state
        .plugin_review
        .propose_target_action(
            authentication.principal(),
            target_id,
            request.plugin_id,
            request.expected_target_version,
            request.idempotency_key,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_plugin_review_error)?;
    Ok(no_store_json(
        ActiveChannelMonitorPluginActionProposalResponse::from(proposal),
    ))
}

#[utoipa::path(post,path="/api/admin/channel-monitor/action-proposals/{proposal_key}/confirm",operation_id="confirmAdminChannelMonitorActionProposal",summary="确认并执行渠道监控目标插件动作",params(("proposal_key"=String,Path,min_length=1,max_length=36)),request_body=ActiveChannelMonitorPluginActionConfirmRequest,responses((status=200,body=ActiveChannelMonitorPluginActionProposalResponse)),security(("bearerAuth" = []))) ]
async fn confirm_monitor_action_proposal(
    State(state): State<ActiveChannelMonitorState>,
    Path(proposal_key): Path<String>,
    Extension(request_id): Extension<af_telemetry::RequestId>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<ActiveChannelMonitorPluginActionConfirmRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let proposal = state
        .plugin_review
        .confirm_target_action(
            authentication.principal(),
            proposal_key,
            request.confirmation_key,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_plugin_review_error)?;
    Ok(no_store_json(
        ActiveChannelMonitorPluginActionProposalResponse::from(proposal),
    ))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_settings,
        set_settings,
        list_targets,
        upsert_target,
        delete_target,
        list_events,
        list_plugin_reviewers,
        review_monitor_events,
        propose_monitor_action,
        confirm_monitor_action_proposal
    ),
    components(schemas(
        ActiveChannelMonitorSettingsRequest,
        ActiveChannelMonitorSettingsResponse,
        ActiveChannelMonitorTargetRequest,
        ActiveChannelMonitorTargetResponse,
        ActiveChannelMonitorEventResponse,
        ActiveChannelMonitorEventKindResponse,
        ActiveChannelMonitorPluginReviewRequest,
        ActiveChannelMonitorPluginActionProposalRequest,
        ActiveChannelMonitorPluginActionConfirmRequest,
        PluginActionProposalStatusResponse,
        ActiveChannelMonitorPluginActionProposalResponse,
        ActiveChannelMonitorPluginActionHintResponse,
        ActiveChannelMonitorPluginReviewerResponse,
        ActiveChannelMonitorPluginReviewerListResponse,
        ActiveChannelMonitorPluginReviewStatusResponse,
        ActiveChannelMonitorPluginReviewCheckResponse,
        ActiveChannelMonitorPluginReviewResponse
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
        assert_eq!(paths.len(), 8);
        for path in paths.values() {
            for operation in path.as_object().unwrap().values() {
                assert_eq!(
                    operation["security"],
                    serde_json::json!([{"bearerAuth": []}])
                );
            }
        }
    }

    #[test]
    fn monitor_plugin_review_routes_are_explicitly_admin_only_and_typed() {
        let document = serde_json::to_value(active_channel_monitor_openapi_document()).unwrap();
        let list = &document["paths"]["/api/admin/channel-monitor/plugin-reviewers"]["get"];
        assert_eq!(
            list["operationId"],
            "listAdminChannelMonitorPluginReviewers"
        );
        assert_eq!(list["summary"], "列出渠道监控分析插件");
        assert_eq!(list["security"], serde_json::json!([{"bearerAuth": []}]));

        let review =
            &document["paths"]["/api/admin/channel-monitor/targets/{id}/plugin-review"]["post"];
        assert_eq!(
            review["operationId"],
            "reviewAdminChannelMonitorTargetWithPlugin"
        );
        assert_eq!(
            review["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ActiveChannelMonitorPluginReviewRequest"
        );
        let request_schema =
            &document["components"]["schemas"]["ActiveChannelMonitorPluginReviewRequest"];
        assert_eq!(request_schema["required"], serde_json::json!(["plugin_id"]));
        assert!(request_schema["properties"].get("capability_id").is_none());
        assert_eq!(
            document["components"]["schemas"]["ActiveChannelMonitorPluginReviewStatusResponse"]["enum"],
            serde_json::json!(["pass", "info", "warning", "fail"])
        );
        assert_eq!(
            document["components"]["schemas"]["ActiveChannelMonitorPluginReviewResponse"]["properties"]
                ["ui_views"]["maxItems"],
            8
        );
    }
}
