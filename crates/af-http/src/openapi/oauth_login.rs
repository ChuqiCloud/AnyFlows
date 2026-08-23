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
    post,
    path = "/api/auth/oauth/wechat/start",
    operation_id = "startWeChatOAuthLogin",
    tag = "OAuth 登录",
    summary = "启动微信开放平台二维码登录",
    responses(
        (status = 200, description = "服务端构造的微信官方授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "微信登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_wechat_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/telegram/start",
    operation_id = "startTelegramLogin",
    tag = "OAuth 登录",
    summary = "启动 Telegram 登录",
    responses(
        (status = 200, description = "服务端构造的 Telegram OIDC 授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "Telegram 登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_telegram_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/google/start",
    operation_id = "startGoogleLogin",
    tag = "OAuth 登录",
    summary = "启动 Google OIDC 登录",
    responses(
        (status = 200, description = "服务端构造的 Google OIDC 授权地址", body = OAuthLoginStartResponse),
        (status = 409, description = "Google 登录尚未完整配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_google_oauth_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/oauth/custom/{provider_key}/start",
    operation_id = "startCustomOAuth2Login",
    tag = "OAuth 登录",
    summary = "启动自定义 OAuth2 登录",
    params((
        "provider_key" = String,
        Path,
        pattern = "^custom_[a-z0-9_-]+$",
        description = "已启用的 custom_ 命名空间 Provider key"
    )),
    responses(
        (status = 200, description = "服务端按固定配置构造的授权地址", body = OAuthLoginStartResponse),
        (status = 400, description = "Provider key 格式无效", body = ManagementErrorBody),
        (status = 409, description = "Provider 不存在、已禁用或配置不完整", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn start_custom_oauth2_login() {}

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
    get,
    path = "/api/auth/oauth/wechat/callback",
    operation_id = "completeWeChatOAuthLogin",
    tag = "OAuth 登录",
    summary = "接收微信开放平台登录回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "微信授权码")
    ),
    responses(
        (status = 303, description = "跳转到前端单次票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_wechat_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/telegram/callback",
    operation_id = "completeTelegramLogin",
    tag = "OAuth 登录",
    summary = "接收 Telegram OIDC 回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "Telegram 授权码"),
        ("error" = Option<String>, Query, description = "Telegram 拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端单次票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_telegram_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/google/callback",
    operation_id = "completeGoogleLogin",
    tag = "OAuth 登录",
    summary = "接收 Google OIDC 回调",
    params(
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "Google 授权码"),
        ("error" = Option<String>, Query, description = "Google 拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端一次性票据交换页"),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_google_oauth_login() {}

#[utoipa::path(
    get,
    path = "/api/auth/oauth/custom/{provider_key}/callback",
    operation_id = "completeCustomOAuth2Login",
    tag = "OAuth 登录",
    summary = "接收自定义 OAuth2 回调",
    params(
        (
            "provider_key" = String,
            Path,
            pattern = "^custom_[a-z0-9_-]+$",
            description = "启动登录时固定的 custom_ 命名空间 Provider key"
        ),
        ("state" = Option<String>, Query, description = "服务端生成的一次性 state"),
        ("code" = Option<String>, Query, description = "第三方授权码"),
        ("error" = Option<String>, Query, description = "第三方拒绝分类")
    ),
    responses(
        (status = 303, description = "跳转到前端一次性票据交换页"),
        (status = 400, description = "Provider key 格式无效", body = ManagementErrorBody),
        (status = 500, description = "无法构造安全回调地址", body = ManagementErrorBody)
    )
)]
fn complete_custom_oauth2_login() {}

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
    get,
    path = "/api/admin/authentication-settings/oauth/wechat",
    operation_id = "getAdminWeChatOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "读取微信开放平台登录设置",
    responses(
        (status = 200, description = "脱敏后的微信开放平台设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_wechat_oauth_login_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/telegram",
    operation_id = "getAdminTelegramOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "读取 Telegram 登录设置",
    responses(
        (status = 200, description = "脱敏后的 Telegram OIDC 设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_telegram_oauth_login_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/google",
    operation_id = "getAdminGoogleOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "读取 Google OIDC 登录设置",
    responses(
        (status = 200, description = "脱敏后的 Google OIDC 设置", body = AdminOAuthLoginProviderSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_google_oauth_login_settings() {}

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

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/wechat",
    operation_id = "updateAdminWeChatOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖微信开放平台登录设置",
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
fn update_admin_wechat_oauth_login_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/telegram",
    operation_id = "updateAdminTelegramOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖 Telegram 登录设置",
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
fn update_admin_telegram_oauth_login_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/google",
    operation_id = "updateAdminGoogleOAuthLoginSettings",
    tag = "OAuth 登录",
    summary = "覆盖 Google OIDC 登录设置",
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
fn update_admin_google_oauth_login_settings() {}

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
        update_admin_discord_oauth_login_settings
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

#[derive(OpenApi)]
#[openapi(paths(
    start_oidc_oauth_login,
    start_linuxdo_oauth_login,
    start_wechat_oauth_login,
    start_telegram_oauth_login,
    start_google_oauth_login,
    start_custom_oauth2_login,
    complete_oidc_oauth_login,
    complete_linuxdo_oauth_login,
    complete_wechat_oauth_login,
    complete_telegram_oauth_login,
    complete_google_oauth_login,
    complete_custom_oauth2_login,
    get_admin_oidc_oauth_login_settings,
    update_admin_oidc_oauth_login_settings,
    get_admin_linuxdo_oauth_login_settings,
    update_admin_linuxdo_oauth_login_settings,
    get_admin_wechat_oauth_login_settings,
    update_admin_wechat_oauth_login_settings,
    get_admin_telegram_oauth_login_settings,
    update_admin_telegram_oauth_login_settings,
    get_admin_google_oauth_login_settings,
    update_admin_google_oauth_login_settings
))]
struct OidcOAuthLoginApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OAuthLoginApi::openapi()
}

/// 构建需要在既有管理接口后保持稳定排序的 OIDC 登录文档。
pub(super) fn oidc_document() -> utoipa::openapi::OpenApi {
    OidcOAuthLoginApi::openapi()
}
