use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_MEDIA_BYTES, MAX_MEDIA_URL_BYTES, MAX_MESSAGES,
        MAX_PARTS_PER_MESSAGE, MAX_STOP_BYTES, MAX_STOP_SEQUENCES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
        MAX_TOOL_DESCRIPTION_BYTES, MAX_TOOLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES,
        MAX_TOTAL_TEXT_BYTES, normalize_remote_media_url, validate_call_id, validate_model,
        validate_raw_fields, validate_schema, validate_tool_name, validate_value_shape,
    },
};
use crate::bounded_json::validate_object;
use crate::{
    CanonicalRequest, ContentBlock, MediaSource, Message, ReasoningEffort, ToolChoice, ToolDef,
    UnsupportedCapability, validate_request_capabilities,
};

/// 将 Canonical 请求构造为 OpenAI Chat Completions JSON。
///
/// Canonical 字段可由调用方直接构造，因此本函数会重新校验所有协议边界，
/// 并在合并同源 raw 前再次检查字段冲突。
pub fn build_request(request: &CanonicalRequest) -> Result<Value, BuildRequestError> {
    if request.operation != Operation::Chat {
        return Err(BuildRequestError::UnsupportedOperation);
    }
    validate_request_capabilities(Protocol::OpenAiChat, request)
        .map_err(BuildRequestError::UnsupportedCapability)?;
    validate_model(&request.model).map_err(map_request_validation_error)?;
    if !request.attachments.is_empty() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    if !request.continuation.is_empty() {
        return Err(BuildRequestError::UnsupportedFeature);
    }

    let tools = build_tools(&request.tools)?;
    let tool_names = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<HashSet<_>>();
    let tool_choice = build_tool_choice(&request.tool_choice, &tool_names)?;
    let messages = build_messages(&request.messages)?;

    let mut root = Map::new();
    root.insert("model".to_owned(), Value::String(request.model.clone()));
    root.insert("messages".to_owned(), Value::Array(messages));
    root.insert("stream".to_owned(), Value::Bool(request.stream));
    if request.stream_options.include_usage() {
        if !request.stream {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "stream_options".to_owned(),
            Value::Object(Map::from_iter([(
                "include_usage".to_owned(),
                Value::Bool(true),
            )])),
        );
    }
    if !tools.is_empty() {
        root.insert("tools".to_owned(), Value::Array(tools));
    }
    root.insert("tool_choice".to_owned(), tool_choice);
    insert_reasoning(&mut root, request)?;
    insert_sampling(&mut root, request)?;
    insert_metadata(&mut root, request)?;
    merge_raw(&mut root, request)?;
    validate_final_request(&root)?;
    Ok(Value::Object(root))
}

fn validate_final_request(root: &Map<String, Value>) -> Result<(), BuildRequestError> {
    validate_object(root, super::REQUEST_JSON_LIMITS, super::MAX_BODY_BYTES)
        .map_err(|_| BuildRequestError::StructureLimitExceeded)
}

