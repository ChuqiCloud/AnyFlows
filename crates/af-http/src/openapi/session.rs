//! 管理登录与当前会话 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::{LoginRequest, LoginResponse, SessionResponse, SessionUser},
};

#[utoipa::path(
    post,
    path = "/api/auth/login",
    operation_id = "loginManagementSession",
    tag = "管理会话",
    summary = "登录并签发管理会话",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "登录成功", body = LoginResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "用户名或密码错误", body = ManagementErrorBody),
        (status = 403, description = "用户名密码登录已关闭", body = ManagementErrorBody),
        (status = 503, description = "Turnstile 验证服务不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn login_management_session() {}

#[utoipa::path(
    get,
    path = "/api/auth/session",
    operation_id = "getManagementSession",
    tag = "管理会话",
    summary = "读取当前管理会话",
    responses(
        (status = 200, description = "当前会话", body = SessionResponse),
        (status = 401, description = "会话无效或已过期", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_management_session() {}

#[derive(OpenApi)]
#[openapi(
    paths(login_management_session, get_management_session),
    components(schemas(
        LoginRequest,
        SessionUser,
        LoginResponse,
        SessionResponse,
        ManagementErrorBody
    ))
)]
struct SessionApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    SessionApi::openapi()
}
