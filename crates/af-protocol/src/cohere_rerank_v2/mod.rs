//! Cohere v2 `POST /v2/rerank` 协议转换。
//!
//! 该模块只接受官方文本 documents 交集，并把 Cohere search unit 与可选真实输入 token
//! 分开保留。search unit 绝不猜测换算为 token；无法建模的非零计费维度失败关闭。

use std::{error::Error, fmt};

use serde_json::{Map, Value};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    CanonicalRerankRequest, CanonicalRerankResponse, RerankRelevanceScore, RerankResult,
    RerankSearchUnits, RerankUsage, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
};

mod wire;

use wire::{
    CohereBilledUnitsWire, CohereRerankMetaWire, CohereRerankResponseWire, CohereTokenUsageWire,
    Field,
};

/// Cohere v2 Rerank 请求与响应的最大正文大小。
pub const MAX_BODY_BYTES: usize = crate::rerank_v1::MAX_BODY_BYTES;

const RESPONSE_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 10,
    max_nodes: 24_576,
    max_object_entries: 32,
    max_array_items: crate::MAX_RERANK_DOCUMENTS,
    max_string_bytes: crate::MAX_TOTAL_RERANK_TEXT_BYTES,
    max_key_bytes: 128,
};

/// 将 Canonical Rerank 请求构造成 Cohere v2 JSON。
///
/// Cohere v2 只接受字符串文档且不提供 `return_documents`；对象文档在已验证 Canonical
/// 边界内提取文本，公开响应需要文档时由网关依据原请求索引安全回填。
pub fn build_request(request: &CanonicalRerankRequest) -> Value {
    let mut root = Map::from_iter([
        (
            "model".to_owned(),
            Value::String(request.model().to_owned()),
        ),
        (
            "query".to_owned(),
            Value::String(request.query().to_owned()),
        ),
        (
            "documents".to_owned(),
            Value::Array(
                request
                    .documents()
                    .iter()
                    .map(|document| Value::String(document.text().to_owned()))
                    .collect(),
            ),
        ),
    ]);
    if let Some(top_n) = request.top_n() {
        root.insert("top_n".to_owned(), Value::Number(top_n.get().into()));
    }
    Value::Object(root)
}

/// 解析 Cohere v2 Rerank 完整响应。
pub fn parse_response(body: &[u8]) -> Result<CanonicalRerankResponse, ParseCohereRerankError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseCohereRerankError::BodyTooLarge);
    }
    let value = bounded_json::parse_value(body, RESPONSE_JSON_LIMITS).map_err(map_json_error)?;
    let wire: CohereRerankResponseWire =
        serde_json::from_value(value).map_err(|_| ParseCohereRerankError::InvalidValue)?;
    convert_response(wire)
}

