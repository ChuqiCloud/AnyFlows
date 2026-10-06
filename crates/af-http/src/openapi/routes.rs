//! 智能路由管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_route_writes::{AdminRouteChannelWriteRequest, AdminRouteWriteRequest},
    management_routes::{AdminRouteChannelResponse, AdminRouteListResponse, AdminRouteResponse},
    openapi::schema::{AdminRouteModeSchema, AdminRouteStrategySchema},
};

#[utoipa::path(
    get,
    path = "/api/admin/routes",
    operation_id = "listAdminRoutes",
    tag = "智能路由",
    summary = "读取智能路由列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的路由 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "智能路由列表", body = AdminRouteListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_routes() {}

#[utoipa::path(
    post,
    path = "/api/admin/routes",
    operation_id = "createAdminRoute",
    tag = "智能路由",
    summary = "创建智能路由",
    request_body = AdminRouteWriteRequest,
    responses(
        (status = 201, description = "智能路由已创建", body = AdminRouteResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "路由冲突或候选引用无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_route() {}

#[utoipa::path(
    get,
    path = "/api/admin/routes/{id}",
    operation_id = "getAdminRoute",
    tag = "智能路由",
    summary = "读取智能路由详情",
    params(("id" = i64, Path, minimum = 1, description = "路由 ID")),
    responses(
        (status = 200, description = "智能路由详情", body = AdminRouteResponse),
        (status = 400, description = "路由 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "智能路由不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_route() {}

#[utoipa::path(
    put,
    path = "/api/admin/routes/{id}",
    operation_id = "updateAdminRoute",
    tag = "智能路由",
    summary = "完整更新智能路由",
    params(("id" = i64, Path, minimum = 1, description = "路由 ID")),
    request_body = AdminRouteWriteRequest,
    responses(
        (status = 200, description = "智能路由已更新", body = AdminRouteResponse),
        (status = 400, description = "路由 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "智能路由不存在", body = ManagementErrorBody),
        (status = 409, description = "路由冲突或候选引用无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_route() {}

#[utoipa::path(
    delete,
    path = "/api/admin/routes/{id}",
    operation_id = "deleteAdminRoute",
    tag = "智能路由",
    summary = "软删除智能路由",
    params(("id" = i64, Path, minimum = 1, description = "路由 ID")),
    responses(
        (status = 204, description = "智能路由已删除"),
        (status = 400, description = "路由 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "智能路由不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_route() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_routes,
        create_admin_route,
        get_admin_route,
        update_admin_route,
        delete_admin_route
    ),
    components(schemas(
        AdminRouteChannelResponse,
        AdminRouteResponse,
        AdminRouteListResponse,
        AdminRouteChannelWriteRequest,
        AdminRouteWriteRequest,
        AdminRouteModeSchema,
        AdminRouteStrategySchema,
        ManagementErrorBody
    ))
)]
struct RoutesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RoutesApi::openapi()
}
