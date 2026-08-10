use std::collections::HashSet;

use af_domain::{MAX_MODEL_NAME_BYTES, Operation, Protocol};
use serde_json::{Map, Value};

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    input::convert_input,
    wire::{
        ContextManagementTypeWire, ContextManagementWire, ConversationWire, Field,
        FunctionToolWire, FunctionTypeWire, NamedToolChoiceWire, ReasoningEffortWire,
        ReasoningWire, ResponsesRequestWire, ToolChoiceModeWire, ToolChoiceWire, ToolWire,
    },
};
use crate::{
    CanonicalRequest, ReasoningConfig, ReasoningEffort, RequestContinuation, RequestMetadata,
    Sampling, TokenCount, ToolChoice, ToolDef,
};

pub(super) const MAX_MESSAGES: usize = 256;
pub(super) const MAX_PARTS_PER_MESSAGE: usize = 128;
pub(super) const MAX_CONTENT_BLOCKS: usize = 1_024;
pub(super) const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_TOTAL_TEXT_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_TOOLS: usize = 128;
pub(super) const MAX_TOOL_CALLS: usize = 128;
pub(super) const MAX_TOOL_DESCRIPTION_BYTES: usize = 8 * 1024;
pub(super) const MAX_TOOL_CALL_ID_BYTES: usize = 256;
pub(super) const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
pub(super) const MAX_TOTAL_ARGUMENT_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_TOTAL_ARGUMENT_NODES: usize = 32_768;
pub(super) const MAX_MEDIA_URL_BYTES: usize = 8 * 1024;
pub(super) const MAX_MEDIA_BYTES: usize = 20 * 1024 * 1024;
pub(super) const MAX_ENCRYPTED_REASONING_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_REASONING_SUMMARIES: usize = 16;
pub(super) const MAX_COMPACTION_THRESHOLD: i64 = MAX_OUTPUT_TOKENS;

const MAX_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_TOTAL_SCHEMA_BYTES: usize = 512 * 1024;
const MAX_USER_ID_BYTES: usize = 512;
const MAX_CONTINUATION_ID_BYTES: usize = 512;
const MAX_PROMPT_CACHE_KEY_BYTES: usize = 256;
const MAX_RAW_FIELDS: usize = 16;
const MAX_RAW_BYTES: usize = 64 * 1024;
const MAX_RAW_DEPTH: usize = 8;
const MAX_RAW_NODES: usize = 4_096;
const MAX_RAW_OBJECT_ENTRIES: usize = 256;
const MAX_RAW_KEY_BYTES: usize = 128;
const MAX_METADATA_FIELDS: usize = 16;
const MAX_METADATA_KEY_BYTES: usize = 64;
const MAX_METADATA_VALUE_BYTES: usize = 512;
const MAX_INCLUDE_ITEMS: usize = 32;
const MAX_INCLUDE_VALUE_BYTES: usize = 128;

const RAW_FIELD_ALLOWLIST: &[&str] = &[
    "context_management",
    "include",
    "metadata",
    "parallel_tool_calls",
    "prompt_cache_options",
    "prompt_cache_retention",
    "safety_identifier",
    "service_tier",
    "store",
    "truncation",
];

const FORBIDDEN_RAW_KEYS: &[&str] = &[
    "__proto__",
    "api_key",
    "authorization",
    "base_url",
    "constructor",
    "credential",
    "credentials",
    "endpoint",
    "headers",
    "prototype",
    "proxy",
    "token",
    "url",
];

