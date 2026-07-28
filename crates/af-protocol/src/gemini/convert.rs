use std::collections::{HashMap, HashSet};

use af_domain::{Operation, Role};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use serde_json::{Map, Value};

use super::{ParseRequestError, wire::*};
use crate::{
    CanonicalRequest, ContentBlock, MediaSource, Message, ReasoningConfig, ReasoningEffort,
    Sampling, TokenCount, ToolChoice, ToolDef,
    bounded_json::{BoundedJsonError, JsonLimits, validate_object},
};

const MAX_MODEL_NAME_BYTES: usize = 256;
pub(super) const MAX_CONTENTS: usize = 4_096;
pub(super) const MAX_NORMALIZED_MESSAGES: usize = 8_192;
pub(super) const MAX_PARTS_PER_CONTENT: usize = 1_024;
pub(super) const MAX_CONTENT_BLOCKS: usize = 16_384;
pub(super) const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_TOTAL_TEXT_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_MEDIA_BYTES: usize = 20 * 1024 * 1024;
const MAX_MIME_TYPE_BYTES: usize = 127;
pub(super) const MAX_TOOLS: usize = 128;
const MAX_TOOL_NAME_BYTES: usize = 64;
pub(super) const MAX_TOOL_DESCRIPTION_BYTES: usize = 16 * 1024;
pub(super) const MAX_SCHEMA_BYTES: usize = 256 * 1024;
pub(super) const MAX_TOTAL_SCHEMA_BYTES: usize = 1024 * 1024;
pub(super) const MAX_TOOL_CALLS: usize = 128;
const MAX_TOOL_CALL_ID_BYTES: usize = 256;
pub(super) const MAX_ARGUMENT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_TOTAL_ARGUMENT_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_RESULT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_TOTAL_RESULT_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_TOTAL_JSON_NODES: usize = 16_384;
pub(super) const MAX_SIGNATURE_BYTES: usize = 64 * 1024;
pub(super) const MAX_TOTAL_SIGNATURE_BYTES: usize = 1024 * 1024;
pub(super) const MAX_OUTPUT_TOKENS: i64 = 1_000_000;
const MAX_STOP_SEQUENCES: usize = 5;
const MAX_STOP_BYTES: usize = 4_096;
const MAX_TOTAL_STOP_BYTES: usize = 16 * 1024;

pub(super) const SCHEMA_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 16_384,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_SCHEMA_BYTES,
    max_key_bytes: 1_024,
};
pub(super) const TOOL_PAYLOAD_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

pub(super) fn convert_request(
    model_resource: &str,
    wire: GenerateContentRequestWire,
) -> Result<CanonicalRequest, ParseRequestError> {
    let model = normalize_model_resource(model_resource)?;
    let (tools, tool_names) = convert_tools(wire.tools)?;
    let tool_choice = convert_tool_choice(wire.tool_config, &tool_names)?;
    let (sampling, reasoning) = convert_generation_config(wire.generation_config)?;
    let messages = convert_messages(wire.system_instruction, wire.contents)?;

    let mut request = CanonicalRequest::new(Operation::Chat, model, messages, false);
    request.tools = tools;
    request.tool_choice = tool_choice;
    request.sampling = sampling;
    request.reasoning = reasoning;
    Ok(request)
}

fn normalize_model_resource(value: &str) -> Result<String, ParseRequestError> {
    let Some(model) = value.strip_prefix("models/") else {
        return Err(ParseRequestError::InvalidValue);
    };
    validate_model(model)?;
    Ok(model.to_owned())
}

