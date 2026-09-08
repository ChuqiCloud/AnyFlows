//! 管理员在线支付设置 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    payment_settings::{AdminPaymentSettingsRequest, AdminPaymentSettingsResponse},
};

#[utoipa::path(
    get,
    path = "/api/admin/payment-settings",
    operation_id = "getAdminPaymentSettings",
    tag = "支付设置",
    summary = "读取在线支付设置",
    responses(
        (status = 200, description = "密钥保持脱敏的支付设置", body = AdminPaymentSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_payment_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/payment-settings",
    operation_id = "updateAdminPaymentSettings",
    tag = "支付设置",
    summary = "更新在线支付设置",
    request_body = AdminPaymentSettingsRequest,
    responses(
        (status = 200, description = "更新并热生效后的支付设置", body = AdminPaymentSettingsResponse),
        (status = 400, description = "请求正文或支付设置字段无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "运行时切换失败或服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_payment_settings() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_payment_settings, update_admin_payment_settings),
    components(schemas(
        AdminPaymentSettingsResponse,
        AdminPaymentSettingsRequest,
        ManagementErrorBody
    ))
)]
struct PaymentSettingsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PaymentSettingsApi::openapi()
}
