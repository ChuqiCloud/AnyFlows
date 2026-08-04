//! Gemini `generateContent` 协议转换。

use std::{error::Error, fmt};

use af_domain::Protocol;
use bytes::Bytes;
use serde_json::Value;

use crate::{
    CanonicalRequest, CanonicalRequestEnvelope,
    bounded_json::{self, BoundedJsonError, JsonLimits},
};

mod build;
mod build_response;
mod convert;
mod error;
mod parse_response;
mod response_wire;
mod streaming;
mod upstream_error;
mod wire;

pub use build::{BuildRequestError, build_request};
pub use build_response::{BuildResponseError, build_response};
pub use error::encode_error;
pub use parse_response::{ParseResponseError, parse_response};
pub use streaming::{
    DEFAULT_MAX_SSE_EVENT_BYTES, EncodeStreamError, GeminiGenerateContentStreamDecoder,
    GeminiGenerateContentStreamEncoder, MAX_SSE_EVENT_BYTES, ParseStreamError, SseParseError,
};
pub use upstream_error::{GeminiUpstreamErrorKind, classify_error_response};

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod response_tests;
#[cfg(test)]
mod tests;

/// Gemini `generateContent` 非流式请求与响应的最大正文大小。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_BODY_BYTES,
    max_key_bytes: 1_024,
};

/// 解析 Gemini `generateContent` 非流式入站请求。
///
/// `model_resource` 必须来自官方路径参数并使用 `models/{model}` 格式。该函数会在
/// 进入 Canonical 前关闭重复键、超预算结构、未知字段和无法无损表达的 Gemini 能力。
pub fn parse_request(
    model_resource: &str,
    body: &[u8],
) -> Result<CanonicalRequest, ParseRequestError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseRequestError::BodyTooLarge);
    }

    let value = bounded_json::parse_value(body, REQUEST_JSON_LIMITS).map_err(map_json_error)?;
    if uses_known_unsupported_feature(&value) {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    let wire = serde_json::from_value(value).map_err(|_| ParseRequestError::InvalidValue)?;
    convert::convert_request(model_resource, wire)
}

/// 解析请求并保留只可用于 Gemini 同协议直通的原始正文。
pub fn parse_request_envelope(
    model_resource: &str,
    body: Bytes,
) -> Result<CanonicalRequestEnvelope, ParseRequestError> {
    parse_request_envelope_for_mode(model_resource, body, false)
}

/// 解析由 `streamGenerateContent` 或 `alt=sse` 选择的流式请求。
///
/// Gemini 把响应模式放在 URL 而不是正文中，因此协议边界必须在保留已验证源正文的
/// 同时显式写入 Canonical `stream` 语义。
pub fn parse_stream_request_envelope(
    model_resource: &str,
    body: Bytes,
) -> Result<CanonicalRequestEnvelope, ParseRequestError> {
    parse_request_envelope_for_mode(model_resource, body, true)
}

fn parse_request_envelope_for_mode(
    model_resource: &str,
    body: Bytes,
    stream: bool,
) -> Result<CanonicalRequestEnvelope, ParseRequestError> {
    let mut canonical = parse_request(model_resource, &body)?;
    canonical.stream = stream;
    Ok(CanonicalRequestEnvelope::from_validated_source(
        canonical,
        Protocol::Gemini,
        body,
    ))
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(request) = value.as_object() else {
        return false;
    };
    if ["cachedContent", "safetySettings", "serviceTier", "store"]
        .iter()
        .any(|key| request.contains_key(*key))
    {
        return true;
    }
    request
        .get("generationConfig")
        .is_some_and(generation_config_is_unsupported)
        || request
            .get("toolConfig")
            .is_some_and(tool_config_is_unsupported)
        || request
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| tools.iter().any(tool_is_unsupported))
        || request
            .get("systemInstruction")
            .is_some_and(content_is_unsupported)
        || request
            .get("contents")
            .and_then(Value::as_array)
            .is_some_and(|contents| contents.iter().any(content_is_unsupported))
}

