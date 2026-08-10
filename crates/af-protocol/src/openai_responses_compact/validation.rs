use std::collections::HashSet;

use af_domain::MAX_MODEL_NAME_BYTES;
use serde_json::{Map, Value};

use super::error::{
    ParseResponsesCompactionRequestError, ParseResponsesCompactionResponseError,
    ResponsesCompactionUsageError,
};
use super::model::{ResponsesCompactionInput, ResponsesCompactionItem, ResponsesCompactionUsage};
use super::{MAX_BODY_BYTES, MAX_CONTEXT_ITEMS, MAX_ENCRYPTED_CONTENT_BYTES};
use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{TokenCount, UsageError};

const MAX_CONTEXT_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONTEXT_ITEM_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_CONTEXT_ITEM_BYTES: usize = 24 * 1024 * 1024;
const MAX_CONTEXT_ID_BYTES: usize = 512;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_CONTENT_PARTS: usize = 128;
const MAX_REASONING_SUMMARIES: usize = 16;

pub(super) const JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_BODY_BYTES,
    max_key_bytes: 1_024,
};

pub(super) fn parse_input(value: &Value) -> Result<ResponsesCompactionInput, ItemValidationError> {
    match value {
        Value::String(text) => {
            validate_text(text)?;
            Ok(ResponsesCompactionInput::Text(text.clone()))
        }
        Value::Array(items) => Ok(ResponsesCompactionInput::Items(parse_items(items)?)),
        _ => Err(ItemValidationError::InvalidValue),
    }
}

/// 校验程序化构造的输入窗口；Item 本身只能由本模块的解析边界创建。
pub(super) fn validate_input_model(
    input: &ResponsesCompactionInput,
) -> Result<(), ItemValidationError> {
    match input {
        ResponsesCompactionInput::Text(text) => validate_text(text),
        ResponsesCompactionInput::Items(items) => {
            if items.len() > MAX_CONTEXT_ITEMS {
                return Err(ItemValidationError::StructureLimitExceeded);
            }
            let mut total_bytes = 0_usize;
            for item in items {
                let bytes = serde_json::to_vec(item.as_value())
                    .map_err(|_| ItemValidationError::InvalidValue)?;
                total_bytes = total_bytes
                    .checked_add(bytes.len())
                    .ok_or(ItemValidationError::StructureLimitExceeded)?;
                if total_bytes > MAX_TOTAL_CONTEXT_ITEM_BYTES {
                    return Err(ItemValidationError::StructureLimitExceeded);
                }
            }
            Ok(())
        }
    }
}

pub(super) fn parse_items(
    items: &[Value],
) -> Result<Vec<ResponsesCompactionItem>, ItemValidationError> {
    if items.len() > MAX_CONTEXT_ITEMS {
        return Err(ItemValidationError::StructureLimitExceeded);
    }
    let mut total_bytes = 0_usize;
    let mut seen_ids = HashSet::new();
    let mut converted = Vec::with_capacity(items.len());
    for item in items {
        validate_context_item(item)?;
        let bytes = serde_json::to_vec(item).map_err(|_| ItemValidationError::InvalidValue)?;
        if bytes.len() > MAX_CONTEXT_ITEM_BYTES {
            return Err(ItemValidationError::StructureLimitExceeded);
        }
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or(ItemValidationError::StructureLimitExceeded)?;
        if total_bytes > MAX_TOTAL_CONTEXT_ITEM_BYTES {
            return Err(ItemValidationError::StructureLimitExceeded);
        }
        if let Some(id) = item.get("id").and_then(Value::as_str)
            && !seen_ids.insert(id.to_owned())
        {
            return Err(ItemValidationError::InvalidValue);
        }
        converted.push(ResponsesCompactionItem(item.clone()));
    }
    Ok(converted)
}

fn validate_context_item(value: &Value) -> Result<(), ItemValidationError> {
    let object = value.as_object().ok_or(ItemValidationError::InvalidValue)?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ItemValidationError::InvalidValue)?;
    match kind {
        "compaction" => validate_compaction_item(object),
        "message" => validate_message_item(object),
        "reasoning" => validate_reasoning_item(object),
        "function_call" => validate_function_call_item(object),
        "function_call_output" => validate_function_output_item(object),
        _ => Err(ItemValidationError::UnsupportedFeature),
    }
}

