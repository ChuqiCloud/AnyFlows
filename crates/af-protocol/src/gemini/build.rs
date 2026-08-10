use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_CONTENTS, MAX_MEDIA_BYTES,
        MAX_NORMALIZED_MESSAGES, MAX_OUTPUT_TOKENS, MAX_PARTS_PER_CONTENT, MAX_RESULT_BYTES,
        MAX_SCHEMA_BYTES, MAX_SIGNATURE_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
        MAX_TOOL_DESCRIPTION_BYTES, MAX_TOOLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_JSON_NODES,
        MAX_TOTAL_RESULT_BYTES, MAX_TOTAL_SCHEMA_BYTES, MAX_TOTAL_SIGNATURE_BYTES,
        MAX_TOTAL_TEXT_BYTES, SCHEMA_LIMITS, TOOL_PAYLOAD_LIMITS, decode_base64,
        normalize_mime_type, validate_call_id, validate_json_object, validate_model,
        validate_stop_sequences, validate_tool_name,
    },
};
use crate::{
    CanonicalRequest, ContentBlock, MediaSource, Message, ReasoningEffort, RequestCapability,
    ToolChoice, ToolDef, UnsupportedCapability, bounded_json::validate_object,
    validate_request_capabilities,
};

const MODELED_FIELDS: &[&str] = &[
    "contents",
    "generationConfig",
    "model",
    "systemInstruction",
    "toolConfig",
    "tools",
];

/// 将 Canonical 请求构造为 Gemini `generateContent` 非流式正文。
///
/// Gemini 模型名由 HTTP 路径承载，因此返回对象不会包含 `model`。Canonical 也可由
/// 调用方直接构造，本函数会重新校验全部协议预算、角色顺序和工具调用关联关系。
pub fn build_request(request: &CanonicalRequest) -> Result<Value, BuildRequestError> {
    if request.operation != Operation::Chat {
        return Err(BuildRequestError::UnsupportedOperation);
    }
    validate_request_capabilities(Protocol::Gemini, request)
        .map_err(BuildRequestError::UnsupportedCapability)?;
    validate_model(&request.model).map_err(map_request_validation_error)?;
    if request.stream
        || request.stream_options.include_usage()
        || !request.attachments.is_empty()
        || request.metadata.user_id().is_some()
        || request.metadata.session_id().is_some()
        || request.tools.iter().any(|tool| tool.strict == Some(true))
        || !request.continuation.is_empty()
    {
        return Err(BuildRequestError::UnsupportedFeature);
    }

    let (tools, tool_names) = build_tools(&request.tools)?;
    let tool_config = build_tool_config(&request.tool_choice, &tool_names)?;
    let (system_instruction, contents) = build_contents(&request.messages)?;

    let mut root = Map::new();
    root.insert("contents".to_owned(), Value::Array(contents));
    if let Some(system_instruction) = system_instruction {
        root.insert("systemInstruction".to_owned(), system_instruction);
    }
    if !tools.is_empty() {
        root.insert(
            "tools".to_owned(),
            Value::Array(vec![Value::Object(Map::from_iter([(
                "functionDeclarations".to_owned(),
                Value::Array(tools),
            )]))]),
        );
    }
    root.insert("toolConfig".to_owned(), tool_config);
    insert_generation_config(&mut root, request)?;
    reject_raw(&root, request)?;
    validate_final_request(&root)?;
    Ok(Value::Object(root))
}

fn validate_final_request(root: &Map<String, Value>) -> Result<(), BuildRequestError> {
    validate_object(root, super::REQUEST_JSON_LIMITS, super::MAX_BODY_BYTES)
        .map_err(|_| BuildRequestError::StructureLimitExceeded)
}