pub(super) fn convert_request(
    wire: ResponsesRequestWire,
) -> Result<CanonicalRequest, ParseRequestError> {
    validate_model(&wire.model)?;
    let stream = convert_stream(wire.stream);
    let continuation = convert_continuation(
        wire.conversation,
        wire.previous_response_id,
        wire.prompt_cache_key,
    )?;
    let messages = convert_input(wire.instructions, wire.input, &continuation)?;
    let (tools, tool_names) = convert_tools(wire.tools)?;
    let tool_choice = convert_tool_choice(wire.tool_choice, &tool_names)?;
    let reasoning = convert_reasoning(wire.reasoning)?;
    let sampling = convert_sampling(wire.temperature, wire.top_p, wire.max_output_tokens)?;
    let metadata = convert_metadata(wire.user)?;
    let mut raw = validate_raw(wire.extra)?.unwrap_or_default();
    if let Field::Value(context_management) = wire.context_management {
        let value = convert_context_management(context_management)?;
        raw.insert("context_management".to_owned(), value);
    }
    let raw = (!raw.is_empty()).then_some(raw);

    let mut request = CanonicalRequest::new(Operation::Responses, wire.model, messages, stream);
    request.tools = tools;
    request.tool_choice = tool_choice;
    request.reasoning = reasoning;
    request.sampling = sampling;
    request.metadata = metadata;
    request.continuation = continuation;
    Ok(request
        .with_validated_raw_passthrough(raw.map(|fields| (Protocol::OpenAiResponses, fields))))
}

fn convert_context_management(
    values: Vec<ContextManagementWire>,
) -> Result<Value, ParseRequestError> {
    if values.len() != 1 {
        return Err(ParseRequestError::InvalidValue);
    }
    let ContextManagementWire {
        kind: ContextManagementTypeWire::Compaction,
        compact_threshold,
    } = values.into_iter().next().expect("长度已校验");
    if !(1..=MAX_COMPACTION_THRESHOLD).contains(&compact_threshold) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(Value::Array(vec![Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("compaction".to_owned())),
        (
            "compact_threshold".to_owned(),
            Value::Number(compact_threshold.into()),
        ),
    ]))]))
}

pub(super) fn validate_model(model: &str) -> Result<(), ParseRequestError> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.chars().any(char::is_control)
        || model.trim() != model
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn convert_stream(stream: Field<bool>) -> bool {
    match stream {
        Field::Missing | Field::Value(false) => false,
        Field::Value(true) => true,
    }
}

fn convert_continuation(
    conversation: Field<ConversationWire>,
    previous_response_id: Field<String>,
    prompt_cache_key: Field<String>,
) -> Result<RequestContinuation, ParseRequestError> {
    let conversation_id = match conversation {
        Field::Missing => None,
        Field::Value(ConversationWire::Id(id)) => Some(id),
        Field::Value(ConversationWire::Object(object)) => Some(object.id),
    };
    let previous_response_id = field_value(previous_response_id);
    let prompt_cache_key = field_value(prompt_cache_key);
    if conversation_id.is_some() && previous_response_id.is_some() {
        return Err(ParseRequestError::ConflictingParameters);
    }
    if let Some(value) = conversation_id.as_deref() {
        validate_opaque_id(value, MAX_CONTINUATION_ID_BYTES)?;
    }
    if let Some(value) = previous_response_id.as_deref() {
        validate_opaque_id(value, MAX_CONTINUATION_ID_BYTES)?;
    }
    if let Some(value) = prompt_cache_key.as_deref() {
        validate_opaque_id(value, MAX_PROMPT_CACHE_KEY_BYTES)?;
    }
    Ok(RequestContinuation::new(
        previous_response_id,
        conversation_id,
        prompt_cache_key,
    ))
}

