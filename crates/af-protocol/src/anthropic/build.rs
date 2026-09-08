use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CACHE_BREAKPOINTS, MAX_CONTENT_BLOCKS, MAX_MEDIA_BYTES,
        MAX_MEDIA_URL_BYTES, MAX_MESSAGES, MAX_NORMALIZED_MESSAGES, MAX_PARTS_PER_MESSAGE,
        MAX_STOP_BYTES, MAX_STOP_SEQUENCES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
        MAX_TOOL_DESCRIPTION_BYTES, MAX_TOOLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES,
        MAX_TOTAL_TEXT_BYTES, MAX_USER_ID_BYTES, decoded_base64_len, normalize_remote_media_url,
        validate_call_id, validate_model, validate_schema, validate_tool_name,
        validate_value_shape,
    },
};
use crate::bounded_json::validate_object;
use crate::{
    CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message, ReasoningEffort,
    RequestCapability, ToolChoice, ToolDef, UnsupportedCapability, validate_request_capabilities,
};

const MODELED_FIELDS: &[&str] = &[
    "max_tokens",
    "messages",
    "metadata",
    "model",
    "output_config",
    "stop_sequences",
    "stream",
    "system",
    "temperature",
    "thinking",
    "tool_choice",
    "tools",
    "top_p",
];

