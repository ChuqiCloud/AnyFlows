//! OpenAI Responses 协议转换。

use std::{error::Error, fmt};

use af_domain::Protocol;
use bytes::Bytes;
use serde_json::Value;

use crate::{CanonicalRequest, CanonicalRequestEnvelope, bounded_json};

mod build;
mod build_input;
mod build_response;
mod convert;
mod input;
mod parse_response;
mod response_raw;
mod response_wire;
mod streaming;
mod wire;

pub use build::{BuildRequestError, build_request};
pub use build_response::{BuildResponseError, build_response};
pub use parse_response::{ParseResponseError, parse_response};
pub use streaming::{
    DEFAULT_MAX_SSE_EVENT_BYTES, EncodeStreamError, MAX_SSE_EVENT_BYTES,
    OpenAiResponsesStreamDecoder, OpenAiResponsesStreamEncoder, ParseStreamError, SseParseError,
};

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod response_tests;
#[cfg(test)]
mod tests;

/// Responses 请求允许的最大正文大小。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// Responses 请求允许声明的最大输出令牌数。
pub const MAX_OUTPUT_TOKENS: i64 = 1_000_000;

const REQUEST_JSON_LIMITS: bounded_json::JsonLimits = bounded_json::JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: 32 * 1024 * 1024,
    max_key_bytes: 1_024,
};

const UNSUPPORTED_TOP_LEVEL_FIELDS: &[&str] = &[
    "background",
    "max_tool_calls",
    "moderation",
    "prompt",
    "stream_options",
    "text",
    "top_logprobs",
];

const UNSUPPORTED_INPUT_ITEM_TYPES: &[&str] = &[
    "additional_tools",
    "apply_patch_call",
    "apply_patch_call_output",
    "code_interpreter_call",
    "compaction_trigger",
    "computer_call",
    "computer_call_output",
    "custom_tool_call",
    "custom_tool_call_output",
    "file_search_call",
    "image_generation_call",
    "item_reference",
    "local_shell_call",
    "local_shell_call_output",
    "mcp_approval_request",
    "mcp_approval_response",
    "mcp_call",
    "mcp_list_tools",
    "program",
    "program_output",
    "shell_call",
    "shell_call_output",
    "tool_search_call",
    "tool_search_output",
    "web_search_call",
];

/// 解析 OpenAI Responses 完整或流式请求。
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

/// 解析请求并保留只可用于 OpenAI Responses 同协议直通的原始正文。
pub fn parse_request_envelope(body: Bytes) -> Result<CanonicalRequestEnvelope, ParseRequestError> {
    let canonical = parse_request(&body)?;
    Ok(CanonicalRequestEnvelope::from_validated_source(
        canonical,
        Protocol::OpenAiResponses,
        body,
    ))
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(request) = value.as_object() else {
        return false;
    };
    if UNSUPPORTED_TOP_LEVEL_FIELDS
        .iter()
        .any(|key| request.contains_key(*key))
    {
        return true;
    }
    if request
        .get("reasoning")
        .and_then(Value::as_object)
        .is_some_and(|reasoning| reasoning.keys().any(|key| key != "effort"))
    {
        return true;
    }
    if request
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(tool_uses_unsupported_feature))
    {
        return true;
    }

    request
        .get("input")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(input_item_uses_unsupported_feature))
}

fn tool_uses_unsupported_feature(tool: &Value) -> bool {
    let Some(tool) = tool.as_object() else {
        return false;
    };
    match tool.get("type").and_then(Value::as_str) {
        Some("function") => ["allowed_callers", "defer_loading", "output_schema"]
            .iter()
            .any(|key| tool.contains_key(*key)),
        Some(_) => true,
        None => false,
    }
}

fn input_item_uses_unsupported_feature(item: &Value) -> bool {
    let Some(item) = item.as_object() else {
        return false;
    };
    let kind = item.get("type").and_then(Value::as_str);
    if kind.is_some_and(|kind| UNSUPPORTED_INPUT_ITEM_TYPES.contains(&kind)) {
        return true;
    }

    match kind {
        None | Some("message") => {
            if ["id", "phase", "status"]
                .iter()
                .any(|key| item.contains_key(*key))
            {
                return true;
            }
            item.get("content")
                .and_then(Value::as_array)
                .is_some_and(|parts| parts.iter().any(content_uses_unsupported_feature))
        }
        Some("function_call") => ["caller", "id", "namespace", "status"]
            .iter()
            .any(|key| item.contains_key(*key)),
        Some("function_call_output") => {
            ["caller", "id"].iter().any(|key| item.contains_key(*key))
                || item
                    .get("output")
                    .and_then(Value::as_array)
                    .is_some_and(|parts| parts.iter().any(content_uses_unsupported_feature))
        }
        Some(_) => false,
    }
}

fn content_uses_unsupported_feature(content: &Value) -> bool {
    let Some(content) = content.as_object() else {
        return false;
    };
    if content.contains_key("prompt_cache_breakpoint") {
        return true;
    }
    match content.get("type").and_then(Value::as_str) {
        Some("input_file") => true,
        Some("input_image") => {
            content.contains_key("file_id")
                || matches!(
                    content.get("detail").and_then(Value::as_str),
                    Some("low" | "high" | "original")
                )
        }
        _ => false,
    }
}

fn map_json_error(error: bounded_json::BoundedJsonError) -> ParseRequestError {
    match error {
        bounded_json::BoundedJsonError::InvalidJson => ParseRequestError::InvalidJson,
        bounded_json::BoundedJsonError::DuplicateKey => ParseRequestError::DuplicateKey,
        bounded_json::BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
    }
}

/// OpenAI Responses 请求解析错误，不保留字段名、路径或外部输入。
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
