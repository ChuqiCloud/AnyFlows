//! 独立模型商品元数据管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_model_writes::{AdminModelCreateRequest, AdminModelUpdateRequest},
    management_models::{
        AdminModelLifecycleValue, AdminModelListResponse, AdminModelModalityValue,
        AdminModelResponse, AdminModelVisibilityValue,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/models",
    operation_id = "listAdminModels",
    tag = "模型管理",
    summary = "读取管理员模型元数据列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的模型元数据 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "模型元数据列表", body = AdminModelListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_models() {}

#[utoipa::path(
    post,
    path = "/api/admin/models",
    operation_id = "createAdminModel",
    tag = "模型管理",
    summary = "创建独立模型商品元数据",
    request_body = AdminModelCreateRequest,
    responses(
        (status = 201, description = "模型元数据已创建", body = AdminModelResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "Canonical 模型标识冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_model() {}

#[utoipa::path(
    get,
    path = "/api/admin/models/{id}",
    operation_id = "getAdminModel",
    tag = "模型管理",
    summary = "读取管理员模型元数据详情",
    params(("id" = i64, Path, minimum = 1, description = "模型元数据 ID")),
    responses(
        (status = 200, description = "模型元数据详情", body = AdminModelResponse),
        (status = 400, description = "模型元数据 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "模型元数据不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_model() {}

#[utoipa::path(
    put,
    path = "/api/admin/models/{id}",
    operation_id = "updateAdminModel",
    tag = "模型管理",
    summary = "更新模型商品的可变元数据",
    params(("id" = i64, Path, minimum = 1, description = "模型元数据 ID")),
    request_body = AdminModelUpdateRequest,
    responses(
        (status = 200, description = "模型元数据已更新", body = AdminModelResponse),
        (status = 400, description = "模型元数据 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "模型元数据不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_model() {}

#[utoipa::path(
    delete,
    path = "/api/admin/models/{id}",
    operation_id = "deleteAdminModel",
    tag = "模型管理",
    summary = "软删除模型商品元数据",
    params(("id" = i64, Path, minimum = 1, description = "模型元数据 ID")),
    responses(
        (status = 204, description = "模型元数据已软删除"),
        (status = 400, description = "模型元数据 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "模型元数据不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_model() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_models,
        create_admin_model,
        get_admin_model,
        update_admin_model,
        delete_admin_model
    ),
    components(schemas(
        AdminModelResponse,
        AdminModelListResponse,
        AdminModelCreateRequest,
        AdminModelUpdateRequest,
        AdminModelVisibilityValue,
        AdminModelLifecycleValue,
        AdminModelModalityValue,
        ManagementErrorBody
    ))
)]
struct ModelMetadataApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ModelMetadataApi::openapi()
}