fn validate_opaque_id(value: &str, max_bytes: usize) -> Result<(), ParseRequestError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn convert_tools(
    tools: Field<Vec<ToolWire>>,
) -> Result<(Vec<ToolDef>, HashSet<String>), ParseRequestError> {
    let tools = match tools {
        Field::Missing => Vec::new(),
        Field::Value(tools) => tools,
    };
    if tools.len() > MAX_TOOLS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let mut converted = Vec::with_capacity(tools.len());
    let mut names = HashSet::with_capacity(tools.len());
    let mut total_schema_bytes = 0_usize;
    for tool in tools {
        let ToolWire::Function(FunctionToolWire {
            name,
            parameters,
            strict,
            description,
            allowed_callers,
            defer_loading,
            output_schema,
        }) = tool;
        if field_is_present(&allowed_callers)
            || field_is_present(&defer_loading)
            || field_is_present(&output_schema)
        {
            return Err(ParseRequestError::UnsupportedFeature);
        }
        validate_tool_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(ParseRequestError::InvalidValue);
        }
        let description = field_value(description);
        if description
            .as_ref()
            .is_some_and(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
        {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        validate_schema(&parameters, &mut total_schema_bytes)?;
        converted.push(ToolDef {
            name,
            description,
            input_schema: parameters,
            strict: Some(strict),
        });
    }
    Ok((converted, names))
}

pub(super) fn validate_tool_name(name: &str) -> Result<(), ParseRequestError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_schema(
    schema: &Value,
    total_schema_bytes: &mut usize,
) -> Result<(), ParseRequestError> {
    if !schema.is_object() {
        return Err(ParseRequestError::InvalidValue);
    }
    let _ = validate_value_shape(schema, 16, 4_096, 1_024)?;
    validate_local_schema_references(schema)?;
    let bytes = serde_json::to_vec(schema).map_err(|_| ParseRequestError::InvalidValue)?;
    if bytes.len() > MAX_SCHEMA_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    *total_schema_bytes = total_schema_bytes
        .checked_add(bytes.len())
        .ok_or(ParseRequestError::StructureLimitExceeded)?;
    if *total_schema_bytes > MAX_TOTAL_SCHEMA_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    Ok(())
}

fn validate_local_schema_references(value: &Value) -> Result<(), ParseRequestError> {
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef") {
                        let reference = child.as_str().ok_or(ParseRequestError::InvalidValue)?;
                        if !reference.starts_with('#') {
                            return Err(ParseRequestError::UnsupportedFeature);
                        }
                    }
                    stack.push(child);
                }
            }
            Value::Array(array) => stack.extend(array),
            _ => {}
        }
    }
    Ok(())
}

fn convert_tool_choice(
    choice: Field<ToolChoiceWire>,
    tool_names: &HashSet<String>,
) -> Result<ToolChoice, ParseRequestError> {
    match choice {
        Field::Missing if tool_names.is_empty() => Ok(ToolChoice::None),
        Field::Missing | Field::Value(ToolChoiceWire::Mode(ToolChoiceModeWire::Auto)) => {
            Ok(ToolChoice::Auto)
        }
        Field::Value(ToolChoiceWire::Mode(ToolChoiceModeWire::None)) => Ok(ToolChoice::None),
        Field::Value(ToolChoiceWire::Mode(ToolChoiceModeWire::Required)) => {
            if tool_names.is_empty() {
                Err(ParseRequestError::InvalidValue)
            } else {
                Ok(ToolChoice::Required)
            }
        }
        Field::Value(ToolChoiceWire::Named(NamedToolChoiceWire {
            kind: FunctionTypeWire::Function,
            name,
        })) => {
            validate_tool_name(&name)?;
            if !tool_names.contains(&name) {
                return Err(ParseRequestError::InvalidValue);
            }
            Ok(ToolChoice::Named { name })
        }
    }
}

