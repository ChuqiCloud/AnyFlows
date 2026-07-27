use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    channel_probe::AdminChannelProbeOutcome, chat_completions::HttpState,
    management_channels::parse_channel_id, management_error::ManagementError,
};

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminChannelProbeStatus, rename_all = "snake_case")]
pub(crate) enum AdminChannelProbeStatus {
    Healthy,
    Unhealthy,
    Timeout,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminChannelProbeResponse)]
pub(crate) struct AdminChannelProbeResponse {
    status: AdminChannelProbeStatus,
    #[schema(minimum = 0)]
    latency_ms: u64,
}

/// 对指定渠道执行一次有界真实测活，仅返回脱敏结论与端到端耗时。
pub(crate) async fn probe_admin_channel(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(_authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let probe = state
        .admin_channel_probe
        .as_ref()
        .ok_or(ManagementError::ProbeUnavailable)?;
    let started_at = Instant::now();
    let status = match probe.probe(channel_id).await {
        AdminChannelProbeOutcome::Healthy => AdminChannelProbeStatus::Healthy,
        AdminChannelProbeOutcome::Unhealthy => AdminChannelProbeStatus::Unhealthy,
        AdminChannelProbeOutcome::TimedOut => AdminChannelProbeStatus::Timeout,
    };
    let latency_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut response = (
        StatusCode::OK,
        Json(AdminChannelProbeResponse { status, latency_ms }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}
