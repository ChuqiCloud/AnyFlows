//! 通用 `/v1/rerank` 非流式协议转换。
//!
//! 该契约采用 Jina 与 new-api 的文本请求交集，并显式保留 Cohere search unit 用量。
//! 对象字段排序、多模态文档、返回 embedding、分块和截断扩展由后续供应商适配器处理。

use std::{error::Error, fmt};

use serde_json::{Map, Number, Value};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    CanonicalRerankRequest, CanonicalRerankRequestError, CanonicalRerankResponse, RerankDocument,
    RerankRelevanceScore, RerankResult, RerankSearchUnits, RerankTopN, RerankUsage, TokenCount,
    Usage, UsageDetails, UsageSemantics, UsageSource,
};

mod wire;

use wire::{Field, RerankDocumentWire, RerankRequestWire, RerankResponseWire};

/// 通用 Rerank 请求和响应的最大正文大小。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 8,
    max_nodes: 16_384,
    max_object_entries: 24,
    max_array_items: crate::MAX_RERANK_DOCUMENTS,
    max_string_bytes: crate::MAX_TOTAL_RERANK_TEXT_BYTES,
    max_key_bytes: 128,
};

const RESPONSE_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 10,
    max_nodes: 24_576,
    max_object_entries: 24,
    max_array_items: crate::MAX_RERANK_DOCUMENTS,
    max_string_bytes: crate::MAX_TOTAL_RERANK_TEXT_BYTES,
    max_key_bytes: 128,
};

/// 解析通用 `POST /v1/rerank` 文本请求。
///
/// 该函数拒绝重复键、显式空值、未知字段和无法无损归一的供应商扩展。缺失
/// `return_documents` 固定归一为 `false`，避免跨渠道默认值漂移。
pub fn parse_request(body: &[u8]) -> Result<CanonicalRerankRequest, ParseRerankRequestError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseRerankRequestError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, REQUEST_JSON_LIMITS).map_err(map_request_json_error)?;
    let wire = serde_json::from_value(value).map_err(|_| ParseRerankRequestError::InvalidValue)?;
    convert_request(wire)
}

/// 将 Canonical Rerank 请求构造成稳定的 `/v1/rerank` JSON。
///
/// 输出总是显式写出 `return_documents`，防止供应商默认值差异改变响应正文。
pub fn build_request(request: &CanonicalRerankRequest) -> Result<Value, BuildRerankRequestError> {
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
            Value::Array(request.documents().iter().map(build_document).collect()),
        ),
        (
            "return_documents".to_owned(),
            Value::Bool(request.return_documents()),
        ),
    ]);
    if let Some(top_n) = request.top_n() {
        root.insert(
            "top_n".to_owned(),
            Value::Number(
                u64::try_from(top_n.get())
                    .map_err(|_| BuildRerankRequestError::InvalidValue)?
                    .into(),
            ),
        );
    }
    let value = Value::Object(root);
    revalidate_request(&value)?;
    Ok(value)
}

/// 解析已完整读取的通用 Rerank 响应。
///
/// Jina token 用量和 Cohere search unit 分别保留，二者不会互相估算；响应缺失用量时
/// 保持 `None`，由后续生产计费切片决定是否使用本地 tokenizer。
pub fn parse_response(body: &[u8]) -> Result<CanonicalRerankResponse, ParseRerankResponseError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseRerankResponseError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, RESPONSE_JSON_LIMITS).map_err(map_response_json_error)?;
    let wire = serde_json::from_value(value).map_err(|_| ParseRerankResponseError::InvalidValue)?;
    convert_response(wire)
}

