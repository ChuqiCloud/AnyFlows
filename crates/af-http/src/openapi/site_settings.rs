//! 公开站点投影与管理员站点设置 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    site_settings::{
        AdminBrandSettingsRequest, AdminSiteNavigationRequest, AdminSiteSettingsRequest,
        AdminSiteSettingsResponse, BalanceDisplayModeDto, BalanceDisplaySettingsDto,
        BalanceDisplaySymbolPositionDto, PublicAuthenticationCapabilitiesResponse,
        PublicBrandSettingsResponse, PublicOAuthLoginProviderResponse, PublicSiteSettingsResponse,
        SiteNavigationDto, SiteNavigationGroupDto, SiteNavigationLinkDto, SiteSidebarLinkDto,
    },
};

#[utoipa::path(
    get,
    path = "/api/site",
    operation_id = "getPublicSiteSettings",
    tag = "站点设置",
    summary = "读取公开站点与认证能力投影",
    responses(
        (status = 200, description = "公开站点身份、品牌与认证能力", body = PublicSiteSettingsResponse),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn get_public_site_settings() {}

#[utoipa::path(
    get,
    path = "/api/admin/site-settings",
    operation_id = "getAdminSiteSettings",
    tag = "站点设置",
    summary = "读取管理员站点设置",
    responses(
        (status = 200, description = "完整站点设置", body = AdminSiteSettingsResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_site_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/site-settings",
    operation_id = "updateAdminSiteSettings",
    tag = "站点设置",
    summary = "覆盖管理员站点设置",
    request_body = AdminSiteSettingsRequest,
    responses(
        (status = 200, description = "更新后的完整站点设置", body = AdminSiteSettingsResponse),
        (status = 400, description = "站点文本或 URL 无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "站点设置版本已变化", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_site_settings() {}

#[utoipa::path(
    put,
    path = "/api/admin/site-settings/navigation",
    operation_id = "updateAdminSiteNavigation",
    tag = "站点设置",
    summary = "更新公开顶栏与页脚导航",
    request_body = AdminSiteNavigationRequest,
    responses(
        (status = 200, description = "更新后的完整站点设置", body = AdminSiteSettingsResponse),
        (status = 400, description = "导航文本或链接无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "站点设置版本已变化", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_site_navigation() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_public_site_settings,
        get_admin_site_settings,
        update_admin_site_settings,
        update_admin_site_navigation
    ),
    components(schemas(
        PublicBrandSettingsResponse,
        PublicOAuthLoginProviderResponse,
        PublicAuthenticationCapabilitiesResponse,
        BalanceDisplayModeDto,
        BalanceDisplaySymbolPositionDto,
        BalanceDisplaySettingsDto,
        SiteNavigationLinkDto,
        SiteNavigationGroupDto,
        SiteSidebarLinkDto,
        SiteNavigationDto,
        PublicSiteSettingsResponse,
        AdminBrandSettingsRequest,
        AdminSiteNavigationRequest,
        AdminSiteSettingsRequest,
        AdminSiteSettingsResponse,
        ManagementErrorBody
    ))
)]
struct SiteSettingsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    SiteSettingsApi::openapi()
}
