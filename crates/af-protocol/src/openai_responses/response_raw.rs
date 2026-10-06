use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    convert::{MAX_TOOL_DESCRIPTION_BYTES, MAX_TOOLS, validate_schema, validate_tool_name},
    response_wire::ResponseStatusWire,
};
use crate::bounded_json::{JsonLimits, validate_object};

const MAX_RAW_FIELDS: usize = 32;
const MAX_RAW_BYTES: usize = 8 * 1024 * 1024;
const MAX_METADATA_FIELDS: usize = 16;
const MAX_METADATA_KEY_BYTES: usize = 64;
const MAX_METADATA_VALUE_BYTES: usize = 512;
const MAX_CONTINUATION_ID_BYTES: usize = 512;
const MAX_PROMPT_CACHE_KEY_BYTES: usize = 256;
const MAX_USER_ID_BYTES: usize = 512;

const RESPONSE_RAW_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 50_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_RAW_BYTES,
    max_key_bytes: 1_024,
};

const RAW_FIELD_ALLOWLIST: &[&str] = &[
    "background",
    "completed_at",
    "conversation",
    "frequency_penalty",
    "instructions",
    "max_output_tokens",
    "max_tool_calls",
    "metadata",
    "parallel_tool_calls",
    "previous_response_id",
    "prompt",
    "prompt_cache_key",
    "prompt_cache_options",
    "prompt_cache_retention",
    "presence_penalty",
    "reasoning",
    "safety_identifier",
    "service_tier",
    "_sub2api_display_scaled",
    "store",
    "temperature",
    "text",
    "tool_choice",
    "tool_usage",
    "tools",
    "top_logprobs",
    "top_p",
    "truncation",
    "user",
];

const FORBIDDEN_METADATA_KEYS: &[&str] = &[
    "__proto__",
    "api_key",
    "authorization",
    "constructor",
    "credential",
    "credentials",
    "headers",
    "prototype",
    "token",
];

/// 校验 Responses 响应中只能同源回显的顶层字段。
pub(super) fn validate_raw_fields(
    fields: &Map<String, Value>,
    status: ResponseStatusWire,
    created_at: i64,
) -> Result<(), ResponseRawError> {
    if fields.len() > MAX_RAW_FIELDS {
        return Err(ResponseRawError::StructureLimitExceeded);
    }
    for key in fields.keys() {
        if !RAW_FIELD_ALLOWLIST.contains(&key.as_str()) {
            return Err(ResponseRawError::InvalidValue);
        }
    }

    let tool_names = validate_response_tools(fields.get("tools"))?;
    validate_tool_choice(fields.get("tool_choice"), &tool_names)?;
    for (key, value) in fields {
        if value.is_null() || matches!(key.as_str(), "tools" | "tool_choice") {
            continue;
        }
        match key.as_str() {
            "background" => match value.as_bool() {
                Some(false) => {}
                Some(true) => return Err(ResponseRawError::UnsupportedFeature),
                None => return Err(ResponseRawError::InvalidValue),
            },
            "completed_at" => validate_completed_at(value, status, created_at)?,
            "conversation" => validate_conversation(value)?,
            "frequency_penalty" | "presence_penalty" => validate_number_range(value, -2.0, 2.0)?,
            "instructions" => validate_instructions(value)?,
            "max_output_tokens" => validate_max_output_tokens(value)?,
            "max_tool_calls" | "prompt" => {
                return Err(ResponseRawError::UnsupportedFeature);
            }
            "metadata" => validate_metadata(value)?,
            "parallel_tool_calls" | "store" => validate_boolean(value)?,
            "previous_response_id" => {
                validate_opaque_string(value, MAX_CONTINUATION_ID_BYTES)?;
            }
            "prompt_cache_key" => {
                validate_opaque_string(value, MAX_PROMPT_CACHE_KEY_BYTES)?;
            }
            "prompt_cache_options" => validate_prompt_cache_options(value)?,
            "prompt_cache_retention" => validate_enum(value, &["in_memory", "24h"])?,
            "reasoning" => validate_reasoning(value)?,
            "safety_identifier" => validate_opaque_string(value, 64)?,
            "service_tier" => {
                validate_enum(value, &["auto", "default", "flex", "scale", "priority"])?;
            }
            "_sub2api_display_scaled" => {
                if !value.is_boolean() {
                    return Err(ResponseRawError::InvalidValue);
                }
            }
            "temperature" => validate_number_range(value, 0.0, 2.0)?,
            "text" => validate_text_config(value)?,
            "tool_usage" => validate_tool_usage(value)?,
            "top_logprobs" => validate_top_logprobs(value)?,
            "top_p" => validate_number_range(value, 0.0, 1.0)?,
            "truncation" => validate_enum(value, &["auto", "disabled"])?,
            "user" => validate_user(value)?,
            _ => return Err(ResponseRawError::InvalidValue),
        }
    }

    validate_continuation_conflict(fields)?;
    validate_object(fields, RESPONSE_RAW_LIMITS, MAX_RAW_BYTES)
        .map_err(|_| ResponseRawError::StructureLimitExceeded)
}