/// 将 Canonical Rerank 响应构造成通用 JSON。
///
/// token 用量输出 `prompt_tokens == total_tokens`；search unit 输出 Cohere 风格
/// `meta.billed_units.search_units`。缺失事实不会被补成零值。
pub fn build_response(
    response: &CanonicalRerankResponse,
) -> Result<Value, BuildRerankResponseError> {
    let mut root = Map::from_iter([
        ("object".to_owned(), Value::String("list".to_owned())),
        (
            "results".to_owned(),
            Value::Array(
                response
                    .results()
                    .iter()
                    .map(build_result)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
    ]);
    if let Some(response_id) = response.response_id() {
        root.insert("id".to_owned(), Value::String(response_id.to_owned()));
    }
    if let Some(model) = response.model() {
        root.insert("model".to_owned(), Value::String(model.to_owned()));
    }
    if let Some(usage) = response.usage() {
        if let Some(token_usage) = usage.token_usage() {
            root.insert(
                "usage".to_owned(),
                Value::Object(Map::from_iter([
                    (
                        "prompt_tokens".to_owned(),
                        Value::Number(token_usage.input_tokens().get().into()),
                    ),
                    (
                        "total_tokens".to_owned(),
                        Value::Number(token_usage.input_tokens().get().into()),
                    ),
                ])),
            );
        }
        if let Some(search_units) = usage.search_units() {
            root.insert(
                "meta".to_owned(),
                Value::Object(Map::from_iter([(
                    "billed_units".to_owned(),
                    Value::Object(Map::from_iter([(
                        "search_units".to_owned(),
                        Value::Number(
                            u64::try_from(search_units.get())
                                .map_err(|_| BuildRerankResponseError::InvalidValue)?
                                .into(),
                        ),
                    )])),
                )])),
            );
        }
    }
    let value = Value::Object(root);
    revalidate_response(&value)?;
    Ok(value)
}

fn convert_request(
    wire: RerankRequestWire,
) -> Result<CanonicalRerankRequest, ParseRerankRequestError> {
    if matches!(wire.rank_fields, Field::Value(_))
        || matches!(wire.max_tokens_per_doc, Field::Value(_))
        || matches!(wire.max_chunk_per_doc, Field::Value(_))
        || matches!(wire.overlap_tokens, Field::Value(_))
        || matches!(wire.return_embeddings, Field::Value(_))
    {
        return Err(ParseRerankRequestError::UnsupportedFeature);
    }
    let documents = wire.documents.into_iter().map(convert_document).collect();
    let top_n = match wire.top_n {
        Field::Missing => None,
        Field::Value(value) => {
            Some(RerankTopN::new(value).map_err(|_| ParseRerankRequestError::InvalidValue)?)
        }
    };
    let return_documents = match wire.return_documents {
        Field::Missing => false,
        Field::Value(value) => value,
    };
    CanonicalRerankRequest::new(wire.model, wire.query, documents, top_n, return_documents)
        .map_err(map_request_error)
}

fn convert_response(
    wire: RerankResponseWire,
) -> Result<CanonicalRerankResponse, ParseRerankResponseError> {
    if matches!(&wire.object, Field::Value(value) if value != "list")
        || wire
            .results
            .iter()
            .any(|result| matches!(&result.embedding, Field::Value(_)))
    {
        return Err(ParseRerankResponseError::UnsupportedFeature);
    }
    let results = wire
        .results
        .into_iter()
        .map(|result| {
            let score = RerankRelevanceScore::new(result.relevance_score)
                .map_err(|_| ParseRerankResponseError::InvalidValue)?;
            let document = match result.document {
                Field::Missing => None,
                Field::Value(document) => Some(convert_document(document)),
            };
            RerankResult::new(result.index, score, document)
                .map_err(|_| ParseRerankResponseError::InvalidValue)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let token_usage = match wire.usage {
        Field::Missing => None,
        Field::Value(usage) => {
            if matches!(usage.prompt_tokens, Field::Value(value) if value != usage.total_tokens)
                || matches!(usage.completion_tokens, Field::Value(value) if value != 0)
            {
                return Err(ParseRerankResponseError::InvalidValue);
            }
            let input_tokens = TokenCount::new(usage.total_tokens)
                .map_err(|_| ParseRerankResponseError::InvalidValue)?;
            Some(
                Usage::new(
                    input_tokens,
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
                .map_err(|_| ParseRerankResponseError::InvalidValue)?,
            )
        }
    };
    let search_units = match wire.meta {
        Field::Missing => None,
        Field::Value(meta) => Some(
            RerankSearchUnits::new(meta.billed_units.search_units)
                .map_err(|_| ParseRerankResponseError::InvalidValue)?,
        ),
    };
    let usage = if token_usage.is_none() && search_units.is_none() {
        None
    } else {
        Some(
            RerankUsage::new(token_usage, search_units)
                .map_err(|_| ParseRerankResponseError::InvalidValue)?,
        )
    };
    let response_id = match wire.id {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    };
    let model = match wire.model {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    };
    CanonicalRerankResponse::new(response_id, model, results, usage)
        .map_err(|_| ParseRerankResponseError::InvalidValue)
}

fn convert_document(document: RerankDocumentWire) -> RerankDocument {
    match document {
        RerankDocumentWire::Text(text) => RerankDocument::Text(text),
        RerankDocumentWire::TextObject(document) => RerankDocument::TextObject(document.text),
    }
}

fn build_document(document: &RerankDocument) -> Value {
    match document {
        RerankDocument::Text(text) => Value::String(text.clone()),
        RerankDocument::TextObject(text) => Value::Object(Map::from_iter([(
            "text".to_owned(),
            Value::String(text.clone()),
        )])),
    }
}

fn build_result(result: &RerankResult) -> Result<Value, BuildRerankResponseError> {
    let mut value = Map::from_iter([
        (
            "index".to_owned(),
            Value::Number(u64::from(result.index()).into()),
        ),
        (
            "relevance_score".to_owned(),
            Number::from_f64(result.relevance_score().get())
                .map(Value::Number)
                .ok_or(BuildRerankResponseError::InvalidValue)?,
        ),
    ]);
    if let Some(document) = result.document() {
        value.insert("document".to_owned(), build_document(document));
    }
    Ok(Value::Object(value))
}

fn revalidate_request(value: &Value) -> Result<(), BuildRerankRequestError> {
    let body = serde_json::to_vec(value).map_err(|_| BuildRerankRequestError::InvalidValue)?;
    parse_request(&body).map_err(|_| BuildRerankRequestError::InvalidValue)?;
    Ok(())
}

fn revalidate_response(value: &Value) -> Result<(), BuildRerankResponseError> {
    let body = serde_json::to_vec(value).map_err(|_| BuildRerankResponseError::InvalidValue)?;
    parse_response(&body).map_err(|_| BuildRerankResponseError::InvalidValue)?;
    Ok(())
}

fn map_request_json_error(error: BoundedJsonError) -> ParseRerankRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseRerankRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseRerankRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRerankRequestError::StructureLimitExceeded,
    }
}

fn map_response_json_error(error: BoundedJsonError) -> ParseRerankResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseRerankResponseError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseRerankResponseError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRerankResponseError::StructureLimitExceeded,
    }
}

fn map_request_error(error: CanonicalRerankRequestError) -> ParseRerankRequestError {
    match error {
        CanonicalRerankRequestError::TotalTextTooLarge => {
            ParseRerankRequestError::StructureLimitExceeded
        }
        CanonicalRerankRequestError::InvalidModel
        | CanonicalRerankRequestError::InvalidQuery
        | CanonicalRerankRequestError::InvalidDocumentCount
        | CanonicalRerankRequestError::InvalidDocument
        | CanonicalRerankRequestError::InvalidTopN => ParseRerankRequestError::InvalidValue,
    }
}

/// 通用 Rerank 请求解析错误，不保留模型、查询、文档或字段路径。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseRerankRequestError {
    /// 请求体超过协议层正文预算。
    BodyTooLarge,
    /// 请求体不是单个合法 JSON 值。
    InvalidJson,
    /// JSON 对象出现重复键。
    DuplicateKey,
    /// JSON 或业务结构超过受限预算。
    StructureLimitExceeded,
    /// 请求字段类型、取值或关联关系无效。
    InvalidValue,
    /// 请求使用了首切片尚未建模的供应商特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseRerankRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Rerank 请求体超过大小限制",
            Self::InvalidJson => "Rerank 请求体不是有效 JSON",
            Self::DuplicateKey => "Rerank 请求包含重复字段",
            Self::StructureLimitExceeded => "Rerank 请求结构超过限制",
            Self::InvalidValue => "Rerank 请求字段无效",
            Self::UnsupportedFeature => "Rerank 请求包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseRerankRequestError {}

/// 通用 Rerank 请求构造错误，不保留请求内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRerankRequestError {
    /// Canonical 请求无法重新满足通用 wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildRerankRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Rerank 请求")
    }
}

