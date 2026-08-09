//! OpenAI Embeddings 协议转换。
//!
//! 首切片只处理非流式文本向量化：请求接受单条文本或文本数组，响应只接受有限的
//! `float` 向量。token ID 输入、base64 向量和网关运行时接线由后续独立切片处理。

use std::{error::Error, fmt};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    CanonicalEmbeddingRequest, CanonicalEmbeddingRequestError, CanonicalEmbeddingResponse,
    EmbeddingDimensions, EmbeddingInput, EmbeddingVector, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource,
};
use serde_json::{Map, Number, Value};

mod wire;

use wire::{
    EmbeddingEncodingFormatWire, EmbeddingInputWire, EmbeddingRequestWire, EmbeddingResponseWire,
    Field,
};

/// OpenAI Embeddings 请求和非流式响应的最大正文大小。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 8,
    max_nodes: 8_192,
    max_object_entries: 16,
    max_array_items: crate::MAX_EMBEDDING_INPUTS,
    max_string_bytes: crate::MAX_TOTAL_EMBEDDING_TEXT_BYTES,
    max_key_bytes: 128,
};

const RESPONSE_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 8,
    max_nodes: crate::MAX_TOTAL_EMBEDDING_VALUES + 16_384,
    max_object_entries: 16,
    max_array_items: crate::MAX_EMBEDDING_DIMENSIONS,
    max_string_bytes: crate::MAX_TOTAL_EMBEDDING_TEXT_BYTES,
    max_key_bytes: 128,
};

/// 解析 OpenAI `POST /v1/embeddings` 请求。
///
/// 该函数是文本向量化请求的信任边界，拒绝重复键、未知字段、token ID 输入、base64
/// 输出格式和超过结构预算的 JSON；它不接受也不产生 Chat 的消息或工具语义。
pub fn parse_request(body: &[u8]) -> Result<CanonicalEmbeddingRequest, ParseEmbeddingRequestError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseEmbeddingRequestError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, REQUEST_JSON_LIMITS).map_err(map_request_json_error)?;
    let wire =
        serde_json::from_value(value).map_err(|_| ParseEmbeddingRequestError::InvalidValue)?;
    convert_request(wire)
}

/// 将 Canonical Embeddings 请求构造成 OpenAI wire JSON。
///
/// 输出固定为 `encoding_format: "float"`，并重新经过入站解析器校验，避免程序化构造绕过
/// 协议的结构、文本和维度预算。
pub fn build_request(
    request: &CanonicalEmbeddingRequest,
) -> Result<Value, BuildEmbeddingRequestError> {
    let mut root = Map::from_iter([
        (
            "model".to_owned(),
            Value::String(request.model().to_owned()),
        ),
        ("input".to_owned(), build_input(request.input())),
        (
            "encoding_format".to_owned(),
            Value::String("float".to_owned()),
        ),
    ]);
    if let Some(dimensions) = request.dimensions() {
        root.insert(
            "dimensions".to_owned(),
            Value::Number(
                u64::try_from(dimensions.get())
                    .map_err(|_| BuildEmbeddingRequestError::InvalidValue)?
                    .into(),
            ),
        );
    }
    let value = Value::Object(root);
    revalidate_request(&value)?;
    Ok(value)
}

/// 解析已从上游完整读取的 OpenAI Embeddings 响应。
///
/// 该边界只生成自洽的 Canonical 响应；调用方必须再调用
/// [`CanonicalEmbeddingResponse::validate_for_request`] 关联原请求的条数和维度。
pub fn parse_response(
    body: &[u8],
) -> Result<CanonicalEmbeddingResponse, ParseEmbeddingResponseError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseEmbeddingResponseError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, RESPONSE_JSON_LIMITS).map_err(map_response_json_error)?;
    let wire =
        serde_json::from_value(value).map_err(|_| ParseEmbeddingResponseError::InvalidValue)?;
    convert_response(wire)
}

