use std::collections::HashSet;

use af_domain::{MAX_MODEL_NAME_BYTES, Operation, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use url::Url;

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    wire::{
        AssistantContentPartWire, AssistantContentWire, AssistantMessageWire,
        Base64ImageSourceWire, CacheControlTypeWire, CacheControlWire, CacheTtlWire, EmptyWire,
        Field, ImageBlockWire, ImageMediaTypeWire, ImageSourceWire, MessageWire,
        MessagesRequestWire, MetadataWire, NamedToolChoiceWire, OutputConfigWire, OutputEffortWire,
        SystemTextBlockWire, SystemWire, TextBlockWire, ThinkingAdaptiveWire, ThinkingDisplayWire,
        ThinkingEnabledWire, ThinkingWire, ToolChoiceParallelWire, ToolChoiceWire,
        ToolResultBlockWire, ToolResultContentPartWire, ToolResultContentWire, ToolUseBlockWire,
        ToolWire, UrlImageSourceWire, UserContentPartWire, UserContentWire, UserMessageWire,
    },
};
use crate::{
    CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message, ReasoningConfig,
    ReasoningEffort, RequestMetadata, Sampling, TokenCount, ToolChoice, ToolDef,
};

pub(super) const MAX_MESSAGES: usize = 256;
pub(super) const MAX_NORMALIZED_MESSAGES: usize = MAX_MESSAGES + MAX_TOOL_CALLS + 1;
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
const MAX_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_TOTAL_SCHEMA_BYTES: usize = 512 * 1024;
pub const MAX_STOP_SEQUENCES: usize = 4;
pub const MAX_STOP_BYTES: usize = 1024;
pub(super) const MAX_USER_ID_BYTES: usize = 512;
pub(super) const MAX_MEDIA_URL_BYTES: usize = 8 * 1024;
pub(super) const MAX_MEDIA_BYTES: usize = 20 * 1024 * 1024;
pub(super) const MAX_CACHE_BREAKPOINTS: usize = 4;

pub(super) fn convert_request(
    wire: MessagesRequestWire,
) -> Result<CanonicalRequest, ParseRequestError> {
    let MessagesRequestWire {
        model,
        max_tokens,
        system,
        messages,
        tools,
        tool_choice,
        thinking,
        output_config,
        temperature,
        top_p,
        stop_sequences,
        metadata,
        stream,
    } = wire;

    validate_model(&model)?;
    let stream = convert_stream(stream);
    let sampling = convert_sampling(max_tokens, temperature, top_p, stop_sequences)?;
    let reasoning = convert_reasoning(thinking, output_config, max_tokens)?;
    let metadata = convert_metadata(metadata)?;
    let (tools, tool_names) = convert_tools(tools)?;
    let tool_choice = convert_tool_choice(tool_choice, &tool_names)?;
    let messages = convert_messages(system, messages)?;

    let mut request = CanonicalRequest::new(Operation::Chat, model, messages, stream);
    request.tools = tools;
    request.tool_choice = tool_choice;
    request.reasoning = reasoning;
    request.sampling = sampling;
    request.metadata = metadata;
    Ok(request)
}

pub(super) fn validate_model(model: &str) -> Result<(), ParseRequestError> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.chars().any(char::is_control)
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

