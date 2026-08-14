//! 普通用户 API Key OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    openapi::schema::UserTokenStatusSchema,
    user_tokens::{
        IssuedUserTokenResponse, UserTokenListResponse, UserTokenResponse, UserTokenWriteRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/tokens",
    operation_id = "listUserTokens",
    tag = "我的 API Key",
    summary = "读取我的 API Key 列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的 Key ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "当前用户的 Key 列表", body = UserTokenListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_tokens() {}

#[utoipa::path(
    post,
    path = "/api/tokens",
    operation_id = "createUserToken",
    tag = "我的 API Key",
    summary = "签发我的 API Key",
    request_body = UserTokenWriteRequest,
    responses(
        (status = 201, description = "Key 已签发，完整明文仅返回一次", body = IssuedUserTokenResponse<'static>),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "当前用户的 Key 已达到 32 个", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_user_token() {}

#[utoipa::path(
    post,
    path = "/api/playground/token",
    operation_id = "createPlaygroundToken",
    tag = "试炼场",
    summary = "签发试炼场托管 Key",
    description = "字段由服务端固定，完整明文仅在本次响应返回，用于当前登录用户的试炼场会话。",
    responses(
        (status = 201, description = "试炼场托管 Key 已签发", body = IssuedUserTokenResponse<'static>),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 409, description = "当前用户的 Key 已达到容量上限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_playground_token() {}

#[utoipa::path(
    get,
    path = "/api/tokens/{id}",
    operation_id = "getUserToken",
    tag = "我的 API Key",
    summary = "读取我的 API Key 详情",
    params(("id" = i64, Path, minimum = 1, description = "Key ID")),
    responses(
        (status = 200, description = "当前用户拥有的 Key", body = UserTokenResponse),
        (status = 400, description = "Key ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "Key 不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_token() {}

#[utoipa::path(
    put,
    path = "/api/tokens/{id}",
    operation_id = "updateUserToken",
    tag = "我的 API Key",
    summary = "更新我的 API Key",
    params(("id" = i64, Path, minimum = 1, description = "Key ID")),
    request_body = UserTokenWriteRequest,
    responses(
        (status = 200, description = "Key 已更新", body = UserTokenResponse),
        (status = 400, description = "Key ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "Key 不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_user_token() {}

#[utoipa::path(
    delete,
    path = "/api/tokens/{id}",
    operation_id = "deleteUserToken",
    tag = "我的 API Key",
    summary = "删除我的 API Key",
    params(("id" = i64, Path, minimum = 1, description = "Key ID")),
    responses(
        (status = 204, description = "Key 已删除"),
        (status = 400, description = "Key ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "Key 不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_user_token() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_user_tokens,
        create_user_token,
        create_playground_token,
        get_user_token,
        update_user_token,
        delete_user_token
    ),
    components(schemas(
        UserTokenStatusSchema,
        UserTokenResponse,
        UserTokenListResponse,
        UserTokenWriteRequest,
        IssuedUserTokenResponse<'static>,
        ManagementErrorBody
    ))
)]
struct UserTokensApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserTokensApi::openapi()
}