fn validate_completed_at(
    value: &Value,
    status: ResponseStatusWire,
    created_at: i64,
) -> Result<(), ResponseRawError> {
    let completed_at = value.as_i64().ok_or(ResponseRawError::InvalidValue)?;
    if status != ResponseStatusWire::Completed || completed_at < created_at {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn validate_conversation(value: &Value) -> Result<(), ResponseRawError> {
    let object = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    if object.len() != 1 {
        return Err(ResponseRawError::InvalidValue);
    }
    let id = object.get("id").ok_or(ResponseRawError::InvalidValue)?;
    validate_opaque_string(id, MAX_CONTINUATION_ID_BYTES)
}

fn validate_instructions(value: &Value) -> Result<(), ResponseRawError> {
    let text = value.as_str().ok_or(ResponseRawError::UnsupportedFeature)?;
    if text.len() > super::convert::MAX_TOTAL_TEXT_BYTES {
        return Err(ResponseRawError::StructureLimitExceeded);
    }
    Ok(())
}

fn validate_max_output_tokens(value: &Value) -> Result<(), ResponseRawError> {
    match value.as_i64() {
        Some(value) if (1..=MAX_OUTPUT_TOKENS).contains(&value) => Ok(()),
        _ => Err(ResponseRawError::InvalidValue),
    }
}

fn validate_metadata(value: &Value) -> Result<(), ResponseRawError> {
    let metadata = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    if metadata.len() > MAX_METADATA_FIELDS {
        return Err(ResponseRawError::StructureLimitExceeded);
    }
    for (key, value) in metadata {
        let value = value.as_str().ok_or(ResponseRawError::InvalidValue)?;
        if key.is_empty()
            || key.len() > MAX_METADATA_KEY_BYTES
            || key.chars().any(char::is_control)
            || FORBIDDEN_METADATA_KEYS.contains(&key.as_str())
            || value.len() > MAX_METADATA_VALUE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ResponseRawError::InvalidValue);
        }
    }
    Ok(())
}

fn validate_boolean(value: &Value) -> Result<(), ResponseRawError> {
    if value.is_boolean() {
        Ok(())
    } else {
        Err(ResponseRawError::InvalidValue)
    }
}

