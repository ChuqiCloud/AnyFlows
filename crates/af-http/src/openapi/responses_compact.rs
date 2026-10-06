//! OpenAI Responses Compact 公开网关契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

/// AnyFlows 当前生产支持的无状态 Compact 请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactRequest)]
struct ResponsesCompactRequestSchema {
    #[schema(min_length = 1, max_length = 255)]
    model: String,
    /// 完整上下文窗口；支持字符串简写或已建模的 Responses Item 数组。
    input: ResponsesCompactInputSchema,
    /// 本轮顶层指令；不得包含控制字符。
    #[schema(max_length = 8388608)]
    instructions: Option<String>,
}

/// Compact 接受的字符串简写或有序 Responses Item 窗口。
#[derive(Deserialize, ToSchema)]
#[serde(untagged)]
#[schema(as = ResponsesCompactInput)]
enum ResponsesCompactInputSchema {
    Text(String),
    Items(Vec<ResponsesCompactItemSchema>),
}

/// Compact 输入与输出共享的已校验 Responses Item。
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactItem)]
struct ResponsesCompactItemSchema {
    #[serde(flatten)]
    #[schema(value_type = Object)]
    value: serde_json::Value,
}

/// 独立压缩调用的完整 token 用量。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactUsage)]
struct ResponsesCompactUsageSchema {
    #[schema(minimum = 0)]
    input_tokens: i64,
    input_tokens_details: ResponsesCompactInputUsageDetailsSchema,
    #[schema(minimum = 0)]
    output_tokens: i64,
    output_tokens_details: ResponsesCompactOutputUsageDetailsSchema,
    #[schema(minimum = 0)]
    total_tokens: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactInputUsageDetails)]
struct ResponsesCompactInputUsageDetailsSchema {
    #[schema(minimum = 0)]
    cached_tokens: i64,
    #[schema(minimum = 0)]
    cache_write_tokens: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactOutputUsageDetails)]
struct ResponsesCompactOutputUsageDetailsSchema {
    #[schema(minimum = 0)]
    reasoning_tokens: i64,
}

/// 可直接作为下一次 `/v1/responses` 输入的规范压缩窗口。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactResponse)]
struct ResponsesCompactResponseSchema {
    #[schema(min_length = 1, max_length = 512)]
    id: String,
    #[schema(example = "response.compaction")]
    object: String,
    #[schema(minimum = 0)]
    created_at: i64,
    #[schema(max_items = 1024)]
    output: Vec<ResponsesCompactItemSchema>,
    usage: ResponsesCompactUsageSchema,
}

/// OpenAI Responses 风格的稳定公开错误。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactErrorBody)]
struct ResponsesCompactErrorBodySchema {
    code: String,
    message: String,
    #[schema(required = true)]
    param: Option<String>,
    #[serde(rename = "type")]
    error_type: String,
}

/// OpenAI 兼容错误信封。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ResponsesCompactError)]
struct ResponsesCompactErrorSchema {
    error: ResponsesCompactErrorBodySchema,
}

#[utoipa::path(
    post,
    path = "/v1/responses/compact",
    operation_id = "compactResponse",
    tag = "OpenAI Responses",
    summary = "压缩完整 Responses 上下文窗口",
    description = "仅调度明确支持 Compact 的原生 OpenAI Responses 渠道；当前公开生产入口要求显式 input，不接受 previous_response_id，也不会回退普通 Responses。",
    request_body = ResponsesCompactRequestSchema,
    responses(
        (status = 200, description = "规范压缩窗口与独立 usage", body = ResponsesCompactResponseSchema),
        (status = 400, description = "请求或计费输入边界无效", body = ResponsesCompactErrorSchema),
        (status = 401, description = "API Key 无效", body = ResponsesCompactErrorSchema),
        (status = 429, description = "额度、并发或请求频率受限", body = ResponsesCompactErrorSchema),
        (status = 500, description = "用量无法按当前价格桶精确结算", body = ResponsesCompactErrorSchema),
        (status = 503, description = "没有可用的 Compact 候选", body = ResponsesCompactErrorSchema)
    ),
    security(("apiKeyAuth" = []))
)]
fn compact_response() {}

#[derive(OpenApi)]
#[openapi(
    paths(compact_response),
    components(schemas(
        ResponsesCompactRequestSchema,
        ResponsesCompactInputSchema,
        ResponsesCompactItemSchema,
        ResponsesCompactUsageSchema,
        ResponsesCompactInputUsageDetailsSchema,
        ResponsesCompactOutputUsageDetailsSchema,
        ResponsesCompactResponseSchema,
        ResponsesCompactErrorBodySchema,
        ResponsesCompactErrorSchema
    ))
)]
struct ResponsesCompactApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ResponsesCompactApi::openapi()
}
