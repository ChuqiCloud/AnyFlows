//! 用户名优先 Passkey 登录 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::LoginResponse,
    passkey_auth::{
        PasskeyAuthenticationOptionsRequest, PasskeyAuthenticationOptionsResponse,
        PasskeyAuthenticationVerifyRequest,
    },
};

#[utoipa::path(
    post,
    path = "/api/auth/passkey/options",
    operation_id = "startPasskeyAuthentication",
    tag = "Passkey 登录",
    summary = "创建 Passkey 登录挑战",
    request_body = PasskeyAuthenticationOptionsRequest,
    responses(
        (status = 200, description = "WebAuthn 登录选项", body = PasskeyAuthenticationOptionsResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "Passkey 登录失败", body = ManagementErrorBody),
        (status = 403, description = "密码登录策略已关闭", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_passkey_authentication() {}

#[utoipa::path(
    post,
    path = "/api/auth/passkey/verify",
    operation_id = "finishPasskeyAuthentication",
    tag = "Passkey 登录",
    summary = "验证 Passkey 并签发会话",
    request_body = PasskeyAuthenticationVerifyRequest,
    responses(
        (status = 200, description = "登录成功", body = LoginResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "Passkey 登录失败", body = ManagementErrorBody),
        (status = 403, description = "密码登录策略已关闭", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn finish_passkey_authentication() {}

#[derive(OpenApi)]
#[openapi(
    paths(start_passkey_authentication, finish_passkey_authentication),
    components(schemas(
        PasskeyAuthenticationOptionsRequest,
        PasskeyAuthenticationOptionsResponse,
        PasskeyAuthenticationVerifyRequest,
        LoginResponse,
        ManagementErrorBody
    ))
)]
struct PasskeyAuthenticationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PasskeyAuthenticationApi::openapi()
}