fn validate_compaction_item(object: &Map<String, Value>) -> Result<(), ItemValidationError> {
    ensure_keys(object, &["type", "id", "encrypted_content"])?;
    validate_optional_id(object)?;
    let encrypted = required_string(object, "encrypted_content")?;
    if encrypted.is_empty()
        || encrypted.len() > MAX_ENCRYPTED_CONTENT_BYTES
        || encrypted.chars().any(char::is_control)
    {
        return Err(ItemValidationError::InvalidValue);
    }
    Ok(())
}

fn validate_message_item(object: &Map<String, Value>) -> Result<(), ItemValidationError> {
    ensure_keys(
        object,
        &["type", "id", "role", "content", "status", "phase"],
    )?;
    validate_optional_id(object)?;
    validate_enum_field(
        object,
        "role",
        &["system", "developer", "user", "assistant"],
    )?;
    validate_optional_enum(
        object,
        "status",
        &["in_progress", "completed", "incomplete"],
    )?;
    validate_optional_enum(object, "phase", &["commentary", "final_answer"])?;
    validate_content(
        object
            .get("content")
            .ok_or(ItemValidationError::InvalidValue)?,
    )
}

fn validate_reasoning_item(object: &Map<String, Value>) -> Result<(), ItemValidationError> {
    ensure_keys(
        object,
        &[
            "type",
            "id",
            "encrypted_content",
            "summary",
            "content",
            "status",
        ],
    )?;
    validate_optional_id(object)?;
    validate_optional_enum(
        object,
        "status",
        &["in_progress", "completed", "incomplete"],
    )?;
    if let Some(value) = object.get("encrypted_content") {
        let value = value.as_str().ok_or(ItemValidationError::InvalidValue)?;
        if value.is_empty()
            || value.len() > MAX_ENCRYPTED_CONTENT_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ItemValidationError::InvalidValue);
        }
    }
    if let Some(summary) = object.get("summary") {
        let summary = summary
            .as_array()
            .ok_or(ItemValidationError::InvalidValue)?;
        if summary.len() > MAX_REASONING_SUMMARIES {
            return Err(ItemValidationError::StructureLimitExceeded);
        }
        for part in summary {
            let part = part.as_object().ok_or(ItemValidationError::InvalidValue)?;
            ensure_keys(part, &["type", "text"])?;
            if part.get("type").and_then(Value::as_str) != Some("summary_text") {
                return Err(ItemValidationError::UnsupportedFeature);
            }
            validate_text(required_string(part, "text")?)?;
        }
    }
    if let Some(content) = object.get("content") {
        let content = content
            .as_array()
            .ok_or(ItemValidationError::InvalidValue)?;
        if !content.is_empty() {
            return Err(ItemValidationError::UnsupportedFeature);
        }
    }
    Ok(())
}

fn validate_function_call_item(object: &Map<String, Value>) -> Result<(), ItemValidationError> {
    ensure_keys(
        object,
        &["type", "id", "call_id", "name", "arguments", "status"],
    )?;
    validate_optional_id(object)?;
    validate_optional_enum(
        object,
        "status",
        &["in_progress", "completed", "incomplete"],
    )?;
    validate_opaque_string(object, "call_id", MAX_CONTEXT_ID_BYTES)?;
    validate_opaque_string(object, "name", 64)?;
    let arguments = required_string(object, "arguments")?;
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(ItemValidationError::StructureLimitExceeded);
    }
    let parsed = bounded_json::parse_value(
        arguments.as_bytes(),
        JsonLimits {
            max_depth: 16,
            max_nodes: 4_096,
            max_object_entries: 1_024,
            max_array_items: 4_096,
            max_string_bytes: MAX_ARGUMENT_BYTES,
            max_key_bytes: 1_024,
        },
    )
    .map_err(map_bounded_item_error)?;
    if !parsed.is_object() {
        return Err(ItemValidationError::UnsupportedFeature);
    }
    Ok(())
}

fn validate_function_output_item(object: &Map<String, Value>) -> Result<(), ItemValidationError> {
    ensure_keys(object, &["type", "id", "call_id", "output", "status"])?;
    validate_optional_id(object)?;
    validate_optional_enum(
        object,
        "status",
        &["in_progress", "completed", "incomplete"],
    )?;
    validate_opaque_string(object, "call_id", MAX_CONTEXT_ID_BYTES)?;
    let output = object
        .get("output")
        .ok_or(ItemValidationError::InvalidValue)?;
    match output {
        Value::String(text) => validate_text(text),
        Value::Array(_) => validate_content(output),
        _ => Err(ItemValidationError::InvalidValue),
    }
}