fn build_tools(tools: &[ToolDef]) -> Result<Vec<Value>, BuildRequestError> {
    if tools.len() > MAX_TOOLS {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut names = HashSet::with_capacity(tools.len());
    let mut total_schema_bytes = 0_usize;
    let mut encoded = Vec::with_capacity(tools.len());
    for tool in tools {
        validate_tool_name(&tool.name).map_err(map_request_validation_error)?;
        if !names.insert(tool.name.as_str()) {
            return Err(BuildRequestError::InvalidValue);
        }
        if tool
            .description
            .as_ref()
            .is_some_and(|description| description.len() > MAX_TOOL_DESCRIPTION_BYTES)
        {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        validate_schema(&tool.input_schema, &mut total_schema_bytes)
            .map_err(map_request_validation_error)?;
        let mut function = Map::new();
        function.insert("name".to_owned(), Value::String(tool.name.clone()));
        if let Some(description) = &tool.description {
            function.insert("description".to_owned(), Value::String(description.clone()));
        }
        function.insert("parameters".to_owned(), tool.input_schema.clone());
        if let Some(strict) = tool.strict {
            function.insert("strict".to_owned(), Value::Bool(strict));
        }

        let mut wrapper = Map::new();
        wrapper.insert("type".to_owned(), Value::String("function".to_owned()));
        wrapper.insert("function".to_owned(), Value::Object(function));
        encoded.push(Value::Object(wrapper));
    }
    Ok(encoded)
}

fn build_tool_choice(
    choice: &ToolChoice,
    tool_names: &HashSet<&str>,
) -> Result<Value, BuildRequestError> {
    if tool_names.is_empty() {
        return match choice {
            ToolChoice::Auto => Ok(Value::String("auto".to_owned())),
            ToolChoice::None => Ok(Value::String("none".to_owned())),
            ToolChoice::Required | ToolChoice::Named { .. } => Err(BuildRequestError::InvalidValue),
        };
    }

    match choice {
        ToolChoice::Auto => Ok(Value::String("auto".to_owned())),
        ToolChoice::None => Ok(Value::String("none".to_owned())),
        ToolChoice::Required => Ok(Value::String("required".to_owned())),
        ToolChoice::Named { name } => {
            validate_tool_name(name).map_err(map_request_validation_error)?;
            if !tool_names.contains(name.as_str()) {
                return Err(BuildRequestError::InvalidValue);
            }
            let mut function = Map::new();
            function.insert("name".to_owned(), Value::String(name.clone()));
            let mut named = Map::new();
            named.insert("type".to_owned(), Value::String("function".to_owned()));
            named.insert("function".to_owned(), Value::Object(function));
            Ok(Value::Object(named))
        }
    }
}

fn insert_reasoning(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(reasoning) = request.reasoning else {
        return Ok(());
    };
    if reasoning.budget_tokens().is_some() || reasoning.include_thinking() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    let Some(effort) = reasoning.effort() else {
        return Err(BuildRequestError::InvalidValue);
    };
    let value = match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::ExtraHigh => "xhigh",
        ReasoningEffort::Max => "max",
    };
    root.insert(
        "reasoning_effort".to_owned(),
        Value::String(value.to_owned()),
    );
    Ok(())
}

fn insert_sampling(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    if let Some(temperature) = request.sampling.temperature() {
        if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "temperature".to_owned(),
            number_value(temperature).ok_or(BuildRequestError::InvalidValue)?,
        );
    }
    if let Some(top_p) = request.sampling.top_p() {
        if !top_p.is_finite() || !(0.0..=1.0).contains(&top_p) {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "top_p".to_owned(),
            number_value(top_p).ok_or(BuildRequestError::InvalidValue)?,
        );
    }
    if let Some(tokens) = request.sampling.max_output_tokens() {
        if !(1..=MAX_OUTPUT_TOKENS).contains(&tokens.get()) {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "max_completion_tokens".to_owned(),
            Value::Number(tokens.get().into()),
        );
    }

    let stop = request.sampling.stop_sequences();
    if stop.len() > MAX_STOP_SEQUENCES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    if stop
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_STOP_BYTES)
    {
        return Err(BuildRequestError::InvalidValue);
    }
    if !stop.is_empty() {
        root.insert(
            "stop".to_owned(),
            Value::Array(stop.iter().cloned().map(Value::String).collect()),
        );
    }
    Ok(())
}

