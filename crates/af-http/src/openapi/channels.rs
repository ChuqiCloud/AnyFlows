//! 渠道管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_channel_probe::{AdminChannelProbeResponse, AdminChannelProbeStatus},
    management_channel_writes::{AdminChannelCreateRequest, AdminChannelUpdateRequest},
    management_channels::{
        AdminChannelListResponse, AdminChannelResponse, AdminResponsesCompactProbeDto,
    },
    management_error::ManagementErrorBody,
    openapi::schema::{
        AdminChannelProtocolSchema, AdminChannelTypeSchema, AdminRoutingStatusSchema,
        AdminRoutingWriteStatusSchema, ClientSimulationBodyProfileSchema,
        ClientSimulationProfileSchema, ResponsesCompactModeSchema,
        ResponsesCompactProbeResultSchema,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/channels",
    operation_id = "listAdminChannels",
    tag = "渠道管理",
    summary = "读取管理员渠道列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的渠道 ID"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "渠道列表", body = AdminChannelListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_channels() {}

#[utoipa::path(
    post,
    path = "/api/admin/channels",
    operation_id = "createAdminChannel",
    tag = "渠道管理",
    summary = "创建管理员渠道",
    request_body = AdminChannelCreateRequest,
    responses(
        (status = 201, description = "渠道已创建", body = AdminChannelResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_channel() {}

#[utoipa::path(
    get,
    path = "/api/admin/channels/{id}",
    operation_id = "getAdminChannel",
    tag = "渠道管理",
    summary = "读取管理员渠道详情",
    params(("id" = i64, Path, minimum = 1, description = "渠道 ID")),
    responses(
        (status = 200, description = "渠道详情", body = AdminChannelResponse),
        (status = 400, description = "渠道 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_channel() {}

#[utoipa::path(
    put,
    path = "/api/admin/channels/{id}",
    operation_id = "updateAdminChannel",
    tag = "渠道管理",
    summary = "更新管理员渠道",
    params(("id" = i64, Path, minimum = 1, description = "渠道 ID")),
    request_body = AdminChannelUpdateRequest,
    responses(
        (status = 200, description = "渠道已更新", body = AdminChannelResponse),
        (status = 400, description = "渠道 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_channel() {}

#[utoipa::path(
    delete,
    path = "/api/admin/channels/{id}",
    operation_id = "deleteAdminChannel",
    tag = "渠道管理",
    summary = "软删除管理员渠道",
    params(("id" = i64, Path, minimum = 1, description = "渠道 ID")),
    responses(
        (status = 204, description = "渠道已删除"),
        (status = 400, description = "渠道 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_channel() {}

#[utoipa::path(
    post,
    path = "/api/admin/channels/{id}/probe",
    operation_id = "probeAdminChannel",
    tag = "渠道管理",
    summary = "对管理员渠道执行一次真实测活",
    params(("id" = i64, Path, minimum = 1, description = "渠道 ID")),
    responses(
        (status = 200, description = "脱敏测活结论", body = AdminChannelProbeResponse),
        (status = 400, description = "渠道 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 503, description = "当前实例未注入渠道测活服务", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn probe_admin_channel() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_channels,
        create_admin_channel,
        get_admin_channel,
        update_admin_channel,
        delete_admin_channel,
        probe_admin_channel
    ),
    components(schemas(
        AdminChannelTypeSchema,
        AdminChannelProtocolSchema,
        AdminRoutingStatusSchema,
        AdminRoutingWriteStatusSchema,
        ClientSimulationBodyProfileSchema,
        ClientSimulationProfileSchema,
        ResponsesCompactModeSchema,
        ResponsesCompactProbeResultSchema,
        AdminChannelResponse,
        AdminChannelListResponse,
        AdminChannelCreateRequest,
        AdminChannelUpdateRequest,
        AdminResponsesCompactProbeDto,
        AdminChannelProbeStatus,
        AdminChannelProbeResponse,
        ManagementErrorBody
    ))
)]
struct ChannelsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ChannelsApi::openapi()
}