fn validate_opaque_string(value: &Value, max_bytes: usize) -> Result<(), ResponseRawError> {
    let value = value.as_str().ok_or(ResponseRawError::InvalidValue)?;
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn validate_prompt_cache_options(value: &Value) -> Result<(), ResponseRawError> {
    let options = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    if options
        .keys()
        .any(|key| !matches!(key.as_str(), "mode" | "ttl"))
    {
        return Err(ResponseRawError::InvalidValue);
    }
    if options.get("mode").is_some_and(|value| {
        !value.is_null() && !matches!(value.as_str(), Some("implicit" | "explicit"))
    }) || options
        .get("ttl")
        .is_some_and(|value| !value.is_null() && value.as_str() != Some("30m"))
    {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn validate_reasoning(value: &Value) -> Result<(), ResponseRawError> {
    let reasoning = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    for (key, value) in reasoning {
        if value.is_null() {
            continue;
        }
        match key.as_str() {
            "effort" => validate_enum(
                value,
                &["none", "minimal", "low", "medium", "high", "xhigh", "max"],
            )?,
            "context" => validate_enum(value, &["auto", "current_turn", "all_turns"])?,
            "generate_summary" | "summary" => {
                validate_enum(value, &["auto", "concise", "detailed"])?;
            }
            "mode" => validate_enum(value, &["standard", "pro"])?,
            _ => return Err(ResponseRawError::InvalidValue),
        }
    }
    Ok(())
}

fn validate_text_config(value: &Value) -> Result<(), ResponseRawError> {
    let text = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    if text
        .keys()
        .any(|key| !matches!(key.as_str(), "format" | "verbosity"))
    {
        return Err(ResponseRawError::InvalidValue);
    }
    if let Some(verbosity) = text.get("verbosity")
        && !verbosity.is_null()
    {
        validate_enum(verbosity, &["low", "medium", "high"])?;
    }
    let Some(format) = text.get("format") else {
        return Ok(());
    };
    if format.is_null() {
        return Ok(());
    }
    validate_text_format(format)
}

fn validate_text_format(value: &Value) -> Result<(), ResponseRawError> {
    let format = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    let kind = format
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ResponseRawError::InvalidValue)?;
    match kind {
        "text" | "json_object" if format.len() == 1 => Ok(()),
        "json_schema" => validate_json_schema_format(format),
        _ => Err(ResponseRawError::InvalidValue),
    }
}

fn validate_json_schema_format(format: &Map<String, Value>) -> Result<(), ResponseRawError> {
    if format.keys().any(|key| {
        !matches!(
            key.as_str(),
            "type" | "name" | "schema" | "description" | "strict"
        )
    }) {
        return Err(ResponseRawError::InvalidValue);
    }
    let name = format
        .get("name")
        .and_then(Value::as_str)
        .ok_or(ResponseRawError::InvalidValue)?;
    validate_tool_name(name).map_err(map_request_error)?;
    let schema = format.get("schema").ok_or(ResponseRawError::InvalidValue)?;
    let mut total_schema_bytes = 0;
    validate_schema(schema, &mut total_schema_bytes).map_err(map_request_error)?;
    if format.get("description").is_some_and(|value| {
        !value.is_null()
            && value
                .as_str()
                .is_none_or(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
    }) || format
        .get("strict")
        .is_some_and(|value| !value.is_null() && !value.is_boolean())
    {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn validate_response_tools(value: Option<&Value>) -> Result<HashSet<String>, ResponseRawError> {
    let Some(value) = value else {
        return Ok(HashSet::new());
    };
    if value.is_null() {
        return Ok(HashSet::new());
    }
    let tools = value.as_array().ok_or(ResponseRawError::InvalidValue)?;
    if tools.len() > MAX_TOOLS {
        return Err(ResponseRawError::StructureLimitExceeded);
    }
    let mut names = HashSet::with_capacity(tools.len());
    let mut total_schema_bytes = 0;
    for tool in tools {
        let tool = tool.as_object().ok_or(ResponseRawError::InvalidValue)?;
        match tool.get("type").and_then(Value::as_str) {
            Some("function") => {
                if tool.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "type"
                            | "name"
                            | "parameters"
                            | "strict"
                            | "description"
                            | "allowed_callers"
                            | "defer_loading"
                            | "output_schema"
                    )
                }) {
                    return Err(ResponseRawError::InvalidValue);
                }
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or(ResponseRawError::InvalidValue)?;
                validate_tool_name(name).map_err(map_request_error)?;
                if !names.insert(name.to_owned()) {
                    return Err(ResponseRawError::InvalidValue);
                }
                let parameters = tool
                    .get("parameters")
                    .ok_or(ResponseRawError::InvalidValue)?;
                validate_schema(parameters, &mut total_schema_bytes).map_err(map_request_error)?;
                // `strict` 在 Responses 函数工具中是可选字段；供应商回显省略值时仍属于合法协议。
                if tool
                    .get("strict")
                    .is_some_and(|value| !value.is_null() && !value.is_boolean())
                {
                    return Err(ResponseRawError::InvalidValue);
                }
                if tool.get("description").is_some_and(|value| {
                    !value.is_null()
                        && value
                            .as_str()
                            .is_none_or(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
                }) {
                    return Err(ResponseRawError::InvalidValue);
                }
                if ["allowed_callers", "defer_loading", "output_schema"]
                    .iter()
                    .any(|key| tool.get(*key).is_some_and(|value| !value.is_null()))
                {
                    return Err(ResponseRawError::UnsupportedFeature);
                }
            }
            Some(kind) if is_builtin_response_tool_kind(kind) => {
                // Responses 上游会回显内置工具；这些工具不进入当前 Canonical 工具调用模型，
                // 但必须保留其有界 JSON 形状，才能透传同源字段而不误报协议错误。
                validate_builtin_response_tool(tool)?;
                validate_object(tool, RESPONSE_RAW_LIMITS, MAX_RAW_BYTES)
                    .map_err(|_| ResponseRawError::StructureLimitExceeded)?;
            }
            Some(_) | None => return Err(ResponseRawError::UnsupportedFeature),
        }
    }
    Ok(names)
}

fn validate_builtin_response_tool(tool: &Map<String, Value>) -> Result<(), ResponseRawError> {
    if tool.keys().any(|key| {
        key.is_empty()
            || key.len() > 128
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    }) {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn is_builtin_response_tool_kind(kind: &str) -> bool {
    matches!(
        kind,
        "code_interpreter"
            | "computer_use_preview"
            | "computer_use"
            | "file_search"
            | "image_generation"
            | "mcp"
            | "web_search"
            | "web_search_preview"
    ) || kind.ends_with("_generation")
        || kind.starts_with("web_search")
}

fn validate_tool_choice(
    value: Option<&Value>,
    tool_names: &HashSet<String>,
) -> Result<(), ResponseRawError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_null() {
        return Ok(());
    }
    if let Some(mode) = value.as_str() {
        return match mode {
            "none" | "auto" => Ok(()),
            "required" if !tool_names.is_empty() => Ok(()),
            "required" => Err(ResponseRawError::InvalidValue),
            _ => Err(ResponseRawError::InvalidValue),
        };
    }
    let choice = value.as_object().ok_or(ResponseRawError::InvalidValue)?;
    if choice.len() != 2 || choice.get("type").and_then(Value::as_str) != Some("function") {
        return Err(ResponseRawError::UnsupportedFeature);
    }
    let name = choice
        .get("name")
        .and_then(Value::as_str)
        .ok_or(ResponseRawError::InvalidValue)?;
    validate_tool_name(name).map_err(map_request_error)?;
    if !tool_names.contains(name) {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn validate_top_logprobs(value: &Value) -> Result<(), ResponseRawError> {
    match value.as_i64() {
        Some(0) => Ok(()),
        Some(value) if (0..=20).contains(&value) => Err(ResponseRawError::UnsupportedFeature),
        _ => Err(ResponseRawError::InvalidValue),
    }
}

fn validate_number_range(value: &Value, min: f64, max: f64) -> Result<(), ResponseRawError> {
    match value.as_f64() {
        Some(value) if value.is_finite() && (min..=max).contains(&value) => Ok(()),
        _ => Err(ResponseRawError::InvalidValue),
    }
}

fn validate_tool_usage(value: &Value) -> Result<(), ResponseRawError> {
    let mut visited = 0_usize;
    validate_tool_usage_value(value, 0, &mut visited)
}

fn validate_tool_usage_value(
    value: &Value,
    depth: usize,
    visited: &mut usize,
) -> Result<(), ResponseRawError> {
    *visited = visited
        .checked_add(1)
        .ok_or(ResponseRawError::StructureLimitExceeded)?;
    if depth > 8 || *visited > 256 {
        return Err(ResponseRawError::StructureLimitExceeded);
    }
    match value {
        Value::Object(object) => {
            if object.len() > 64 {
                return Err(ResponseRawError::StructureLimitExceeded);
            }
            for (key, child) in object {
                if key.is_empty()
                    || key.len() > 64
                    || !key
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    return Err(ResponseRawError::InvalidValue);
                }
                validate_tool_usage_value(child, depth + 1, visited)?;
            }
            Ok(())
        }
        Value::Number(number) if number.as_u64().is_some() => Ok(()),
        Value::Number(_) => Err(ResponseRawError::InvalidValue),
        _ => Err(ResponseRawError::InvalidValue),
    }
}

fn validate_enum(value: &Value, allowed: &[&str]) -> Result<(), ResponseRawError> {
    match value.as_str() {
        Some(value) if allowed.contains(&value) => Ok(()),
        _ => Err(ResponseRawError::InvalidValue),
    }
}

fn validate_user(value: &Value) -> Result<(), ResponseRawError> {
    let value = value.as_str().ok_or(ResponseRawError::InvalidValue)?;
    if value.is_empty() || value.len() > MAX_USER_ID_BYTES || value.chars().any(char::is_control) {
        return Err(ResponseRawError::InvalidValue);
    }
    Ok(())
}

fn validate_continuation_conflict(fields: &Map<String, Value>) -> Result<(), ResponseRawError> {
    let has_conversation = fields
        .get("conversation")
        .is_some_and(|value| !value.is_null());
    let has_previous = fields
        .get("previous_response_id")
        .is_some_and(|value| !value.is_null());
    if has_conversation && has_previous {
        Err(ResponseRawError::InvalidValue)
    } else {
        Ok(())
    }
}

fn map_request_error(error: ParseRequestError) -> ResponseRawError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            ResponseRawError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => ResponseRawError::UnsupportedFeature,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::ConflictingParameters
        | ParseRequestError::InvalidValue => ResponseRawError::InvalidValue,
    }
}

/// Responses 响应 raw 校验错误，不携带字段名或外部值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResponseRawError {
    /// raw 字段的类型、取值或关联关系无效。
    InvalidValue,
    /// raw 字段使用了当前尚未建模的 Responses 能力。
    UnsupportedFeature,
    /// raw 字段超过结构或序列化预算。
    StructureLimitExceeded,
}
