//! Anthropic Messages 协议转换。

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
pub use convert::{MAX_STOP_BYTES, MAX_STOP_SEQUENCES};
pub use error::encode_error;
pub use parse_response::{ParseResponseError, parse_response};
pub use streaming::{
    AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder, DEFAULT_MAX_SSE_EVENT_BYTES,
    EncodeStreamError, MAX_SSE_EVENT_BYTES, ParseStreamError, SseParseError,
};
pub use upstream_error::{AnthropicUpstreamErrorKind, classify_error_response};

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod response_tests;
#[cfg(test)]
mod tests;

/// Anthropic Messages 非流式请求的最大正文大小。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// Anthropic Messages 接受的最大输出令牌数防御上限。
pub const MAX_OUTPUT_TOKENS: i64 = 1_000_000;
const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: 32 * 1024 * 1024,
    max_key_bytes: 1_024,
};

/// 解析 Anthropic Messages 非流式入站请求。
///
/// 该函数是协议输入的信任边界：重复键、超预算结构、未知字段和无法无损归一的
/// Anthropic 能力都会在进入 Canonical 前失败关闭。
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

/// 解析请求并保留只可用于 Anthropic 同协议直通的原始正文。
pub fn parse_request_envelope(body: Bytes) -> Result<CanonicalRequestEnvelope, ParseRequestError> {
    let canonical = parse_request(&body)?;
    Ok(CanonicalRequestEnvelope::from_validated_source(
        canonical,
        Protocol::Anthropic,
        body,
    ))
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(request) = value.as_object() else {
        return false;
    };
    if [
        "cache_control",
        "container",
        "context_management",
        "inference_geo",
        "max_tokens_to_sample",
        "mcp_servers",
        "output_format",
        "prompt",
        "service_tier",
        "speed",
        "top_k",
    ]
    .iter()
    .any(|key| request.contains_key(*key))
    {
        return true;
    }

    request
        .get("system")
        .and_then(Value::as_array)
        .is_some_and(|blocks| blocks.iter().any(system_block_is_unsupported))
        || request
            .get("messages")
            .and_then(Value::as_array)
            .is_some_and(|messages| messages.iter().any(message_is_unsupported))
        || request
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| tools.iter().any(tool_is_unsupported))
}

fn system_block_is_unsupported(block: &Value) -> bool {
    block.as_object().is_some_and(|block| {
        block.get("type").and_then(Value::as_str) != Some("text") || block.contains_key("citations")
    })
}

fn message_is_unsupported(message: &Value) -> bool {
    let Some(message) = message.as_object() else {
        return false;
    };
    if message.get("role").and_then(Value::as_str) == Some("system") {
        return true;
    }
    message
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|blocks| blocks.iter().any(content_block_is_unsupported))
}

fn content_block_is_unsupported(block: &Value) -> bool {
    let Some(block) = block.as_object() else {
        return false;
    };
    if block.contains_key("citations") || block.contains_key("caller") {
        return true;
    }
    let kind = block.get("type").and_then(Value::as_str);
    if matches!(
        kind,
        Some(
            "bash_code_execution_tool_result"
                | "code_execution_tool_result"
                | "container_upload"
                | "document"
                | "mid_conv_system"
                | "redacted_thinking"
                | "search_result"
                | "server_tool_use"
                | "text_editor_code_execution_tool_result"
                | "thinking"
                | "tool_reference"
                | "tool_search_tool_result"
                | "web_fetch_tool_result"
                | "web_search_tool_result"
        )
    ) {
        return true;
    }
    kind == Some("tool_result")
        && block
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|parts| parts.iter().any(content_block_is_unsupported))
}

fn tool_is_unsupported(tool: &Value) -> bool {
    tool.as_object().is_some_and(|tool| {
        [
            "allowed_callers",
            "cache_control",
            "defer_loading",
            "input_examples",
            "strict",
            "type",
        ]
        .iter()
        .any(|key| tool.contains_key(*key))
    })
}

fn map_json_error(error: BoundedJsonError) -> ParseRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
    }
}

/// Anthropic Messages 请求解析错误，不保留字段名、路径或外部输入。
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