/// 将 Canonical Embeddings 响应构造成 OpenAI wire JSON。
///
/// 调用方在生产网关中应先把模型名改写为客户端请求模型，再调用本函数，禁止回显上游的
/// 私有模型标识。
pub fn build_response(
    response: &CanonicalEmbeddingResponse,
) -> Result<Value, BuildEmbeddingResponseError> {
    let data = response
        .vectors()
        .iter()
        .map(build_vector)
        .collect::<Result<Vec<_>, _>>()?;
    let usage = response.usage();
    let total_tokens = usage
        .checked_total_tokens()
        .map_err(|_| BuildEmbeddingResponseError::InvalidValue)?;
    let value = Value::Object(Map::from_iter([
        ("object".to_owned(), Value::String("list".to_owned())),
        ("data".to_owned(), Value::Array(data)),
        (
            "model".to_owned(),
            Value::String(response.model().to_owned()),
        ),
        (
            "usage".to_owned(),
            Value::Object(Map::from_iter([
                (
                    "prompt_tokens".to_owned(),
                    Value::Number(usage.input_tokens().get().into()),
                ),
                (
                    "total_tokens".to_owned(),
                    Value::Number(total_tokens.get().into()),
                ),
            ])),
        ),
    ]));
    revalidate_response(&value)?;
    Ok(value)
}

fn convert_request(
    wire: EmbeddingRequestWire,
) -> Result<CanonicalEmbeddingRequest, ParseEmbeddingRequestError> {
    let input = match wire.input {
        EmbeddingInputWire::Text(text) => EmbeddingInput::Text(text),
        EmbeddingInputWire::Texts(texts) => EmbeddingInput::Texts(texts),
    };
    match wire.encoding_format {
        Field::Missing | Field::Value(EmbeddingEncodingFormatWire::Float) => {}
        Field::Value(EmbeddingEncodingFormatWire::Base64) => {
            return Err(ParseEmbeddingRequestError::UnsupportedFeature);
        }
    }
    let dimensions = match wire.dimensions {
        Field::Missing => None,
        Field::Value(value) => Some(
            EmbeddingDimensions::new(value)
                .map_err(|_| ParseEmbeddingRequestError::InvalidValue)?,
        ),
    };
    CanonicalEmbeddingRequest::new(wire.model, input, dimensions).map_err(map_request_error)
}