fn convert_sampling(
    max_tokens: i64,
    temperature: Field<f64>,
    top_p: Field<f64>,
    stop_sequences: Field<Vec<String>>,
) -> Result<Sampling, ParseRequestError> {
    if !(0..=MAX_OUTPUT_TOKENS).contains(&max_tokens) {
        return Err(ParseRequestError::InvalidValue);
    }
    let max_output_tokens =
        TokenCount::new(max_tokens).map_err(|_| ParseRequestError::InvalidValue)?;

    let temperature = field_value(temperature);
    if temperature.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }
    let top_p = field_value(top_p);
    if top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }

    let stop_sequences = field_value(stop_sequences).unwrap_or_default();
    if stop_sequences.len() > MAX_STOP_SEQUENCES
        || stop_sequences
            .iter()
            .any(|value| value.is_empty() || value.len() > MAX_STOP_BYTES)
    {
        return Err(ParseRequestError::InvalidValue);
    }

    Sampling::new(temperature, top_p, Some(max_output_tokens), stop_sequences)
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn convert_reasoning(
    thinking: Field<ThinkingWire>,
    output_config: Field<OutputConfigWire>,
    max_tokens: i64,
) -> Result<Option<ReasoningConfig>, ParseRequestError> {
    let effort = match output_config {
        Field::Missing => None,
        Field::Value(OutputConfigWire { effort }) => Some(match effort {
            OutputEffortWire::Low => ReasoningEffort::Low,
            OutputEffortWire::Medium => ReasoningEffort::Medium,
            OutputEffortWire::High => ReasoningEffort::High,
            OutputEffortWire::ExtraHigh => ReasoningEffort::ExtraHigh,
            OutputEffortWire::Max => ReasoningEffort::Max,
        }),
    };
    let thinking_present = !matches!(&thinking, Field::Missing);
    let (thinking_effort, budget, include) = match thinking {
        Field::Missing => (None, None, false),
        Field::Value(ThinkingWire::Disabled(EmptyWire {})) => {
            (Some(ReasoningEffort::None), None, false)
        }
        Field::Value(ThinkingWire::Enabled(ThinkingEnabledWire {
            budget_tokens,
            display,
        })) => {
            if budget_tokens < 1_024 || budget_tokens >= max_tokens {
                return Err(ParseRequestError::InvalidValue);
            }
            let budget =
                TokenCount::new(budget_tokens).map_err(|_| ParseRequestError::InvalidValue)?;
            (None, Some(budget), include_thinking(display))
        }
        Field::Value(ThinkingWire::Adaptive(ThinkingAdaptiveWire { display })) => {
            if max_tokens == 0 {
                return Err(ParseRequestError::InvalidValue);
            }
            (None, None, include_thinking(display))
        }
    };

    if thinking_effort == Some(ReasoningEffort::None) && effort.is_some() {
        return Err(ParseRequestError::InvalidValue);
    }
    if !thinking_present && effort.is_none() {
        return Ok(None);
    }
    ReasoningConfig::new(effort.or(thinking_effort), budget, include)
        .map(Some)
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn include_thinking(display: Field<ThinkingDisplayWire>) -> bool {
    !matches!(display, Field::Value(ThinkingDisplayWire::Omitted))
}

fn convert_metadata(metadata: Field<MetadataWire>) -> Result<RequestMetadata, ParseRequestError> {
    let user_id = match metadata {
        Field::Missing => None,
        Field::Value(MetadataWire { user_id }) => field_value(user_id),
    };
    if user_id.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > MAX_USER_ID_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(RequestMetadata::new(user_id, None))
}

fn convert_tools(
    tools: Field<Vec<ToolWire>>,
) -> Result<(Vec<ToolDef>, HashSet<String>), ParseRequestError> {
    let tools = field_value(tools).unwrap_or_default();
    if tools.len() > MAX_TOOLS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let mut converted = Vec::with_capacity(tools.len());
    let mut names = HashSet::with_capacity(tools.len());
    let mut total_schema_bytes = 0_usize;
    for tool in tools {
        validate_tool_name(&tool.name)?;
        if !names.insert(tool.name.clone()) {
            return Err(ParseRequestError::InvalidValue);
        }
        let description = field_value(tool.description);
        if description
            .as_ref()
            .is_some_and(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
        {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        validate_schema(&tool.input_schema, &mut total_schema_bytes)?;
        converted.push(ToolDef {
            name: tool.name,
            description,
            input_schema: tool.input_schema,
            strict: None,
        });
    }
    Ok((converted, names))
}

fn convert_tool_choice(
    choice: Field<ToolChoiceWire>,
    tool_names: &HashSet<String>,
) -> Result<ToolChoice, ParseRequestError> {
    match choice {
        Field::Missing if tool_names.is_empty() => Ok(ToolChoice::None),
        Field::Missing => Ok(ToolChoice::Auto),
        Field::Value(ToolChoiceWire::Auto(payload)) => {
            validate_parallel_tool_use(payload)?;
            Ok(ToolChoice::Auto)
        }
        Field::Value(ToolChoiceWire::None(EmptyWire {})) => Ok(ToolChoice::None),
        Field::Value(ToolChoiceWire::Any(payload)) => {
            validate_parallel_tool_use(payload)?;
            if tool_names.is_empty() {
                Err(ParseRequestError::InvalidValue)
            } else {
                Ok(ToolChoice::Required)
            }
        }
        Field::Value(ToolChoiceWire::Tool(NamedToolChoiceWire {
            name,
            disable_parallel_tool_use,
        })) => {
            validate_tool_name(&name)?;
            reject_disabled_parallel(disable_parallel_tool_use)?;
            if !tool_names.contains(&name) {
                return Err(ParseRequestError::InvalidValue);
            }
            Ok(ToolChoice::Named { name })
        }
    }
}

fn validate_parallel_tool_use(payload: ToolChoiceParallelWire) -> Result<(), ParseRequestError> {
    reject_disabled_parallel(payload.disable_parallel_tool_use)
}

fn reject_disabled_parallel(value: Field<bool>) -> Result<(), ParseRequestError> {
    if matches!(value, Field::Value(true)) {
        Err(ParseRequestError::UnsupportedFeature)
    } else {
        Ok(())
    }
}

fn convert_messages(
    system: Field<SystemWire>,
    messages: Vec<MessageWire>,
) -> Result<Vec<Message>, ParseRequestError> {
    if messages.is_empty() || messages.len() > MAX_MESSAGES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let mut state = MessageConvertState::default();
    let mut converted = Vec::with_capacity(messages.len() + 1);
    if let Field::Value(system) = system {
        converted.push(Message::new(
            Role::System,
            convert_system(system, &mut state)?,
        ));
    }

    for message in messages {
        match message {
            MessageWire::User(message) => {
                converted.extend(convert_user_message(message, &mut state)?);
            }
            MessageWire::Assistant(message) => {
                if !state.pending_tool_call_ids.is_empty() {
                    return Err(ParseRequestError::InvalidValue);
                }
                converted.push(convert_assistant_message(message, &mut state)?);
            }
        }
        if converted.len() > MAX_NORMALIZED_MESSAGES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
    }
    if !state.pending_tool_call_ids.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(converted)
}

fn convert_system(
    system: SystemWire,
    state: &mut MessageConvertState,
) -> Result<Vec<ContentBlock>, ParseRequestError> {
    match system {
        SystemWire::Text(text) => Ok(vec![state.add_text(text)?]),
        SystemWire::Parts(parts) => {
            validate_parts_len(parts.len())?;
            let mut content = Vec::with_capacity(parts.len() * 2);
            for part in parts {
                let SystemTextBlockWire::Text(block) = part;
                push_text_block(&mut content, block, state)?;
            }
            Ok(content)
        }
    }
}

fn convert_assistant_message(
    message: AssistantMessageWire,
    state: &mut MessageConvertState,
) -> Result<Message, ParseRequestError> {
    let mut content = Vec::new();
    match message.content {
        AssistantContentWire::Text(text) => content.push(state.add_text(text)?),
        AssistantContentWire::Parts(parts) => {
            validate_parts_len(parts.len())?;
            content.reserve(parts.len() * 2);
            for part in parts {
                match part {
                    AssistantContentPartWire::Text(block) => {
                        push_text_block(&mut content, block, state)?;
                    }
                    AssistantContentPartWire::ToolUse(block) => {
                        let (tool_use, cache_control) = convert_tool_use(block, state)?;
                        content.push(tool_use);
                        append_cache_control(&mut content, cache_control, state)?;
                    }
                }
            }
        }
    }
    Ok(Message::new(Role::Assistant, content))
}

fn convert_user_message(
    message: UserMessageWire,
    state: &mut MessageConvertState,
) -> Result<Vec<Message>, ParseRequestError> {
    match message.content {
        UserContentWire::Text(text) => {
            if !state.pending_tool_call_ids.is_empty() {
                return Err(ParseRequestError::InvalidValue);
            }
            Ok(vec![Message::new(Role::User, vec![state.add_text(text)?])])
        }
        UserContentWire::Parts(parts) => convert_user_parts(parts, state),
    }
}

fn convert_user_parts(
    parts: Vec<UserContentPartWire>,
    state: &mut MessageConvertState,
) -> Result<Vec<Message>, ParseRequestError> {
    validate_parts_len(parts.len())?;
    let mut converted = Vec::new();
    let mut user_content = Vec::new();
    let mut saw_regular_content = false;

    for part in parts {
        match part {
            UserContentPartWire::ToolResult(block) if !saw_regular_content => {
                let (tool_result, cache_control) = convert_tool_result(block, state)?;
                let mut content = vec![tool_result];
                append_cache_control(&mut content, cache_control, state)?;
                converted.push(Message::new(Role::Tool, content));
            }
            UserContentPartWire::ToolResult(_) => return Err(ParseRequestError::InvalidValue),
            UserContentPartWire::Text(block) => {
                saw_regular_content = true;
                push_text_block(&mut user_content, block, state)?;
            }
            UserContentPartWire::Image(block) => {
                saw_regular_content = true;
                push_image_block(&mut user_content, block, state)?;
            }
        }
    }

    if !state.pending_tool_call_ids.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }
    if !user_content.is_empty() {
        converted.push(Message::new(Role::User, user_content));
    }
    Ok(converted)
}

fn push_text_block(
    output: &mut Vec<ContentBlock>,
    block: TextBlockWire,
    state: &mut MessageConvertState,
) -> Result<(), ParseRequestError> {
    output.push(state.add_text(block.text)?);
    append_cache_control(output, block.cache_control, state)
}

fn push_image_block(
    output: &mut Vec<ContentBlock>,
    block: ImageBlockWire,
    state: &mut MessageConvertState,
) -> Result<(), ParseRequestError> {
    let cache_control = block.cache_control;
    output.push(convert_image(block.source, state)?);
    append_cache_control(output, cache_control, state)
}

fn convert_tool_use(
    block: ToolUseBlockWire,
    state: &mut MessageConvertState,
) -> Result<(ContentBlock, Field<CacheControlWire>), ParseRequestError> {
    let ToolUseBlockWire {
        id,
        name,
        input,
        cache_control,
    } = block;
    validate_call_id(&id)?;
    validate_tool_name(&name)?;
    if !input.is_object() || !state.seen_tool_call_ids.insert(id.clone()) {
        return Err(ParseRequestError::InvalidValue);
    }
    let argument_nodes = validate_value_shape(&input, 16, 4_096, 1_024)?;
    let argument_bytes = serde_json::to_vec(&input)
        .map_err(|_| ParseRequestError::InvalidValue)?
        .len();
    if argument_bytes > MAX_ARGUMENT_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    state.add_tool_arguments(argument_bytes, argument_nodes)?;
    state.pending_tool_call_ids.insert(id.clone());
    state.add_block()?;
    Ok((
        ContentBlock::ToolUse {
            id,
            name,
            input,
            signature: None,
        },
        cache_control,
    ))
}

fn convert_tool_result(
    block: ToolResultBlockWire,
    state: &mut MessageConvertState,
) -> Result<(ContentBlock, Field<CacheControlWire>), ParseRequestError> {
    let ToolResultBlockWire {
        tool_use_id,
        content,
        is_error,
        cache_control,
    } = block;
    validate_call_id(&tool_use_id)?;
    if !state.pending_tool_call_ids.remove(&tool_use_id) {
        return Err(ParseRequestError::InvalidValue);
    }

    let content = match content {
        Field::Missing => Vec::new(),
        Field::Value(ToolResultContentWire::Text(text)) => vec![state.add_text(text)?],
        Field::Value(ToolResultContentWire::Parts(parts)) => {
            if parts.len() > MAX_PARTS_PER_MESSAGE {
                return Err(ParseRequestError::StructureLimitExceeded);
            }
            let mut content = Vec::with_capacity(parts.len() * 2);
            for part in parts {
                match part {
                    ToolResultContentPartWire::Text(block) => {
                        push_text_block(&mut content, block, state)?;
                    }
                    ToolResultContentPartWire::Image(block) => {
                        push_image_block(&mut content, block, state)?;
                    }
                }
            }
            content
        }
    };
    let is_error = field_value(is_error).unwrap_or(false);
    state.add_block()?;
    Ok((
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content: None,
            is_error,
        },
        cache_control,
    ))
}

fn convert_image(
    source: ImageSourceWire,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let (source, mime_type) = match source {
        ImageSourceWire::Base64(Base64ImageSourceWire { media_type, data }) => {
            validate_base64(&data, state)?;
            let mime_type = match media_type {
                ImageMediaTypeWire::Jpeg => "image/jpeg",
                ImageMediaTypeWire::Png => "image/png",
                ImageMediaTypeWire::Gif => "image/gif",
                ImageMediaTypeWire::Webp => "image/webp",
            };
            (MediaSource::Base64(data), Some(mime_type.to_owned()))
        }
        ImageSourceWire::Url(UrlImageSourceWire { url }) => {
            if url.len() > MAX_MEDIA_URL_BYTES {
                return Err(ParseRequestError::StructureLimitExceeded);
            }
            (MediaSource::Url(normalize_remote_media_url(&url)?), None)
        }
    };
    state.add_block()?;
    Ok(ContentBlock::Image { source, mime_type })
}

fn append_cache_control(
    output: &mut Vec<ContentBlock>,
    cache_control: Field<CacheControlWire>,
    state: &mut MessageConvertState,
) -> Result<(), ParseRequestError> {
    let Field::Value(CacheControlWire { kind, ttl }) = cache_control else {
        return Ok(());
    };
    let CacheControlTypeWire::Ephemeral = kind;
    let hint = match ttl {
        Field::Missing | Field::Value(CacheTtlWire::FiveMinutes) => CacheHint::Ephemeral5Minutes,
        Field::Value(CacheTtlWire::OneHour) => CacheHint::Ephemeral1Hour,
    };
    output.push(state.add_cache_control(hint)?);
    Ok(())
}

fn validate_base64(
    encoded: &str,
    state: &mut MessageConvertState,
) -> Result<(), ParseRequestError> {
    let decoded_bytes = decoded_base64_len(encoded)?;
    state.add_media_bytes(decoded_bytes)
}

pub(super) fn decoded_base64_len(encoded: &str) -> Result<usize, ParseRequestError> {
    if encoded.is_empty() || !encoded.len().is_multiple_of(4) {
        return Err(ParseRequestError::InvalidValue);
    }
    let padding = if encoded.ends_with("==") {
        2
    } else if encoded.ends_with('=') {
        1
    } else {
        0
    };
    let estimated_bytes = encoded
        .len()
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|bytes| bytes.checked_sub(padding))
        .ok_or(ParseRequestError::StructureLimitExceeded)?;
    if estimated_bytes > MAX_MEDIA_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| ParseRequestError::InvalidValue)?;
    if decoded.len() != estimated_bytes {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(decoded.len())
}