/// 将 Canonical 请求构造为 Anthropic Messages 非流式 JSON。
///
/// Canonical 可由调用方直接构造，因此本函数会重新校验所有协议边界；当前入站
/// 不能再解析的 Anthropic 特性不会由出站方向生成。
pub fn build_request(request: &CanonicalRequest) -> Result<Value, BuildRequestError> {
    if request.operation != Operation::Chat {
        return Err(BuildRequestError::UnsupportedOperation);
    }
    validate_request_capabilities(Protocol::Anthropic, request)
        .map_err(BuildRequestError::UnsupportedCapability)?;
    validate_model(&request.model).map_err(map_request_validation_error)?;
    if request.stream_options.include_usage()
        || !request.attachments.is_empty()
        || request.tools.iter().any(|tool| tool.strict == Some(true))
        || !request.continuation.is_empty()
    {
        return Err(BuildRequestError::UnsupportedFeature);
    }

    let tools = build_tools(&request.tools)?;
    let tool_names = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<HashSet<_>>();
    let tool_choice = build_tool_choice(&request.tool_choice, &tool_names)?;
    let (system, messages) = build_messages(&request.messages)?;

    let mut root = Map::new();
    root.insert("model".to_owned(), Value::String(request.model.clone()));
    root.insert("messages".to_owned(), Value::Array(messages));
    if request.stream {
        root.insert("stream".to_owned(), Value::Bool(true));
    }
    if let Some(system) = system {
        root.insert("system".to_owned(), system);
    }
    if !tools.is_empty() {
        root.insert("tools".to_owned(), Value::Array(tools));
    }
    if let Some(tool_choice) = tool_choice {
        root.insert("tool_choice".to_owned(), tool_choice);
    }
    let max_tokens = insert_sampling(&mut root, request)?;
    insert_reasoning(&mut root, request, max_tokens)?;
    insert_metadata(&mut root, request)?;
    reject_raw(&root, request)?;
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
            .is_some_and(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
        {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        validate_schema(&tool.input_schema, &mut total_schema_bytes)
            .map_err(map_request_validation_error)?;

        let mut encoded_tool = Map::new();
        encoded_tool.insert("name".to_owned(), Value::String(tool.name.clone()));
        if let Some(description) = &tool.description {
            encoded_tool.insert("description".to_owned(), Value::String(description.clone()));
        }
        encoded_tool.insert("input_schema".to_owned(), tool.input_schema.clone());
        encoded.push(Value::Object(encoded_tool));
    }
    Ok(encoded)
}

fn build_tool_choice(
    choice: &ToolChoice,
    tool_names: &HashSet<&str>,
) -> Result<Option<Value>, BuildRequestError> {
    if tool_names.is_empty() {
        return match choice {
            ToolChoice::None => Ok(None),
            ToolChoice::Auto => Ok(Some(tool_choice_value("auto", None))),
            ToolChoice::Required | ToolChoice::Named { .. } => Err(BuildRequestError::InvalidValue),
        };
    }

    match choice {
        ToolChoice::Auto => Ok(None),
        ToolChoice::None => Ok(Some(tool_choice_value("none", None))),
        ToolChoice::Required => Ok(Some(tool_choice_value("any", None))),
        ToolChoice::Named { name } => {
            validate_tool_name(name).map_err(map_request_validation_error)?;
            if !tool_names.contains(name.as_str()) {
                return Err(BuildRequestError::InvalidValue);
            }
            Ok(Some(tool_choice_value("tool", Some(name.as_str()))))
        }
    }
}

fn tool_choice_value(kind: &str, name: Option<&str>) -> Value {
    let mut choice = Map::new();
    choice.insert("type".to_owned(), Value::String(kind.to_owned()));
    if let Some(name) = name {
        choice.insert("name".to_owned(), Value::String(name.to_owned()));
    }
    Value::Object(choice)
}

fn insert_sampling(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<i64, BuildRequestError> {
    let max_tokens = request
        .sampling
        .max_output_tokens()
        .ok_or(BuildRequestError::InvalidValue)?
        .get();
    if !(0..=MAX_OUTPUT_TOKENS).contains(&max_tokens) {
        return Err(BuildRequestError::InvalidValue);
    }
    root.insert("max_tokens".to_owned(), Value::Number(max_tokens.into()));

    if let Some(temperature) = request.sampling.temperature() {
        if !temperature.is_finite() || !(0.0..=1.0).contains(&temperature) {
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
            "stop_sequences".to_owned(),
            Value::Array(stop.iter().cloned().map(Value::String).collect()),
        );
    }
    Ok(max_tokens)
}

fn insert_reasoning(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
    max_tokens: i64,
) -> Result<(), BuildRequestError> {
    let Some(reasoning) = request.reasoning else {
        return Ok(());
    };

    let thinking = match reasoning.effort() {
        Some(ReasoningEffort::None) => {
            if reasoning.budget_tokens().is_some() || reasoning.include_thinking() {
                return Err(BuildRequestError::InvalidValue);
            }
            Some(thinking_value("disabled", None, true))
        }
        Some(effort) => {
            let effort = match effort {
                ReasoningEffort::Low => "low",
                ReasoningEffort::Medium => "medium",
                ReasoningEffort::High => "high",
                // Anthropic 使用 max 表达 OpenAI 兼容协议中的 xhigh。
                ReasoningEffort::ExtraHigh | ReasoningEffort::Max => "max",
                ReasoningEffort::None | ReasoningEffort::Minimal => {
                    return Err(BuildRequestError::UnsupportedFeature);
                }
            };
            root.insert(
                "output_config".to_owned(),
                Value::Object(Map::from_iter([(
                    "effort".to_owned(),
                    Value::String(effort.to_owned()),
                )])),
            );
            reasoning_thinking_value(reasoning, max_tokens)?
        }
        None => reasoning_thinking_value(reasoning, max_tokens)?,
    };
    if let Some(thinking) = thinking {
        root.insert("thinking".to_owned(), thinking);
    }
    Ok(())
}

/// 构造固定预算或自适应思考对象；仅配置输出强度时不额外注入思考字段。
fn reasoning_thinking_value(
    reasoning: crate::ReasoningConfig,
    max_tokens: i64,
) -> Result<Option<Value>, BuildRequestError> {
    match reasoning.budget_tokens() {
        Some(budget) => {
            let budget = budget.get();
            if !(1_024..max_tokens).contains(&budget) {
                return Err(BuildRequestError::InvalidValue);
            }
            Ok(Some(thinking_value(
                "enabled",
                Some(budget),
                reasoning.include_thinking(),
            )))
        }
        None if reasoning.include_thinking() || reasoning.effort().is_none() => {
            if max_tokens == 0 {
                return Err(BuildRequestError::InvalidValue);
            }
            Ok(Some(thinking_value(
                "adaptive",
                None,
                reasoning.include_thinking(),
            )))
        }
        None => Ok(None),
    }
}

fn thinking_value(kind: &str, budget: Option<i64>, include_thinking: bool) -> Value {
    let mut thinking = Map::new();
    thinking.insert("type".to_owned(), Value::String(kind.to_owned()));
    if let Some(budget) = budget {
        thinking.insert("budget_tokens".to_owned(), Value::Number(budget.into()));
    }
    if !include_thinking && kind != "disabled" {
        thinking.insert("display".to_owned(), Value::String("omitted".to_owned()));
    }
    Value::Object(thinking)
}

fn insert_metadata(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    if request.metadata.session_id().is_some() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    let Some(user_id) = request.metadata.user_id() else {
        return Ok(());
    };
    if user_id.is_empty()
        || user_id.len() > MAX_USER_ID_BYTES
        || user_id.chars().any(char::is_control)
    {
        return Err(BuildRequestError::InvalidValue);
    }
    root.insert(
        "metadata".to_owned(),
        Value::Object(Map::from_iter([(
            "user_id".to_owned(),
            Value::String(user_id.to_owned()),
        )])),
    );
    Ok(())
}

fn reject_raw(
    root: &Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(raw) = request.raw_passthrough() else {
        return Ok(());
    };
    let fields = raw
        .fields_for_protocol(Protocol::Anthropic)
        .map_err(|_| BuildRequestError::RawProtocolMismatch)?;
    if fields.is_empty() {
        return Ok(());
    }
    if fields
        .keys()
        .any(|key| root.contains_key(key) || MODELED_FIELDS.contains(&key.as_str()))
    {
        return Err(BuildRequestError::FieldConflict);
    }
    Err(BuildRequestError::UnsupportedCapability(
        UnsupportedCapability::request(Protocol::Anthropic, RequestCapability::SameProtocolRaw),
    ))
}

fn build_messages(messages: &[Message]) -> Result<(Option<Value>, Vec<Value>), BuildRequestError> {
    if messages.is_empty() || messages.len() > MAX_NORMALIZED_MESSAGES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut state = MessageBuildState::default();
    let (system, start_index) = if messages[0].role == Role::System {
        (
            Some(build_system_content(&messages[0].content, &mut state)?),
            1,
        )
    } else {
        (None, 0)
    };
    if start_index == messages.len() {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut encoded = Vec::with_capacity(messages.len() - start_index);
    let mut index = start_index;
    while index < messages.len() {
        let message = &messages[index];
        match message.role {
            Role::System | Role::Developer => return Err(BuildRequestError::UnsupportedFeature),
            Role::Assistant => {
                if !state.pending_tool_call_ids.is_empty() {
                    return Err(BuildRequestError::InvalidValue);
                }
                encoded.push(message_value(
                    "assistant",
                    build_assistant_content(&message.content, &mut state)?,
                ));
                index += 1;
            }
            Role::User => {
                if !state.pending_tool_call_ids.is_empty() {
                    return Err(BuildRequestError::InvalidValue);
                }
                encoded.push(message_value(
                    "user",
                    parts_content_value(build_user_parts(&message.content, &mut state)?),
                ));
                index += 1;
            }
            Role::Tool => {
                let (value, next_index) = build_tool_result_turn(messages, index, &mut state)?;
                encoded.push(value);
                index = next_index;
            }
        }
        if encoded.len() > MAX_MESSAGES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
    }
    if !state.pending_tool_call_ids.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok((system, encoded))
}

fn build_tool_result_turn(
    messages: &[Message],
    start_index: usize,
    state: &mut MessageBuildState,
) -> Result<(Value, usize), BuildRequestError> {
    if state.pending_tool_call_ids.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }

    let mut parts = Vec::new();
    let mut index = start_index;
    while index < messages.len() && messages[index].role == Role::Tool {
        parts.push(build_tool_result_part(&messages[index].content, state)?);
        index += 1;
    }
    if !state.pending_tool_call_ids.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    if index < messages.len() && messages[index].role == Role::User {
        parts.extend(build_user_parts(&messages[index].content, state)?);
        index += 1;
    }
    if parts.len() > MAX_PARTS_PER_MESSAGE {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    Ok((message_value("user", Value::Array(parts)), index))
}

fn build_system_content(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    if blocks.is_empty() {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::CacheControl(hint) => {
                attach_cache_control(&mut parts, *hint, state)?;
            }
            ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    validate_parts(&parts)?;
    Ok(parts_content_value(parts))
}

fn build_user_parts(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Vec<Value>, BuildRequestError> {
    if blocks.is_empty() {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::Image { source, mime_type } => {
                parts.push(build_image_part(source, mime_type.as_deref(), state)?);
            }
            ContentBlock::CacheControl(hint) => {
                attach_cache_control(&mut parts, *hint, state)?;
            }
            ContentBlock::ToolResult { .. } => return Err(BuildRequestError::InvalidValue),
            ContentBlock::Audio { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    validate_parts(&parts)?;
    Ok(parts)
}

fn build_assistant_content(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    if blocks.is_empty() {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => {
                if signature.is_some() {
                    return Err(BuildRequestError::UnsupportedFeature);
                }
                parts.push(build_tool_use_part(id, name, input, state)?);
            }
            ContentBlock::CacheControl(hint) => {
                attach_cache_control(&mut parts, *hint, state)?;
            }
            ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    validate_parts(&parts)?;
    Ok(parts_content_value(parts))
}

fn build_tool_result_part(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let (result, cache_hint) = match blocks {
        [
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                structured_content,
                is_error,
            },
        ] => ((tool_use_id, content, structured_content, is_error), None),
        [
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                structured_content,
                is_error,
            },
            ContentBlock::CacheControl(hint),
        ] => (
            (tool_use_id, content, structured_content, is_error),
            Some(*hint),
        ),
        _ => return Err(BuildRequestError::InvalidValue),
    };
    let (tool_use_id, content, structured_content, is_error) = result;
    if structured_content.is_some() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    validate_call_id(tool_use_id).map_err(map_request_validation_error)?;
    if !state.pending_tool_call_ids.remove(tool_use_id) {
        return Err(BuildRequestError::InvalidValue);
    }
    state.add_block()?;

    let mut part = Map::new();
    part.insert("type".to_owned(), Value::String("tool_result".to_owned()));
    part.insert("tool_use_id".to_owned(), Value::String(tool_use_id.clone()));
    let content = build_tool_result_content(content, state)?;
    if let Some(content) = content {
        part.insert("content".to_owned(), content);
    }
    if *is_error {
        part.insert("is_error".to_owned(), Value::Bool(true));
    }
    let mut value = Value::Object(part);
    if let Some(hint) = cache_hint {
        attach_cache_to_value(&mut value, hint, state)?;
    }
    Ok(value)
}

fn build_tool_result_content(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Option<Value>, BuildRequestError> {
    if blocks.is_empty() {
        return Ok(None);
    }
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::Image { source, mime_type } => {
                parts.push(build_image_part(source, mime_type.as_deref(), state)?);
            }
            ContentBlock::CacheControl(hint) => {
                attach_cache_control(&mut parts, *hint, state)?;
            }
            ContentBlock::Audio { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    validate_parts(&parts)?;
    Ok(Some(parts_content_value(parts)))
}

fn build_text_part(text: &str, state: &mut MessageBuildState) -> Result<Value, BuildRequestError> {
    state.add_text(text)?;
    Ok(Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("text".to_owned())),
        ("text".to_owned(), Value::String(text.to_owned())),
    ])))
}

fn build_image_part(
    source: &MediaSource,
    mime_type: Option<&str>,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let source = match source {
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
            Map::from_iter([
                ("type".to_owned(), Value::String("url".to_owned())),
                ("url".to_owned(), Value::String(normalized)),
            ])
        }
        MediaSource::Base64(data) => {
            let mime_type = mime_type.ok_or(BuildRequestError::InvalidValue)?;
            if !matches!(
                mime_type,
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            ) {
                return Err(BuildRequestError::UnsupportedFeature);
            }
            let bytes = decoded_base64_len(data).map_err(map_request_validation_error)?;
            state.add_media(bytes)?;
            Map::from_iter([
                ("type".to_owned(), Value::String("base64".to_owned())),
                ("media_type".to_owned(), Value::String(mime_type.to_owned())),
                ("data".to_owned(), Value::String(data.clone())),
            ])
        }
    };
    Ok(Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("image".to_owned())),
        ("source".to_owned(), Value::Object(source)),
    ])))
}

