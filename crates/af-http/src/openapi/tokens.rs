//! 令牌管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_token_writes::{AdminTokenWriteRequest, IssuedAdminTokenResponse},
    management_tokens::{AdminTokenListResponse, AdminTokenResponse},
    openapi::schema::AdminTokenStatusSchema,
};

#[utoipa::path(
    get,
    path = "/api/admin/tokens",
    operation_id = "listAdminTokens",
    tag = "令牌管理",
    summary = "读取管理员令牌列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的令牌 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "令牌列表", body = AdminTokenListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_tokens() {}

#[utoipa::path(
    post,
    path = "/api/admin/tokens",
    operation_id = "createAdminToken",
    tag = "令牌管理",
    summary = "签发管理员令牌",
    request_body = AdminTokenWriteRequest,
    responses(
        (status = 201, description = "令牌已签发，完整 API Key 仅返回一次", body = IssuedAdminTokenResponse<'static>),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "所属用户的 Key 已达到 32 个", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_token() {}

#[utoipa::path(
    get,
    path = "/api/admin/tokens/{id}",
    operation_id = "getAdminToken",
    tag = "令牌管理",
    summary = "读取管理员令牌详情",
    params(("id" = i64, Path, minimum = 1, description = "令牌 ID")),
    responses(
        (status = 200, description = "令牌详情", body = AdminTokenResponse),
        (status = 400, description = "令牌 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "令牌不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_token() {}

#[utoipa::path(
    put,
    path = "/api/admin/tokens/{id}",
    operation_id = "updateAdminToken",
    tag = "令牌管理",
    summary = "完整更新管理员令牌",
    params(("id" = i64, Path, minimum = 1, description = "令牌 ID")),
    request_body = AdminTokenWriteRequest,
    responses(
        (status = 200, description = "令牌已更新", body = AdminTokenResponse),
        (status = 400, description = "令牌 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "令牌不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_token() {}

#[utoipa::path(
    delete,
    path = "/api/admin/tokens/{id}",
    operation_id = "deleteAdminToken",
    tag = "令牌管理",
    summary = "软删除管理员令牌",
    params(("id" = i64, Path, minimum = 1, description = "令牌 ID")),
    responses(
        (status = 204, description = "令牌已删除"),
        (status = 400, description = "令牌 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "令牌不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_token() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_tokens,
        create_admin_token,
        get_admin_token,
        update_admin_token,
        delete_admin_token
    ),
    components(schemas(
        AdminTokenStatusSchema,
        AdminTokenResponse,
        AdminTokenListResponse,
        AdminTokenWriteRequest,
        IssuedAdminTokenResponse<'static>,
        ManagementErrorBody
    ))
)]
struct TokensApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    TokensApi::openapi()
}
