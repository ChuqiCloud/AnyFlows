//! 管理看板 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_dashboard::{
        AdminDashboardHourlyPointResponse, AdminDashboardPerformanceResponse,
        AdminDashboardResponse,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/dashboard",
    operation_id = "getAdminDashboard",
    tag = "管理看板",
    summary = "读取管理员看板概览",
    responses(
        (status = 200, description = "最近 24 小时用量和当前渠道状态", body = AdminDashboardResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_dashboard() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_dashboard),
    components(schemas(
        AdminDashboardResponse,
        AdminDashboardHourlyPointResponse,
        AdminDashboardPerformanceResponse,
        ManagementErrorBody
    ))
)]
struct DashboardApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    DashboardApi::openapi()
}
