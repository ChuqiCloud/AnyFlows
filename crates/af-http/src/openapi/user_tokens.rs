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
    description = "group_id 省略或为 null 时跟随用户默认组；指定 ID 时固定绑定该路由及计费组，并在事务中复验可选权限。",
    request_body = UserTokenWriteRequest,
    responses(
        (status = 201, description = "Key 已签发，完整明文仅返回一次", body = IssuedUserTokenResponse<'static>),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "目标分组已失效或当前用户无权选择", body = ManagementErrorBody),
        (status = 409, description = "当前用户的 Key 已达到 32 个", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_user_token() {}

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
    description = "仅更新用户可控字段。group_id 省略或 null 均保留已有分组，非 null 值被拒绝；需要更换路由及计费分组时创建新 Key。",
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
