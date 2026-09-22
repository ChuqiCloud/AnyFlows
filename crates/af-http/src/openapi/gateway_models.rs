//! API Key 模型发现的 OpenAI 兼容契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use crate::openai_models::{OpenAiModelListResponse, OpenAiModelResponse};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OpenAiModelListErrorBody)]
struct ModelListErrorBody {
    code: String,
    message: String,
    #[schema(required = true)]
    param: Option<String>,
    #[serde(rename = "type")]
    error_type: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OpenAiModelListError)]
struct ModelListError {
    error: ModelListErrorBody,
}

#[utoipa::path(
    get,
    path = "/v1/models",
    operation_id = "listGatewayModels",
    tag = "模型目录",
    summary = "读取 API Key 可用模型列表",
    description = "使用 API Key 鉴权，按令牌有效分组、模型白名单、目录可见性、价格配置及运行时候选筛选，返回完整 OpenAI 兼容列表；不需要网页登录会话，不调用上游。",
    responses(
        (status = 200, description = "当前 API Key 可见的完整模型列表", body = OpenAiModelListResponse),
        (status = 401, description = "API Key 无效", body = ModelListError),
        (status = 429, description = "请求次数或频率受限", body = ModelListError),
        (status = 500, description = "模型目录不可用", body = ModelListError)
    ),
    security(("apiKeyAuth" = []))
)]
fn list_gateway_models() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_gateway_models),
    components(schemas(
        OpenAiModelResponse,
        OpenAiModelListResponse,
        ModelListError,
        ModelListErrorBody
    ))
)]
struct GatewayModelsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    GatewayModelsApi::openapi()
}