fn generation_config_is_unsupported(value: &Value) -> bool {
    value.as_object().is_some_and(|config| {
        [
            "_responseJsonSchema",
            "audioTranscriptionConfig",
            "enableAffectiveDialog",
            "enableEnhancedCivicAnswers",
            "frequencyPenalty",
            "imageConfig",
            "logprobs",
            "mediaResolution",
            "presencePenalty",
            "responseFormat",
            "responseJsonSchema",
            "responseLogprobs",
            "responseMimeType",
            "responseModalities",
            "responseSchema",
            "seed",
            "speechConfig",
            "topK",
            "translationConfig",
        ]
        .iter()
        .any(|key| config.contains_key(*key))
    })
}

fn tool_config_is_unsupported(value: &Value) -> bool {
    value.as_object().is_some_and(|config| {
        ["includeServerSideToolInvocations", "retrievalConfig"]
            .iter()
            .any(|key| config.contains_key(*key))
    })
}

fn tool_is_unsupported(value: &Value) -> bool {
    let Some(tool) = value.as_object() else {
        return false;
    };
    if [
        "codeExecution",
        "computerUse",
        "fileSearch",
        "googleMaps",
        "googleSearch",
        "googleSearchRetrieval",
        "mcpServers",
        "urlContext",
    ]
    .iter()
    .any(|key| tool.contains_key(*key))
    {
        return true;
    }
    tool.get("functionDeclarations")
        .and_then(Value::as_array)
        .is_some_and(|declarations| declarations.iter().any(function_declaration_is_unsupported))
}

fn function_declaration_is_unsupported(value: &Value) -> bool {
    value.as_object().is_some_and(|declaration| {
        ["behavior", "response", "responseJsonSchema"]
            .iter()
            .any(|key| declaration.contains_key(*key))
    })
}

fn content_is_unsupported(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .is_some_and(|parts| parts.iter().any(part_is_unsupported))
}

fn part_is_unsupported(value: &Value) -> bool {
    let Some(part) = value.as_object() else {
        return false;
    };
    if [
        "codeExecutionResult",
        "executableCode",
        "fileData",
        "mediaResolution",
        "partMetadata",
        "toolCall",
        "toolResponse",
        "videoMetadata",
    ]
    .iter()
    .any(|key| part.contains_key(*key))
    {
        return true;
    }
    part.get("functionResponse")
        .and_then(Value::as_object)
        .is_some_and(|response| {
            ["parts", "scheduling", "willContinue"]
                .iter()
                .any(|key| response.contains_key(*key))
        })
}

fn map_json_error(error: BoundedJsonError) -> ParseRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
    }
}

/// Gemini 请求解析错误，不保留字段名、路径或外部输入。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseRequestError {
    /// 请求体超过协议层防御性大小上限。
    BodyTooLarge,
    /// 请求体不是单个合法 JSON 值。
    InvalidJson,
    /// 任意层级的 JSON 对象包含重复键。
    DuplicateKey,
    /// 请求的 JSON 或业务结构超过预算。
    StructureLimitExceeded,
    /// 路径模型或请求字段的类型、取值、角色与关联关系无效。
    InvalidValue,
    /// 请求使用了当前切片无法无损归一的协议能力。
    UnsupportedFeature,
}

impl fmt::Display for ParseRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BodyTooLarge => formatter.write_str("请求体超过大小限制"),
            Self::InvalidJson => formatter.write_str("请求体不是有效 JSON"),
            Self::DuplicateKey => formatter.write_str("请求体包含重复字段"),
            Self::StructureLimitExceeded => formatter.write_str("请求结构超过限制"),
            Self::InvalidValue => formatter.write_str("请求字段值无效"),
            Self::UnsupportedFeature => formatter.write_str("请求包含当前不支持的特性"),
        }
    }
}

impl Error for ParseRequestError {}