fn build_tools(tools: &[ToolDef]) -> Result<(Vec<Value>, HashSet<String>), BuildRequestError> {
    if tools.len() > MAX_TOOLS {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut encoded = Vec::with_capacity(tools.len());
    let mut names = HashSet::with_capacity(tools.len());
    let mut total_schema_bytes = 0_usize;
    for tool in tools {
        validate_tool_name(&tool.name).map_err(map_request_validation_error)?;
        if !names.insert(tool.name.clone()) {
            return Err(BuildRequestError::InvalidValue);
        }
        if tool
            .description
            .as_ref()
            .is_some_and(|value| value.len() > MAX_TOOL_DESCRIPTION_BYTES)
        {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        let (schema_bytes, _) =
            validate_json_object(&tool.input_schema, SCHEMA_LIMITS, MAX_SCHEMA_BYTES)
                .map_err(map_request_validation_error)?;
        total_schema_bytes = total_schema_bytes
            .checked_add(schema_bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if total_schema_bytes > MAX_TOTAL_SCHEMA_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }

        let mut declaration = Map::new();
        declaration.insert("name".to_owned(), Value::String(tool.name.clone()));
        if let Some(description) = &tool.description {
            declaration.insert("description".to_owned(), Value::String(description.clone()));
        }
        declaration.insert("parametersJsonSchema".to_owned(), tool.input_schema.clone());
        encoded.push(Value::Object(declaration));
    }
    Ok((encoded, names))
}

fn build_tool_config(
    choice: &ToolChoice,
    tool_names: &HashSet<String>,
) -> Result<Value, BuildRequestError> {
    let mut function_config = Map::new();
    match choice {
        ToolChoice::Auto => {
            function_config.insert("mode".to_owned(), Value::String("AUTO".to_owned()));
        }
        ToolChoice::None => {
            function_config.insert("mode".to_owned(), Value::String("NONE".to_owned()));
        }
        ToolChoice::Required => {
            if tool_names.is_empty() {
                return Err(BuildRequestError::InvalidValue);
            }
            function_config.insert("mode".to_owned(), Value::String("ANY".to_owned()));
        }
        ToolChoice::Named { name } => {
            validate_tool_name(name).map_err(map_request_validation_error)?;
            if !tool_names.contains(name) {
                return Err(BuildRequestError::InvalidValue);
            }
            function_config.insert("mode".to_owned(), Value::String("ANY".to_owned()));
            function_config.insert(
                "allowedFunctionNames".to_owned(),
                Value::Array(vec![Value::String(name.clone())]),
            );
        }
    }
    Ok(Value::Object(Map::from_iter([(
        "functionCallingConfig".to_owned(),
        Value::Object(function_config),
    )])))
}

fn insert_generation_config(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let mut config = Map::new();
    if let Some(temperature) = request.sampling.temperature() {
        if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
            return Err(BuildRequestError::InvalidValue);
        }
        config.insert(
            "temperature".to_owned(),
            number_value(temperature).ok_or(BuildRequestError::InvalidValue)?,
        );
    }
    if let Some(top_p) = request.sampling.top_p() {
        if !top_p.is_finite() || !(0.0..=1.0).contains(&top_p) {
            return Err(BuildRequestError::InvalidValue);
        }
        config.insert(
            "topP".to_owned(),
            number_value(top_p).ok_or(BuildRequestError::InvalidValue)?,
        );
    }
    if let Some(max_output_tokens) = request.sampling.max_output_tokens() {
        let max_output_tokens = max_output_tokens.get();
        if !(1..=MAX_OUTPUT_TOKENS).contains(&max_output_tokens) {
            return Err(BuildRequestError::InvalidValue);
        }
        config.insert(
            "maxOutputTokens".to_owned(),
            Value::Number(max_output_tokens.into()),
        );
    }
    let stop_sequences = request.sampling.stop_sequences();
    validate_stop_sequences(stop_sequences).map_err(map_request_validation_error)?;
    if !stop_sequences.is_empty() {
        config.insert(
            "stopSequences".to_owned(),
            Value::Array(stop_sequences.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(reasoning) = request.reasoning {
        config.insert(
            "thinkingConfig".to_owned(),
            build_thinking_config(reasoning)?,
        );
    }
    if !config.is_empty() {
        root.insert("generationConfig".to_owned(), Value::Object(config));
    }
    Ok(())
}

fn build_thinking_config(reasoning: crate::ReasoningConfig) -> Result<Value, BuildRequestError> {
    if reasoning.effort().is_some() && reasoning.budget_tokens().is_some() {
        return Err(BuildRequestError::InvalidValue);
    }

    let mut config = Map::new();
    match reasoning.effort() {
        Some(ReasoningEffort::None) => {
            if reasoning.budget_tokens().is_some() || reasoning.include_thinking() {
                return Err(BuildRequestError::InvalidValue);
            }
            config.insert("thinkingBudget".to_owned(), Value::Number(0.into()));
        }
        Some(ReasoningEffort::Minimal) => {
            config.insert(
                "thinkingLevel".to_owned(),
                Value::String("MINIMAL".to_owned()),
            );
        }
        Some(ReasoningEffort::Low) => {
            config.insert("thinkingLevel".to_owned(), Value::String("LOW".to_owned()));
        }
        Some(ReasoningEffort::Medium) => {
            config.insert(
                "thinkingLevel".to_owned(),
                Value::String("MEDIUM".to_owned()),
            );
        }
        Some(ReasoningEffort::High) => {
            config.insert("thinkingLevel".to_owned(), Value::String("HIGH".to_owned()));
        }
        Some(ReasoningEffort::ExtraHigh | ReasoningEffort::Max) => {
            return Err(BuildRequestError::UnsupportedFeature);
        }
        None => {
            if let Some(budget) = reasoning.budget_tokens() {
                let budget = budget.get();
                if budget == 0 || budget > i64::from(i32::MAX) {
                    return Err(BuildRequestError::InvalidValue);
                }
                config.insert("thinkingBudget".to_owned(), Value::Number(budget.into()));
            }
        }
    }
    if reasoning.include_thinking() {
        config.insert("includeThoughts".to_owned(), Value::Bool(true));
    }
    Ok(Value::Object(config))
}

fn reject_raw(
    root: &Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(raw) = request.raw_passthrough() else {
        return Ok(());
    };
    let fields = raw
        .fields_for_protocol(Protocol::Gemini)
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
        UnsupportedCapability::request(Protocol::Gemini, RequestCapability::SameProtocolRaw),
    ))
}

fn build_contents(messages: &[Message]) -> Result<(Option<Value>, Vec<Value>), BuildRequestError> {
    if messages.is_empty() || messages.len() > MAX_NORMALIZED_MESSAGES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut state = MessageBuildState::default();
    let (system_instruction, start_index) = if messages[0].role == Role::System {
        (
            Some(content_value(
                None,
                build_system_parts(&messages[0].content, &mut state)?,
            )),
            1,
        )
    } else {
        (None, 0)
    };
    if start_index == messages.len() {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut contents = Vec::with_capacity(messages.len() - start_index);
    let mut index = start_index;
    while index < messages.len() {
        let message = &messages[index];
        match message.role {
            Role::System | Role::Developer => return Err(BuildRequestError::UnsupportedFeature),
            Role::User => {
                if !state.pending_tool_calls.is_empty() {
                    return Err(BuildRequestError::InvalidValue);
                }
                contents.push(content_value(
                    Some("user"),
                    build_user_parts(&message.content, &mut state)?,
                ));
                index += 1;
            }
            Role::Assistant => {
                if !state.pending_tool_calls.is_empty() {
                    return Err(BuildRequestError::InvalidValue);
                }
                contents.push(content_value(
                    Some("model"),
                    build_assistant_parts(&message.content, &mut state)?,
                ));
                index += 1;
            }
            Role::Tool => {
                let (content, next_index) = build_tool_result_turn(messages, index, &mut state)?;
                contents.push(content);
                index = next_index;
            }
        }
        if contents.len() > MAX_CONTENTS {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
    }
    if !state.pending_tool_calls.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok((system_instruction, contents))
}

fn build_tool_result_turn(
    messages: &[Message],
    start_index: usize,
    state: &mut MessageBuildState,
) -> Result<(Value, usize), BuildRequestError> {
    if state.pending_tool_calls.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }

    let mut parts = Vec::new();
    let mut index = start_index;
    while index < messages.len() && messages[index].role == Role::Tool {
        parts.push(build_tool_result_part(&messages[index].content, state)?);
        index += 1;
    }
    if !state.pending_tool_calls.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    if index < messages.len() && messages[index].role == Role::User {
        parts.extend(build_user_parts(&messages[index].content, state)?);
        index += 1;
    }
    validate_parts(&parts)?;
    Ok((content_value(Some("user"), parts), index))
}

fn build_system_parts(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Vec<Value>, BuildRequestError> {
    if blocks.is_empty() || blocks.len() > MAX_PARTS_PER_CONTENT {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    Ok(parts)
}

fn build_user_parts(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Vec<Value>, BuildRequestError> {
    if blocks.is_empty() || blocks.len() > MAX_PARTS_PER_CONTENT {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::Image { source, mime_type } => {
                parts.push(build_image_part(source, mime_type.as_deref(), state)?);
            }
            ContentBlock::Audio { source, mime_type } => {
                parts.push(build_audio_part(source, mime_type, state)?);
            }
            ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. } => return Err(BuildRequestError::InvalidValue),
            ContentBlock::CacheControl(_) | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    Ok(parts)
}

fn build_assistant_parts(
    blocks: &[ContentBlock],
    state: &mut MessageBuildState,
) -> Result<Vec<Value>, BuildRequestError> {
    if blocks.is_empty() || blocks.len() > MAX_PARTS_PER_CONTENT {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(build_text_part(text, state)?),
            ContentBlock::Image { source, mime_type } => {
                parts.push(build_image_part(source, mime_type.as_deref(), state)?);
            }
            ContentBlock::Audio { source, mime_type } => {
                parts.push(build_audio_part(source, mime_type, state)?);
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => parts.push(build_tool_use_part(
                id,
                name,
                input,
                signature.as_deref(),
                state,
            )?),
            ContentBlock::Thinking { text, signature } => {
                parts.push(build_thinking_part(text, signature.as_deref(), state)?)
            }
            ContentBlock::ToolResult { .. } => return Err(BuildRequestError::InvalidValue),
            ContentBlock::CacheControl(_) | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    Ok(parts)
}

fn build_tool_result_part(
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
    validate_call_id(tool_use_id).map_err(map_request_validation_error)?;
    let name = state
        .pending_tool_calls
        .get(tool_use_id)
        .cloned()
        .ok_or(BuildRequestError::InvalidValue)?;
    let response = build_tool_response(content, structured_content.as_ref(), *is_error, state)?;
    state.pending_tool_calls.remove(tool_use_id);
    state.add_block()?;

    Ok(Value::Object(Map::from_iter([(
        "functionResponse".to_owned(),
        Value::Object(Map::from_iter([
            ("id".to_owned(), Value::String(tool_use_id.clone())),
            ("name".to_owned(), Value::String(name)),
            ("response".to_owned(), response),
        ])),
    )])))
}

fn build_tool_response(
    content: &[ContentBlock],
    structured_content: Option<&Value>,
    is_error: bool,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let response = if let Some(structured_content) = structured_content {
        if !content.is_empty() {
            return Err(BuildRequestError::InvalidValue);
        }
        let object = structured_content
            .as_object()
            .ok_or(BuildRequestError::InvalidValue)?;
        if object.contains_key("error") != is_error {
            return Err(BuildRequestError::InvalidValue);
        }
        structured_content.clone()
    } else {
        match content {
            [] if !is_error => Value::Object(Map::new()),
            [ContentBlock::Text(text)] => {
                state.add_text(text)?;
                Value::Object(Map::from_iter([(
                    if is_error { "error" } else { "result" }.to_owned(),
                    Value::String(text.clone()),
                )]))
            }
            [] => return Err(BuildRequestError::InvalidValue),
            _ => return Err(BuildRequestError::UnsupportedFeature),
        }
    };
    let (bytes, nodes) = validate_json_object(&response, TOOL_PAYLOAD_LIMITS, MAX_RESULT_BYTES)
        .map_err(map_request_validation_error)?;
    state.add_result(bytes, nodes)?;
    Ok(response)
}

fn build_text_part(text: &str, state: &mut MessageBuildState) -> Result<Value, BuildRequestError> {
    state.add_text(text)?;
    Ok(Value::Object(Map::from_iter([(
        "text".to_owned(),
        Value::String(text.to_owned()),
    )])))
}

fn build_thinking_part(
    text: &str,
    signature: Option<&str>,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    state.add_text(text)?;
    let mut part = Map::from_iter([
        ("text".to_owned(), Value::String(text.to_owned())),
        ("thought".to_owned(), Value::Bool(true)),
    ]);
    if let Some(signature) = signature {
        validate_signature(signature, state)?;
        part.insert(
            "thoughtSignature".to_owned(),
            Value::String(signature.to_owned()),
        );
    }
    Ok(Value::Object(part))
}

fn build_tool_use_part(
    id: &str,
    name: &str,
    input: &Value,
    signature: Option<&str>,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    validate_call_id(id).map_err(map_request_validation_error)?;
    validate_tool_name(name).map_err(map_request_validation_error)?;
    if !state.seen_tool_call_ids.insert(id.to_owned()) {
        return Err(BuildRequestError::InvalidValue);
    }
    let (bytes, nodes) = validate_json_object(input, TOOL_PAYLOAD_LIMITS, MAX_ARGUMENT_BYTES)
        .map_err(map_request_validation_error)?;
    state.add_argument(bytes, nodes)?;
    state.add_block()?;
    if let Some(signature) = signature {
        validate_signature(signature, state)?;
    }
    state
        .pending_tool_calls
        .insert(id.to_owned(), name.to_owned());

    let mut part = Map::from_iter([(
        "functionCall".to_owned(),
        Value::Object(Map::from_iter([
            ("id".to_owned(), Value::String(id.to_owned())),
            ("name".to_owned(), Value::String(name.to_owned())),
            ("args".to_owned(), input.clone()),
        ])),
    )]);
    if let Some(signature) = signature {
        part.insert(
            "thoughtSignature".to_owned(),
            Value::String(signature.to_owned()),
        );
    }
    Ok(Value::Object(part))
}

fn build_image_part(
    source: &MediaSource,
    mime_type: Option<&str>,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let MediaSource::Base64(data) = source else {
        return Err(BuildRequestError::UnsupportedFeature);
    };
    let mime_type = mime_type.ok_or(BuildRequestError::InvalidValue)?;
    let mime_type = normalize_mime_type(mime_type).map_err(map_request_validation_error)?;
    if !mime_type.starts_with("image/") {
        return Err(BuildRequestError::InvalidValue);
    }
    build_inline_data_part(data, mime_type, state)
}

fn build_audio_part(
    source: &MediaSource,
    mime_type: &str,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let MediaSource::Base64(data) = source else {
        return Err(BuildRequestError::UnsupportedFeature);
    };
    let mime_type = normalize_mime_type(mime_type).map_err(map_request_validation_error)?;
    if !mime_type.starts_with("audio/") {
        return Err(BuildRequestError::InvalidValue);
    }
    build_inline_data_part(data, mime_type, state)
}

fn build_inline_data_part(
    data: &str,
    mime_type: String,
    state: &mut MessageBuildState,
) -> Result<Value, BuildRequestError> {
    let decoded = decode_base64(data).ok_or(BuildRequestError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_MEDIA_BYTES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    state.add_media(decoded.len())?;
    Ok(Value::Object(Map::from_iter([(
        "inlineData".to_owned(),
        Value::Object(Map::from_iter([
            ("mimeType".to_owned(), Value::String(mime_type)),
            ("data".to_owned(), Value::String(data.to_owned())),
        ])),
    )])))
}

fn validate_signature(
    signature: &str,
    state: &mut MessageBuildState,
) -> Result<(), BuildRequestError> {
    let decoded = decode_base64(signature).ok_or(BuildRequestError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_SIGNATURE_BYTES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    state.add_signature(decoded.len())
}

fn validate_parts(parts: &[Value]) -> Result<(), BuildRequestError> {
    if parts.is_empty() || parts.len() > MAX_PARTS_PER_CONTENT {
        Err(BuildRequestError::StructureLimitExceeded)
    } else {
        Ok(())
    }
}

fn content_value(role: Option<&str>, parts: Vec<Value>) -> Value {
    let mut content = Map::new();
    if let Some(role) = role {
        content.insert("role".to_owned(), Value::String(role.to_owned()));
    }
    content.insert("parts".to_owned(), Value::Array(parts));
    Value::Object(content)
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
    argument_bytes: usize,
    result_bytes: usize,
    json_nodes: usize,
    signature_bytes: usize,
    seen_tool_call_ids: HashSet<String>,
    pending_tool_calls: HashMap<String, String>,
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
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_argument(&mut self, bytes: usize, nodes: usize) -> Result<(), BuildRequestError> {
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        self.add_json_nodes(nodes)?;
        if self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.seen_tool_call_ids.len() > MAX_TOOL_CALLS
        {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_result(&mut self, bytes: usize, nodes: usize) -> Result<(), BuildRequestError> {
        self.result_bytes = self
            .result_bytes
            .checked_add(bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        self.add_json_nodes(nodes)?;
        if self.result_bytes > MAX_TOTAL_RESULT_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_json_nodes(&mut self, nodes: usize) -> Result<(), BuildRequestError> {
        self.json_nodes = self
            .json_nodes
            .checked_add(nodes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.json_nodes > MAX_TOTAL_JSON_NODES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_signature(&mut self, bytes: usize) -> Result<(), BuildRequestError> {
        self.signature_bytes = self
            .signature_bytes
            .checked_add(bytes)
            .ok_or(BuildRequestError::StructureLimitExceeded)?;
        if self.signature_bytes > MAX_TOTAL_SIGNATURE_BYTES {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        Ok(())
    }
}

/// Gemini `generateContent` 请求构造错误，不保留 Canonical 中的敏感内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRequestError {
    /// Canonical 操作不是 Chat。
    UnsupportedOperation,
    /// Canonical 字段的类型、取值、角色或关联关系无效。
    InvalidValue,
    /// Canonical 请求超过 Gemini 协议结构预算。
    StructureLimitExceeded,
    /// 目标协议缺少一项已声明的请求能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 请求使用了 Gemini `generateContent` 无法无损表达的特性。
    UnsupportedFeature,
    /// raw 字段来源协议与 Gemini 不一致。
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
