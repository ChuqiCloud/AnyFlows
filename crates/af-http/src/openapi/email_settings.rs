//! 管理员邮件设置与测试投递 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    email_settings::{
        AdminEmailSettingsRequest, AdminEmailSettingsResponse, AdminEmailTestRequest,
        AdminEmailTlsModeDto,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/email-settings",
    operation_id = "getAdminEmailSettings",
    tag = "邮件设置",
    summary = "读取管理员 SMTP 设置",
    responses(
        (status = 200, description = "密码保持脱敏的完整 SMTP 设置", body = AdminEmailSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_email_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/email-settings",
    operation_id = "updateAdminEmailSettings",
    tag = "邮件设置",
    summary = "覆盖管理员 SMTP 设置",
    request_body = AdminEmailSettingsRequest,
    responses(
        (status = 200, description = "更新后的脱敏 SMTP 设置", body = AdminEmailSettingsResponse),
        (status = 400, description = "请求正文或 SMTP 字段无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_email_settings() {}

#[utoipa::path(
    post,
    path = "/api/admin/email-settings/test",
    operation_id = "sendAdminEmailTest",
    tag = "邮件设置",
    summary = "发送固定正文 SMTP 测试邮件",
    request_body = AdminEmailTestRequest,
    responses(
        (status = 204, description = "测试邮件投递成功，不回显收件人或正文"),
        (status = 400, description = "请求正文或收件人无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "SMTP 设置尚未完整配置", body = ManagementErrorBody),
        (status = 502, description = "SMTP 连接、认证或投递失败", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn send_admin_email_test() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_admin_email_settings,
        update_admin_email_settings,
        send_admin_email_test
    ),
    components(schemas(
        AdminEmailSettingsResponse,
        AdminEmailSettingsRequest,
        AdminEmailTestRequest,
        AdminEmailTlsModeDto,
        ManagementErrorBody
    ))
)]
struct EmailSettingsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    EmailSettingsApi::openapi()
}
