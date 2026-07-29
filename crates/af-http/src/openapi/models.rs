//! 游客与登录用户共用的模型目录 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    model_catalog::{
        ModelCatalogItemResponse, ModelCatalogListResponse, ModelCatalogRatiosResponse,
        ModelCatalogTokenPricesResponse,
    },
    openapi::schema::{ModelCatalogBillingModeSchema, ModelCatalogPricingScopeSchema},
};

#[utoipa::path(
    get,
    path = "/api/models",
    operation_id = "listModels",
    tag = "模型目录",
    summary = "读取公开或当前分组模型目录",
    description = "无 Authorization 时返回公开基础目录；携带 Bearer JWT 时严格认证并返回当前默认分组的可用模型与实际价格。",
    params(
        ("q" = Option<String>, Query, min_length = 1, max_length = 128, description = "模型名包含搜索"),
        ("billing_mode" = Option<ModelCatalogBillingModeSchema>, Query, description = "计费模式筛选"),
        ("after" = Option<String>, Query, min_length = 1, max_length = 256, description = "上一页最后一个模型名"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 24")
    ),
    responses(
        (status = 200, description = "公开基础目录或当前默认分组可用目录", body = ModelCatalogListResponse),
        (status = 400, description = "搜索、筛选或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "运行时目录或定价快照不可用", body = ManagementErrorBody)
    ),
    security((), ("bearerAuth" = []))
)]
fn list_models() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_models),
    components(schemas(
        ModelCatalogBillingModeSchema,
        ModelCatalogPricingScopeSchema,
        ModelCatalogTokenPricesResponse,
        ModelCatalogRatiosResponse,
        ModelCatalogItemResponse,
        ModelCatalogListResponse,
        ManagementErrorBody
    ))
)]
struct ModelsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ModelsApi::openapi()
}
