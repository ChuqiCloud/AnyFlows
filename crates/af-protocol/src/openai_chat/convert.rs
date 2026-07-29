use std::collections::HashSet;

use af_domain::{MAX_MODEL_NAME_BYTES, Operation, Protocol, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};
use url::Url;

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    wire::{
        AssistantContentWire, AssistantMessageWire, ChatRequestWire, DeveloperMessageWire, Field,
        FunctionDefinitionWire, FunctionTypeWire, ImageDetailWire, InputAudioFormatWire,
        MessageWire, NamedFunctionWire, NamedToolChoiceWire, ReasoningEffortWire, StopWire,
        StreamOptionsWire, SystemMessageWire, TextContentWire, TextPartWire, ToolCallWire,
        ToolChoiceModeWire, ToolChoiceWire, ToolMessageWire, ToolWire, UserContentPartWire,
        UserContentWire, UserMessageWire,
    },
};
use crate::bounded_json::{BoundedJsonError, JsonLimits, parse_value};
use crate::{
    CanonicalRequest, ContentBlock, MediaSource, Message, ReasoningConfig, ReasoningEffort,
    RequestMetadata, Sampling, StreamOptions, TokenCount, ToolChoice, ToolDef,
};

pub(super) const MAX_MESSAGES: usize = 256;
pub(super) const MAX_PARTS_PER_MESSAGE: usize = 128;
pub(super) const MAX_CONTENT_BLOCKS: usize = 1_024;
pub(super) const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_TOTAL_TEXT_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_TOOLS: usize = 128;
pub(super) const MAX_TOOL_CALLS: usize = 128;
pub(super) const MAX_TOOL_DESCRIPTION_BYTES: usize = 8 * 1024;
const MAX_TOOL_CALL_ID_BYTES: usize = 256;
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
const MAX_RAW_FIELDS: usize = 32;
const MAX_RAW_BYTES: usize = 64 * 1024;
const MAX_RAW_DEPTH: usize = 8;
const MAX_RAW_NODES: usize = 4_096;
const MAX_RAW_OBJECT_ENTRIES: usize = 256;
const MAX_RAW_KEY_BYTES: usize = 128;
const MAX_METADATA_FIELDS: usize = 16;
const MAX_METADATA_KEY_BYTES: usize = 64;
const MAX_METADATA_VALUE_BYTES: usize = 512;

pub(super) const ARGUMENT_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

const RAW_FIELD_ALLOWLIST: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "parallel_tool_calls",
    "presence_penalty",
    "prompt_cache_key",
    "prompt_cache_retention",
    "safety_identifier",
    "seed",
    "service_tier",
    "store",
    "top_logprobs",
    "verbosity",
];

const UNSUPPORTED_TOP_LEVEL_FIELDS: &[&str] = &[
    "audio",
    "function_call",
    "functions",
    "modalities",
    "moderation",
    "prediction",
    "prompt_cache_breakpoint",
    "prompt_cache_options",
    "response_format",
    "web_search_options",
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
    wire: ChatRequestWire,
) -> Result<CanonicalRequest, ParseRequestError> {
    validate_model(&wire.model)?;
    let raw = validate_raw(wire.extra)?;
    let (tools, tool_names) = convert_tools(wire.tools)?;
    let tool_choice = convert_tool_choice(wire.tool_choice, &tool_names)?;
    let reasoning = convert_reasoning(wire.reasoning_effort)?;
    let sampling = convert_sampling(
        wire.temperature,
        wire.top_p,
        wire.max_tokens,
        wire.max_completion_tokens,
        wire.stop,
    )?;
    let stream = convert_stream(wire.stream)?;
    let stream_options = convert_stream_options(wire.stream_options, stream)?;
    let metadata = convert_metadata(wire.user)?;
    let messages = convert_messages(wire.messages)?;

    let mut request = CanonicalRequest::new(Operation::Chat, wire.model, messages, stream);
    request.tools = tools;
    request.tool_choice = tool_choice;
    request.reasoning = reasoning;
    request.sampling = sampling;
    request.stream_options = stream_options;
    request.metadata = metadata;
    Ok(request.with_validated_raw_passthrough(raw.map(|fields| (Protocol::OpenAiChat, fields))))
}

