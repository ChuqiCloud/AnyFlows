//! 正式模型价格、公开参考价与原子应用 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_model_prices::{
        AdminModelPriceBatchRequest, AdminModelPriceBatchResponse, AdminModelPriceBillingModeValue,
        AdminModelPriceExpressionPreviewRequest, AdminModelPriceExpressionPreviewResponse,
        AdminModelPriceExpressionRatiosRequest, AdminModelPriceExpressionUsageRequest,
        AdminModelPriceExpressionUsageSemanticsValue, AdminModelPriceExpressionVariablesResponse,
        AdminModelPriceListResponse, AdminModelPriceResponse,
        AdminModelPriceSourceCandidateResponse, AdminModelPriceSourceCostsResponse,
        AdminModelPriceSourcePreviewResponse, AdminModelPriceValues,
        AdminModelPriceWriteItemRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/model-prices",
    operation_id = "listAdminModelPrices",
    tag = "模型管理",
    summary = "按 Canonical 游标列出正式模型价格",
    params(
        ("after" = Option<String>, Query, max_length = 256, description = "上一页末尾的 Canonical 模型标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "正式模型价格列表", body = AdminModelPriceListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_model_prices() {}

#[utoipa::path(
    post,
    path = "/api/admin/model-prices/models-dev-preview",
    operation_id = "previewAdminModelPrices",
    tag = "模型管理",
    summary = "从固定 models.dev 端点预览公开参考价",
    responses(
        (status = 200, description = "与本地权威模型身份精确匹配的参考价", body = AdminModelPriceSourcePreviewResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 502, description = "公开来源不可用、响应过大、损坏或候选超限", body = ManagementErrorBody),
        (status = 504, description = "公开来源请求超时", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn preview_admin_model_prices() {}

#[utoipa::path(
    post,
    path = "/api/admin/model-prices/litellm-preview",
    operation_id = "previewAdminLiteLlmModelPrices",
    tag = "模型管理",
    summary = "从 LiteLLM 官方固定价表预览公开参考价",
    responses(
        (status = 200, description = "与本地权威模型身份精确匹配的参考价", body = AdminModelPriceSourcePreviewResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 502, description = "公开来源不可用、响应过大、损坏或候选超限", body = ManagementErrorBody),
        (status = 504, description = "公开来源请求超时", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn preview_admin_litellm_model_prices() {}

#[utoipa::path(
    post,
    path = "/api/admin/model-prices/expression-preview",
    operation_id = "previewAdminModelPriceExpression",
    tag = "模型管理",
    summary = "使用生产计费链路试算未保存表达式",
    request_body = AdminModelPriceExpressionPreviewRequest,
    responses(
        (status = 200, description = "精确的 tier、变量、美元成本与整数额度明细", body = AdminModelPriceExpressionPreviewResponse),
        (status = 400, description = "用量或倍率输入无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 422, description = "表达式无效或无法在当前输入下执行", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn preview_admin_model_price_expression() {}

#[utoipa::path(
    post,
    path = "/api/admin/model-prices/batch",
    operation_id = "applyAdminModelPrices",
    tag = "模型管理",
    summary = "按乐观版本原子应用正式模型价格",
    request_body = AdminModelPriceBatchRequest,
    responses(
        (status = 200, description = "正式模型价格已写入且运行时快照已刷新", body = AdminModelPriceBatchResponse),
        (status = 400, description = "模型、版本、模式、十进制价格或计费表达式无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "模型元数据不存在", body = ManagementErrorBody),
        (status = 409, description = "价格版本发生并发冲突", body = ManagementErrorBody),
        (status = 500, description = "持久化或运行时快照刷新失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn apply_admin_model_prices() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_model_prices,
        preview_admin_model_prices,
        preview_admin_litellm_model_prices,
        preview_admin_model_price_expression,
        apply_admin_model_prices
    ),
    components(schemas(
        AdminModelPriceBillingModeValue,
        AdminModelPriceValues,
        AdminModelPriceResponse,
        AdminModelPriceListResponse,
        AdminModelPriceSourceCostsResponse,
        AdminModelPriceSourceCandidateResponse,
        AdminModelPriceSourcePreviewResponse,
        AdminModelPriceExpressionUsageSemanticsValue,
        AdminModelPriceExpressionUsageRequest,
        AdminModelPriceExpressionRatiosRequest,
        AdminModelPriceExpressionPreviewRequest,
        AdminModelPriceExpressionVariablesResponse,
        AdminModelPriceExpressionPreviewResponse,
        AdminModelPriceWriteItemRequest,
        AdminModelPriceBatchRequest,
        AdminModelPriceBatchResponse,
        ManagementErrorBody
    ))
)]
struct ModelPricesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ModelPricesApi::openapi()
}
