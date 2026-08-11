use serde::{Deserialize, Deserializer, de::Error as _};

/// 区分字段缺失与显式空值；Cohere v2 响应中的显式 `null` 一律拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 上游未提供字段。
    #[default]
    Missing,
    /// 上游提供了非空字段。
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

/// Cohere v2 Rerank 完整响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CohereRerankResponseWire {
    /// 按相关度降序排列的结果。
    pub(super) results: Vec<CohereRerankResultWire>,
    /// Cohere 可选响应标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// Cohere 可选元数据。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) meta: Field<CohereRerankMetaWire>,
}

/// Cohere v2 单条排序结果。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CohereRerankResultWire {
    pub(super) index: u32,
    pub(super) relevance_score: f64,
}

/// Cohere v2 响应元数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CohereRerankMetaWire {
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) api_version: Field<CohereApiVersionWire>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) billed_units: Field<CohereBilledUnitsWire>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tokens: Field<CohereTokenUsageWire>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) cached_tokens: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) warnings: Field<Vec<String>>,
}

/// Cohere API 版本元数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CohereApiVersionWire {
    pub(super) version: String,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) is_deprecated: Field<bool>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) is_experimental: Field<bool>,
}

/// Cohere 通用计费维度；Rerank 只接受 search unit 与输入 token。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CohereBilledUnitsWire {
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) images: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) input_tokens: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) image_tokens: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_tokens: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) search_units: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) classifications: Field<f64>,
}

/// Cohere 实际 token 用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CohereTokenUsageWire {
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) input_tokens: Field<f64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_tokens: Field<f64>,
}
