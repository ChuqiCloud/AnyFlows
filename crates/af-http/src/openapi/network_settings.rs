//! 管理员全局网络设置 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    network_settings::{
        AdminNetworkSettingsModeDto, AdminNetworkSettingsRequest, AdminNetworkSettingsResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/network-settings",
    operation_id = "getAdminNetworkSettings",
    tag = "网络与代理",
    summary = "读取全局出站网络设置",
    responses(
        (status = 200, description = "密码保持脱敏的网络设置", body = AdminNetworkSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_network_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/network-settings",
    operation_id = "updateAdminNetworkSettings",
    tag = "网络与代理",
    summary = "更新全局出站网络设置",
    request_body = AdminNetworkSettingsRequest,
    responses(
        (status = 200, description = "更新后的网络设置", body = AdminNetworkSettingsResponse),
        (status = 400, description = "请求正文或网络设置字段无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_network_settings() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_network_settings, update_admin_network_settings),
    components(schemas(
        AdminNetworkSettingsResponse,
        AdminNetworkSettingsRequest,
        AdminNetworkSettingsModeDto,
        ManagementErrorBody
    ))
)]
struct NetworkSettingsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    NetworkSettingsApi::openapi()
}
