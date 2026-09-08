//! 企业调用日志、管理审计和平台代理读取的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_usage_logs::AdminUsageLogResponse,
    organization_activity::{
        OrganizationAuditLogListResponse, OrganizationAuditLogResponse,
        OrganizationAuditOutcomeResponse, OrganizationUsageLogListResponse,
        OrganizationUsageLogSummaryResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/usage-logs",
    operation_id = "listOrganizationUsageLogs",
    tag = "企业活动",
    summary = "按企业角色读取调用日志",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("scope" = Option<String>, Query, description = "读取范围：organization、self 或 summary；默认 self"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾日志标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量")
    ),
    responses(
        (status = 200, description = "角色裁剪后的企业调用日志", body = OrganizationUsageLogListResponse),
        (status = 400, description = "企业标识、范围或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前企业角色无权读取该范围", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_usage_logs() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/audit-logs",
    operation_id = "listOrganizationAuditLogs",
    tag = "企业活动",
    summary = "读取企业管理审计",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾审计标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量"),
        ("actor_user_id" = Option<i64>, Query, minimum = 1, description = "按操作者筛选"),
        ("operation" = Option<String>, Query, max_length = 96, description = "按稳定操作码筛选"),
        ("target_type" = Option<String>, Query, max_length = 32, description = "按目标类型筛选"),
        ("outcome" = Option<String>, Query, description = "按 succeeded、denied 或 failed 筛选")
    ),
    responses(
        (status = 200, description = "按角色脱敏后的管理审计", body = OrganizationAuditLogListResponse),
        (status = 400, description = "企业标识或筛选参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前企业角色无权读取审计", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_audit_logs() {}

#[utoipa::path(
    get,
    path = "/api/platform/organizations/{organization_id}/usage-logs",
    operation_id = "listPlatformOrganizationUsageLogs",
    tag = "平台企业活动",
    summary = "平台管理员跨企业读取调用日志",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "目标企业内部标识"),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业访问理由"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾日志标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量")
    ),
    responses(
        (status = 200, description = "企业完整调用日志；访问已进入平台代理审计", body = OrganizationUsageLogListResponse),
        (status = 400, description = "企业标识、理由或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "不是平台管理员", body = ManagementErrorBody),
        (status = 404, description = "企业不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_platform_organization_usage_logs() {}

#[utoipa::path(
    get,
    path = "/api/platform/organizations/{organization_id}/audit-logs",
    operation_id = "listPlatformOrganizationAuditLogs",
    tag = "平台企业活动",
    summary = "平台管理员跨企业读取管理审计",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "目标企业内部标识"),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业访问理由"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾审计标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量"),
        ("actor_user_id" = Option<i64>, Query, minimum = 1, description = "按操作者筛选"),
        ("operation" = Option<String>, Query, max_length = 96, description = "按稳定操作码筛选"),
        ("target_type" = Option<String>, Query, max_length = 32, description = "按目标类型筛选"),
        ("outcome" = Option<String>, Query, description = "按 succeeded、denied 或 failed 筛选")
    ),
    responses(
        (status = 200, description = "企业管理审计；本次访问也已进入平台代理审计", body = OrganizationAuditLogListResponse),
        (status = 400, description = "企业标识、理由或筛选参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "不是平台管理员", body = ManagementErrorBody),
        (status = 404, description = "企业不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_platform_organization_audit_logs() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_usage_logs,
        list_organization_audit_logs,
        list_platform_organization_usage_logs,
        list_platform_organization_audit_logs
    ),
    components(schemas(
        AdminUsageLogResponse,
        OrganizationUsageLogSummaryResponse,
        OrganizationUsageLogListResponse,
        OrganizationAuditOutcomeResponse,
        OrganizationAuditLogResponse,
        OrganizationAuditLogListResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationActivityApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationActivityApi::openapi()
}