fn convert_stream_options(
    options: Field<StreamOptionsWire>,
    stream: bool,
) -> Result<StreamOptions, ParseRequestError> {
    let Field::Value(options) = options else {
        return Ok(StreamOptions::EMPTY);
    };
    if !stream {
        return Err(ParseRequestError::InvalidValue);
    }
    let include_usage = match options.include_usage {
        Field::Missing | Field::Value(false) => false,
        Field::Value(true) => true,
    };
    Ok(StreamOptions::new(include_usage))
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

fn convert_stream(stream: Field<bool>) -> Result<bool, ParseRequestError> {
    match stream {
        Field::Missing | Field::Value(false) => Ok(false),
        Field::Value(true) => Ok(true),
    }
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

fn convert_reasoning(
    effort: Field<ReasoningEffortWire>,
) -> Result<Option<ReasoningConfig>, ParseRequestError> {
    let effort = match effort {
        Field::Missing => return Ok(None),
        Field::Value(ReasoningEffortWire::None) => ReasoningEffort::None,
        Field::Value(ReasoningEffortWire::Minimal) => ReasoningEffort::Minimal,
        Field::Value(ReasoningEffortWire::Low) => ReasoningEffort::Low,
        Field::Value(ReasoningEffortWire::Medium) => ReasoningEffort::Medium,
        Field::Value(ReasoningEffortWire::High) => ReasoningEffort::High,
        Field::Value(ReasoningEffortWire::ExtraHigh) => ReasoningEffort::ExtraHigh,
        Field::Value(ReasoningEffortWire::Max) => ReasoningEffort::Max,
    };
    ReasoningConfig::new(Some(effort), None, false)
        .map(Some)
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn convert_sampling(
    temperature: Field<f64>,
    top_p: Field<f64>,
    max_tokens: Field<i64>,
    max_completion_tokens: Field<i64>,
    stop: Field<StopWire>,
) -> Result<Sampling, ParseRequestError> {
    let temperature = field_value(temperature);
    if temperature.is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }

    let top_p = field_value(top_p);
    if top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(ParseRequestError::InvalidValue);
    }

    let max_output_tokens = match (max_tokens, max_completion_tokens) {
        (Field::Value(_), Field::Value(_)) => {
            return Err(ParseRequestError::ConflictingParameters);
        }
        (Field::Value(value), Field::Missing) | (Field::Missing, Field::Value(value)) => {
            if !(1..=MAX_OUTPUT_TOKENS).contains(&value) {
                return Err(ParseRequestError::InvalidValue);
            }
            Some(TokenCount::new(value).map_err(|_| ParseRequestError::InvalidValue)?)
        }
        (Field::Missing, Field::Missing) => None,
    };

    let stop_sequences = match stop {
        Field::Missing => Vec::new(),
        Field::Value(StopWire::One(value)) => vec![value],
        Field::Value(StopWire::Many(values)) => {
            if values.is_empty() || values.len() > MAX_STOP_SEQUENCES {
                return Err(ParseRequestError::InvalidValue);
            }
            values
        }
    };
    if stop_sequences
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_STOP_BYTES)
    {
        return Err(ParseRequestError::InvalidValue);
    }

    Sampling::new(temperature, top_p, max_output_tokens, stop_sequences)
        .map_err(|_| ParseRequestError::InvalidValue)
}

