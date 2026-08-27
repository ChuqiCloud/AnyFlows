#![allow(dead_code, reason = "OpenAPI 文档端点仅由 utoipa 宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    scim::{
        ResourceTypeListResponse, ResourceTypeResponse, ScimAuthenticationSchemeResponse,
        ScimBulkResponse, ScimErrorResponse, ScimFilterResponse, ScimGroupMember, ScimGroupMeta,
        ScimGroupResponse, ScimGroupWriteRequest, ScimSupportResponse, ScimUserMeta,
        ScimUserResponse, ScimUserWriteRequest, ServiceProviderConfigResponse,
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
        (status = 200, description = "SCIM 服务发现配置", body = ServiceProviderConfigResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 认证服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
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
        (status = 200, description = "SCIM 资源类型列表", body = ResourceTypeListResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 认证服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
    ),
    security(("scimBearerAuth" = []))
)]
fn list_scim_resource_types() {}

#[utoipa::path(
    post,
    path = "/scim/v2/{organization_id}/Users",
    operation_id = "createScimUser",
    tag = "企业 SCIM",
    request_body(content = ScimUserWriteRequest, content_type = "application/scim+json"),
    params(("organization_id" = i64, Path, minimum = 1, description = "企业 ID")),
    responses(
        (status = 201, description = "创建 SCIM 用户资源", body = ScimUserResponse, content_type = "application/scim+json"),
        (status = 400, description = "SCIM 用户资源请求无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 403, description = "SCIM 用户资源范围不足", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 409, description = "SCIM 用户资源版本或绑定冲突", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 422, description = "SCIM 用户资源生命周期被拒绝", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
    ),
    security(("scimBearerAuth" = []))
)]
fn create_scim_user() {}

#[utoipa::path(
    put,
    path = "/scim/v2/{organization_id}/Users/{resource_id}",
    operation_id = "replaceScimUser",
    tag = "企业 SCIM",
    request_body(content = ScimUserWriteRequest, content_type = "application/scim+json"),
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业 ID"),
        ("resource_id" = String, Path, description = "SCIM 用户资源 ID")
    ),
    responses(
        (status = 200, description = "替换 SCIM 用户资源", body = ScimUserResponse, content_type = "application/scim+json"),
        (status = 400, description = "SCIM 用户资源请求无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 403, description = "SCIM 用户资源范围不足", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 404, description = "SCIM 用户资源目标不可用", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 409, description = "SCIM 用户资源版本或绑定冲突", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 422, description = "SCIM 用户资源生命周期被拒绝", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
    ),
    security(("scimBearerAuth" = []))
)]
fn replace_scim_user() {}

#[utoipa::path(
    post,
    path = "/scim/v2/{organization_id}/Groups",
    operation_id = "createScimGroup",
    tag = "企业 SCIM",
    request_body(content = ScimGroupWriteRequest, content_type = "application/scim+json"),
    params(("organization_id" = i64, Path, minimum = 1, description = "企业 ID")),
    responses(
        (status = 201, description = "创建 SCIM 组资源", body = ScimGroupResponse, content_type = "application/scim+json"),
        (status = 400, description = "SCIM 目录资源请求无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 403, description = "SCIM 组资源范围不足", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 404, description = "SCIM 组成员或团队不可用", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 409, description = "SCIM 组资源版本或绑定冲突", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 422, description = "SCIM 组资源生命周期被拒绝", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
    ),
    security(("scimBearerAuth" = []))
)]
fn create_scim_group() {}

#[utoipa::path(
    put,
    path = "/scim/v2/{organization_id}/Groups/{resource_id}",
    operation_id = "replaceScimGroup",
    tag = "企业 SCIM",
    request_body(content = ScimGroupWriteRequest, content_type = "application/scim+json"),
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业 ID"),
        ("resource_id" = String, Path, description = "SCIM 组资源 ID")
    ),
    responses(
        (status = 200, description = "替换 SCIM 组资源", body = ScimGroupResponse, content_type = "application/scim+json"),
        (status = 400, description = "SCIM 目录资源请求无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 403, description = "SCIM 组资源范围不足", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 404, description = "SCIM 组成员或团队不可用", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 409, description = "SCIM 组资源版本或绑定冲突", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 422, description = "SCIM 组资源生命周期被拒绝", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
    ),
    security(("scimBearerAuth" = []))
)]
fn replace_scim_group() {}

#[utoipa::path(
    delete,
    path = "/scim/v2/{organization_id}/Groups/{resource_id}",
    operation_id = "deleteScimGroup",
    tag = "企业 SCIM",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业 ID"),
        ("resource_id" = String, Path, description = "SCIM 组资源 ID"),
        ("if-match" = String, Header, description = "待删除资源的精确版本，例如 \"1\"")
    ),
    responses(
        (status = 204, description = "删除 SCIM 组资源"),
        (status = 400, description = "SCIM 目录资源请求无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 401, description = "SCIM Bearer 令牌无效", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 403, description = "SCIM 组资源范围不足", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 404, description = "SCIM 组资源或团队不可用", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 409, description = "SCIM 组资源版本冲突", body = ScimErrorResponse, content_type = "application/scim+json"),
        (status = 503, description = "SCIM 服务暂不可用", body = ScimErrorResponse, content_type = "application/scim+json")
    ),
    security(("scimBearerAuth" = []))
)]
fn delete_scim_group() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_scim_service_provider_config,
        list_scim_resource_types,
        create_scim_user,
        replace_scim_user,
        create_scim_group,
        replace_scim_group,
        delete_scim_group
    ),
    components(schemas(
        ServiceProviderConfigResponse,
        ScimSupportResponse,
        ScimBulkResponse,
        ScimFilterResponse,
        ScimAuthenticationSchemeResponse,
        ResourceTypeListResponse,
        ResourceTypeResponse,
        ScimUserWriteRequest,
        ScimUserMeta,
        ScimUserResponse,
        ScimGroupWriteRequest,
        ScimGroupMember,
        ScimGroupMeta,
        ScimGroupResponse,
        ScimErrorResponse,
        ManagementErrorBody
    ))
)]
struct ScimApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ScimApi::openapi()
}
