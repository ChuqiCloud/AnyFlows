//! 管理员余额预警设置 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    balance_alert_settings::{AdminBalanceAlertSettingsRequest, AdminBalanceAlertSettingsResponse},
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/balance-alert-settings",
    operation_id = "getAdminBalanceAlertSettings",
    tag = "余额预警",
    summary = "读取余额预警全局设置",
    responses(
        (status = 200, description = "余额预警设置", body = AdminBalanceAlertSettingsResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_balance_alert_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/balance-alert-settings",
    operation_id = "updateAdminBalanceAlertSettings",
    tag = "余额预警",
    summary = "更新余额预警全局设置",
    request_body = AdminBalanceAlertSettingsRequest,
    responses(
        (status = 200, description = "余额预警设置已更新", body = AdminBalanceAlertSettingsResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_balance_alert_settings() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_balance_alert_settings, update_admin_balance_alert_settings),
    components(schemas(
        AdminBalanceAlertSettingsResponse,
        AdminBalanceAlertSettingsRequest,
        ManagementErrorBody
    ))
)]
struct BalanceAlertSettingsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    BalanceAlertSettingsApi::openapi()
}