pub(super) fn validate_model(model: &str) -> Result<(), ParseRequestError> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.contains('/')
        || model.chars().any(char::is_control)
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn convert_tools(
    tools: Field<Vec<ToolWire>>,
) -> Result<(Vec<ToolDef>, HashSet<String>), ParseRequestError> {
    let tools = field_value(tools).unwrap_or_default();
    if tools.len() > MAX_TOOLS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let mut converted = Vec::new();
    let mut names = HashSet::new();
    let mut total_schema_bytes = 0_usize;
    for tool in tools {
        let declarations =
            field_value(tool.function_declarations).ok_or(ParseRequestError::InvalidValue)?;
        if declarations.is_empty() {
            return Err(ParseRequestError::InvalidValue);
        }
        for declaration in declarations {
            if converted.len() >= MAX_TOOLS {
                return Err(ParseRequestError::StructureLimitExceeded);
            }
            validate_tool_name(&declaration.name)?;
            if !names.insert(declaration.name.clone()) {
                return Err(ParseRequestError::InvalidValue);
            }
            let description = field_value(declaration.description);
            if description
                .as_ref()
                .is_some_and(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
            {
                return Err(ParseRequestError::StructureLimitExceeded);
            }
            let input_schema = match (declaration.parameters, declaration.parameters_json_schema) {
                (Field::Missing, Field::Missing) => Value::Object(Map::new()),
                (Field::Value(schema), Field::Missing) | (Field::Missing, Field::Value(schema)) => {
                    schema
                }
                (Field::Value(_), Field::Value(_)) => {
                    return Err(ParseRequestError::InvalidValue);
                }
            };
            let (schema_bytes, _) =
                validate_json_object(&input_schema, SCHEMA_LIMITS, MAX_SCHEMA_BYTES)?;
            total_schema_bytes = total_schema_bytes
                .checked_add(schema_bytes)
                .ok_or(ParseRequestError::StructureLimitExceeded)?;
            if total_schema_bytes > MAX_TOTAL_SCHEMA_BYTES {
                return Err(ParseRequestError::StructureLimitExceeded);
            }
            converted.push(ToolDef {
                name: declaration.name,
                description,
                input_schema,
                strict: None,
            });
        }
    }
    Ok((converted, names))
}

fn convert_tool_choice(
    config: Field<ToolConfigWire>,
    tool_names: &HashSet<String>,
) -> Result<ToolChoice, ParseRequestError> {
    let function_config = match config {
        Field::Missing => None,
        Field::Value(config) => field_value(config.function_calling_config),
    };
    let Some(config) = function_config else {
        return Ok(if tool_names.is_empty() {
            ToolChoice::None
        } else {
            ToolChoice::Auto
        });
    };

    let mode = field_value(config.mode).unwrap_or(FunctionCallingModeWire::Auto);
    let allowed = field_value(config.allowed_function_names).unwrap_or_default();
    if allowed.len() > MAX_TOOLS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    let mut allowed_names = HashSet::with_capacity(allowed.len());
    for name in allowed {
        validate_tool_name(&name)?;
        if !allowed_names.insert(name.clone()) || !tool_names.contains(&name) {
            return Err(ParseRequestError::InvalidValue);
        }
    }

    match mode {
        FunctionCallingModeWire::Unspecified => Err(ParseRequestError::InvalidValue),
        FunctionCallingModeWire::Auto => {
            if !allowed_names.is_empty() {
                return Err(ParseRequestError::InvalidValue);
            }
            Ok(if tool_names.is_empty() {
                ToolChoice::None
            } else {
                ToolChoice::Auto
            })
        }
        FunctionCallingModeWire::None => {
            if !allowed_names.is_empty() {
                return Err(ParseRequestError::InvalidValue);
            }
            Ok(ToolChoice::None)
        }
        FunctionCallingModeWire::Any => {
            if tool_names.is_empty() {
                return Err(ParseRequestError::InvalidValue);
            }
            if allowed_names.len() == 1 {
                let Some(name) = allowed_names.into_iter().next() else {
                    unreachable!("单元素集合必须包含工具名称");
                };
                Ok(ToolChoice::Named { name })
            } else if allowed_names.is_empty() || &allowed_names == tool_names {
                Ok(ToolChoice::Required)
            } else {
                Err(ParseRequestError::UnsupportedFeature)
            }
        }
        FunctionCallingModeWire::Validated => Err(ParseRequestError::UnsupportedFeature),
    }
}

fn convert_generation_config(
    config: Field<GenerationConfigWire>,
) -> Result<(Sampling, Option<ReasoningConfig>), ParseRequestError> {
    let Some(config) = field_value(config) else {
        return Ok((Sampling::EMPTY, None));
    };
    match config.candidate_count {
        Field::Missing | Field::Value(1) => {}
        Field::Value(value) if value > 1 => {
            return Err(ParseRequestError::UnsupportedFeature);
        }
        Field::Value(_) => return Err(ParseRequestError::InvalidValue),
    }

    let temperature = field_value(config.temperature);
    if temperature.is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }
    let top_p = field_value(config.top_p);
    if top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }
    let max_output_tokens = match field_value(config.max_output_tokens) {
        None => None,
        Some(value) if (1..=MAX_OUTPUT_TOKENS).contains(&value) => {
            Some(TokenCount::new(value).map_err(|_| ParseRequestError::InvalidValue)?)
        }
        Some(_) => return Err(ParseRequestError::InvalidValue),
    };
    let stop_sequences = field_value(config.stop_sequences).unwrap_or_default();
    validate_stop_sequences(&stop_sequences)?;
    let sampling = Sampling::new(temperature, top_p, max_output_tokens, stop_sequences)
        .map_err(|_| ParseRequestError::InvalidValue)?;
    let reasoning = convert_thinking_config(config.thinking_config)?;
    Ok((sampling, reasoning))
}

