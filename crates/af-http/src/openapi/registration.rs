//! 公开注册与管理员注册策略 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::{LoginResponse, SessionUser},
    registration::{
        RegistrationPolicyRequest, RegistrationPolicyResponse, RegistrationRequest,
        RegistrationStatusResponse,
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
    path = "/api/registration",
    operation_id = "registerUser",
    tag = "公开注册",
    summary = "注册普通用户并签发登录会话",
    request_body = RegistrationRequest,
    responses(
        (status = 201, description = "注册及自动登录成功", body = LoginResponse),
        (status = 400, description = "请求正文或字段无效", body = ManagementErrorBody),
        (status = 403, description = "公开注册未启用", body = ManagementErrorBody),
        (status = 409, description = "用户名或邮箱冲突", body = ManagementErrorBody),
        (status = 429, description = "固定窗口注册尝试次数已耗尽", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn register_user() {}

#[utoipa::path(
    get,
    path = "/api/admin/registration",
    operation_id = "getAdminRegistrationPolicy",
    tag = "注册设置",
    summary = "读取管理员注册策略",
    responses(
        (status = 200, description = "完整注册策略", body = RegistrationPolicyResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_registration_policy() {}

#[utoipa::path(
    put,
    path = "/api/admin/registration",
    operation_id = "updateAdminRegistrationPolicy",
    tag = "注册设置",
    summary = "覆盖管理员注册策略",
    request_body = RegistrationPolicyRequest,
    responses(
        (status = 200, description = "更新后的完整注册策略", body = RegistrationPolicyResponse),
        (status = 400, description = "策略字段或默认分组无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_registration_policy() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_registration_status,
        register_user,
        get_admin_registration_policy,
        update_admin_registration_policy
    ),
    components(schemas(
        RegistrationStatusResponse,
        RegistrationRequest,
        RegistrationPolicyResponse,
        RegistrationPolicyRequest,
        SessionUser,
        LoginResponse,
        ManagementErrorBody
    ))
)]
struct RegistrationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RegistrationApi::openapi()
}