fn convert_reasoning(
    reasoning: Field<ReasoningWire>,
) -> Result<Option<ReasoningConfig>, ParseRequestError> {
    let effort = match reasoning {
        Field::Missing => return Ok(None),
        Field::Value(ReasoningWire {
            effort: Field::Missing,
        }) => return Ok(None),
        Field::Value(ReasoningWire {
            effort: Field::Value(effort),
        }) => effort,
    };
    let effort = match effort {
        ReasoningEffortWire::None => ReasoningEffort::None,
        ReasoningEffortWire::Minimal => ReasoningEffort::Minimal,
        ReasoningEffortWire::Low => ReasoningEffort::Low,
        ReasoningEffortWire::Medium => ReasoningEffort::Medium,
        ReasoningEffortWire::High => ReasoningEffort::High,
        ReasoningEffortWire::ExtraHigh => ReasoningEffort::ExtraHigh,
        ReasoningEffortWire::Max => ReasoningEffort::Max,
    };
    ReasoningConfig::new(Some(effort), None, false)
        .map(Some)
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn convert_sampling(
    temperature: Field<f64>,
    top_p: Field<f64>,
    max_output_tokens: Field<i64>,
) -> Result<Sampling, ParseRequestError> {
    let temperature = field_value(temperature);
    if temperature.is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }
    let top_p = field_value(top_p);
    if top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }
    let max_output_tokens = match field_value(max_output_tokens) {
        None => None,
        Some(value) if (1..=MAX_OUTPUT_TOKENS).contains(&value) => {
            Some(TokenCount::new(value).map_err(|_| ParseRequestError::InvalidValue)?)
        }
        Some(_) => return Err(ParseRequestError::InvalidValue),
    };
    Sampling::new(temperature, top_p, max_output_tokens, Vec::new())
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn convert_metadata(user: Field<String>) -> Result<RequestMetadata, ParseRequestError> {
    let user_id = match user {
        Field::Missing => None,
        Field::Value(user_id) => {
            if user_id.is_empty()
                || user_id.len() > MAX_USER_ID_BYTES
                || user_id.chars().any(char::is_control)
            {
                return Err(ParseRequestError::InvalidValue);
            }
            Some(user_id)
        }
    };
    Ok(RequestMetadata::new(user_id, None))
}

fn validate_raw(
    extra: Map<String, Value>,
) -> Result<Option<Map<String, Value>>, ParseRequestError> {
    if extra.is_empty() {
        return Ok(None);
    }
    validate_raw_fields(&extra)?;
    Ok(Some(extra))
}

pub(super) fn validate_raw_fields(extra: &Map<String, Value>) -> Result<(), ParseRequestError> {
    if extra.len() > MAX_RAW_FIELDS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    for key in extra.keys() {
        if super::UNSUPPORTED_TOP_LEVEL_FIELDS.contains(&key.as_str()) {
            return Err(ParseRequestError::UnsupportedFeature);
        }
        if !RAW_FIELD_ALLOWLIST.contains(&key.as_str()) {
            return Err(ParseRequestError::InvalidValue);
        }
    }
    validate_raw_semantics(extra)?;
    let encoded = serde_json::to_vec(extra).map_err(|_| ParseRequestError::InvalidValue)?;
    if encoded.len() > MAX_RAW_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    validate_raw_values(extra)?;
    Ok(())
}

