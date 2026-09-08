#![allow(dead_code, reason = "OpenAPI 文档端点仅由 utoipa 宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_scim_audit::OrganizationScimAuditLogListResponse,
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/scim/audit-logs",
    operation_id = "listOrganizationScimAuditLogs",
    tag = "企业 SCIM",
    summary = "读取企业 SCIM 脱敏审计",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业 ID"),
        ("before" = Option<i64>, Query, minimum = 1, description = "倒序游标"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量"),
        ("event_type" = Option<String>, Query, description = "闭合 SCIM 事件码"),
        ("result" = Option<String>, Query, description = "闭合 SCIM 结果码"),
        ("reason" = Option<String>, Query, description = "闭合 SCIM 原因码"),
        ("token_key" = Option<String>, Query, description = "令牌公开标识"),
        ("resource_kind" = Option<String>, Query, description = "资源种类 users/groups")
    ),
    responses(
        (status = 200, description = "企业范围的脱敏 SCIM 审计事实", body = OrganizationScimAuditLogListResponse),
        (status = 400, description = "查询参数无效", body = ManagementErrorBody),
        (status = 401, description = "管理会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少企业审计读取权限", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或不可访问", body = ManagementErrorBody),
        (status = 500, description = "SCIM 审计读取服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_scim_audit_logs() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_organization_scim_audit_logs),
    components(schemas(OrganizationScimAuditLogListResponse, ManagementErrorBody))
)]
struct OrganizationScimAuditApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationScimAuditApi::openapi()
}
