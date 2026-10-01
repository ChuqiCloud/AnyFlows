//! 管理看板 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_dashboard::{
        AdminDashboardChannelFlowResponse, AdminDashboardFailureKindResponse,
        AdminDashboardFailureResponse, AdminDashboardFlowPathResponse,
        AdminDashboardHourlyPointResponse, AdminDashboardPerformanceResponse,
        AdminDashboardResponse,
    },
    management_error::ManagementErrorBody,
    management_service_levels::{
        ServiceLevelPointResponse, ServiceLevelReportResponse, ServiceLevelRowResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/dashboard",
    operation_id = "getAdminDashboard",
    tag = "管理看板",
    summary = "读取管理员看板概览",
    responses(
        (status = 200, description = "最近 24 小时用量、请求终态和当前渠道状态", body = AdminDashboardResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_dashboard() {}

#[utoipa::path(
    get,
    path = "/api/admin/dashboard/service-levels",
    operation_id = "getAdminServiceLevels",
    tag = "管理看板",
    summary = "分页读取模型与渠道请求 SLA",
    params(
        ("dimension" = Option<String>, Query, description = "model 或 channel，默认 model"),
        ("search" = Option<String>, Query, description = "名称筛选，最多 128 个字符"),
        ("page" = Option<u32>, Query, description = "页码，1 到 10000"),
        ("page_size" = Option<u32>, Query, description = "每页 1 到 20 条，默认 10"),
        ("sort" = Option<String>, Query, description = "requests 或 failures，默认 requests")
    ),
    responses(
        (status = 200, description = "最近 24 小时的请求终态与小时健康分布；未知结果不计入成功率", body = ServiceLevelReportResponse),
        (status = 400, description = "筛选参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "统计暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_service_levels() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_dashboard, get_service_levels),
    components(schemas(
        AdminDashboardResponse,
        AdminDashboardFailureKindResponse,
        AdminDashboardFailureResponse,
        AdminDashboardChannelFlowResponse,
        AdminDashboardFlowPathResponse,
        AdminDashboardHourlyPointResponse,
        AdminDashboardPerformanceResponse,
        ServiceLevelReportResponse,
        ServiceLevelRowResponse,
        ServiceLevelPointResponse,
        ManagementErrorBody
    ))
)]
struct DashboardApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    DashboardApi::openapi()
}
