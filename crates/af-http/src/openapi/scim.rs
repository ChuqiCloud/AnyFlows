#![allow(dead_code, reason = "OpenAPI 文档端点仅由 utoipa 宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    scim::{
        ResourceTypeListResponse, ResourceTypeResponse, ScimAuthenticationSchemeResponse,
        ScimBulkResponse, ScimErrorResponse, ScimFilterResponse, ScimSupportResponse,
        ServiceProviderConfigResponse,
    },
};

#[utoipa::path(
    get,
    path = "/scim/v2/{organization_id}/ServiceProviderConfig",
    operation_id = "getScimServiceProviderConfig",
    tag = "企业 SCIM",
    summary = "读取企业 SCIM 服务发现配置",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业 ID")),
    responses(
        (status = 200, description = "SCIM 服务发现配置", body = ServiceProviderConfigResponse),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse),
        (status = 503, description = "SCIM 认证服务暂不可用", body = ScimErrorResponse)
    ),
    security(("scimBearerAuth" = []))
)]
fn get_scim_service_provider_config() {}

#[utoipa::path(
    get,
    path = "/scim/v2/{organization_id}/ResourceTypes",
    operation_id = "listScimResourceTypes",
    tag = "企业 SCIM",
    summary = "读取企业 SCIM 资源类型发现",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业 ID")),
    responses(
        (status = 200, description = "SCIM 资源类型列表", body = ResourceTypeListResponse),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse),
        (status = 503, description = "SCIM 认证服务暂不可用", body = ScimErrorResponse)
    ),
    security(("scimBearerAuth" = []))
)]
fn list_scim_resource_types() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_scim_service_provider_config, list_scim_resource_types),
    components(schemas(
        ServiceProviderConfigResponse,
        ScimSupportResponse,
        ScimBulkResponse,
        ScimFilterResponse,
        ScimAuthenticationSchemeResponse,
        ResourceTypeListResponse,
        ResourceTypeResponse,
        ScimErrorResponse,
        ManagementErrorBody
    ))
)]
struct ScimApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ScimApi::openapi()
}