pub(super) fn normalize_remote_media_url(value: &str) -> Result<String, ParseRequestError> {
    if value.trim() != value || value.contains('\\') {
        return Err(ParseRequestError::InvalidValue);
    }
    let url = Url::parse(value).map_err(|_| ParseRequestError::InvalidValue)?;
    if url.scheme() != "https"
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(url.to_string())
}

fn validate_parts_len(parts: usize) -> Result<(), ParseRequestError> {
    if parts == 0 || parts > MAX_PARTS_PER_MESSAGE {
        Err(ParseRequestError::StructureLimitExceeded)
    } else {
        Ok(())
    }
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

pub(super) fn validate_call_id(id: &str) -> Result<(), ParseRequestError> {
    if id.is_empty() || id.len() > MAX_TOOL_CALL_ID_BYTES || id.chars().any(char::is_control) {
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

#[derive(Default)]
struct MessageConvertState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    cache_breakpoints: usize,
    seen_tool_call_ids: HashSet<String>,
    pending_tool_call_ids: HashSet<String>,
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
        self.add_block()?;
        Ok(ContentBlock::Text(text))
    }

    fn add_media_bytes(&mut self, bytes: usize) -> Result<(), ParseRequestError> {
        if bytes == 0 {
            return Err(ParseRequestError::InvalidValue);
        }
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_tool_arguments(&mut self, bytes: usize, nodes: usize) -> Result<(), ParseRequestError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES
        {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_cache_control(&mut self, hint: CacheHint) -> Result<ContentBlock, ParseRequestError> {
        self.cache_breakpoints = self
            .cache_breakpoints
            .checked_add(1)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.cache_breakpoints > MAX_CACHE_BREAKPOINTS {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        self.add_block()?;
        Ok(ContentBlock::CacheControl(hint))
    }
}