fn insert_metadata(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    if request.metadata.session_id().is_some() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    if let Some(user_id) = request.metadata.user_id() {
        if user_id.is_empty()
            || user_id.len() > super::convert::MAX_USER_ID_BYTES
            || user_id.chars().any(char::is_control)
        {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert("user".to_owned(), Value::String(user_id.to_owned()));
    }
    Ok(())
}

fn merge_raw(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(raw) = request.raw_passthrough() else {
        return Ok(());
    };
    let fields = raw
        .fields_for_protocol(Protocol::OpenAiChat)
        .map_err(|_| BuildRequestError::RawProtocolMismatch)?;
    for key in fields.keys() {
        if root.contains_key(key) {
            return Err(BuildRequestError::FieldConflict);
        }
    }
    validate_raw_fields(fields).map_err(map_request_validation_error)?;
    validate_response_safe_raw_fields(fields)?;
    for (key, value) in fields {
        root.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn validate_response_safe_raw_fields(fields: &Map<String, Value>) -> Result<(), BuildRequestError> {
    // 这些请求选项会触发当前响应 IR 无法保存的 logprobs 或缓存写入明细，
    // 为避免出站成功后在响应边界必然失败，这里先明确拒绝。
    for key in fields.keys() {
        if matches!(
            key.as_str(),
            "logprobs" | "top_logprobs" | "prompt_cache_key" | "prompt_cache_retention"
        ) {
            return Err(BuildRequestError::UnsupportedFeature);
        }
    }
    Ok(())
}

fn build_messages(messages: &[Message]) -> Result<Vec<Value>, BuildRequestError> {
    if messages.is_empty() || messages.len() > MAX_MESSAGES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut state = MessageBuildState::default();
    let mut encoded = Vec::with_capacity(messages.len());
    for message in messages {
        if !state.pending_tool_call_ids.is_empty() && message.role != Role::Tool {
            return Err(BuildRequestError::InvalidValue);
        }
        encoded.push(match message.role {
            Role::System => build_text_message("system", &message.content, &mut state)?,
            Role::Developer => build_text_message("developer", &message.content, &mut state)?,
            Role::User => build_user_message(&message.content, &mut state)?,
            Role::Assistant => build_assistant_message(&message.content, &mut state)?,
            Role::Tool => build_tool_message(&message.content, &mut state)?,
        });
    }
    if !state.pending_tool_call_ids.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok(encoded)
}

#[derive(Default)]
struct MessageBuildState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    seen_tool_call_ids: HashSet<String>,
    pending_tool_call_ids: HashSet<String>,
}

impl MessageBuildState {
    fn add_block(&mut self) -> Result<(), BuildRequestError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text(&mut self, text: &str) -> Result<(), BuildRequestError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_media(&mut self, bytes: usize) -> Result<(), BuildRequestError> {
        if bytes == 0 || bytes > MAX_MEDIA_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_tool_arguments(&mut self, bytes: usize, nodes: usize) -> Result<(), BuildRequestError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES
        {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.add_block()
    }
}

fn build_text_message(
    role: &str,
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let content = build_text_content(blocks, state)?;
    Ok(message_value(role, content, None, None))
}

fn build_text_content(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    if blocks.is_empty() || blocks.len() > MAX_PARTS_PER_MESSAGE {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut texts = Vec::with_capacity(blocks.len());
    for block in blocks {
        let ContentBlock::Text(text) = block else {
            return Err(BuildRequestError::UnsupportedFeature);
        };
        state.add_text(text)?;
        texts.push(text.clone());
    }
    Ok(text_content_value(texts))
}

fn build_user_message(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    if blocks.is_empty() || blocks.len() > MAX_PARTS_PER_MESSAGE {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    if let [ContentBlock::Text(text)] = blocks {
        state.add_text(text)?;
        return Ok(message_value(
            "user",
            Value::String(text.clone()),
            None,
            None,
        ));
    }

    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        parts.push(match block {
            ContentBlock::Text(text) => {
                state.add_text(text)?;
                text_part_value(text)
            }
            ContentBlock::Image { source, mime_type } => {
                build_image_part(source, mime_type.as_deref(), state)?
            }
            ContentBlock::Audio { source, mime_type } => {
                build_audio_part(source, mime_type, state)?
            }
            ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        });
    }
    Ok(message_value("user", Value::Array(parts), None, None))
}

fn build_assistant_message(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    if blocks.is_empty() || blocks.len() > MAX_PARTS_PER_MESSAGE {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut saw_tool_call = false;
    for block in blocks {
        match block {
            ContentBlock::Text(text) if !saw_tool_call => {
                state.add_text(text)?;
                texts.push(text.clone());
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => {
                if signature.is_some() {
                    return Err(BuildRequestError::UnsupportedFeature);
                }
                saw_tool_call = true;
                tool_calls.push(build_tool_call(id, name, input, state)?);
            }
            ContentBlock::Text(_)
            | ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    if texts.is_empty() && tool_calls.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    let content = if texts.is_empty() {
        Value::Null
    } else {
        text_content_value(texts)
    };
    let tool_calls = (!tool_calls.is_empty()).then_some(tool_calls);
    Ok(message_value("assistant", content, tool_calls, None))
}

fn build_tool_call(
    id: &str,
    name: &str,
    input: &Value,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    validate_call_id(id).map_err(map_request_validation_error)?;
    validate_tool_name(name).map_err(map_request_validation_error)?;
    if !state.seen_tool_call_ids.insert(id.to_owned()) {
        return Err(BuildRequestError::InvalidValue);
    }
    state.pending_tool_call_ids.insert(id.to_owned());
    if !input.is_object() {
        return Err(BuildRequestError::InvalidValue);
    }
    let nodes =
        validate_value_shape(input, 16, 4_096, 1_024).map_err(map_request_validation_error)?;
    let arguments = serde_json::to_string(input).map_err(|_| BuildRequestError::InvalidValue)?;
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    state.add_tool_arguments(arguments.len(), nodes)?;

    let mut function = Map::new();
    function.insert("name".to_owned(), Value::String(name.to_owned()));
    function.insert("arguments".to_owned(), Value::String(arguments));
    let mut call = Map::new();
    call.insert("id".to_owned(), Value::String(id.to_owned()));
    call.insert("type".to_owned(), Value::String("function".to_owned()));
    call.insert("function".to_owned(), Value::Object(function));
    Ok(Value::Object(call))
}

fn build_tool_message(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let [
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content,
            is_error,
        },
    ] = blocks
    else {
        return Err(BuildRequestError::InvalidValue);
    };
    if *is_error || structured_content.is_some() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    validate_call_id(tool_use_id).map_err(map_request_validation_error)?;
    if !state.pending_tool_call_ids.remove(tool_use_id) {
        return Err(BuildRequestError::InvalidValue);
    }
    state.add_block()?;
    let content = build_text_content(content, state)?;
    Ok(message_value(
        "tool",
        content,
        None,
        Some(tool_use_id.clone()),
    ))
}

fn build_image_part(
    source: &MediaSource,
    mime_type: Option<&str>,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let url = match source {
        MediaSource::Url(url) => {
            if mime_type.is_some() {
                return Err(BuildRequestError::UnsupportedFeature);
            }
            if url.len() > MAX_MEDIA_URL_BYTES {
                return Err(BuildRequestError::StructureLimitExceeded);
            }
            let normalized =
                normalize_remote_media_url(url).map_err(map_request_validation_error)?;
            if normalized.len() > MAX_MEDIA_URL_BYTES {
                return Err(BuildRequestError::StructureLimitExceeded);
            }
            state.add_block()?;
            normalized
        }
        MediaSource::Base64(data) => {
            let mime_type = mime_type.ok_or(BuildRequestError::InvalidValue)?;
            if !matches!(
                mime_type,
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            ) {
                return Err(BuildRequestError::UnsupportedFeature);
            }
            validate_base64(data, state)?;
            format!("data:{mime_type};base64,{data}")
        }
    };

    let mut image = Map::new();
    image.insert("url".to_owned(), Value::String(url));
    let mut part = Map::new();
    part.insert("type".to_owned(), Value::String("image_url".to_owned()));
    part.insert("image_url".to_owned(), Value::Object(image));
    Ok(Value::Object(part))
}

fn build_audio_part(
    source: &MediaSource,
    mime_type: &str,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let MediaSource::Base64(data) = source else {
        return Err(BuildRequestError::UnsupportedFeature);
    };
    let format = match mime_type {
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        _ => return Err(BuildRequestError::UnsupportedFeature),
    };
    validate_base64(data, state)?;

    let mut audio = Map::new();
    audio.insert("data".to_owned(), Value::String(data.clone()));
    audio.insert("format".to_owned(), Value::String(format.to_owned()));
    let mut part = Map::new();
    part.insert("type".to_owned(), Value::String("input_audio".to_owned()));
    part.insert("input_audio".to_owned(), Value::Object(audio));
    Ok(Value::Object(part))
}

fn validate_base64(encoded: &str, state: &mut MessageBuildState) -> Result<(), BuildRequestError> {
    if encoded.is_empty() || !encoded.len().is_multiple_of(4) {
        return Err(BuildRequestError::InvalidValue);
    }
    let padding = if encoded.ends_with("==") {
        2
    } else if encoded.ends_with('=') {
        1
    } else {
        0
    };
    let estimated = encoded
        .len()
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|bytes| bytes.checked_sub(padding))
        .ok_or(BuildRequestError::StructureLimitExceeded)?;
    if estimated == 0 || estimated > MAX_MEDIA_BYTES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| BuildRequestError::InvalidValue)?;
    if decoded.len() != estimated {
        return Err(BuildRequestError::InvalidValue);
    }
    state.add_media(decoded.len())
}

fn message_value(
    role: &str,
    content: Value,
    tool_calls: Option<Vec<Value>>,
    tool_call_id: Option<String>,
) -> Value {
    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String(role.to_owned()));
    message.insert("content".to_owned(), content);
    if let Some(tool_calls) = tool_calls {
        message.insert("tool_calls".to_owned(), Value::Array(tool_calls));
    }
    if let Some(tool_call_id) = tool_call_id {
        message.insert("tool_call_id".to_owned(), Value::String(tool_call_id));
    }
    Value::Object(message)
}

fn text_content_value(texts: Vec<String>) -> Value {
    match texts.as_slice() {
        [text] => Value::String(text.clone()),
        _ => Value::Array(texts.iter().map(|text| text_part_value(text)).collect()),
    }
}

fn text_part_value(text: &str) -> Value {
    let mut part = Map::new();
    part.insert("type".to_owned(), Value::String("text".to_owned()));
    part.insert("text".to_owned(), Value::String(text.to_owned()));
    Value::Object(part)
}

fn number_value(value: f64) -> Option<Value> {
    serde_json::Number::from_f64(value).map(Value::Number)
}

fn map_request_validation_error(error: ParseRequestError) -> BuildRequestError {
    match error {
        ParseRequestError::StructureLimitExceeded | ParseRequestError::BodyTooLarge => {
            BuildRequestError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => BuildRequestError::UnsupportedFeature,
        ParseRequestError::ConflictingParameters => BuildRequestError::FieldConflict,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => BuildRequestError::InvalidValue,
    }
}

/// OpenAI Chat 请求构造错误，不保留 Canonical 中的敏感内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRequestError {
    /// Canonical 操作不是 Chat。
    UnsupportedOperation,
    /// Canonical 字段的类型、取值或关联关系无效。
    InvalidValue,
    /// Canonical 请求超过协议结构预算。
    StructureLimitExceeded,
    /// 目标协议缺少一项已声明的请求能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 请求使用了 OpenAI Chat 无法无损表达的特性。
    UnsupportedFeature,
    /// raw 字段来源协议与 OpenAI Chat 不一致。
    RawProtocolMismatch,
    /// raw 字段与已编码的 Canonical 字段发生碰撞。
    FieldConflict,
}

impl fmt::Display for BuildRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation => formatter.write_str("请求操作不是 Chat"),
            Self::InvalidValue => formatter.write_str("Canonical 请求字段值无效"),
            Self::StructureLimitExceeded => formatter.write_str("Canonical 请求结构超过限制"),
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedFeature => formatter.write_str("Canonical 请求包含当前不支持的特性"),
            Self::RawProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
            Self::FieldConflict => formatter.write_str("未归一化字段与 Canonical 字段冲突"),
        }
    }
}

impl Error for BuildRequestError {}
