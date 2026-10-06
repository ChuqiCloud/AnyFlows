//! 调试追踪管理 API 的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 宏读取")]

use utoipa::OpenApi;

use crate::{
    debug_traces::{
        AdminDebugTraceAttemptResponse, AdminDebugTraceAttemptSnapshotResponse,
        AdminDebugTraceDetailResponse, AdminDebugTraceDownstreamRequestResponse,
        AdminDebugTraceListResponse, AdminDebugTraceResponse, AdminDebugTraceSettingsRequest,
        AdminDebugTraceSettingsResponse, AdminDebugTraceSnapshotRequest,
        AdminDebugTraceSnapshotScopeRequest, AdminDebugTraceSnapshotsResponse,
    },
    management_error::ManagementErrorBody,
    openapi::schema::{
        ClientSimulationBodyPatchResultSchema, ClientSimulationProfileSchema,
        ClientSimulationResultSchema,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/debug-trace-settings",
    operation_id = "getAdminDebugTraceSettings",
    tag = "调试追踪",
    summary = "读取调试追踪设置",
    responses(
        (status = 200, description = "当前实例使用的调试追踪设置", body = AdminDebugTraceSettingsResponse),
        (status = 401, description = "管理会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_debug_trace_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/debug-trace-settings",
    operation_id = "updateAdminDebugTraceSettings",
    tag = "调试追踪",
    summary = "更新调试追踪设置",
    request_body = AdminDebugTraceSettingsRequest,
    responses(
        (status = 200, description = "已保存并应用到当前实例的设置", body = AdminDebugTraceSettingsResponse),
        (status = 400, description = "设置参数无效", body = ManagementErrorBody),
        (status = 401, description = "管理会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_debug_trace_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/debug-traces",
    operation_id = "listAdminDebugTraces",
    tag = "调试追踪",
    summary = "读取脱敏调试追踪列表",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的追踪 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50"),
        ("outcome" = Option<String>, Query, description = "succeeded 或 failed"),
        ("model" = Option<String>, Query, description = "按请求模型精确筛选"),
        ("request_id" = Option<String>, Query, min_length = 1, max_length = 64, description = "按服务端请求 ID 精确筛选")
    ),
    responses(
        (status = 200, description = "从新到旧排列的脱敏追踪摘要", body = AdminDebugTraceListResponse),
        (status = 400, description = "查询参数无效", body = ManagementErrorBody),
        (status = 401, description = "管理会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_debug_traces() {}

#[utoipa::path(
    get,
    path = "/api/admin/debug-traces/{id}",
    operation_id = "getAdminDebugTrace",
    tag = "调试追踪",
    summary = "读取调试追踪候选时间线",
    params(("id" = i64, Path, minimum = 1, description = "追踪记录 ID")),
    responses(
        (status = 200, description = "脱敏请求摘要和候选时间线", body = AdminDebugTraceDetailResponse),
        (status = 400, description = "追踪 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "管理会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "追踪记录不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_debug_trace() {}

#[utoipa::path(
    post,
    path = "/api/admin/debug-traces/{id}/snapshots",
    operation_id = "readAdminDebugTraceSnapshots",
    tag = "调试追踪",
    summary = "审计读取调试追踪敏感快照",
    description = "显式读取 Header 或正文其中一个范围；每次读取都会写入独立审计。",
    params(("id" = i64, Path, minimum = 1, description = "追踪记录 ID")),
    request_body = AdminDebugTraceSnapshotRequest,
    responses(
        (status = 200, description = "已审计并解密的指定范围快照", body = AdminDebugTraceSnapshotsResponse),
        (status = 400, description = "追踪 ID 或读取范围无效", body = ManagementErrorBody),
        (status = 401, description = "管理会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "追踪或指定范围快照不存在", body = ManagementErrorBody),
        (status = 500, description = "审计写入、解密或服务内部失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn read_admin_debug_trace_snapshots() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_admin_debug_trace_settings,
        update_admin_debug_trace_settings,
        list_admin_debug_traces,
        get_admin_debug_trace,
        read_admin_debug_trace_snapshots
    ),
    components(schemas(
        AdminDebugTraceSettingsResponse,
        AdminDebugTraceSettingsRequest,
        AdminDebugTraceListResponse,
        AdminDebugTraceDetailResponse,
        AdminDebugTraceResponse,
        AdminDebugTraceAttemptResponse,
        AdminDebugTraceDownstreamRequestResponse,
        AdminDebugTraceSnapshotScopeRequest,
        AdminDebugTraceSnapshotRequest,
        AdminDebugTraceAttemptSnapshotResponse,
        AdminDebugTraceSnapshotsResponse,
        ClientSimulationBodyPatchResultSchema,
        ClientSimulationProfileSchema,
        ClientSimulationResultSchema,
        ManagementErrorBody
    ))
)]
struct DebugTracesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    DebugTracesApi::openapi()
}