fn validate_content(value: &Value) -> Result<(), ItemValidationError> {
    match value {
        Value::String(text) => validate_text(text),
        Value::Array(parts) => {
            if parts.len() > MAX_CONTENT_PARTS {
                return Err(ItemValidationError::StructureLimitExceeded);
            }
            for part in parts {
                validate_content_part(part)?;
            }
            Ok(())
        }
        _ => Err(ItemValidationError::InvalidValue),
    }
}

fn validate_content_part(value: &Value) -> Result<(), ItemValidationError> {
    let part = value.as_object().ok_or(ItemValidationError::InvalidValue)?;
    let kind = part
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ItemValidationError::InvalidValue)?;
    match kind {
        "input_text" | "output_text" | "summary_text" => {
            ensure_keys(part, &["type", "text", "annotations", "logprobs"])?;
            validate_text(required_string(part, "text")?)?;
            for key in ["annotations", "logprobs"] {
                if let Some(value) = part.get(key)
                    && !value.as_array().is_some_and(Vec::is_empty)
                {
                    return Err(ItemValidationError::UnsupportedFeature);
                }
            }
            Ok(())
        }
        "refusal" => {
            ensure_keys(part, &["type", "refusal"])?;
            validate_text(required_string(part, "refusal")?)
        }
        "input_image" | "input_file" => validate_media_part(part),
        _ => Err(ItemValidationError::UnsupportedFeature),
    }
}

fn validate_media_part(part: &Map<String, Value>) -> Result<(), ItemValidationError> {
    ensure_keys(
        part,
        &[
            "type",
            "detail",
            "file_id",
            "image_url",
            "file_data",
            "file_url",
            "filename",
        ],
    )?;
    let has_source = ["file_id", "image_url", "file_data", "file_url"]
        .iter()
        .any(|key| part.contains_key(*key));
    if !has_source {
        return Err(ItemValidationError::InvalidValue);
    }
    for key in [
        "detail",
        "file_id",
        "image_url",
        "file_data",
        "file_url",
        "filename",
    ] {
        if let Some(value) = part.get(key) {
            let value = value.as_str().ok_or(ItemValidationError::InvalidValue)?;
            if value.len() > MAX_CONTEXT_TEXT_BYTES || value.chars().any(char::is_control) {
                return Err(ItemValidationError::InvalidValue);
            }
        }
    }
    Ok(())
}

pub(super) fn ensure_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), ItemValidationError> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ItemValidationError::InvalidValue);
    }
    Ok(())
}

pub(super) fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, ItemValidationError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(ItemValidationError::InvalidValue)
}

pub(super) fn optional_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, ItemValidationError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(ItemValidationError::InvalidValue),
    }
}

fn validate_optional_id(object: &Map<String, Value>) -> Result<(), ItemValidationError> {
    if let Some(value) = object.get("id") {
        validate_string(value, MAX_CONTEXT_ID_BYTES, true)?;
    }
    Ok(())
}

fn validate_opaque_string(
    object: &Map<String, Value>,
    key: &str,
    max_bytes: usize,
) -> Result<(), ItemValidationError> {
    validate_string_value(required_string(object, key)?, max_bytes, true)
}