fn validate_raw_semantics(extra: &Map<String, Value>) -> Result<(), ParseRequestError> {
    for (key, value) in extra {
        match key.as_str() {
            "context_management" => validate_context_management(value)?,
            "include" => validate_include(value)?,
            "metadata" => validate_raw_metadata(value)?,
            "parallel_tool_calls" | "store" => {
                if !value.is_boolean() {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "prompt_cache_options" => validate_prompt_cache_options(value)?,
            "prompt_cache_retention" => {
                if !matches!(value.as_str(), Some("in_memory" | "24h")) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "safety_identifier" => validate_bounded_raw_string(value, 64)?,
            "service_tier" => {
                if !matches!(
                    value.as_str(),
                    Some("auto" | "default" | "flex" | "scale" | "priority")
                ) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "truncation" => {
                if !matches!(value.as_str(), Some("auto" | "disabled")) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            _ => return Err(ParseRequestError::InvalidValue),
        }
    }
    Ok(())
}

fn validate_context_management(value: &Value) -> Result<(), ParseRequestError> {
    let values = value.as_array().ok_or(ParseRequestError::InvalidValue)?;
    if values.len() != 1 {
        return Err(ParseRequestError::InvalidValue);
    }
    let object = values[0]
        .as_object()
        .ok_or(ParseRequestError::InvalidValue)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "compact_threshold"))
        || object.get("type").and_then(Value::as_str) != Some("compaction")
    {
        return Err(ParseRequestError::InvalidValue);
    }
    let threshold = object
        .get("compact_threshold")
        .and_then(Value::as_i64)
        .ok_or(ParseRequestError::InvalidValue)?;
    if !(1..=MAX_COMPACTION_THRESHOLD).contains(&threshold) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn validate_include(value: &Value) -> Result<(), ParseRequestError> {
    let include = value.as_array().ok_or(ParseRequestError::InvalidValue)?;
    if include.len() > MAX_INCLUDE_ITEMS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    for item in include {
        validate_bounded_raw_string(item, MAX_INCLUDE_VALUE_BYTES)?;
    }
    Ok(())
}

fn validate_prompt_cache_options(value: &Value) -> Result<(), ParseRequestError> {
    let options = value.as_object().ok_or(ParseRequestError::InvalidValue)?;
    if options
        .keys()
        .any(|key| !matches!(key.as_str(), "mode" | "ttl"))
    {
        return Err(ParseRequestError::InvalidValue);
    }
    if options
        .get("mode")
        .is_some_and(|value| !matches!(value.as_str(), Some("implicit" | "explicit")))
        || options
            .get("ttl")
            .is_some_and(|value| value.as_str() != Some("30m"))
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn validate_raw_metadata(value: &Value) -> Result<(), ParseRequestError> {
    let metadata = value.as_object().ok_or(ParseRequestError::InvalidValue)?;
    if metadata.len() > MAX_METADATA_FIELDS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    for (key, value) in metadata {
        let value = value.as_str().ok_or(ParseRequestError::InvalidValue)?;
        if key.is_empty()
            || key.len() > MAX_METADATA_KEY_BYTES
            || key.chars().any(char::is_control)
            || value.len() > MAX_METADATA_VALUE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ParseRequestError::InvalidValue);
        }
    }
    Ok(())
}

fn validate_bounded_raw_string(value: &Value, max_bytes: usize) -> Result<(), ParseRequestError> {
    let value = value.as_str().ok_or(ParseRequestError::InvalidValue)?;
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn validate_raw_values(extra: &Map<String, Value>) -> Result<(), ParseRequestError> {
    let mut nodes = 1_usize;
    let mut stack = extra
        .values()
        .map(|value| (value, 1_usize))
        .collect::<Vec<_>>();
    while let Some((value, depth)) = stack.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if nodes > MAX_RAW_NODES || depth > MAX_RAW_DEPTH {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        match value {
            Value::Object(object) => {
                if object.len() > MAX_RAW_OBJECT_ENTRIES {
                    return Err(ParseRequestError::StructureLimitExceeded);
                }
                for (key, child) in object {
                    if key.len() > MAX_RAW_KEY_BYTES || FORBIDDEN_RAW_KEYS.contains(&key.as_str()) {
                        return Err(ParseRequestError::InvalidValue);
                    }
                    stack.push((child, depth + 1));
                }
            }
            Value::Array(array) => {
                stack.extend(array.iter().map(|child| (child, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn validate_value_shape(
    value: &Value,
    max_depth: usize,
    max_nodes: usize,
    max_object_entries: usize,
) -> Result<usize, ParseRequestError> {
    let mut nodes = 0_usize;
    let mut stack = vec![(value, 0_usize)];
    while let Some((value, depth)) = stack.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if nodes > max_nodes || depth > max_depth {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        match value {
            Value::Object(object) => {
                if object.len() > max_object_entries {
                    return Err(ParseRequestError::StructureLimitExceeded);
                }
                stack.extend(object.values().map(|child| (child, depth + 1)));
            }
            Value::Array(array) => {
                stack.extend(array.iter().map(|child| (child, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(nodes)
}

fn field_value<T>(field: Field<T>) -> Option<T> {
    match field {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    }
}

fn field_is_present<T>(field: &Field<T>) -> bool {
    matches!(field, Field::Value(_))
}
