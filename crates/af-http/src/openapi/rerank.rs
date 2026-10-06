use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = RerankRequest)]
struct RerankRequestSchema {
    #[schema(
        min_length = 1,
        max_length = 256,
        example = "jina-reranker-v2-base-multilingual"
    )]
    model: String,
    #[schema(min_length = 1, max_length = 1048576)]
    query: String,
    #[schema(min_items = 1, max_items = 1000)]
    documents: Vec<RerankDocumentSchema>,
    #[schema(minimum = 1, maximum = 1000, required = false)]
    top_n: u32,
    #[schema(default = false, required = false)]
    return_documents: bool,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
#[schema(as = RerankDocument)]
enum RerankDocumentSchema {
    Text(String),
    TextObject(RerankTextDocumentSchema),
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(Serialize, ToSchema)]
#[schema(as = RerankTextDocument)]
struct RerankTextDocumentSchema {
    #[schema(min_length = 1, max_length = 1048576)]
    text: String,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = RerankResult)]
struct RerankResultSchema {
    #[schema(minimum = 0, maximum = 999)]
    index: u32,
    relevance_score: f64,
    #[schema(required = false)]
    document: RerankDocumentSchema,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = RerankUsage)]
struct RerankUsageSchema {
    #[schema(minimum = 0)]
    prompt_tokens: i64,
    #[schema(minimum = 0)]
    total_tokens: i64,
    #[schema(minimum = 0, required = false)]
    completion_tokens: i64,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = RerankResponse)]
struct RerankResponseSchema {
    #[schema(max_length = 256, required = false)]
    id: String,
    #[schema(min_length = 1, max_length = 256, required = false)]
    model: String,
    #[schema(example = "list")]
    object: String,
    #[schema(min_items = 1, max_items = 1000)]
    results: Vec<RerankResultSchema>,
    #[schema(required = false)]
    usage: RerankUsageSchema,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = RerankErrorBody)]
struct RerankErrorBodySchema {
    code: String,
    message: String,
    #[schema(required = false)]
    param: Option<String>,
    #[schema(rename = "type")]
    error_type: String,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI schema")]
#[derive(ToSchema)]
#[schema(as = RerankError)]
struct RerankErrorSchema {
    error: RerankErrorBodySchema,
}

#[allow(dead_code, reason = "仅用于生成 OpenAPI endpoint")]
#[utoipa::path(
    post,
    path = "/v1/rerank",
    operation_id = "rerank",
    tag = "Rerank",
    security(("bearerAuth" = [])),
    request_body = RerankRequestSchema,
    responses(
        (status = 200, description = "返回按相关度降序排列的文档结果", body = RerankResponseSchema),
        (status = 400, description = "请求正文或计费输入边界无效", body = RerankErrorSchema),
        (status = 401, description = "API Key 无效", body = RerankErrorSchema),
        (status = 429, description = "额度、并发或请求频率受限", body = RerankErrorSchema),
        (status = 500, description = "用量无法按当前价格桶精确结算", body = RerankErrorSchema),
        (status = 503, description = "没有可用的 Jina Rerank 候选", body = RerankErrorSchema)
    )
)]
fn rerank() {}

#[derive(OpenApi)]
#[openapi(
    paths(rerank),
    components(schemas(
        RerankRequestSchema,
        RerankDocumentSchema,
        RerankTextDocumentSchema,
        RerankResultSchema,
        RerankUsageSchema,
        RerankResponseSchema,
        RerankErrorBodySchema,
        RerankErrorSchema
    ))
)]
struct RerankApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RerankApi::openapi()
}
