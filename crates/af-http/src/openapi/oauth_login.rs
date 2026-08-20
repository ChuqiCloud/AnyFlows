//! 用户登录 OAuth 与管理员内置 OAuth App 设置契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::LoginResponse,
    oauth_login::{
        AdminOAuthLoginProviderSettingsRequest, AdminOAuthLoginProviderSettingsResponse,
        OAuthLoginExchangeRequest, OAuthLoginStartResponse,
    },
};

#[utoipa::path(
    post,
    path = "/api/auth/oauth/github/start",
    operation_id = "startGitHubOAuthLogin",
    tag = "OAuth 登录",
    summary = "启动 GitHub OAuth 登录",
    responses(
        (status = 200, description = "服务端构造的 GitHub 授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "GitHub OAuth 登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_github_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/discord/start",
    operation_id = "startDiscordOAuthLogin",
    tag = "OAuth 登录",
    summary = "启动 Discord OAuth 登录",
    responses(
        (status = 200, description = "服务端构造的 Discord 授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "Discord OAuth 登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_discord_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/oidc/start",
    operation_id = "startOidcLogin",
    tag = "OAuth 登录",
    summary = "启动 OIDC 登录",
    responses(
        (status = 200, description = "服务端构造的 OIDC 授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "OIDC 登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_oidc_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/linuxdo/start",
    operation_id = "startLinuxDoLogin",
    tag = "OAuth 登录",
    summary = "启动 LinuxDO 登录",
    responses(
        (status = 200, description = "服务端构造的 LinuxDO 授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "LinuxDO 登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_linuxdo_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/github/callback",
    operation_id = "completeGitHubOAuthLogin",
    tag = "OAuth 登录",
    summary = "接收 GitHub OAuth 回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "GitHub 授权码"),
        ("error" = Option<String>, Query, description = "GitHub 拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端单次票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_github_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/discord/callback",
    operation_id = "completeDiscordOAuthLogin",
    tag = "OAuth 登录",
    summary = "接收 Discord OAuth 回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "Discord 授权码"),
        ("error" = Option<String>, Query, description = "Discord 拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端单次票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_discord_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/oidc/callback",
    operation_id = "completeOidcLogin",
    tag = "OAuth 登录",
    summary = "接收 OIDC 回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "OIDC 授权码"),
        ("error" = Option<String>, Query, description = "OIDC 拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端单次票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_oidc_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/linuxdo/callback",
    operation_id = "completeLinuxDoLogin",
    tag = "OAuth 登录",
    summary = "接收 LinuxDO 回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "LinuxDO 授权码"),
        ("error" = Option<String>, Query, description = "LinuxDO 拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端单次票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_linuxdo_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/exchange",
    operation_id = "exchangeOAuthLoginTicket",
    tag = "OAuth 登录",
    summary = "交换一次性 OAuth 登录票据",
    request_body = OAuthLoginExchangeRequest,
    responses(
        (status = 200, description = "现有 bearer 登录会话", body = LoginResponse),
        (status = 400, description = "票据格式无效", body = ManagementErrorBody),
        (status = 409, description = "票据无效、过期或已消费", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn exchange_oauth_login_ticket() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/github",
    operation_id = "getAdminGitHubOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "读取 GitHub OAuth 登录设置",
    responses(
        (status = 200, description = "脱敏后的 GitHub OAuth App 设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_github_oauth_login_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/discord",
    operation_id = "getAdminDiscordOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "读取 Discord OAuth 登录设置",
    responses(
        (status = 200, description = "脱敏后的 Discord OAuth App 设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_discord_oauth_login_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/oidc",
    operation_id = "getAdminOidcLoginSettings",
    tag = "OAuth 登录",
    summary = "读取 OIDC 登录设置",
    responses(
        (status = 200, description = "脱敏后的 OIDC 设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_oidc_oauth_login_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/linuxdo",
    operation_id = "getAdminLinuxDoLoginSettings",
    tag = "OAuth 登录",
    summary = "读取 LinuxDO 登录设置",
    responses(
        (status = 200, description = "脱敏后的 LinuxDO 设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_linuxdo_oauth_login_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/github",
    operation_id = "updateAdminGitHubOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖 GitHub OAuth 登录设置",
    request_body = AdminOAuthLoginProviderSettingsRequest,
    responses(
        (status = 200, description = "更新后的脱敏设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 400, description = "配置组合无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "设置版本冲突或回调基址未配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_github_oauth_login_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/discord",
    operation_id = "updateAdminDiscordOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖 Discord OAuth 登录设置",
    request_body = AdminOAuthLoginProviderSettingsRequest,
    responses(
        (status = 200, description = "更新后的脱敏设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 400, description = "配置组合无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "设置版本冲突或回调基址未配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_discord_oauth_login_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/oidc",
    operation_id = "updateAdminOidcLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖 OIDC 登录设置",
    request_body = AdminOAuthLoginProviderSettingsRequest,
    responses(
        (status = 200, description = "更新后的脱敏设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 400, description = "配置组合无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "设置版本冲突或回调基址未配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_oidc_oauth_login_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/linuxdo",
    operation_id = "updateAdminLinuxDoLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖 LinuxDO 登录设置",
    request_body = AdminOAuthLoginProviderSettingsRequest,
    responses(
        (status = 200, description = "更新后的脱敏设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 400, description = "配置组合无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "设置版本冲突或回调基址未配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_linuxdo_oauth_login_settings() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        start_github_oauth_login,
        start_discord_oauth_login,
        complete_github_oauth_login,
        complete_discord_oauth_login,
        exchange_oauth_login_ticket,
        get_admin_github_oauth_login_settings,
        update_admin_github_oauth_login_settings,
        get_admin_discord_oauth_login_settings,
        update_admin_discord_oauth_login_settings,
        start_oidc_oauth_login,
        start_linuxdo_oauth_login,
        complete_oidc_oauth_login,
        complete_linuxdo_oauth_login,
        get_admin_oidc_oauth_login_settings,
        update_admin_oidc_oauth_login_settings,
        get_admin_linuxdo_oauth_login_settings,
        update_admin_linuxdo_oauth_login_settings
    ),
    components(schemas(
        OAuthLoginStartResponse,
        OAuthLoginExchangeRequest,
        AdminOAuthLoginProviderSettingsResponse,
        AdminOAuthLoginProviderSettingsRequest,
        LoginResponse,
        ManagementErrorBody
    ))
)]
struct OAuthLoginApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OAuthLoginApi::openapi()
}