fn field_value<T>(field: Field<T>) -> Option<T> {
    match field {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    }
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
        let ToolWire::Function(payload) = tool;
        let FunctionDefinitionWire {
            name,
            description,
            parameters,
            strict,
        } = payload.function;

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
        let strict = field_value(strict);

        let input_schema = field_value(parameters).unwrap_or_else(|| Value::Object(Map::new()));
        validate_schema(&input_schema, &mut total_schema_bytes)?;
        converted.push(ToolDef {
            name,
            description,
            input_schema,
            strict,
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
            function: NamedFunctionWire { name },
        })) => {
            validate_tool_name(&name)?;
            if !tool_names.contains(&name) {
                return Err(ParseRequestError::InvalidValue);
            }
            Ok(ToolChoice::Named { name })
        }
    }
}

fn convert_messages(messages: Vec<MessageWire>) -> Result<Vec<Message>, ParseRequestError> {
    if messages.is_empty() || messages.len() > MAX_MESSAGES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let mut state = MessageConvertState::default();
    let mut converted = Vec::with_capacity(messages.len());
    for message in messages {
        if !state.pending_tool_call_ids.is_empty() && !matches!(&message, MessageWire::Tool(_)) {
            return Err(ParseRequestError::InvalidValue);
        }
        converted.push(match message {
            MessageWire::System(SystemMessageWire { content }) => {
                Message::new(Role::System, convert_text_content(content, &mut state)?)
            }
            MessageWire::Developer(DeveloperMessageWire { content }) => {
                Message::new(Role::Developer, convert_text_content(content, &mut state)?)
            }
            MessageWire::User(UserMessageWire { content }) => {
                Message::new(Role::User, convert_user_content(content, &mut state)?)
            }
            MessageWire::Assistant(message) => convert_assistant_message(message, &mut state)?,
            MessageWire::Tool(message) => convert_tool_message(message, &mut state)?,
        });
    }
    if !state.pending_tool_call_ids.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(converted)
}

#[derive(Default)]
struct MessageConvertState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
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
        if bytes == 0 || bytes > MAX_MEDIA_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
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
}

fn convert_text_content(
    content: TextContentWire,
    state: &mut MessageConvertState,
) -> Result<Vec<ContentBlock>, ParseRequestError> {
    match content {
        TextContentWire::Text(text) => Ok(vec![state.add_text(text)?]),
        TextContentWire::Parts(parts) => {
            validate_parts_len(parts.len())?;
            parts
                .into_iter()
                .map(|part| {
                    let TextPartWire::Text(payload) = part;
                    state.add_text(payload.text)
                })
                .collect()
        }
    }
}

fn convert_user_content(
    content: UserContentWire,
    state: &mut MessageConvertState,
) -> Result<Vec<ContentBlock>, ParseRequestError> {
    match content {
        UserContentWire::Text(text) => Ok(vec![state.add_text(text)?]),
        UserContentWire::Parts(parts) => {
            validate_parts_len(parts.len())?;
            parts
                .into_iter()
                .map(|part| match part {
                    UserContentPartWire::Text(payload) => state.add_text(payload.text),
                    UserContentPartWire::ImageUrl(payload) => {
                        convert_image(payload.image_url, state)
                    }
                    UserContentPartWire::InputAudio(payload) => {
                        convert_audio(payload.input_audio, state)
                    }
                })
                .collect()
        }
    }
}

fn validate_parts_len(parts: usize) -> Result<(), ParseRequestError> {
    if parts == 0 || parts > MAX_PARTS_PER_MESSAGE {
        Err(ParseRequestError::StructureLimitExceeded)
    } else {
        Ok(())
    }
}

