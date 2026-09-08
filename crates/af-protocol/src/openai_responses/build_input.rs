use std::collections::HashSet;

use af_domain::Role;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};
use url::Url;

use super::{
    build::{BuildRequestError, map_request_validation_error},
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_ENCRYPTED_REASONING_BYTES, MAX_MEDIA_BYTES,
        MAX_MEDIA_URL_BYTES, MAX_MESSAGES, MAX_PARTS_PER_MESSAGE, MAX_TEXT_BYTES,
        MAX_TOOL_CALL_ID_BYTES, MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES,
        MAX_TOTAL_TEXT_BYTES, validate_tool_name, validate_value_shape,
    },
    input::MAX_INPUT_ITEMS,
};
use crate::{ContentBlock, MediaSource, Message, RequestContinuation};

/// 将有序 Canonical 消息编码为 Responses Input Item。
pub(super) fn build_input(
    messages: &[Message],
    continuation: &RequestContinuation,
) -> Result<Vec<Value>, BuildRequestError> {
    if messages.len() > MAX_MESSAGES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut items = Vec::with_capacity(messages.len());
    let mut state = InputBuildState::new(
        continuation.previous_response_id().is_some() || continuation.conversation_id().is_some(),
    );
    for message in messages {
        if !state.pending_tool_call_ids.is_empty() && message.role != Role::Tool {
            return Err(BuildRequestError::InvalidValue);
        }
        match message.role {
            Role::System => {
                let item = build_text_message("system", &message.content, &mut state)?;
                push_item(&mut items, item)?;
            }
            Role::Developer => {
                let item = build_text_message("developer", &message.content, &mut state)?;
                push_item(&mut items, item)?;
            }
            Role::User => {
                let item = build_user_message(&message.content, &mut state)?;
                push_item(&mut items, item)?;
            }
            Role::Assistant => build_assistant_items(&mut items, &message.content, &mut state)?,
            Role::Tool => build_tool_output_items(&mut items, &message.content, &mut state)?,
        }
    }
    if !state.pending_tool_call_ids.is_empty() {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok(items)
}

fn push_item(items: &mut Vec<Value>, item: Value) -> Result<(), BuildRequestError> {
    if items.len() >= MAX_INPUT_ITEMS {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    items.push(item);
    Ok(())
}

fn build_text_message(
    role: &str,
    blocks: &[ContentBlock],
    state: &mut InputBuildState,
) -> Result<Value, BuildRequestError> {
    validate_parts_len(blocks.len())?;
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        let ContentBlock::Text(text) = block else {
            return Err(BuildRequestError::UnsupportedFeature);
        };
        state.add_text(text)?;
        parts.push(input_text_part(text));
    }
    Ok(message_item(role, parts))
}

fn build_user_message(
    blocks: &[ContentBlock],
    state: &mut InputBuildState,
) -> Result<Value, BuildRequestError> {
    validate_parts_len(blocks.len())?;
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        parts.push(match block {
            ContentBlock::Text(text) => {
                state.add_text(text)?;
                input_text_part(text)
            }
            ContentBlock::Image { source, mime_type } => {
                image_part(source, mime_type.as_deref(), state)?
            }
            ContentBlock::Audio { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        });
    }
    Ok(message_item("user", parts))
}

fn build_assistant_items(
    items: &mut Vec<Value>,
    blocks: &[ContentBlock],
    state: &mut InputBuildState,
) -> Result<(), BuildRequestError> {
    validate_parts_len(blocks.len())?;
    let mut reasoning_items = Vec::new();
    let mut text_parts = Vec::new();
    let mut calls = Vec::new();
    let mut saw_non_reasoning = false;
    let mut saw_call = false;
    for block in blocks {
        match block {
            ContentBlock::Compaction(item) if blocks.len() == 1 => {
                push_item(items, item.as_value().clone())?;
                return Ok(());
            }
            ContentBlock::Thinking { text, signature } if !saw_non_reasoning => {
                reasoning_items.push(build_reasoning_item(text, signature.as_deref(), state)?);
            }
            ContentBlock::Text(text) if !saw_call => {
                saw_non_reasoning = true;
                state.add_text(text)?;
                text_parts.push(output_text_part(text));
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => {
                saw_non_reasoning = true;
                saw_call = true;
                calls.push(build_function_call(
                    id,
                    name,
                    input,
                    signature.as_deref(),
                    state,
                )?);
            }
            ContentBlock::Thinking { .. } => return Err(BuildRequestError::InvalidValue),
            ContentBlock::Text(_)
            | ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildRequestError::UnsupportedFeature);
            }
        }
    }
    for reasoning in reasoning_items {
        push_item(items, reasoning)?;
    }
    if !text_parts.is_empty() {
        push_item(items, message_item("assistant", text_parts))?;
    }
    for call in calls {
        push_item(items, call)?;
    }
    Ok(())
}