fn convert_response(
    wire: CohereRerankResponseWire,
) -> Result<CanonicalRerankResponse, ParseCohereRerankError> {
    let results = wire
        .results
        .into_iter()
        .map(|result| {
            let score = RerankRelevanceScore::new(result.relevance_score)
                .map_err(|_| ParseCohereRerankError::InvalidValue)?;
            RerankResult::new(result.index, score, None)
                .map_err(|_| ParseCohereRerankError::InvalidValue)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let usage = match wire.meta {
        Field::Missing => None,
        Field::Value(meta) => convert_meta(meta)?,
    };
    let response_id = match wire.id {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    };
    CanonicalRerankResponse::new(response_id, None, results, usage)
        .map_err(|_| ParseCohereRerankError::InvalidValue)
}

fn convert_meta(meta: CohereRerankMetaWire) -> Result<Option<RerankUsage>, ParseCohereRerankError> {
    if let Field::Value(version) = meta.api_version {
        if version.version != "2" {
            return Err(ParseCohereRerankError::UnsupportedFeature);
        }
        let _ = (version.is_deprecated, version.is_experimental);
    }
    if nonzero(meta.cached_tokens)? || nonempty_warnings(meta.warnings) {
        return Err(ParseCohereRerankError::UnsupportedFeature);
    }

    let (billed_input_tokens, search_units) = match meta.billed_units {
        Field::Missing => (None, None),
        Field::Value(units) => convert_billed_units(units)?,
    };
    let actual_input_tokens = match meta.tokens {
        Field::Missing => None,
        Field::Value(tokens) => convert_token_usage(tokens)?,
    };
    if billed_input_tokens.is_some()
        && actual_input_tokens.is_some()
        && billed_input_tokens != actual_input_tokens
    {
        return Err(ParseCohereRerankError::InvalidValue);
    }
    let input_tokens = actual_input_tokens.or(billed_input_tokens);
    let token_usage = input_tokens.map(build_token_usage).transpose()?;
    if token_usage.is_none() && search_units.is_none() {
        Ok(None)
    } else {
        RerankUsage::new(token_usage, search_units)
            .map(Some)
            .map_err(|_| ParseCohereRerankError::InvalidValue)
    }
}

fn convert_billed_units(
    units: CohereBilledUnitsWire,
) -> Result<(Option<i64>, Option<RerankSearchUnits>), ParseCohereRerankError> {
    if nonzero(units.images)?
        || nonzero(units.image_tokens)?
        || nonzero(units.output_tokens)?
        || nonzero(units.classifications)?
    {
        return Err(ParseCohereRerankError::UnsupportedFeature);
    }
    let input_tokens = optional_integer(units.input_tokens)?;
    let search_units = optional_integer(units.search_units)?
        .map(|value| {
            u32::try_from(value)
                .ok()
                .and_then(|value| RerankSearchUnits::new(value).ok())
                .ok_or(ParseCohereRerankError::InvalidValue)
        })
        .transpose()?;
    Ok((input_tokens, search_units))
}

fn convert_token_usage(
    tokens: CohereTokenUsageWire,
) -> Result<Option<i64>, ParseCohereRerankError> {
    if nonzero(tokens.output_tokens)? {
        return Err(ParseCohereRerankError::UnsupportedFeature);
    }
    optional_integer(tokens.input_tokens)
}

fn build_token_usage(input_tokens: i64) -> Result<Usage, ParseCohereRerankError> {
    Usage::new(
        TokenCount::new(input_tokens).map_err(|_| ParseCohereRerankError::InvalidValue)?,
        TokenCount::ZERO,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| ParseCohereRerankError::InvalidValue)
}

fn optional_integer(field: Field<f64>) -> Result<Option<i64>, ParseCohereRerankError> {
    match field {
        Field::Missing => Ok(None),
        Field::Value(value)
            if value.is_finite()
                && value >= 0.0
                && value.fract() == 0.0
                && value <= i64::MAX as f64 =>
        {
            Ok(Some(value as i64))
        }
        Field::Value(_) => Err(ParseCohereRerankError::InvalidValue),
    }
}

fn nonzero(field: Field<f64>) -> Result<bool, ParseCohereRerankError> {
    Ok(optional_integer(field)?.is_some_and(|value| value != 0))
}

fn nonempty_warnings(field: Field<Vec<String>>) -> bool {
    matches!(field, Field::Value(warnings) if !warnings.is_empty())
}

fn map_json_error(error: BoundedJsonError) -> ParseCohereRerankError {
    match error {
        BoundedJsonError::InvalidJson => ParseCohereRerankError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseCohereRerankError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseCohereRerankError::StructureLimitExceeded,
    }
}

/// Cohere v2 Rerank 响应解析错误，不保留上游正文或用量数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseCohereRerankError {
    /// 响应体超过协议预算。
    BodyTooLarge,
    /// 响应不是单个合法 JSON 值。
    InvalidJson,
    /// 响应包含重复字段。
    DuplicateKey,
    /// 响应结构超过受限预算。
    StructureLimitExceeded,
    /// 响应字段类型、取值或关联关系无效。
    InvalidValue,
    /// 响应携带当前无法无损归一的 Cohere 特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseCohereRerankError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BodyTooLarge => "Cohere Rerank 响应体超过大小限制",
            Self::InvalidJson => "Cohere Rerank 响应不是有效 JSON",
            Self::DuplicateKey => "Cohere Rerank 响应包含重复字段",
            Self::StructureLimitExceeded => "Cohere Rerank 响应结构超过限制",
            Self::InvalidValue => "Cohere Rerank 响应字段无效",
            Self::UnsupportedFeature => "Cohere Rerank 响应包含当前不支持的特性",
        })
    }
}

impl Error for ParseCohereRerankError {}

#[cfg(test)]
mod tests;
