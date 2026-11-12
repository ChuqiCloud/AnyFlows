#![allow(dead_code, reason = "OpenAPI declarations")]

use crate::{
    management_error::ManagementErrorBody,
    model_provider_catalog::{
        DeleteModelProviderRequest, ModelProviderCatalogEntryResponse,
        ModelProviderCatalogListResponse, ModelProviderCatalogRequest,
    },
};
use utoipa::OpenApi;

#[utoipa::path(
    get,
    path = "/api/model-provider-catalog",
    operation_id = "listPublicModelProviderCatalog",
    tag = "模型目录",
    summary = "读取公开模型厂商目录",
    responses(
        (status = 200, description = "已启用的公开厂商目录", body = ModelProviderCatalogListResponse),
        (status = 500, description = "厂商目录暂不可用", body = ManagementErrorBody)
    ),
    security(())
)]
fn list_public_model_provider_catalog() {}

#[utoipa::path(
    get,
    path = "/api/admin/model-provider-catalog",
    operation_id = "listAdminModelProviderCatalog",
    tag = "模型厂商目录管理",
    summary = "读取模型厂商目录",
    responses(
        (status = 200, description = "厂商目录", body = ModelProviderCatalogListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
        (status = 500, description = "厂商目录暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_model_provider_catalog() {}

#[utoipa::path(
    get,
    path = "/api/admin/model-provider-catalog/{provider_key}",
    operation_id = "getAdminModelProvider",
    tag = "模型厂商目录管理",
    summary = "读取模型厂商",
    params(("provider_key" = String, Path, description = "厂商标识")),
    responses(
        (status = 200, description = "厂商配置", body = ModelProviderCatalogEntryResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
        (status = 404, description = "厂商不存在", body = ManagementErrorBody),
        (status = 500, description = "厂商目录暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_model_provider() {}

#[utoipa::path(
    put,
    path = "/api/admin/model-provider-catalog/{provider_key}",
    operation_id = "updateAdminModelProvider",
    tag = "模型厂商目录管理",
    summary = "新增或更新模型厂商",
    params(("provider_key" = String, Path, description = "厂商标识")),
    request_body = ModelProviderCatalogRequest,
    responses(
        (status = 200, description = "厂商配置", body = ModelProviderCatalogEntryResponse),
        (status = 400, description = "厂商配置无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
        (status = 404, description = "厂商不存在", body = ManagementErrorBody),
        (status = 409, description = "厂商配置已变化", body = ManagementErrorBody),
        (status = 500, description = "厂商目录暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_model_provider() {}

#[utoipa::path(
    delete,
    path = "/api/admin/model-provider-catalog/{provider_key}",
    operation_id = "deleteAdminModelProvider",
    tag = "模型厂商目录管理",
    summary = "删除模型厂商",
    params(("provider_key" = String, Path, description = "厂商标识")),
    request_body = DeleteModelProviderRequest,
    responses(
        (status = 204, description = "厂商已删除"),
        (status = 400, description = "版本号无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
        (status = 404, description = "厂商不存在", body = ManagementErrorBody),
        (status = 409, description = "厂商配置已变化", body = ManagementErrorBody),
        (status = 500, description = "厂商目录暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_model_provider() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_public_model_provider_catalog,
        list_admin_model_provider_catalog,
        get_admin_model_provider,
        update_admin_model_provider,
        delete_admin_model_provider
    ),
    components(schemas(
        ModelProviderCatalogEntryResponse,
        ModelProviderCatalogListResponse,
        ModelProviderCatalogRequest,
        DeleteModelProviderRequest
    ))
)]
struct ModelProviderCatalogApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ModelProviderCatalogApi::openapi()
}
