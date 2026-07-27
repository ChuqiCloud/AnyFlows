use af_admin::{SessionAuthentication, SessionRole};
use af_analytics::{AdminDashboard, AdminDashboardAccess, AdminDashboardReadError};
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
    free_request_count: i64,
    #[schema(minimum = 0)]
    enabled_channel_count: i64,
    #[schema(minimum = 0)]
    disabled_channel_count: i64,
    #[schema(minimum = 0)]
    auto_disabled_channel_count: i64,
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
    let access = match authentication.principal().role() {
        SessionRole::User => AdminDashboardAccess::User,
        SessionRole::Admin => AdminDashboardAccess::Admin,
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
            free_request_count: dashboard.free_request_count(),
            enabled_channel_count: dashboard.enabled_channel_count(),
            disabled_channel_count: dashboard.disabled_channel_count(),
            auto_disabled_channel_count: dashboard.auto_disabled_channel_count(),
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