fn validate_enum_field(
    object: &Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<(), ItemValidationError> {
    if !allowed.contains(&required_string(object, key)?) {
        return Err(ItemValidationError::InvalidValue);
    }
    Ok(())
}

fn validate_optional_enum(
    object: &Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<(), ItemValidationError> {
    if object.contains_key(key) {
        validate_enum_field(object, key, allowed)?;
    }
    Ok(())
}

fn validate_string(
    value: &Value,
    max_bytes: usize,
    require_non_empty: bool,
) -> Result<(), ItemValidationError> {
    validate_string_value(
        value.as_str().ok_or(ItemValidationError::InvalidValue)?,
        max_bytes,
        require_non_empty,
    )
}

fn validate_string_value(
    value: &str,
    max_bytes: usize,
    require_non_empty: bool,
) -> Result<(), ItemValidationError> {
    if value.len() > max_bytes
        || (require_non_empty && value.is_empty())
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ItemValidationError::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_text(value: &str) -> Result<(), ItemValidationError> {
    if value.len() > MAX_CONTEXT_TEXT_BYTES {
        return Err(ItemValidationError::StructureLimitExceeded);
    }
    Ok(())
}

pub(super) fn validate_model(model: &str) -> Result<(), ItemValidationError> {
    validate_string_value(model, MAX_MODEL_NAME_BYTES, true)
}

pub(super) fn validate_opaque_id(value: &str) -> Result<(), ItemValidationError> {
    validate_string_value(value, MAX_CONTEXT_ID_BYTES, true)
}

pub(super) fn item_kind(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str)
}

pub(super) fn parse_usage(
    value: Option<&Value>,
) -> Result<ResponsesCompactionUsage, UsageValidationError> {
    let object = value
        .and_then(Value::as_object)
        .ok_or(UsageValidationError::InvalidValue)?;
    ensure_keys_usage(
        object,
        &[
            "input_tokens",
            "input_tokens_details",
            "output_tokens",
            "output_tokens_details",
            "total_tokens",
        ],
    )?;
    let input_details = object
        .get("input_tokens_details")
        .and_then(Value::as_object)
        .ok_or(UsageValidationError::InvalidValue)?;
    ensure_keys_usage(input_details, &["cached_tokens", "cache_write_tokens"])?;
    let output_details = object
        .get("output_tokens_details")
        .and_then(Value::as_object)
        .ok_or(UsageValidationError::InvalidValue)?;
    ensure_keys_usage(output_details, &["reasoning_tokens"])?;
    ResponsesCompactionUsage::new(
        integer(object, "input_tokens")?,
        integer(input_details, "cached_tokens")?,
        integer(input_details, "cache_write_tokens")?,
        integer(object, "output_tokens")?,
        integer(output_details, "reasoning_tokens")?,
        integer(object, "total_tokens")?,
    )
    .map_err(UsageValidationError::from)
}

fn ensure_keys_usage(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), UsageValidationError> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(UsageValidationError::InvalidValue);
    }
    Ok(())
}

fn integer(object: &Map<String, Value>, key: &str) -> Result<i64, UsageValidationError> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .ok_or(UsageValidationError::InvalidValue)
}

pub(super) fn token(value: i64) -> Result<TokenCount, ResponsesCompactionUsageError> {
    TokenCount::new(value).map_err(|error| match error {
        UsageError::NegativeTokenCount => ResponsesCompactionUsageError::NegativeTokenCount,
        UsageError::Overflow
        | UsageError::InputDetailsExceedTotal
        | UsageError::OutputDetailsExceedTotal => ResponsesCompactionUsageError::Overflow,
    })
}

fn map_bounded_item_error(error: BoundedJsonError) -> ItemValidationError {
    match error {
        BoundedJsonError::InvalidJson | BoundedJsonError::DuplicateKey => {
            ItemValidationError::InvalidValue
        }
        BoundedJsonError::LimitExceeded => ItemValidationError::StructureLimitExceeded,
    }
}

pub(super) fn map_item_error(error: ItemValidationError) -> ParseResponsesCompactionRequestError {
    match error {
        ItemValidationError::InvalidValue => ParseResponsesCompactionRequestError::InvalidValue,
        ItemValidationError::UnsupportedFeature => {
            ParseResponsesCompactionRequestError::UnsupportedFeature
        }
        ItemValidationError::StructureLimitExceeded => {
            ParseResponsesCompactionRequestError::StructureLimitExceeded
        }
    }
}

pub(super) fn map_item_error_response(
    error: ItemValidationError,
) -> ParseResponsesCompactionResponseError {
    match error {
        ItemValidationError::InvalidValue => ParseResponsesCompactionResponseError::InvalidValue,
        ItemValidationError::UnsupportedFeature => {
            ParseResponsesCompactionResponseError::UnsupportedFeature
        }
        ItemValidationError::StructureLimitExceeded => {
            ParseResponsesCompactionResponseError::StructureLimitExceeded
        }
    }
}

pub(super) fn map_usage_error(
    _error: UsageValidationError,
) -> ParseResponsesCompactionResponseError {
    ParseResponsesCompactionResponseError::InvalidUsage
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemValidationError {
    InvalidValue,
    UnsupportedFeature,
    StructureLimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UsageValidationError {
    InvalidValue,
    Usage,
}

impl From<ResponsesCompactionUsageError> for UsageValidationError {
    fn from(_error: ResponsesCompactionUsageError) -> Self {
        Self::Usage
    }
}