fn build_reasoning_item(
    text: &str,
    signature: Option<&str>,
    state: &mut InputBuildState,
) -> Result<Value, BuildRequestError> {
    let signature = signature.ok_or(BuildRequestError::UnsupportedFeature)?;
    if signature.is_empty()
        || signature.len() > MAX_ENCRYPTED_REASONING_BYTES
        || signature.chars().any(char::is_control)
    {
        return Err(BuildRequestError::InvalidValue);
    }
    state.add_text(text)?;
    let summary = if text.is_empty() {
        Vec::new()
    } else {
        vec![Value::Object(Map::from_iter([
            ("type".to_owned(), Value::String("summary_text".to_owned())),
            ("text".to_owned(), Value::String(text.to_owned())),
        ]))]
    };
    Ok(Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("reasoning".to_owned())),
        (
            "encrypted_content".to_owned(),
            Value::String(signature.to_owned()),
        ),
        ("summary".to_owned(), Value::Array(summary)),
    ])))
}

fn build_tool_output_items(
    items: &mut Vec<Value>,
    blocks: &[ContentBlock],
    state: &mut InputBuildState,
) -> Result<(), BuildRequestError> {
    validate_parts_len(blocks.len())?;
    for block in blocks {
        let ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content,
            is_error,
        } = block
        else {
            return Err(BuildRequestError::UnsupportedFeature);
        };
        let output = build_function_output(
            tool_use_id,
            content,
            structured_content.as_ref(),
            *is_error,
            state,
        )?;
        push_item(items, output)?;
    }
    Ok(())
}

fn build_function_call(
    id: &str,
    name: &str,
    input: &Value,
    signature: Option<&str>,
    state: &mut InputBuildState,
) -> Result<Value, BuildRequestError> {
    if signature.is_some() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    validate_call_id(id)?;
    validate_tool_name(name).map_err(map_request_validation_error)?;
    if !input.is_object() || !state.seen_tool_call_ids.insert(id.to_owned()) {
        return Err(BuildRequestError::InvalidValue);
    }
    let nodes =
        validate_value_shape(input, 16, 4_096, 1_024).map_err(map_request_validation_error)?;
    let arguments = serde_json::to_string(input).map_err(|_| BuildRequestError::InvalidValue)?;
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(BuildRequestError::StructureLimitExceeded);
    }
    state.add_tool_arguments(arguments.len(), nodes)?;
    state.pending_tool_call_ids.insert(id.to_owned());
    Ok(Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("function_call".to_owned())),
        ("call_id".to_owned(), Value::String(id.to_owned())),
        ("name".to_owned(), Value::String(name.to_owned())),
        ("arguments".to_owned(), Value::String(arguments)),
    ])))
}

fn build_function_output(
    call_id: &str,
    content: &[ContentBlock],
    structured_content: Option<&Value>,
    is_error: bool,
    state: &mut InputBuildState,
) -> Result<Value, BuildRequestError> {
    validate_call_id(call_id)?;
    if is_error {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    if !state.seen_tool_result_ids.insert(call_id.to_owned()) {
        return Err(BuildRequestError::InvalidValue);
    }
    if !state.pending_tool_call_ids.remove(call_id) && !state.has_remote_context {
        return Err(BuildRequestError::InvalidValue);
    }
    if structured_content.is_some() && !content.is_empty() {
        return Err(BuildRequestError::UnsupportedFeature);
    }

    let output = if let Some(structured_content) = structured_content {
        let _ = validate_value_shape(structured_content, 16, 4_096, 1_024)
            .map_err(map_request_validation_error)?;
        let output = serde_json::to_string(structured_content)
            .map_err(|_| BuildRequestError::InvalidValue)?;
        state.add_text(&output)?;
        Value::String(output)
    } else if content.is_empty() {
        state.add_text("")?;
        Value::String(String::new())
    } else if let [ContentBlock::Text(text)] = content {
        state.add_text(text)?;
        Value::String(text.clone())
    } else {
        validate_parts_len(content.len())?;
        let mut parts = Vec::with_capacity(content.len());
        for block in content {
            parts.push(match block {
                ContentBlock::Text(text) => {
                    state.add_text(text)?;
                    input_text_part(text)
                }
                ContentBlock::Image { source, mime_type } => {
                    image_part(source, mime_type.as_deref(), state)?
                }
                ContentBlock::Audio { .. }
                | ContentBlock::ToolUse { .. }
                | ContentBlock::ToolResult { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::CacheControl(_)
                | ContentBlock::Compaction(_) => {
                    return Err(BuildRequestError::UnsupportedFeature);
                }
            });
        }
        Value::Array(parts)
    };
    state.add_block()?;
    Ok(Value::Object(Map::from_iter([
        (
            "type".to_owned(),
            Value::String("function_call_output".to_owned()),
        ),
        ("call_id".to_owned(), Value::String(call_id.to_owned())),
        ("output".to_owned(), output),
    ])))
}

fn message_item(role: &str, content: Vec<Value>) -> Value {
    Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("message".to_owned())),
        ("role".to_owned(), Value::String(role.to_owned())),
        ("content".to_owned(), Value::Array(content)),
    ]))
}

