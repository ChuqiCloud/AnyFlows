use af_admin::{PlatformPolicy, SessionAuthentication};
use af_analytics::{
    AdminDashboard, AdminDashboardAccess, AdminDashboardChannelFlow, AdminDashboardFailure,
    AdminDashboardFailureKind, AdminDashboardFlowPath, AdminDashboardHourlyPoint,
    AdminDashboardPerformance, AdminDashboardReadError,
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
    /// 已写入终态事实的同步模型请求数，与用量请求数不要求相等。
    #[schema(minimum = 0)]
    outcome_request_count: i64,
    #[schema(minimum = 0)]
    successful_request_count: i64,
    #[schema(minimum = 0)]
    failed_request_count: i64,
    /// 未进入前十二条可见渠道流向的成功请求数。
    #[schema(minimum = 0)]
    other_success_count: i64,
    /// 按闭合错误分类聚合的失败请求。
    failures: Vec<AdminDashboardFailureResponse>,
    /// 请求量最高的十二条入口协议到最终成功渠道流向。
    channel_flows: Vec<AdminDashboardChannelFlowResponse>,
    /// 能连接已结算用量与成功终态的请求总数。
    #[schema(minimum = 0)]
    flow_request_count: i64,
    /// 能连接完整四层路径的已结算额度总量。
    #[schema(minimum = 0)]
    flow_quota_consumed: i64,
    /// 请求量最高的二十四条用户、分组、渠道和模型流向。
    flow_paths: Vec<AdminDashboardFlowPathResponse>,
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

/// 管理看板公开的闭合失败分类。
#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminDashboardFailureKind)]
pub(crate) enum AdminDashboardFailureKindResponse {
    InvalidRequest,
    ModelNotAllowed,
    InsufficientQuota,
    QuotaLimited,
    ConcurrencyLimited,
    OutcomeUnknown,
    UpstreamRateLimited,
    UpstreamOverloaded,
    UpstreamAuthentication,
    UpstreamQuota,
    UpstreamModel,
    UpstreamProtocol,
    UpstreamServer,
    UpstreamNetwork,
    Internal,
}

/// 管理看板窗口内一个失败分类的请求数。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDashboardFailure)]
pub(crate) struct AdminDashboardFailureResponse {
    kind: AdminDashboardFailureKindResponse,
    #[schema(minimum = 1)]
    request_count: i64,
}

/// 管理看板窗口内一条入口协议到最终成功渠道流向。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDashboardChannelFlow)]
pub(crate) struct AdminDashboardChannelFlowResponse {
    #[schema(value_type = crate::openapi::schema::AdminChannelProtocolSchema)]
    protocol: String,
    #[schema(minimum = 1)]
    channel_id: i64,
    #[schema(min_length = 1, max_length = 128)]
    channel_name: String,
    #[schema(minimum = 1)]
    request_count: i64,
}

/// 管理看板窗口内一条低敏四层请求与额度流向。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDashboardFlowPath)]
pub(crate) struct AdminDashboardFlowPathResponse {
    #[schema(minimum = 1)]
    user_id: i64,
    #[schema(minimum = 1)]
    group_id: i64,
    #[schema(min_length = 1, max_length = 128)]
    group_name: String,
    #[schema(minimum = 1)]
    channel_id: i64,
    #[schema(min_length = 1, max_length = 128)]
    channel_name: String,
    #[schema(min_length = 1, max_length = 255)]
    model: String,
    #[schema(minimum = 1)]
    request_count: i64,
    #[schema(minimum = 0)]
    quota_consumed: i64,
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
            outcome_request_count: dashboard.outcome_request_count(),
            successful_request_count: dashboard.successful_request_count(),
            failed_request_count: dashboard.failed_request_count(),
            other_success_count: dashboard.other_success_count(),
            failures: dashboard
                .failures()
                .iter()
                .copied()
                .map(AdminDashboardFailureResponse::from_failure)
                .collect(),
            channel_flows: dashboard
                .channel_flows()
                .iter()
                .map(AdminDashboardChannelFlowResponse::from_flow)
                .collect(),
            flow_request_count: dashboard.flow_request_count(),
            flow_quota_consumed: dashboard.flow_quota_consumed(),
            flow_paths: dashboard
                .flow_paths()
                .iter()
                .map(AdminDashboardFlowPathResponse::from_path)
                .collect(),
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

impl AdminDashboardFailureKindResponse {
    const fn from_kind(kind: AdminDashboardFailureKind) -> Self {
        match kind {
            AdminDashboardFailureKind::InvalidRequest => Self::InvalidRequest,
            AdminDashboardFailureKind::ModelNotAllowed => Self::ModelNotAllowed,
            AdminDashboardFailureKind::InsufficientQuota => Self::InsufficientQuota,
            AdminDashboardFailureKind::QuotaLimited => Self::QuotaLimited,
            AdminDashboardFailureKind::ConcurrencyLimited => Self::ConcurrencyLimited,
            AdminDashboardFailureKind::OutcomeUnknown => Self::OutcomeUnknown,
            AdminDashboardFailureKind::UpstreamRateLimited => Self::UpstreamRateLimited,
            AdminDashboardFailureKind::UpstreamOverloaded => Self::UpstreamOverloaded,
            AdminDashboardFailureKind::UpstreamAuthentication => Self::UpstreamAuthentication,
            AdminDashboardFailureKind::UpstreamQuota => Self::UpstreamQuota,
            AdminDashboardFailureKind::UpstreamModel => Self::UpstreamModel,
            AdminDashboardFailureKind::UpstreamProtocol => Self::UpstreamProtocol,
            AdminDashboardFailureKind::UpstreamServer => Self::UpstreamServer,
            AdminDashboardFailureKind::UpstreamNetwork => Self::UpstreamNetwork,
            AdminDashboardFailureKind::Internal => Self::Internal,
        }
    }
}

impl AdminDashboardFailureResponse {
    fn from_failure(failure: AdminDashboardFailure) -> Self {
        Self {
            kind: AdminDashboardFailureKindResponse::from_kind(failure.kind()),
            request_count: failure.request_count(),
        }
    }
}

impl AdminDashboardChannelFlowResponse {
    fn from_flow(flow: &AdminDashboardChannelFlow) -> Self {
        Self {
            protocol: flow.protocol().as_str().to_owned(),
            channel_id: flow.channel_id().get(),
            channel_name: flow.channel_name().to_owned(),
            request_count: flow.request_count(),
        }
    }
}

impl AdminDashboardFlowPathResponse {
    fn from_path(path: &AdminDashboardFlowPath) -> Self {
        Self {
            user_id: path.user_id(),
            group_id: path.group_id(),
            group_name: path.group_name().to_owned(),
            channel_id: path.channel_id().get(),
            channel_name: path.channel_name().to_owned(),
            model: path.model().to_owned(),
            request_count: path.request_count(),
            quota_consumed: path.quota_consumed(),
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

pub(crate) fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
