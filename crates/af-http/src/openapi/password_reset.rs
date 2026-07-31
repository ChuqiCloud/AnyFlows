#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    password_reset::{
        PasswordResetConfirmRequest, PasswordResetRequestBody, PasswordResetRequestResponse,
    },
};

#[utoipa::path(
    post,
    path = "/api/auth/password-reset/request",
    operation_id = "requestPasswordReset",
    tag = "密码重置",
    summary = "请求密码重置邮件",
    request_body = PasswordResetRequestBody,
    responses(
        (status = 202, description = "请求已受理，不区分邮箱是否存在", body = PasswordResetRequestResponse),
        (status = 400, description = "邮箱或请求正文无效", body = ManagementErrorBody),
        (status = 409, description = "邮件或公开站点基址尚未配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody),
        (status = 502, description = "SMTP 投递失败", body = ManagementErrorBody)
    )
)]
fn request_password_reset() {}

#[utoipa::path(
    post,
    path = "/api/auth/password-reset/confirm",
    operation_id = "confirmPasswordReset",
    tag = "密码重置",
    summary = "确认密码重置",
    request_body = PasswordResetConfirmRequest,
    responses(
        (status = 204, description = "密码已更新并撤销既有会话"),
        (status = 400, description = "请求正文、令牌或密码无效", body = ManagementErrorBody),
        (status = 409, description = "令牌无效、过期、重放或目标不可重置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn confirm_password_reset() {}

#[derive(OpenApi)]
#[openapi(
    paths(request_password_reset, confirm_password_reset),
    components(schemas(
        PasswordResetRequestBody,
        PasswordResetRequestResponse,
        PasswordResetConfirmRequest,
        ManagementErrorBody
    ))
)]
struct PasswordResetApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PasswordResetApi::openapi()
}