fn input_text_part(text: &str) -> Value {
    Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("input_text".to_owned())),
        ("text".to_owned(), Value::String(text.to_owned())),
    ]))
}

fn output_text_part(text: &str) -> Value {
    Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("output_text".to_owned())),
        ("text".to_owned(), Value::String(text.to_owned())),
    ]))
}

fn image_part(
    source: &MediaSource,
    mime_type: Option<&str>,
    state: &mut InputBuildState,
) -> Result<Value, BuildRequestError> {
    let image_url = match source {
        MediaSource::Url(url) => {
            if mime_type.is_some() {
                return Err(BuildRequestError::UnsupportedFeature);
            }
            if url.len() > MAX_MEDIA_URL_BYTES {
                return Err(BuildRequestError::StructureLimitExceeded);
            }
            let url = normalize_remote_media_url(url)?;
            state.add_block()?;
            url
        }
        MediaSource::Base64(encoded) => {
            let mime_type = mime_type.ok_or(BuildRequestError::InvalidValue)?;
            if !matches!(
                mime_type,
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            ) {
                return Err(BuildRequestError::UnsupportedFeature);
            }
            validate_base64(encoded, state)?;
            format!("data:{mime_type};base64,{encoded}")
        }
    };
    Ok(Value::Object(Map::from_iter([
        ("type".to_owned(), Value::String("input_image".to_owned())),
        ("detail".to_owned(), Value::String("auto".to_owned())),
        ("image_url".to_owned(), Value::String(image_url)),
    ])))
}

fn normalize_remote_media_url(value: &str) -> Result<String, BuildRequestError> {
    if value.trim() != value || value.contains('\\') || value.chars().any(char::is_control) {
        return Err(BuildRequestError::InvalidValue);
    }
    let url = Url::parse(value).map_err(|_| BuildRequestError::InvalidValue)?;
    if url.scheme() != "https"
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok(url.to_string())
}

fn validate_base64(encoded: &str, state: &mut InputBuildState) -> Result<(), BuildRequestError> {
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

fn validate_call_id(id: &str) -> Result<(), BuildRequestError> {
    if id.is_empty() || id.len() > MAX_TOOL_CALL_ID_BYTES || id.chars().any(char::is_control) {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok(())
}

fn validate_parts_len(parts: usize) -> Result<(), BuildRequestError> {
    if parts == 0 || parts > MAX_PARTS_PER_MESSAGE {
        Err(BuildRequestError::StructureLimitExceeded)
    } else {
        Ok(())
    }
}

struct InputBuildState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    seen_tool_call_ids: HashSet<String>,
    seen_tool_result_ids: HashSet<String>,
    pending_tool_call_ids: HashSet<String>,
    has_remote_context: bool,
}

impl InputBuildState {
    fn new(has_remote_context: bool) -> Self {
        Self {
            content_blocks: 0,
            text_bytes: 0,
            media_bytes: 0,
            tool_calls: 0,
            argument_bytes: 0,
            argument_nodes: 0,
            seen_tool_call_ids: HashSet::new(),
            seen_tool_result_ids: HashSet::new(),
            pending_tool_call_ids: HashSet::new(),
            has_remote_context,
        }
    }

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