fn build_tool_use_part(
    id: &str,
    name: &str,
    input: &Value,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    validate_call_id(id).map_err(map_request_validation_error)?;
    validate_tool_name(name).map_err(map_request_validation_error)?;
    if !input.is_object() || !state.seen_tool_call_ids.insert(id.to_owned()) {
        return Err(BuildRequestError::InvalidValue);
    }
    let nodes =
        validate_value_shape(input, 16, 4_096, 1_024).map_err(map_request_validation_error)?;
    let bytes = serde_json::to_vec(input)
        .map_err(|_| BuildRequestError::InvalidValue)?
        .len();
    if bytes > MAX_ARGUMENT_BYTES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    state.add_tool_arguments(bytes, nodes)?;
    state.pending_tool_call_ids.insert(id.to_owned());

    Ok(Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("tool_use".to_owned())),
        ("id".to_owned(), Value::String(id.to_owned())),
        ("name".to_owned(), Value::String(name.to_owned())),
        ("input".to_owned(), input.clone()),
    ])))
}

fn attach_cache_control(
    parts: &mut [Value],
    hint: CacheHint,
    state: &mut MessageBuildState,
) -> Result<(), BuildRequestError> {
    let Some(last) = parts.last_mut() else {
        return Err(BuildRequestError::InvalidValue);
    };
    attach_cache_to_value(last, hint, state)
}

