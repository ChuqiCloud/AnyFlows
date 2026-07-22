//! OpenAI Chat Completions 协议转换。

use std::{error::Error, fmt};

use bounded_json::{BoundedJsonError, JsonLimits};
use serde_json::Value;

use crate::CanonicalRequest;

mod bounded_json;
mod build;
mod convert;
mod error;
mod error_response;
mod parse_response;
mod response_wire;
mod streaming;
mod wire;

pub use build::{BuildRequestError, build_request};
pub use error::encode_error;
pub use error_response::{OpenAiUpstreamErrorKind, classify_error_response};
pub use parse_response::{ParseResponseError, parse_response};
pub use streaming::{
    DEFAULT_MAX_SSE_EVENT_BYTES, EncodeStreamError, MAX_SSE_EVENT_BYTES, OpenAiChatStreamDecoder,
    OpenAiChatStreamEncoder, ParseStreamError, SseEvent, SseParseError, SseParser,
};

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod response_tests;
#[cfg(test)]
mod tests;

/// OpenAI Chat 请求与非流式响应的最大正文大小。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: 32 * 1024 * 1024,
    max_key_bytes: 1_024,
};

/// 解析非流式 OpenAI Chat Completions 请求。
///
/// 本函数是协议输入的信任边界，会拒绝重复 JSON 键、结构超限、未知字段和
/// 当前 Canonical IR 无法无损表达的请求特性。
pub fn parse_request(body: &[u8]) -> Result<CanonicalRequest, ParseRequestError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseRequestError::BodyTooLarge);
    }

    let value = bounded_json::parse_value(body, REQUEST_JSON_LIMITS).map_err(map_json_error)?;
    if uses_known_unsupported_feature(&value) {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    let wire = serde_json::from_value(value).map_err(|_| ParseRequestError::InvalidValue)?;
    convert::convert_request(wire)
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(request) = value.as_object() else {
        return false;
    };
    if [
        "function_call",
        "functions",
        "prompt_cache_breakpoint",
        "stream_options",
    ]
    .iter()
    .any(|key| request.contains_key(*key))
    {
        return true;
    }

    request
        .get("messages")
        .and_then(Value::as_array)
        .is_some_and(|messages| messages.iter().any(message_uses_unsupported_feature))
        || request
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| tools.iter().any(is_custom_tool))
        || request
            .get("tool_choice")
            .is_some_and(is_unsupported_tool_choice)
}

fn message_uses_unsupported_feature(message: &Value) -> bool {
    let Some(message) = message.as_object() else {
        return false;
    };
    let role = message.get("role").and_then(Value::as_str);
    if role == Some("function")
        || (matches!(role, Some("system" | "developer" | "user" | "assistant"))
            && message.contains_key("name"))
    {
        return true;
    }
    if role == Some("assistant")
        && ["audio", "function_call", "refusal"]
            .iter()
            .any(|key| message.contains_key(*key))
    {
        return true;
    }

    let unsupported_content = message
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|parts| {
            parts.iter().any(|part| {
                part.as_object().is_some_and(|part| {
                    part.contains_key("prompt_cache_breakpoint")
                        || matches!(
                            part.get("type").and_then(Value::as_str),
                            Some("file" | "refusal")
                        )
                })
            })
        });
    let custom_tool_call = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| calls.iter().any(is_custom_tool));
    unsupported_content || custom_tool_call
}

fn is_custom_tool(tool: &Value) -> bool {
    tool.as_object()
        .and_then(|tool| tool.get("type"))
        .and_then(Value::as_str)
        == Some("custom")
}

fn is_unsupported_tool_choice(choice: &Value) -> bool {
    matches!(
        choice
            .as_object()
            .and_then(|choice| choice.get("type"))
            .and_then(Value::as_str),
        Some("allowed_tools" | "custom")
    )
}

fn map_json_error(error: BoundedJsonError) -> ParseRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
    }
}

/// OpenAI Chat 请求解析错误，不保留字段名、路径或外部输入。
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
    /// 请求字段的类型、取值或关联关系无效。
    InvalidValue,
    /// 两个不能同时提供的参数发生冲突。
    ConflictingParameters,
    /// 请求使用了本切片尚不能无损处理的协议特性。
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
            Self::ConflictingParameters => formatter.write_str("请求参数相互冲突"),
            Self::UnsupportedFeature => formatter.write_str("请求包含当前不支持的特性"),
        }
    }
}

impl Error for ParseRequestError {}
