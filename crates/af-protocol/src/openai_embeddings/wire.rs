use serde::{Deserialize, Deserializer, de::Error as _};

/// 区分字段缺失与显式提供；显式 `null` 在协议边界拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 客户端未提供字段。
    #[default]
    Missing,
    /// 客户端提供了非空字段。
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

/// OpenAI Embeddings 入站请求。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmbeddingRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 单条文本或按顺序排列的文本数组。
    pub(super) input: EmbeddingInputWire,
    /// 客户端要求的向量编码格式。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) encoding_format: Field<EmbeddingEncodingFormatWire>,
    /// 可选的目标向量维度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) dimensions: Field<u32>,
}

/// OpenAI Embeddings 支持的文本输入形态。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum EmbeddingInputWire {
    /// 单条文本。
    Text(String),
    /// 文本数组。
    Texts(Vec<String>),
}

/// OpenAI Embeddings 的输出编码格式。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum EmbeddingEncodingFormatWire {
    /// JSON 浮点数组。
    Float,
    /// Base64 编码向量；首切片明确不支持。
    Base64,
}

/// OpenAI Embeddings 非流式响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmbeddingResponseWire {
    /// 固定的列表响应标识。
    pub(super) object: String,
    /// 按输入索引对应的向量集合。
    pub(super) data: Vec<EmbeddingResponseItemWire>,
    /// 上游回显的模型名。
    pub(super) model: String,
    /// 上游返回的输入 token 用量。
    pub(super) usage: EmbeddingUsageWire,
}

/// 一条 OpenAI Embeddings 响应向量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmbeddingResponseItemWire {
    /// 固定的向量对象标识。
    pub(super) object: String,
    /// 对应请求输入的稳定索引。
    pub(super) index: u32,
    /// 浮点向量值。
    pub(super) embedding: Vec<f64>,
}

/// OpenAI Embeddings 的输入 token 用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmbeddingUsageWire {
    /// 输入 token 总数。
    pub(super) prompt_tokens: i64,
    /// 总 token 数；Embeddings 必须与输入 token 数相等。
    pub(super) total_tokens: i64,
}
