//! 用户智能路由组契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_routes::{
        UserRouteListResponse, UserRouteResponse, UserRouteStrategySchema, UserRouteWriteRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/user-routes",
    operation_id = "listUserRoutes",
    tag = "我的智能路由",
    summary = "读取我的智能路由",
    responses(
        (status = 200, description = "当前用户的智能路由及容量", body = UserRouteListResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_routes() {}

#[utoipa::path(
    post,
    path = "/api/user-routes",
    operation_id = "createUserRoute",
    tag = "我的智能路由",
    summary = "创建我的智能路由",
    request_body = UserRouteWriteRequest,
    responses(
        (status = 201, description = "路由已创建", body = UserRouteResponse),
        (status = 400, description = "路由或成员字段无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "路由成员分组当前不可用", body = ManagementErrorBody),
        (status = 409, description = "用户路由数量已达上限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_user_route() {}

#[utoipa::path(
    get,
    path = "/api/user-routes/{id}",
    operation_id = "getUserRoute",
    tag = "我的智能路由",
    summary = "读取我的一个智能路由",
    params(("id" = i64, Path, minimum = 1, description = "用户路由 ID")),
    responses(
        (status = 200, description = "当前用户的智能路由", body = UserRouteResponse),
        (status = 400, description = "路由 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 404, description = "路由不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_route() {}

#[utoipa::path(
    put,
    path = "/api/user-routes/{id}",
    operation_id = "updateUserRoute",
    tag = "我的智能路由",
    summary = "完整替换我的一个智能路由",
    params(("id" = i64, Path, minimum = 1, description = "用户路由 ID")),
    request_body = UserRouteWriteRequest,
    responses(
        (status = 200, description = "路由已更新", body = UserRouteResponse),
        (status = 400, description = "路由或成员字段无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "路由成员分组当前不可用", body = ManagementErrorBody),
        (status = 404, description = "路由不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_user_route() {}

#[utoipa::path(
    delete,
    path = "/api/user-routes/{id}",
    operation_id = "deleteUserRoute",
    tag = "我的智能路由",
    summary = "删除我的一个智能路由",
    params(("id" = i64, Path, minimum = 1, description = "用户路由 ID")),
    responses(
        (status = 204, description = "路由已删除"),
        (status = 400, description = "路由 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 404, description = "路由不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_user_route() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_user_routes,
        create_user_route,
        get_user_route,
        update_user_route,
        delete_user_route
    ),
    components(schemas(
        UserRouteStrategySchema,
        UserRouteListResponse,
        UserRouteResponse,
        UserRouteWriteRequest,
        ManagementErrorBody
    ))
)]
struct UserRoutesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserRoutesApi::openapi()
}