pub(super) fn validate_stop_sequences(values: &[String]) -> Result<(), ParseRequestError> {
    if values.len() > MAX_STOP_SEQUENCES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    let mut total = 0_usize;
    for value in values {
        if value.is_empty() || value.len() > MAX_STOP_BYTES {
            return Err(ParseRequestError::InvalidValue);
        }
        total = total
            .checked_add(value.len())
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if total > MAX_TOTAL_STOP_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
    }
    Ok(())
}

fn convert_thinking_config(
    config: Field<ThinkingConfigWire>,
) -> Result<Option<ReasoningConfig>, ParseRequestError> {
    let Some(config) = field_value(config) else {
        return Ok(None);
    };
    if matches!(&config.thinking_budget, Field::Value(_))
        && matches!(&config.thinking_level, Field::Value(_))
    {
        return Err(ParseRequestError::InvalidValue);
    }
    let include_thinking = field_value(config.include_thoughts).unwrap_or(false);
    let (effort, budget_tokens) = match (config.thinking_budget, config.thinking_level) {
        (Field::Value(value), Field::Missing) if value < -1 => {
            return Err(ParseRequestError::InvalidValue);
        }
        (Field::Value(-1), Field::Missing) => (None, None),
        (Field::Value(0), Field::Missing) => (Some(ReasoningEffort::None), None),
        (Field::Value(value), Field::Missing) => (
            None,
            Some(TokenCount::new(value).map_err(|_| ParseRequestError::InvalidValue)?),
        ),
        (Field::Missing, Field::Value(ThinkingLevelWire::Unspecified)) => {
            return Err(ParseRequestError::InvalidValue);
        }
        (Field::Missing, Field::Value(level)) => {
            let effort = match level {
                ThinkingLevelWire::Minimal => ReasoningEffort::Minimal,
                ThinkingLevelWire::Low => ReasoningEffort::Low,
                ThinkingLevelWire::Medium => ReasoningEffort::Medium,
                ThinkingLevelWire::High => ReasoningEffort::High,
                ThinkingLevelWire::Unspecified => unreachable!("未指定强度已提前拒绝"),
            };
            (Some(effort), None)
        }
        (Field::Missing, Field::Missing) => (None, None),
        (Field::Value(_), Field::Value(_)) => unreachable!("预算与强度冲突已提前拒绝"),
    };
    ReasoningConfig::new(effort, budget_tokens, include_thinking)
        .map(Some)
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn convert_messages(
    system_instruction: Field<ContentWire>,
    contents: Vec<ContentWire>,
) -> Result<Vec<Message>, ParseRequestError> {
    if contents.is_empty() || contents.len() > MAX_CONTENTS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let mut state = MessageConvertState::default();
    let mut messages = Vec::with_capacity(contents.len() + 1);
    if let Field::Value(system) = system_instruction {
        messages.push(convert_system_instruction(system, &mut state)?);
    }
    for content in contents {
        let role = match field_value(content.role).as_deref() {
            None | Some("user") => Role::User,
            Some("model") => Role::Assistant,
            Some(_) => return Err(ParseRequestError::InvalidValue),
        };
        if content.parts.is_empty() || content.parts.len() > MAX_PARTS_PER_CONTENT {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        match role {
            Role::User => messages.extend(convert_user_content(content.parts, &mut state)?),
            Role::Assistant => {
                if !state.pending_tool_calls.is_empty() {
                    return Err(ParseRequestError::InvalidValue);
                }
                let mut blocks = Vec::with_capacity(content.parts.len());
                for part in content.parts {
                    blocks.push(convert_regular_part(part, Role::Assistant, &mut state)?);
                }
                messages.push(Message::new(Role::Assistant, blocks));
            }
            Role::System | Role::Developer | Role::Tool => unreachable!("Gemini 角色已闭合映射"),
        }
        if messages.len() > MAX_NORMALIZED_MESSAGES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
    }
    if !state.pending_tool_calls.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(messages)
}

fn convert_system_instruction(
    content: ContentWire,
    state: &mut MessageConvertState,
) -> Result<Message, ParseRequestError> {
    match content.role {
        Field::Missing => {}
        Field::Value(role) if role == "user" => {}
        Field::Value(_) => return Err(ParseRequestError::InvalidValue),
    }
    if content.parts.is_empty() || content.parts.len() > MAX_PARTS_PER_CONTENT {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    let mut blocks = Vec::with_capacity(content.parts.len());
    for part in content.parts {
        let PartWire {
            text,
            inline_data,
            function_call,
            function_response,
            thought,
            thought_signature,
        } = part;
        if !matches!(inline_data, Field::Missing)
            || !matches!(function_call, Field::Missing)
            || !matches!(function_response, Field::Missing)
            || matches!(thought, Field::Value(true))
            || !matches!(thought_signature, Field::Missing)
        {
            return Err(ParseRequestError::UnsupportedFeature);
        }
        let text = field_value(text).ok_or(ParseRequestError::InvalidValue)?;
        blocks.push(state.add_text(text)?);
    }
    Ok(Message::new(Role::System, blocks))
}

fn convert_user_content(
    parts: Vec<PartWire>,
    state: &mut MessageConvertState,
) -> Result<Vec<Message>, ParseRequestError> {
    let mut messages = Vec::new();
    let mut regular = Vec::new();
    for part in parts {
        if matches!(&part.function_response, Field::Value(_)) {
            if !regular.is_empty() {
                return Err(ParseRequestError::InvalidValue);
            }
            let result = convert_function_response(part, state)?;
            messages.push(Message::new(Role::Tool, vec![result]));
        } else {
            if !state.pending_tool_calls.is_empty() {
                return Err(ParseRequestError::InvalidValue);
            }
            regular.push(convert_regular_part(part, Role::User, state)?);
        }
    }
    if !state.pending_tool_calls.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }
    if !regular.is_empty() {
        messages.push(Message::new(Role::User, regular));
    }
    Ok(messages)
}

fn convert_regular_part(
    part: PartWire,
    role: Role,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let PartWire {
        text,
        inline_data,
        function_call,
        function_response,
        thought,
        thought_signature,
    } = part;
    let primary_fields = usize::from(matches!(&text, Field::Value(_)))
        + usize::from(matches!(&inline_data, Field::Value(_)))
        + usize::from(matches!(&function_call, Field::Value(_)))
        + usize::from(matches!(&function_response, Field::Value(_)));
    if primary_fields != 1 {
        return Err(ParseRequestError::InvalidValue);
    }
    let thought = field_value(thought).unwrap_or(false);
    let signature = field_value(thought_signature);

    match (text, inline_data, function_call, function_response) {
        (Field::Value(text), Field::Missing, Field::Missing, Field::Missing) => {
            if thought {
                if role != Role::Assistant {
                    return Err(ParseRequestError::InvalidValue);
                }
                let signature = validate_optional_signature(signature, state)?;
                state.add_thinking(text, signature)
            } else {
                if signature.is_some() {
                    return Err(ParseRequestError::UnsupportedFeature);
                }
                state.add_text(text)
            }
        }
        (Field::Missing, Field::Value(media), Field::Missing, Field::Missing) => {
            if thought || signature.is_some() {
                return Err(ParseRequestError::UnsupportedFeature);
            }
            convert_inline_media(media, state)
        }
        (Field::Missing, Field::Missing, Field::Value(call), Field::Missing) => {
            if role != Role::Assistant {
                return Err(ParseRequestError::InvalidValue);
            }
            if thought {
                return Err(ParseRequestError::UnsupportedFeature);
            }
            convert_function_call(call, signature, state)
        }
        (Field::Missing, Field::Missing, Field::Missing, Field::Value(_)) => {
            Err(ParseRequestError::InvalidValue)
        }
        _ => Err(ParseRequestError::InvalidValue),
    }
}

fn convert_function_call(
    call: FunctionCallWire,
    signature: Option<String>,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let id = field_value(call.id).ok_or(ParseRequestError::UnsupportedFeature)?;
    validate_call_id(&id)?;
    validate_tool_name(&call.name)?;
    if !state.seen_tool_call_ids.insert(id.clone()) {
        return Err(ParseRequestError::InvalidValue);
    }
    let input = field_value(call.args).unwrap_or_else(|| Value::Object(Map::new()));
    let (bytes, nodes) = validate_json_object(&input, TOOL_PAYLOAD_LIMITS, MAX_ARGUMENT_BYTES)?;
    state.add_argument(bytes, nodes)?;
    let signature = validate_optional_signature(signature, state)?;
    state
        .pending_tool_calls
        .insert(id.clone(), call.name.clone());
    state.add_block()?;
    Ok(ContentBlock::ToolUse {
        id,
        name: call.name,
        input,
        signature,
    })
}

fn convert_function_response(
    part: PartWire,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let PartWire {
        text,
        inline_data,
        function_call,
        function_response,
        thought,
        thought_signature,
    } = part;
    if !matches!(text, Field::Missing)
        || !matches!(inline_data, Field::Missing)
        || !matches!(function_call, Field::Missing)
        || matches!(thought, Field::Value(true))
        || !matches!(thought_signature, Field::Missing)
    {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    let response = field_value(function_response).ok_or(ParseRequestError::InvalidValue)?;
    let id = field_value(response.id).ok_or(ParseRequestError::UnsupportedFeature)?;
    validate_call_id(&id)?;
    validate_tool_name(&response.name)?;
    let expected_name = state
        .pending_tool_calls
        .get(&id)
        .ok_or(ParseRequestError::InvalidValue)?;
    if expected_name != &response.name {
        return Err(ParseRequestError::InvalidValue);
    }
    let (bytes, nodes) =
        validate_json_object(&response.response, TOOL_PAYLOAD_LIMITS, MAX_RESULT_BYTES)?;
    state.add_result(bytes, nodes)?;
    state.pending_tool_calls.remove(&id);
    state.add_block()?;
    let is_error = response
        .response
        .as_object()
        .is_some_and(|object| object.contains_key("error"));
    Ok(ContentBlock::ToolResult {
        tool_use_id: id,
        content: Vec::new(),
        structured_content: Some(response.response),
        is_error,
    })
}

fn convert_inline_media(
    media: BlobWire,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let mime_type = normalize_mime_type(&media.mime_type)?;
    let decoded = decode_base64(&media.data).ok_or(ParseRequestError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_MEDIA_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    state.add_media(decoded.len())?;
    state.add_block()?;
    let source = MediaSource::Base64(media.data);
    if mime_type.starts_with("image/") {
        Ok(ContentBlock::Image {
            source,
            mime_type: Some(mime_type),
        })
    } else if mime_type.starts_with("audio/") {
        Ok(ContentBlock::Audio { source, mime_type })
    } else {
        Err(ParseRequestError::UnsupportedFeature)
    }
}

pub(super) fn normalize_mime_type(value: &str) -> Result<String, ParseRequestError> {
    if value.is_empty()
        || value.len() > MAX_MIME_TYPE_BYTES
        || value.trim() != value
        || !value.is_ascii()
        || value.chars().any(char::is_control)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&^_.+-/".contains(&byte))
    {
        return Err(ParseRequestError::InvalidValue);
    }
    let normalized = value.to_ascii_lowercase();
    if !normalized.starts_with("image/") && !normalized.starts_with("audio/") {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    Ok(normalized)
}

fn validate_optional_signature(
    signature: Option<String>,
    state: &mut MessageConvertState,
) -> Result<Option<String>, ParseRequestError> {
    let Some(signature) = signature else {
        return Ok(None);
    };
    let decoded = decode_base64(&signature).ok_or(ParseRequestError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_SIGNATURE_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    state.add_signature(decoded.len())?;
    Ok(Some(signature))
}

pub(super) fn decode_base64(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() {
        return None;
    }
    STANDARD
        .decode(value)
        .or_else(|_| STANDARD_NO_PAD.decode(value))
        .ok()
}

pub(super) fn validate_tool_name(value: &str) -> Result<(), ParseRequestError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(ParseRequestError::InvalidValue);
    };
    if value.len() > MAX_TOOL_NAME_BYTES
        || !(first.is_ascii_alphabetic() || first == '_')
        || chars.any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-'))
        })
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_call_id(value: &str) -> Result<(), ParseRequestError> {
    if value.is_empty()
        || value.len() > MAX_TOOL_CALL_ID_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_json_object(
    value: &Value,
    limits: JsonLimits,
    max_bytes: usize,
) -> Result<(usize, usize), ParseRequestError> {
    let object = value.as_object().ok_or(ParseRequestError::InvalidValue)?;
    validate_object(object, limits, max_bytes).map_err(map_bounded_json_error)?;
    let bytes = serde_json::to_vec(object)
        .map(|encoded| encoded.len())
        .map_err(|_| ParseRequestError::InvalidValue)?;
    Ok((bytes, count_json_nodes(value)?))
}

fn count_json_nodes(value: &Value) -> Result<usize, ParseRequestError> {
    let mut nodes = 0_usize;
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        match value {
            Value::Object(object) => stack.extend(object.values()),
            Value::Array(array) => stack.extend(array),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
    Ok(nodes)
}

fn map_bounded_json_error(error: BoundedJsonError) -> ParseRequestError {
    match error {
        BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
        BoundedJsonError::InvalidJson | BoundedJsonError::DuplicateKey => {
            ParseRequestError::InvalidValue
        }
    }
}

fn field_value<T>(field: Field<T>) -> Option<T> {
    match field {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    }
}

#[derive(Default)]
struct MessageConvertState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    argument_bytes: usize,
    result_bytes: usize,
    json_nodes: usize,
    signature_bytes: usize,
    seen_tool_call_ids: HashSet<String>,
    pending_tool_calls: HashMap<String, String>,
}

impl MessageConvertState {
    fn add_block(&mut self) -> Result<(), ParseRequestError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text(&mut self, text: String) -> Result<ContentBlock, ParseRequestError> {
        self.reserve_text(&text)?;
        self.add_block()?;
        Ok(ContentBlock::Text(text))
    }

    fn add_thinking(
        &mut self,
        text: String,
        signature: Option<String>,
    ) -> Result<ContentBlock, ParseRequestError> {
        self.reserve_text(&text)?;
        self.add_block()?;
        Ok(ContentBlock::Thinking { text, signature })
    }

    fn reserve_text(&mut self, text: &str) -> Result<(), ParseRequestError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_media(&mut self, bytes: usize) -> Result<(), ParseRequestError> {
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_argument(&mut self, bytes: usize, nodes: usize) -> Result<(), ParseRequestError> {
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        self.add_json_nodes(nodes)?;
        if self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.seen_tool_call_ids.len() > MAX_TOOL_CALLS
        {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_result(&mut self, bytes: usize, nodes: usize) -> Result<(), ParseRequestError> {
        self.result_bytes = self
            .result_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        self.add_json_nodes(nodes)?;
        if self.result_bytes > MAX_TOTAL_RESULT_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_json_nodes(&mut self, nodes: usize) -> Result<(), ParseRequestError> {
        self.json_nodes = self
            .json_nodes
            .checked_add(nodes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.json_nodes > MAX_TOTAL_JSON_NODES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_signature(&mut self, bytes: usize) -> Result<(), ParseRequestError> {
        self.signature_bytes = self
            .signature_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.signature_bytes > MAX_TOTAL_SIGNATURE_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }
}
