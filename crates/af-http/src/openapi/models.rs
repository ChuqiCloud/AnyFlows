//! 游客与登录用户共用的模型目录 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_models::AdminModelModalityValue,
    model_catalog::{
        ModelCatalogItemResponse, ModelCatalogLifecycleValue, ModelCatalogListResponse,
        ModelCatalogProtocolValue, ModelCatalogProviderListResponse, ModelCatalogProviderResponse,
        ModelCatalogRatiosResponse, ModelCatalogRuntimeStatusValue,
        ModelCatalogTokenPricesResponse,
    },
    openapi::schema::{
        ModelCatalogBillingModeSchema, ModelCatalogCapabilitySchema, ModelCatalogModalitySchema,
        ModelCatalogPricingScopeSchema,
    },
};

#[utoipa::path(
    get,
    path = "/api/models",
    operation_id = "listModels",
    tag = "模型目录",
    summary = "读取公开或当前分组模型目录",
    description = "无 Authorization 时返回公开基础目录；携带 Bearer JWT 时严格认证并返回当前默认分组的可用模型与实际价格。",
    params(
        ("q" = Option<String>, Query, min_length = 1, max_length = 128, description = "匹配 Canonical 标识、展示名或厂商"),
        ("billing_mode" = Option<ModelCatalogBillingModeSchema>, Query, description = "计费模式筛选"),
        ("provider" = Option<Vec<String>>, Query, min_length = 1, max_length = 64, description = "供应商筛选，重复参数按任一匹配"),
        ("input_modality" = Option<Vec<ModelCatalogModalitySchema>>, Query, description = "输入模态筛选，重复参数按任一匹配"),
        ("output_modality" = Option<Vec<ModelCatalogModalitySchema>>, Query, description = "输出模态筛选，重复参数按任一匹配"),
        ("capability" = Option<Vec<ModelCatalogCapabilitySchema>>, Query, description = "扩展能力筛选，重复参数必须全部满足"),
        ("protocol" = Option<Vec<ModelCatalogProtocolValue>>, Query, description = "当前分组可调用协议筛选，重复参数按任一匹配；游客目录不承诺协议"),
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

#[utoipa::path(
    get,
    path = "/api/model-providers",
    operation_id = "listModelProviders",
    tag = "模型目录",
    summary = "读取当前模型目录的完整供应商聚合",
    description = "无 Authorization 时统计公开基础目录；携带 Bearer JWT 时严格认证并统计当前默认分组真实可用目录。",
    responses(
        (status = 200, description = "当前访问范围内的供应商与模型数量", body = ModelCatalogProviderListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "运行时目录或定价快照不可用", body = ManagementErrorBody)
    ),
    security((), ("bearerAuth" = []))
)]
fn list_model_providers() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_models, list_model_providers),
    components(schemas(
        ModelCatalogBillingModeSchema,
        ModelCatalogModalitySchema,
        ModelCatalogCapabilitySchema,
        ModelCatalogPricingScopeSchema,
        ModelCatalogTokenPricesResponse,
        ModelCatalogRatiosResponse,
        ModelCatalogItemResponse,
        ModelCatalogProtocolValue,
        ModelCatalogListResponse,
        ModelCatalogProviderResponse,
        ModelCatalogProviderListResponse,
        ModelCatalogLifecycleValue,
        ModelCatalogRuntimeStatusValue,
        AdminModelModalityValue,
        ManagementErrorBody
    ))
)]
struct ModelsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ModelsApi::openapi()
}
