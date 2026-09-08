//! 分析导出运维 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_analytics_export::{
        AnalyticsExportHealthState, AnalyticsExportReplayRequest, AnalyticsExportReplayResponse,
        AnalyticsExportStatusResponse,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/analytics/export-status",
    operation_id = "getAdminAnalyticsExportStatus",
    tag = "分析导出",
    summary = "读取 ClickHouse 异步事实导出状态",
    responses(
        (status = 200, description = "导出开关、队列积压和已发布计数", body = AnalyticsExportStatusResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_analytics_export_status() {}

#[utoipa::path(
    post,
    path = "/api/admin/analytics/export-replay",
    operation_id = "replayAdminAnalyticsExport",
    tag = "分析导出",
    summary = "有界重放 ClickHouse 异步事实积压",
    request_body = AnalyticsExportReplayRequest,
    responses(
        (status = 200, description = "本次重排数量与当前积压", body = AnalyticsExportReplayResponse),
        (status = 400, description = "重放上限无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn replay_admin_analytics_export() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_analytics_export_status, replay_admin_analytics_export),
    components(schemas(
        AnalyticsExportHealthState,
        AnalyticsExportStatusResponse,
        AnalyticsExportReplayRequest,
        AnalyticsExportReplayResponse,
        ManagementErrorBody
    ))
)]
struct AnalyticsExportApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    AnalyticsExportApi::openapi()
}
