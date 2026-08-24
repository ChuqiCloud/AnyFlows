//! 企业 SSO Provider 管理接口的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organizations::{
        OrganizationSsoProviderListResponse, OrganizationSsoProviderRequest,
        OrganizationSsoProviderResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/sso/providers",
    operation_id = "listOrganizationSsoProviders",
    tag = "企业 SSO",
    summary = "列出企业 SSO Provider",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业 SSO Provider 脱敏列表", body = OrganizationSsoProviderListResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_sso_providers() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/providers",
    operation_id = "createOrganizationSsoProvider",
    tag = "企业 SSO",
    summary = "创建企业 SSO Provider",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoProviderRequest,
    responses(
        (status = 200, description = "企业 SSO Provider 已创建", body = OrganizationSsoProviderResponse),
        (status = 400, description = "Provider 参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "Provider 版本或状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_sso_provider() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/sso/providers/{provider_key}",
    operation_id = "getOrganizationSsoProvider",
    tag = "企业 SSO",
    summary = "读取企业 SSO Provider",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("provider_key" = String, Path, description = "Provider 稳定标识")
    ),
    responses(
        (status = 200, description = "企业 SSO Provider 脱敏详情", body = OrganizationSsoProviderResponse),
        (status = 400, description = "企业或 Provider 标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "Provider 不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_sso_provider() {}

#[utoipa::path(
    put,
    path = "/api/organizations/{organization_id}/sso/providers/{provider_key}",
    operation_id = "updateOrganizationSsoProvider",
    tag = "企业 SSO",
    summary = "按版本 CAS 更新企业 SSO Provider",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("provider_key" = String, Path, description = "Provider 稳定标识")
    ),
    request_body = OrganizationSsoProviderRequest,
    responses(
        (status = 200, description = "企业 SSO Provider 已更新", body = OrganizationSsoProviderResponse),
        (status = 400, description = "Provider 参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "Provider 不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "Provider 版本或状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_organization_sso_provider() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_sso_providers,
        create_organization_sso_provider,
        get_organization_sso_provider,
        update_organization_sso_provider
    ),
    components(schemas(
        OrganizationSsoProviderResponse,
        OrganizationSsoProviderListResponse,
        OrganizationSsoProviderRequest,
        ManagementErrorBody
    ))
)]
struct OrganizationSsoApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationSsoApi::openapi()
}
