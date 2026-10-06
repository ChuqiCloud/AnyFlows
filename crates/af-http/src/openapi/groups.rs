//! 分组管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_group_writes::AdminGroupWriteRequest,
    management_groups::{AdminGroupListResponse, AdminGroupResponse, AdminGroupWindowResponse},
};

#[utoipa::path(
    get,
    path = "/api/admin/groups",
    operation_id = "listAdminGroups",
    tag = "分组管理",
    summary = "读取管理员分组列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的分组 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "分组列表", body = AdminGroupListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_groups() {}

#[utoipa::path(
    post,
    path = "/api/admin/groups",
    operation_id = "createAdminGroup",
    tag = "分组管理",
    summary = "创建管理员分组",
    request_body = AdminGroupWriteRequest,
    responses(
        (status = 201, description = "分组已创建", body = AdminGroupResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "分组名冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_group() {}

#[utoipa::path(
    get,
    path = "/api/admin/groups/{id}",
    operation_id = "getAdminGroup",
    tag = "分组管理",
    summary = "读取管理员分组详情",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    responses(
        (status = 200, description = "分组详情", body = AdminGroupResponse),
        (status = 400, description = "分组 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_group() {}

#[utoipa::path(
    put,
    path = "/api/admin/groups/{id}",
    operation_id = "updateAdminGroup",
    tag = "分组管理",
    summary = "更新管理员分组",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    request_body = AdminGroupWriteRequest,
    responses(
        (status = 200, description = "分组已更新", body = AdminGroupResponse),
        (status = 400, description = "分组 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在", body = ManagementErrorBody),
        (status = 409, description = "分组名冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_group() {}

#[utoipa::path(
    delete,
    path = "/api/admin/groups/{id}",
    operation_id = "deleteAdminGroup",
    tag = "分组管理",
    summary = "安全软删除管理员分组",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    responses(
        (status = 204, description = "分组已删除"),
        (status = 400, description = "分组 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在", body = ManagementErrorBody),
        (status = 409, description = "分组仍被引用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_group() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_groups,
        create_admin_group,
        get_admin_group,
        update_admin_group,
        delete_admin_group
    ),
    components(schemas(
        AdminGroupResponse,
        AdminGroupWindowResponse,
        AdminGroupListResponse,
        AdminGroupWriteRequest,
        ManagementErrorBody
    ))
)]
struct GroupsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    GroupsApi::openapi()
}