fn convert_assistant_message(
    message: AssistantMessageWire,
    state: &mut MessageConvertState,
) -> Result<Message, ParseRequestError> {
    let mut content = match message.content {
        None => Vec::new(),
        Some(AssistantContentWire::Text(text)) => vec![state.add_text(text)?],
        Some(AssistantContentWire::Parts(parts)) => {
            validate_parts_len(parts.len())?;
            parts
                .into_iter()
                .map(|part| {
                    let TextPartWire::Text(payload) = part;
                    state.add_text(payload.text)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };

    let tool_calls = match message.tool_calls {
        Field::Missing => Vec::new(),
        Field::Value(tool_calls) => tool_calls,
    };
    if tool_calls.len() > MAX_TOOL_CALLS
        || content
            .len()
            .checked_add(tool_calls.len())
            .is_none_or(|parts| parts > MAX_PARTS_PER_MESSAGE)
    {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    if content.is_empty() && tool_calls.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }

    for tool_call in tool_calls {
        content.push(convert_tool_call(tool_call, state)?);
    }
    Ok(Message::new(Role::Assistant, content))
}

fn convert_tool_call(
    tool_call: ToolCallWire,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let ToolCallWire::Function(payload) = tool_call;
    validate_call_id(&payload.id)?;
    validate_tool_name(&payload.function.name)?;
    if !state.seen_tool_call_ids.insert(payload.id.clone()) {
        return Err(ParseRequestError::InvalidValue);
    }
    state.pending_tool_call_ids.insert(payload.id.clone());

    if payload.function.arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    let input = parse_value(payload.function.arguments.as_bytes(), ARGUMENT_JSON_LIMITS)
        .map_err(map_argument_error)?;
    if !input.is_object() {
        return Err(ParseRequestError::InvalidValue);
    }
    let argument_nodes = validate_value_shape(&input, 16, 4_096, 1_024)?;
    state.add_tool_arguments(payload.function.arguments.len(), argument_nodes)?;
    state.add_block()?;
    Ok(ContentBlock::ToolUse {
        id: payload.id,
        name: payload.function.name,
        input,
        signature: None,
    })
}

fn map_argument_error(error: BoundedJsonError) -> ParseRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseRequestError::InvalidValue,
        BoundedJsonError::DuplicateKey => ParseRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
    }
}

pub(super) fn validate_call_id(id: &str) -> Result<(), ParseRequestError> {
    if id.is_empty() || id.len() > MAX_TOOL_CALL_ID_BYTES || id.chars().any(char::is_control) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn convert_tool_message(
    message: ToolMessageWire,
    state: &mut MessageConvertState,
) -> Result<Message, ParseRequestError> {
    validate_call_id(&message.tool_call_id)?;
    if !state.pending_tool_call_ids.remove(&message.tool_call_id) {
        return Err(ParseRequestError::InvalidValue);
    }
    let result_content = convert_text_content(message.content, state)?;
    state.add_block()?;
    Ok(Message::new(
        Role::Tool,
        vec![ContentBlock::ToolResult {
            tool_use_id: message.tool_call_id,
            content: result_content,
            structured_content: None,
            is_error: false,
        }],
    ))
}

fn convert_image(
    image: super::wire::ImageUrlWire,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    if matches!(
        image.detail,
        Field::Value(ImageDetailWire::Low | ImageDetailWire::High)
    ) {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    if image.url.chars().any(char::is_control) {
        return Err(ParseRequestError::InvalidValue);
    }

    let (source, mime_type) = if let Some(data_url) = image.url.strip_prefix("data:") {
        let (media_type, payload) = data_url
            .split_once(',')
            .ok_or(ParseRequestError::InvalidValue)?;
        let mime_type = media_type
            .strip_suffix(";base64")
            .ok_or(ParseRequestError::InvalidValue)?;
        if !matches!(
            mime_type,
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            return Err(ParseRequestError::UnsupportedFeature);
        }
        validate_base64(payload, state)?;
        (
            MediaSource::Base64(payload.to_owned()),
            Some(mime_type.to_owned()),
        )
    } else {
        if image.url.len() > MAX_MEDIA_URL_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        let url = normalize_remote_media_url(&image.url)?;
        (MediaSource::Url(url), None)
    };
    state.add_block()?;
    Ok(ContentBlock::Image { source, mime_type })
}

fn convert_audio(
    audio: super::wire::InputAudioWire,
    state: &mut MessageConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    validate_base64(&audio.data, state)?;
    let mime_type = match audio.format {
        InputAudioFormatWire::Wav => "audio/wav",
        InputAudioFormatWire::Mp3 => "audio/mpeg",
    };
    state.add_block()?;
    Ok(ContentBlock::Audio {
        source: MediaSource::Base64(audio.data),
        mime_type: mime_type.to_owned(),
    })
}

fn validate_base64(
    encoded: &str,
    state: &mut MessageConvertState,
) -> Result<(), ParseRequestError> {
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
    if estimated_bytes == 0 || estimated_bytes > MAX_MEDIA_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }

    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| ParseRequestError::InvalidValue)?;
    if decoded.len() != estimated_bytes {
        return Err(ParseRequestError::InvalidValue);
    }
    state.add_media_bytes(decoded.len())
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

fn validate_raw(
    extra: Map<String, Value>,
) -> Result<Option<Map<String, Value>>, ParseRequestError> {
    if extra.is_empty() {
        return Ok(None);
    }
    if extra.len() > MAX_RAW_FIELDS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    for key in extra.keys() {
        if UNSUPPORTED_TOP_LEVEL_FIELDS.contains(&key.as_str()) {
            return Err(ParseRequestError::UnsupportedFeature);
        }
        if !RAW_FIELD_ALLOWLIST.contains(&key.as_str()) {
            return Err(ParseRequestError::InvalidValue);
        }
    }
    validate_raw_fields(&extra)?;
    Ok(Some(extra))
}

/// 校验待合并到 OpenAI Chat 请求的同源 raw 字段。
pub(super) fn validate_raw_fields(extra: &Map<String, Value>) -> Result<(), ParseRequestError> {
    if extra.len() > MAX_RAW_FIELDS {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    for key in extra.keys() {
        if UNSUPPORTED_TOP_LEVEL_FIELDS.contains(&key.as_str()) {
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
    validate_raw_values(extra)
}

fn validate_raw_semantics(extra: &Map<String, Value>) -> Result<(), ParseRequestError> {
    for (key, value) in extra {
        match key.as_str() {
            "frequency_penalty" | "presence_penalty" => {
                let value = value.as_f64().ok_or(ParseRequestError::InvalidValue)?;
                if !value.is_finite() || !(-2.0..=2.0).contains(&value) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "logit_bias" => validate_logit_bias(value)?,
            "logprobs" | "parallel_tool_calls" | "store" => {
                if !value.is_boolean() {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "metadata" => validate_raw_metadata(value)?,
            "n" => {
                if !matches!(value.as_u64(), Some(1..=128)) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "prompt_cache_key" => validate_bounded_raw_string(value, 256)?,
            "prompt_cache_retention" => {
                if !matches!(value.as_str(), Some("in_memory" | "24h")) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "safety_identifier" => validate_bounded_raw_string(value, 64)?,
            "seed" => {
                if value.as_i64().is_none() {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "service_tier" => {
                if !matches!(
                    value.as_str(),
                    Some("auto" | "default" | "flex" | "scale" | "priority")
                ) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "top_logprobs" => {
                if !matches!(value.as_u64(), Some(0..=20)) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            "verbosity" => {
                if !matches!(value.as_str(), Some("low" | "medium" | "high")) {
                    return Err(ParseRequestError::InvalidValue);
                }
            }
            _ => return Err(ParseRequestError::InvalidValue),
        }
    }
    if extra.contains_key("top_logprobs") && extra.get("logprobs") != Some(&Value::Bool(true)) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn validate_logit_bias(value: &Value) -> Result<(), ParseRequestError> {
    let biases = value.as_object().ok_or(ParseRequestError::InvalidValue)?;
    for (token, bias) in biases {
        if token.parse::<u32>().is_err()
            || bias
                .as_f64()
                .is_none_or(|bias| !bias.is_finite() || !(-100.0..=100.0).contains(&bias))
        {
            return Err(ParseRequestError::InvalidValue);
        }
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
