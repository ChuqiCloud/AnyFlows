use af_admin::{PlatformPolicy, SessionAuthentication};
use af_analytics::{AnalyticsExportControlError, AnalyticsExportQueueSnapshot};
use af_domain::PlatformPermission;
use axum::{
    Json,
    extract::{Extension, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

const DEFAULT_REPLAY_LIMIT: u16 = 64;
const MAX_REPLAY_LIMIT: u16 = 256;

/// ClickHouse 异步事实导出的健康状态。
#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AnalyticsExportHealthState)]
pub(crate) enum AnalyticsExportHealthState {
    /// 未配置 ClickHouse 导出。
    Disabled,
    /// 已配置且当前没有积压。
    Healthy,
    /// 已配置但仍有待投递事实。
    Backlog,
    /// 已配置但主库状态暂时不可读。
    Unavailable,
}

/// 管理端 ClickHouse 异步事实导出状态。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AnalyticsExportStatusResponse)]
pub(crate) struct AnalyticsExportStatusResponse {
    enabled: bool,
    state: AnalyticsExportHealthState,
    #[schema(minimum = 0, nullable = true)]
    pending_count: Option<u64>,
    #[schema(minimum = 0, nullable = true)]
    leased_count: Option<u64>,
    #[schema(minimum = 0, nullable = true)]
    published_count: Option<u64>,
    #[schema(minimum = 0, nullable = true)]
    backlog_count: Option<u64>,
}

/// 管理端手动重放请求；默认只推进有限数量的积压事件。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AnalyticsExportReplayRequest)]
pub(crate) struct AnalyticsExportReplayRequest {
    #[schema(minimum = 1, maximum = 256, nullable = true)]
    limit: Option<u16>,
}

/// 管理端手动重放结果。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AnalyticsExportReplayResponse)]
pub(crate) struct AnalyticsExportReplayResponse {
    #[schema(minimum = 0)]
    replayed_count: u64,
    #[schema(minimum = 0, nullable = true)]
    backlog_count: Option<u64>,
}

/// 返回 ClickHouse 异步事实导出的健康与积压状态。
pub(crate) async fn get_admin_analytics_export_status(
    State(state): State<HttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    require_dashboard_access(authentication)?;
    let body = match state.analytics_export_control.as_deref() {
        None => AnalyticsExportStatusResponse::disabled(),
        Some(control) => match control.status().await {
            Ok(snapshot) => AnalyticsExportStatusResponse::from_snapshot(snapshot),
            Err(_) => AnalyticsExportStatusResponse::unavailable(),
        },
    };
    Ok(no_store_json(body))
}

/// 触发有界积压重放；不会直接执行 ClickHouse 请求或重置已发布事实。
pub(crate) async fn replay_admin_analytics_export(
    State(state): State<HttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AnalyticsExportReplayRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    require_dashboard_access(authentication)?;
    let control = state
        .analytics_export_control
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let limit = request.limit.unwrap_or(DEFAULT_REPLAY_LIMIT);
    if !(1..=MAX_REPLAY_LIMIT).contains(&limit) {
        return Err(ManagementError::InvalidRequest);
    }
    let replayed_count = control.replay(limit).await.map_err(map_control_error)?;
    let backlog_count = control
        .status()
        .await
        .ok()
        .map(|snapshot| snapshot.backlog_count());
    Ok(no_store_json(AnalyticsExportReplayResponse {
        replayed_count,
        backlog_count,
    }))
}

impl AnalyticsExportStatusResponse {
    fn disabled() -> Self {
        Self {
            enabled: false,
            state: AnalyticsExportHealthState::Disabled,
            pending_count: None,
            leased_count: None,
            published_count: None,
            backlog_count: None,
        }
    }

    fn unavailable() -> Self {
        Self {
            enabled: true,
            state: AnalyticsExportHealthState::Unavailable,
            pending_count: None,
            leased_count: None,
            published_count: None,
            backlog_count: None,
        }
    }

    fn from_snapshot(snapshot: AnalyticsExportQueueSnapshot) -> Self {
        Self {
            enabled: true,
            state: if snapshot.backlog_count() == 0 {
                AnalyticsExportHealthState::Healthy
            } else {
                AnalyticsExportHealthState::Backlog
            },
            pending_count: Some(snapshot.pending_count()),
            leased_count: Some(snapshot.leased_count()),
            published_count: Some(snapshot.published_count()),
            backlog_count: Some(snapshot.backlog_count()),
        }
    }
}

fn map_control_error(error: AnalyticsExportControlError) -> ManagementError {
    match error {
        AnalyticsExportControlError::Unavailable | AnalyticsExportControlError::Invariant => {
            ManagementError::Internal
        }
    }
}

fn require_dashboard_access(authentication: SessionAuthentication) -> Result<(), ManagementError> {
    if PlatformPolicy::allows(
        authentication.principal(),
        PlatformPermission::DashboardReadAll,
    ) {
        Ok(())
    } else {
        Err(ManagementError::Forbidden)
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