impl Error for BuildRerankRequestError {}

/// 通用 Rerank 响应解析错误，不保留上游标识、模型、文档或用量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseRerankResponseError {
    /// 响应体超过协议层正文预算。
    BodyTooLarge,
    /// 响应体不是单个合法 JSON 值。
    InvalidJson,
    /// JSON 对象出现重复键。
    DuplicateKey,
    /// JSON 或业务结构超过受限预算。
    StructureLimitExceeded,
    /// 响应字段类型、取值或关联关系无效。
    InvalidValue,
    /// 响应包含首切片尚未建模的供应商特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseRerankResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Rerank 响应体超过大小限制",
            Self::InvalidJson => "Rerank 响应体不是有效 JSON",
            Self::DuplicateKey => "Rerank 响应包含重复字段",
            Self::StructureLimitExceeded => "Rerank 响应结构超过限制",
            Self::InvalidValue => "Rerank 响应字段无效",
            Self::UnsupportedFeature => "Rerank 响应包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseRerankResponseError {}

/// 通用 Rerank 响应构造错误，不保留响应正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRerankResponseError {
    /// Canonical 响应无法重新满足通用 wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildRerankResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Rerank 响应")
    }
}

impl Error for BuildRerankResponseError {}

#[cfg(test)]
mod tests;
