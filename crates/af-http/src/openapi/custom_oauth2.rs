//! 自定义 OAuth2 Provider 管理 API 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    custom_oauth2::{
        AdminCustomOAuth2ProviderListResponse, AdminCustomOAuth2ProviderRequest,
        AdminCustomOAuth2ProviderResponse,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/custom",
    operation_id = "listAdminCustomOAuth2Providers",
    tag = "认证设置",
    summary = "读取自定义 OAuth2 Provider 脱敏配置",
    responses(
        (status = 200, description = "Provider 脱敏配置列表", body = AdminCustomOAuth2ProviderListResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_custom_oauth2_providers() {}

#[utoipa::path(
    get,
    path = "/api/admin/authentication-settings/oauth/custom/{provider_key}",
    operation_id = "getAdminCustomOAuth2Provider",
    tag = "认证设置",
    summary = "读取单个自定义 OAuth2 Provider 脱敏配置",
    params(("provider_key" = String, Path, description = "custom_ 命名空间 Provider key")),
    responses(
        (status = 200, description = "Provider 脱敏配置", body = AdminCustomOAuth2ProviderResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "Provider 不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_custom_oauth2_provider() {}

#[utoipa::path(
    put,
    path = "/api/admin/authentication-settings/oauth/custom/{provider_key}",
    operation_id = "updateAdminCustomOAuth2Provider",
    tag = "认证设置",
    summary = "创建或 CAS 更新自定义 OAuth2 Provider",
    params(("provider_key" = String, Path, description = "custom_ 命名空间 Provider key")),
    request_body = AdminCustomOAuth2ProviderRequest,
    responses(
        (status = 200, description = "保存后的 Provider 脱敏配置", body = AdminCustomOAuth2ProviderResponse),
        (status = 400, description = "请求正文或 Provider 配置无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "Provider 版本冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_custom_oauth2_provider() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_custom_oauth2_providers,
        get_admin_custom_oauth2_provider,
        update_admin_custom_oauth2_provider
    ),
    components(schemas(
        AdminCustomOAuth2ProviderResponse,
        AdminCustomOAuth2ProviderListResponse,
        AdminCustomOAuth2ProviderRequest,
        ManagementErrorBody
    ))
)]
struct CustomOAuth2Api;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    CustomOAuth2Api::openapi()
}
