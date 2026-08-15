#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organizations::{OrganizationListResponse, OrganizationOverviewResponse},
};

#[utoipa::path(
    get,
    path = "/api/account/organizations",
    operation_id = "listAccountOrganizations",
    tag = "企业空间",
    summary = "列出当前用户可进入的企业",
    responses(
        (status = 200, description = "活动企业目录", body = OrganizationListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_account_organizations() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}",
    operation_id = "getOrganizationOverview",
    tag = "企业空间",
    summary = "读取企业概览与当前成员权限",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业概览", body = OrganizationOverviewResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权访问", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_overview() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_account_organizations, get_organization_overview),
    components(schemas(
        OrganizationListResponse,
        OrganizationOverviewResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationApi::openapi()
}
