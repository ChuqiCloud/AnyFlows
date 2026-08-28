//! 企业服务账号运行时 HTTP 鉴权 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_service_account_runtime::OrganizationServiceAccountRuntimeErrorResponse,
};

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/service-accounts/runtime/authenticate",
    operation_id = "authenticateOrganizationServiceAccountRuntime",
    tag = "企业服务账号",
    summary = "使用企业服务账号密钥建立运行时身份",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 204, description = "凭据有效，运行时身份已建立"),
        (status = 401, description = "服务账号密钥无效、缺失或不属于该企业", body = OrganizationServiceAccountRuntimeErrorResponse),
        (status = 503, description = "服务账号认证依赖暂不可用", body = OrganizationServiceAccountRuntimeErrorResponse),
    ),
    security(("serviceAccountBearerAuth" = []))
)]
fn authenticate_organization_service_account_runtime() {}

#[derive(OpenApi)]
#[openapi(
    paths(authenticate_organization_service_account_runtime),
    components(schemas(OrganizationServiceAccountRuntimeErrorResponse, ManagementErrorBody))
)]
struct OrganizationServiceAccountRuntimeApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationServiceAccountRuntimeApi::openapi()
}
