//! 公开注册与管理员注册策略 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::{LoginResponse, SessionUser},
    registration::{
        AdminAuthenticationSettingsRequest, AdminAuthenticationSettingsResponse,
        RegistrationEmailVerificationRequest, RegistrationEmailVerificationResponse,
        RegistrationRequest, RegistrationStatusResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/registration/status",
    operation_id = "getRegistrationStatus",
    tag = "公开注册",
    summary = "读取公开注册状态",
    responses(
        (status = 200, description = "公开注册能力状态", body = RegistrationStatusResponse),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn get_registration_status() {}

#[utoipa::path(
    post,
    path = "/api/registration/email-verification",
    operation_id = "sendRegistrationEmailVerification",
    tag = "公开注册",
    summary = "发送注册邮箱验证码",
    request_body = RegistrationEmailVerificationRequest,
    responses(
        (status = 202, description = "验证码已投递，并返回服务端时间边界", body = RegistrationEmailVerificationResponse),
        (status = 400, description = "请求正文或邮箱无效", body = ManagementErrorBody),
        (status = 403, description = "公开注册未启用", body = ManagementErrorBody),
        (status = 409, description = "SMTP 设置尚未完整配置", body = ManagementErrorBody),
        (status = 429, description = "主体、客户端 IP 或发送冷却窗口仍受限", body = ManagementErrorBody),
        (status = 503, description = "Turnstile 验证服务不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody),
        (status = 502, description = "SMTP 连接、认证或投递失败", body = ManagementErrorBody)
    )
)]
fn send_registration_email_verification() {}

#[utoipa::path(
    post,
    path = "/api/registration",
    operation_id = "registerUser",
    tag = "公开注册",
    summary = "注册普通用户并签发登录会话",
    request_body = RegistrationRequest,
    responses(
        (status = 201, description = "注册及自动登录成功", body = LoginResponse),
        (status = 400, description = "请求正文或字段无效", body = ManagementErrorBody),
        (status = 403, description = "公开注册未启用", body = ManagementErrorBody),
        (status = 503, description = "Turnstile 验证服务不可用", body = ManagementErrorBody),
        (status = 409, description = "验证码无效或公开身份无法创建", body = ManagementErrorBody),
        (status = 429, description = "固定窗口注册尝试次数已耗尽", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn register_user() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings",
    operation_id = "getAdminAuthenticationSettings",
    tag = "认证设置",
    summary = "读取管理员认证与注册设置",
    responses(
        (status = 200, description = "完整认证与注册设置", body = AdminAuthenticationSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_authentication_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings",
    operation_id = "updateAdminAuthenticationSettings",
    tag = "认证设置",
    summary = "覆盖管理员认证与注册设置",
    request_body = AdminAuthenticationSettingsRequest,
    responses(
        (status = 200, description = "更新后的完整认证与注册设置", body = AdminAuthenticationSettingsResponse),
        (status = 400, description = "认证字段或默认分组无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_authentication_settings() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_registration_status,
        send_registration_email_verification,
        register_user,
        get_admin_authentication_settings,
        update_admin_authentication_settings
    ),
    components(schemas(
        RegistrationStatusResponse,
        RegistrationEmailVerificationRequest,
        RegistrationEmailVerificationResponse,
        RegistrationRequest,
        AdminAuthenticationSettingsResponse,
        AdminAuthenticationSettingsRequest,
        SessionUser,
        LoginResponse,
        ManagementErrorBody
    ))
)]
struct RegistrationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RegistrationApi::openapi()
}