fn convert_response(
    wire: EmbeddingResponseWire,
) -> Result<CanonicalEmbeddingResponse, ParseEmbeddingResponseError> {
    let EmbeddingResponseWire {
        object,
        data,
        model,
        usage,
    } = wire;
    if object != "list"
        || data.iter().any(|item| item.object != "embedding")
        || usage.prompt_tokens != usage.total_tokens
    {
        return Err(ParseEmbeddingResponseError::InvalidValue);
    }
    let vectors = data
        .into_iter()
        .map(|item| {
            EmbeddingVector::new(item.index, item.embedding)
                .map_err(|_| ParseEmbeddingResponseError::InvalidValue)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let input_tokens = TokenCount::new(usage.prompt_tokens)
        .map_err(|_| ParseEmbeddingResponseError::InvalidValue)?;
    let usage = Usage::new(
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
    .map_err(|_| ParseEmbeddingResponseError::InvalidValue)?;
    CanonicalEmbeddingResponse::new(model, vectors, usage)
        .map_err(|_| ParseEmbeddingResponseError::InvalidValue)
}

fn build_input(input: &EmbeddingInput) -> Value {
    match input {
        EmbeddingInput::Text(text) => Value::String(text.clone()),
        EmbeddingInput::Texts(texts) => {
            Value::Array(texts.iter().cloned().map(Value::String).collect::<Vec<_>>())
        }
    }
}

fn build_vector(vector: &EmbeddingVector) -> Result<Value, BuildEmbeddingResponseError> {
    let values = vector
        .values()
        .iter()
        .map(|value| {
            Number::from_f64(*value)
                .map(Value::Number)
                .ok_or(BuildEmbeddingResponseError::InvalidValue)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Object(Map::from_iter([
        ("object".to_owned(), Value::String("embedding".to_owned())),
        (
            "index".to_owned(),
            Value::Number(u64::from(vector.index()).into()),
        ),
        ("embedding".to_owned(), Value::Array(values)),
    ])))
}

fn revalidate_request(value: &Value) -> Result<(), BuildEmbeddingRequestError> {
    let body = serde_json::to_vec(value).map_err(|_| BuildEmbeddingRequestError::InvalidValue)?;
    parse_request(&body).map_err(|_| BuildEmbeddingRequestError::InvalidValue)?;
    Ok(())
}

fn revalidate_response(value: &Value) -> Result<(), BuildEmbeddingResponseError> {
    let body = serde_json::to_vec(value).map_err(|_| BuildEmbeddingResponseError::InvalidValue)?;
    parse_response(&body).map_err(|_| BuildEmbeddingResponseError::InvalidValue)?;
    Ok(())
}

fn map_request_json_error(error: BoundedJsonError) -> ParseEmbeddingRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseEmbeddingRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseEmbeddingRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseEmbeddingRequestError::StructureLimitExceeded,
    }
}

fn map_response_json_error(error: BoundedJsonError) -> ParseEmbeddingResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseEmbeddingResponseError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseEmbeddingResponseError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseEmbeddingResponseError::StructureLimitExceeded,
    }
}

fn map_request_error(error: CanonicalEmbeddingRequestError) -> ParseEmbeddingRequestError {
    match error {
        CanonicalEmbeddingRequestError::InvalidModel
        | CanonicalEmbeddingRequestError::InvalidInputCount
        | CanonicalEmbeddingRequestError::InvalidText => ParseEmbeddingRequestError::InvalidValue,
        CanonicalEmbeddingRequestError::TotalTextTooLarge => {
            ParseEmbeddingRequestError::StructureLimitExceeded
        }
    }
}

/// OpenAI Embeddings 请求解析错误，不保留模型名、输入文本或字段路径。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseEmbeddingRequestError {
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
    /// 请求使用了首切片尚未建模的特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseEmbeddingRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Embeddings 请求体超过大小限制",
            Self::InvalidJson => "Embeddings 请求体不是有效 JSON",
            Self::DuplicateKey => "Embeddings 请求包含重复字段",
            Self::StructureLimitExceeded => "Embeddings 请求结构超过限制",
            Self::InvalidValue => "Embeddings 请求字段无效",
            Self::UnsupportedFeature => "Embeddings 请求包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseEmbeddingRequestError {}

/// OpenAI Embeddings 请求构造错误，不保留请求内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildEmbeddingRequestError {
    /// Canonical 请求无法重新满足 OpenAI wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildEmbeddingRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Embeddings 请求")
    }
}

impl Error for BuildEmbeddingRequestError {}

/// OpenAI Embeddings 响应解析错误，不保留模型名、向量或上游正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseEmbeddingResponseError {
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
}

impl fmt::Display for ParseEmbeddingResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Embeddings 响应体超过大小限制",
            Self::InvalidJson => "Embeddings 响应体不是有效 JSON",
            Self::DuplicateKey => "Embeddings 响应包含重复字段",
            Self::StructureLimitExceeded => "Embeddings 响应结构超过限制",
            Self::InvalidValue => "Embeddings 响应字段无效",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseEmbeddingResponseError {}

/// OpenAI Embeddings 响应构造错误，不保留响应模型或向量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildEmbeddingResponseError {
    /// Canonical 响应无法重新满足 OpenAI wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildEmbeddingResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Embeddings 响应")
    }
}

impl Error for BuildEmbeddingResponseError {}

#[cfg(test)]
mod tests;
