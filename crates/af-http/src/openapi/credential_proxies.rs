//! 凭据专属代理目录 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    credential_proxies::{
        AdminCredentialProxyListResponse, AdminCredentialProxyResponse,
        AdminCredentialProxySchemeDto, AdminCredentialProxyWriteRequest,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/proxies",
    operation_id = "listAdminCredentialProxies",
    tag = "专属代理",
    summary = "读取专属代理目录",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的代理 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 100")
    ),
    responses(
        (status = 200, description = "专属代理目录", body = AdminCredentialProxyListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_credential_proxies() {}

#[utoipa::path(
    post,
    path = "/api/admin/proxies",
    operation_id = "createAdminCredentialProxy",
    tag = "专属代理",
    summary = "创建专属代理",
    request_body = AdminCredentialProxyWriteRequest,
    responses(
        (status = 201, description = "专属代理已创建", body = AdminCredentialProxyResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "代理名称冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_credential_proxy() {}

#[utoipa::path(
    get,
    path = "/api/admin/proxies/{id}",
    operation_id = "getAdminCredentialProxy",
    tag = "专属代理",
    summary = "读取专属代理详情",
    params(("id" = i64, Path, minimum = 1, description = "代理 ID")),
    responses(
        (status = 200, description = "专属代理详情", body = AdminCredentialProxyResponse),
        (status = 400, description = "代理 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "专属代理不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_credential_proxy() {}

#[utoipa::path(
    put,
    path = "/api/admin/proxies/{id}",
    operation_id = "updateAdminCredentialProxy",
    tag = "专属代理",
    summary = "完整更新专属代理",
    params(("id" = i64, Path, minimum = 1, description = "代理 ID")),
    request_body = AdminCredentialProxyWriteRequest,
    responses(
        (status = 200, description = "专属代理已更新", body = AdminCredentialProxyResponse),
        (status = 400, description = "代理 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "专属代理不存在", body = ManagementErrorBody),
        (status = 409, description = "名称冲突或代理仍被引用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_credential_proxy() {}

#[utoipa::path(
    delete,
    path = "/api/admin/proxies/{id}",
    operation_id = "deleteAdminCredentialProxy",
    tag = "专属代理",
    summary = "软删除专属代理",
    params(("id" = i64, Path, minimum = 1, description = "代理 ID")),
    responses(
        (status = 204, description = "专属代理已删除"),
        (status = 400, description = "代理 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "专属代理不存在", body = ManagementErrorBody),
        (status = 409, description = "代理仍被凭据引用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_credential_proxy() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_credential_proxies,
        create_admin_credential_proxy,
        get_admin_credential_proxy,
        update_admin_credential_proxy,
        delete_admin_credential_proxy
    ),
    components(schemas(
        AdminCredentialProxyResponse,
        AdminCredentialProxyListResponse,
        AdminCredentialProxyWriteRequest,
        AdminCredentialProxySchemeDto,
        ManagementErrorBody
    ))
)]
struct CredentialProxiesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    CredentialProxiesApi::openapi()
}
