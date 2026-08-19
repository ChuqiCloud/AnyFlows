use af_admin::{PlatformPolicy, SessionAuthentication};
use af_analytics::{
    AdminDashboard, AdminDashboardAccess, AdminDashboardHourlyPoint, AdminDashboardPerformance,
    AdminDashboardReadError,
};
use af_domain::PlatformPermission;
use axum::{
    Json,
    extract::{Extension, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

/// 管理看板最近 24 小时的真实指标响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDashboardResponse)]
pub(crate) struct AdminDashboardResponse {
    /// 统计窗口起点的 Unix 秒数。
    #[schema(minimum = 0)]
    period_start: i64,
    /// 统计窗口终点的 Unix 秒数。
    #[schema(minimum = 1)]
    period_end: i64,
    #[schema(minimum = 0)]
    request_count: i64,
    #[schema(minimum = 0)]
    quota_consumed: i64,
    #[schema(minimum = 0)]
    upstream_usage_count: i64,
    #[schema(minimum = 0)]
    estimated_usage_count: i64,
    #[schema(minimum = 0)]
    per_token_request_count: i64,
    #[schema(minimum = 0)]
    per_call_request_count: i64,
    #[schema(minimum = 0)]
    free_request_count: i64,
    #[schema(minimum = 0)]
    enabled_channel_count: i64,
    #[schema(minimum = 0)]
    disabled_channel_count: i64,
    #[schema(minimum = 0)]
    auto_disabled_channel_count: i64,
    /// 覆盖整个统计窗口的连续一小时用量分桶。
    hourly: Vec<AdminDashboardHourlyPointResponse>,
    /// 只基于真实持久化耗时样本生成的性能健康摘要。
    performance: AdminDashboardPerformanceResponse,
}

/// 管理看板单个一小时用量分桶。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDashboardHourlyPoint)]
pub(crate) struct AdminDashboardHourlyPointResponse {
    #[schema(minimum = 0)]
    period_start: i64,
    #[schema(minimum = 1)]
    period_end: i64,
    #[schema(minimum = 0)]
    request_count: i64,
    #[schema(minimum = 0)]
    quota_consumed: i64,
}

/// 管理看板窗口内真实耗时样本的健康摘要。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDashboardPerformance)]
pub(crate) struct AdminDashboardPerformanceResponse {
    #[schema(minimum = 0)]
    first_token_sample_count: i64,
    #[schema(minimum = 0, nullable = true)]
    average_first_token_ms: Option<i64>,
    #[schema(minimum = 0)]
    slow_first_token_count: i64,
    #[schema(minimum = 1)]
    slow_first_token_threshold_ms: i64,
    #[schema(minimum = 0)]
    duration_sample_count: i64,
    #[schema(minimum = 0, nullable = true)]
    average_duration_ms: Option<i64>,
    #[schema(minimum = 0)]
    slow_request_count: i64,
    #[schema(minimum = 1)]
    slow_request_threshold_ms: i64,
}

/// 返回管理员可见的最近 24 小时看板快照。
pub(crate) async fn get_admin_dashboard(
    State(state): State<HttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let reader = state
        .admin_dashboard_reader
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let access = if PlatformPolicy::allows(
        authentication.principal(),
        PlatformPermission::DashboardReadAll,
    ) {
        AdminDashboardAccess::Admin
    } else {
        AdminDashboardAccess::User
    };
    let dashboard = reader.read(access).await.map_err(map_read_error)?;
    Ok(no_store_json(AdminDashboardResponse::from_dashboard(
        dashboard,
    )))
}

impl AdminDashboardResponse {
    fn from_dashboard(dashboard: AdminDashboard) -> Self {
        Self {
            period_start: dashboard.period_start(),
            period_end: dashboard.period_end(),
            request_count: dashboard.request_count(),
            quota_consumed: dashboard.quota_consumed(),
            upstream_usage_count: dashboard.upstream_usage_count(),
            estimated_usage_count: dashboard.estimated_usage_count(),
            per_token_request_count: dashboard.per_token_request_count(),
            per_call_request_count: dashboard.per_call_request_count(),
            free_request_count: dashboard.free_request_count(),
            enabled_channel_count: dashboard.enabled_channel_count(),
            disabled_channel_count: dashboard.disabled_channel_count(),
            auto_disabled_channel_count: dashboard.auto_disabled_channel_count(),
            hourly: dashboard
                .hourly()
                .iter()
                .copied()
                .map(AdminDashboardHourlyPointResponse::from_point)
                .collect(),
            performance: AdminDashboardPerformanceResponse::from_performance(
                dashboard.performance(),
            ),
        }
    }
}

impl AdminDashboardHourlyPointResponse {
    fn from_point(point: AdminDashboardHourlyPoint) -> Self {
        Self {
            period_start: point.period_start(),
            period_end: point.period_end(),
            request_count: point.request_count(),
            quota_consumed: point.quota_consumed(),
        }
    }
}

impl AdminDashboardPerformanceResponse {
    fn from_performance(performance: AdminDashboardPerformance) -> Self {
        Self {
            first_token_sample_count: performance.first_token_sample_count(),
            average_first_token_ms: performance.average_first_token_ms(),
            slow_first_token_count: performance.slow_first_token_count(),
            slow_first_token_threshold_ms: performance.slow_first_token_threshold_ms(),
            duration_sample_count: performance.duration_sample_count(),
            average_duration_ms: performance.average_duration_ms(),
            slow_request_count: performance.slow_request_count(),
            slow_request_threshold_ms: performance.slow_request_threshold_ms(),
        }
    }
}

fn map_read_error(error: AdminDashboardReadError) -> ManagementError {
    match error {
        AdminDashboardReadError::Forbidden => ManagementError::Forbidden,
        AdminDashboardReadError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
