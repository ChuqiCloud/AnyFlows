//! 平台管理审计 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    platform_audit::{
        PlatformAuditLogListResponse, PlatformAuditLogResponse, PlatformAuditOutcomeResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/audit-logs",
    operation_id = "listAdminPlatformAuditLogs",
    tag = "平台管理审计",
    summary = "读取全站平台管理审计",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的审计 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "完整平台管理审计", body = PlatformAuditLogListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要平台审计读取权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_platform_audit_logs() {}

#[utoipa::path(
    get,
    path = "/api/account/audit-logs",
    operation_id = "listSelfPlatformAuditLogs",
    tag = "平台管理审计",
    summary = "读取当前用户自己的裁剪平台审计",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的审计 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "仅当前用户记录；操作者名、前后值和 audit_info 均已裁剪", body = PlatformAuditLogListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_self_platform_audit_logs() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_admin_platform_audit_logs, list_self_platform_audit_logs),
    components(schemas(
        PlatformAuditOutcomeResponse,
        PlatformAuditLogResponse,
        PlatformAuditLogListResponse,
        ManagementErrorBody
    ))
)]
struct PlatformAuditApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PlatformAuditApi::openapi()
}