fn attach_cache_to_value(
    value: &mut Value,
    hint: CacheHint,
    state: &mut MessageBuildState,
) -> Result<(), BuildRequestError> {
    let object = value
        .as_object_mut()
        .ok_or(BuildRequestError::InvalidValue)?;
    if object.contains_key("cache_control") {
        return Err(BuildRequestError::InvalidValue);
    }
    object.insert("cache_control".to_owned(), cache_control_value(hint));
    state.add_cache_control()
}

fn cache_control_value(hint: CacheHint) -> Value {
    let mut value = Map::new();
    value.insert("type".to_owned(), Value::String("ephemeral".to_owned()));
    if hint == CacheHint::Ephemeral1Hour {
        value.insert("ttl".to_owned(), Value::String("1h".to_owned()));
    }
    Value::Object(value)
}

fn validate_parts(parts: &[Value]) -> Result<(), BuildRequestError> {
    if parts.is_empty() || parts.len() > MAX_PARTS_PER_MESSAGE {
        Err(BuildRequestError::StructureLimitExceeded)
    } else {
        Ok(())
    }
}

fn parts_content_value(parts: Vec<Value>) -> Value {
    if let [Value::Object(part)] = parts.as_slice()
        && part.len() == 2
        && part.get("type").and_then(Value::as_str) == Some("text")
        && let Some(text) = part.get("text").and_then(Value::as_str)
    {
        return Value::String(text.to_owned());
    }
    Value::Array(parts)
}

fn message_value(role: &str, content: Value) -> Value {
    Value::Object(Map::from_iter([
        ("role".to_owned(), Value::String(role.to_owned())),
        ("content".to_owned(), content),
    ]))
}

fn number_value(value: f64) -> Option<Value> {
    serde_json::Number::from_f64(value).map(Value::Number)
}

fn map_request_validation_error(error: ParseRequestError) -> BuildRequestError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            BuildRequestError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => BuildRequestError::UnsupportedFeature,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => BuildRequestError::InvalidValue,
    }
}

#[derive(Default)]
struct MessageBuildState {
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

    fn add_cache_control(&mut self) -> Result<(), BuildRequestError> {
        self.cache_breakpoints = self
            .cache_breakpoints
            .checked_add(1)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.cache_breakpoints > MAX_CACHE_BREAKPOINTS {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.add_block()
    }
}

/// Anthropic Messages 请求构造错误，不保留 Canonical 中的敏感内容。
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
    /// Canonical 请求使用了 Anthropic Messages 无法无损表达的特性。
    UnsupportedFeature,
    /// raw 字段来源协议与 Anthropic 不一致。
    RawProtocolMismatch,
    /// raw 字段与已建模的 Canonical 字段发生碰撞。
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
