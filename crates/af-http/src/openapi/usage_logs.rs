//! 用量日志 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_usage_logs::{
        AdminFailedCallLogResponse, AdminUsageLogListResponse, AdminUsageLogResponse,
        UserFailedCallLogResponse, UserUsageLogListResponse, UserUsageLogResponse,
    },
    openapi::schema::{
        AdminUsageLogBillingModeSchema, AdminUsageLogSemanticsSchema, AdminUsageLogSourceSchema,
        AdminUsageLogVideoResolutionSchema, UsageLogOperationSchema, UsageLogProtocolSchema,
        UsageLogReasoningEffortSchema,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/usage-logs",
    operation_id = "listAdminUsageLogs",
    tag = "用量日志",
    summary = "读取管理员用量日志列表",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的日志 ID"),
        ("failed_before" = Option<i64>, Query, minimum = 1, description = "失败日志上一页末尾的日志 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "从新到旧排列的成功和失败调用日志", body = AdminUsageLogListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_usage_logs() {}

#[utoipa::path(
    get,
    path = "/api/account/usage-logs",
    operation_id = "listUserUsageLogs",
    tag = "调用日志",
    summary = "读取当前用户调用日志列表",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的日志 ID"),
        ("failed_before" = Option<i64>, Query, minimum = 1, description = "失败日志上一页末尾的日志 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "从新到旧排列的本人成功和失败调用", body = UserUsageLogListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_usage_logs() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_admin_usage_logs, list_user_usage_logs),
    components(schemas(
        AdminUsageLogBillingModeSchema,
        AdminUsageLogSourceSchema,
        AdminUsageLogSemanticsSchema,
        AdminUsageLogVideoResolutionSchema,
        UsageLogProtocolSchema,
        UsageLogOperationSchema,
        UsageLogReasoningEffortSchema,
        AdminFailedCallLogResponse,
        AdminUsageLogResponse,
        AdminUsageLogListResponse,
        UserUsageLogResponse,
        UserFailedCallLogResponse,
        UserUsageLogListResponse,
        ManagementErrorBody
    ))
)]
struct UsageLogsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UsageLogsApi::openapi()
}
