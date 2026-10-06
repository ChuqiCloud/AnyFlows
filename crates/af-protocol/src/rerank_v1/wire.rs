use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;

/// 区分字段缺失与显式提供；显式 `null` 在协议边界拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 客户端或上游未提供字段。
    #[default]
    Missing,
    /// 客户端或上游提供了非空字段。
    Value(T),
}

pub(super) fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

/// 通用 `/v1/rerank` 文本请求。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 用于比较文档相关性的查询文本。
    pub(super) query: String,
    /// 字符串或严格 `{text}` 对象文档。
    pub(super) documents: Vec<RerankDocumentWire>,
    /// 可选最大返回条数。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) top_n: Field<u32>,
    /// 是否在结果中回显原文档。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) return_documents: Field<bool>,
    /// Cohere 对象文档字段排序；首切片不支持。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) rank_fields: Field<Value>,
    /// Cohere 单文档截断预算；首切片不支持。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) max_tokens_per_doc: Field<Value>,
    /// new-api 文档分块扩展；首切片不支持。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) max_chunk_per_doc: Field<Value>,
    /// new-api 文档分块重叠扩展；首切片不支持。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) overlap_tokens: Field<Value>,
    /// Jina 返回向量扩展；首切片不支持。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) return_embeddings: Field<Value>,
}

/// Rerank 文档的两种受控文本形态。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RerankDocumentWire {
    /// 直接字符串文档。
    Text(String),
    /// 仅包含 `text` 字段的对象文档。
    TextObject(RerankTextDocumentWire),
}

/// 严格文本对象文档。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankTextDocumentWire {
    pub(super) text: String,
}

/// 通用 `/v1/rerank` 非流式响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankResponseWire {
    /// Cohere 风格的可选响应标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// Jina 风格的可选模型名。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) model: Field<String>,
    /// Jina 固定响应类型；提供时必须为 `list`。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) object: Field<String>,
    /// 已按相关度降序排列的结果。
    pub(super) results: Vec<RerankResultWire>,
    /// Jina/new-api 风格的可选 token 用量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) usage: Field<RerankTokenUsageWire>,
    /// Cohere 风格的可选 search unit 用量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) meta: Field<RerankMetaWire>,
}

/// 单条 Rerank 结果。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankResultWire {
    /// 原始请求文档索引。
    pub(super) index: u32,
    /// 供应商相关的有限相关度分数。
    pub(super) relevance_score: f64,
    /// 按请求要求回显的可选文档。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) document: Field<RerankDocumentWire>,
    /// Jina 返回向量扩展；首切片不支持。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) embedding: Field<Value>,
}

/// Jina/new-api 风格的纯输入 token 用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankTokenUsageWire {
    /// 输入 token 数；Jina 原生响应可能只提供总量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) prompt_tokens: Field<i64>,
    /// Rerank 不产生输出 token，因此必须与输入 token 相等。
    pub(super) total_tokens: i64,
    /// new-api 兼容字段；提供时必须为零。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) completion_tokens: Field<i64>,
}

/// Cohere 风格的响应元数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankMetaWire {
    pub(super) billed_units: RerankBilledUnitsWire,
}

/// Cohere Rerank 按调用计费的 search unit。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankBilledUnitsWire {
    pub(super) search_units: u32,
}
