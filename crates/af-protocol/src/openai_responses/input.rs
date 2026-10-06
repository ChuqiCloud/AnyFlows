use std::collections::HashSet;

use af_domain::Role;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};
use url::Url;

use super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_ENCRYPTED_REASONING_BYTES, MAX_MEDIA_BYTES,
        MAX_MEDIA_URL_BYTES, MAX_MESSAGES, MAX_PARTS_PER_MESSAGE, MAX_REASONING_SUMMARIES,
        MAX_TEXT_BYTES, MAX_TOOL_CALL_ID_BYTES, MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES,
        MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES, validate_tool_name, validate_value_shape,
    },
    wire::{
        CompactionInputItemWire, CompactionItemTypeWire, Field, FunctionCallItemWire,
        FunctionCallOutputItemWire, FunctionCallOutputTypeWire, FunctionCallTypeWire,
        FunctionOutputWire, ImageDetailWire, InputContentWire, InputFileWire, InputImageWire,
        InputItemWire, InputMessageWire, InputTextWire, InputWire, MessageContentWire,
        MessageRoleWire, OutputTextInputWire, ReasoningInputItemWire, ReasoningInputTypeWire,
        ReasoningSummaryTypeWire, ReasoningSummaryWire,
    },
};
use crate::{
    ContentBlock, MediaSource, Message, RequestContinuation, ResponsesCompactionItem,
    bounded_json::{self, BoundedJsonError, JsonLimits},
};

pub(super) const MAX_INPUT_ITEMS: usize = 512;
const ARGUMENT_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

pub(super) fn convert_input(
    instructions: Field<String>,
    input: Field<InputWire>,
    continuation: &RequestContinuation,
) -> Result<Vec<Message>, ParseRequestError> {
    let mut state = InputConvertState::new(
        continuation.previous_response_id().is_some() || continuation.conversation_id().is_some(),
    );
    let mut messages = Vec::new();

    if let Field::Value(instructions) = instructions {
        // Responses 顶层 instructions 对应本轮最高优先级的开发者指令。
        let content = vec![state.add_text(instructions)?];
        push_message(&mut messages, Message::new(Role::Developer, content))?;
    }

    match input {
        Field::Missing => {}
        Field::Value(InputWire::Text(text)) => {
            let content = vec![state.add_text(text)?];
            push_message(&mut messages, Message::new(Role::User, content))?;
        }
        Field::Value(InputWire::Items(items)) => {
            if items.len() > MAX_INPUT_ITEMS {
                return Err(ParseRequestError::StructureLimitExceeded);
            }
            for item in items {
                match item {
                    InputItemWire::Message(message) => {
                        if !state.pending_tool_call_ids.is_empty() {
                            return Err(ParseRequestError::InvalidValue);
                        }
                        let message = convert_message(message, &mut state)?;
                        push_message(&mut messages, message)?;
                    }
                    InputItemWire::Reasoning(reasoning) => {
                        if !state.pending_tool_call_ids.is_empty() {
                            return Err(ParseRequestError::InvalidValue);
                        }
                        let message = convert_reasoning_item(reasoning, &mut state)?;
                        push_message(&mut messages, message)?;
                    }
                    InputItemWire::FunctionCall(call) => {
                        let block = convert_function_call(call, &mut state)?;
                        append_function_call(&mut messages, block)?;
                    }
                    InputItemWire::FunctionCallOutput(output) => {
                        let message = convert_function_output(output, &mut state)?;
                        push_message(&mut messages, message)?;
                    }
                    InputItemWire::Compaction(item) => {
                        if !state.pending_tool_call_ids.is_empty() {
                            return Err(ParseRequestError::InvalidValue);
                        }
                        let item = convert_compaction_item(item, &mut state)?;
                        push_message(&mut messages, Message::new(Role::Assistant, vec![item]))?;
                    }
                }
            }
        }
    }

    if !state.pending_tool_call_ids.is_empty() {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(messages)
}

fn convert_compaction_item(
    item: CompactionInputItemWire,
    state: &mut InputConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let CompactionInputItemWire {
        kind: CompactionItemTypeWire::Compaction,
        id,
        encrypted_content,
    } = item;
    if encrypted_content.is_empty()
        || encrypted_content.len() > MAX_ENCRYPTED_REASONING_BYTES
        || encrypted_content.chars().any(char::is_control)
    {
        return Err(ParseRequestError::InvalidValue);
    }
    let id = match id {
        Field::Missing => None,
        Field::Value(id) => {
            validate_call_id(&id)?;
            if !state.seen_compaction_ids.insert(id.clone()) {
                return Err(ParseRequestError::InvalidValue);
            }
            Some(id)
        }
    };
    let mut object = Map::from_iter([
        ("type".to_owned(), Value::String("compaction".to_owned())),
        (
            "encrypted_content".to_owned(),
            Value::String(encrypted_content),
        ),
    ]);
    if let Some(id) = id {
        object.insert("id".to_owned(), Value::String(id));
    }
    state.add_block()?;
    Ok(ContentBlock::Compaction(
        ResponsesCompactionItem::from_validated_value(Value::Object(object)),
    ))
}

fn convert_reasoning_item(
    reasoning: ReasoningInputItemWire,
    state: &mut InputConvertState,
) -> Result<Message, ParseRequestError> {
    let ReasoningInputItemWire {
        kind: ReasoningInputTypeWire::Reasoning,
        id,
        encrypted_content,
        summary,
        status,
    } = reasoning;
    if let Field::Value(id) = id {
        validate_call_id(&id)?;
    }
    let _ = status;

    let signature = match encrypted_content {
        Field::Missing => return Err(ParseRequestError::UnsupportedFeature),
        Field::Value(value)
            if value.is_empty()
                || value.len() > MAX_ENCRYPTED_REASONING_BYTES
                || value.chars().any(char::is_control) =>
        {
            return Err(ParseRequestError::InvalidValue);
        }
        Field::Value(value) => value,
    };
    let summaries = match summary {
        Field::Missing => Vec::new(),
        Field::Value(values) if values.len() <= MAX_REASONING_SUMMARIES => values,
        Field::Value(_) => return Err(ParseRequestError::StructureLimitExceeded),
    };
    let mut text = String::new();
    for ReasoningSummaryWire {
        kind: ReasoningSummaryTypeWire::SummaryText,
        text: summary,
    } in summaries
    {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&summary);
        if text.len() > MAX_TEXT_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
    }
    let block = state.add_thinking(text, signature)?;
    Ok(Message::new(Role::Assistant, vec![block]))
}

fn push_message(messages: &mut Vec<Message>, message: Message) -> Result<(), ParseRequestError> {
    if messages.len() >= MAX_MESSAGES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    messages.push(message);
    Ok(())
}

fn convert_message(
    message: InputMessageWire,
    state: &mut InputConvertState,
) -> Result<Message, ParseRequestError> {
    let InputMessageWire {
        content,
        role,
        kind,
        phase,
        status,
    } = message;
    if field_is_present(&phase) || field_is_present(&status) {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    let _ = kind;

    let role = match role {
        MessageRoleWire::System => Role::System,
        MessageRoleWire::Developer => Role::Developer,
        MessageRoleWire::User => Role::User,
        MessageRoleWire::Assistant => Role::Assistant,
    };
    let content = convert_message_content(content, role, state)?;
    Ok(Message::new(role, content))
}

fn convert_message_content(
    content: MessageContentWire,
    role: Role,
    state: &mut InputConvertState,
) -> Result<Vec<ContentBlock>, ParseRequestError> {
    match content {
        MessageContentWire::Text(text) => Ok(vec![state.add_text(text)?]),
        MessageContentWire::Parts(parts) => {
            validate_parts_len(parts.len())?;
            parts
                .into_iter()
                .map(|part| convert_content(part, role, state))
                .collect()
        }
    }
}

fn convert_content(
    content: InputContentWire,
    role: Role,
    state: &mut InputConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    match content {
        InputContentWire::Text(InputTextWire {
            text,
            prompt_cache_breakpoint,
        }) => {
            if field_is_present(&prompt_cache_breakpoint) {
                return Err(ParseRequestError::UnsupportedFeature);
            }
            state.add_text(text)
        }
        InputContentWire::OutputText(OutputTextInputWire {
            text,
            annotations,
            logprobs,
        }) if role == Role::Assistant => {
            // 空数组不携带额外语义；非空标注或概率必须等 Canonical 明确建模后再接入。
            if field_has_items(&annotations) || field_has_items(&logprobs) {
                return Err(ParseRequestError::UnsupportedFeature);
            }
            state.add_text(text)
        }
        InputContentWire::OutputText(_) => Err(ParseRequestError::InvalidValue),
        InputContentWire::Image(image) if role == Role::User => convert_image(image, state),
        InputContentWire::Image(_) => Err(ParseRequestError::UnsupportedFeature),
        InputContentWire::File(InputFileWire {
            detail,
            file_data,
            file_id,
            file_url,
            filename,
            prompt_cache_breakpoint,
        }) => {
            let _ = (
                detail,
                file_data,
                file_id,
                file_url,
                filename,
                prompt_cache_breakpoint,
            );
            Err(ParseRequestError::UnsupportedFeature)
        }
    }
}

fn convert_function_call(
    call: FunctionCallItemWire,
    state: &mut InputConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let FunctionCallItemWire {
        kind: FunctionCallTypeWire::FunctionCall,
        arguments,
        call_id,
        name,
        id,
        caller,
        namespace,
        status,
    } = call;
    if field_is_present(&id)
        || field_is_present(&caller)
        || field_is_present(&namespace)
        || field_is_present(&status)
    {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    validate_call_id(&call_id)?;
    validate_tool_name(&name)?;
    if !state.seen_tool_call_ids.insert(call_id.clone()) {
        return Err(ParseRequestError::InvalidValue);
    }
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(ParseRequestError::StructureLimitExceeded);
    }
    let input = bounded_json::parse_value(arguments.as_bytes(), ARGUMENT_JSON_LIMITS)
        .map_err(map_argument_error)?;
    if !input.is_object() {
        return Err(ParseRequestError::InvalidValue);
    }
    let nodes = validate_value_shape(&input, 16, 4_096, 1_024)?;
    state.add_tool_arguments(arguments.len(), nodes)?;
    state.add_block()?;
    state.pending_tool_call_ids.insert(call_id.clone());
    Ok(ContentBlock::ToolUse {
        id: call_id,
        name,
        input,
        signature: None,
    })
}

fn append_function_call(
    messages: &mut Vec<Message>,
    block: ContentBlock,
) -> Result<(), ParseRequestError> {
    if let Some(message) = messages.last_mut()
        && message.role == Role::Assistant
        && message
            .content
            .iter()
            .all(|content| matches!(content, ContentBlock::ToolUse { .. }))
        && message.content.len() < MAX_PARTS_PER_MESSAGE
    {
        message.content.push(block);
        return Ok(());
    }
    push_message(messages, Message::new(Role::Assistant, vec![block]))
}

fn convert_function_output(
    output: FunctionCallOutputItemWire,
    state: &mut InputConvertState,
) -> Result<Message, ParseRequestError> {
    let FunctionCallOutputItemWire {
        kind: FunctionCallOutputTypeWire::FunctionCallOutput,
        call_id,
        output,
        id,
        caller,
    } = output;
    if field_is_present(&id) || field_is_present(&caller) {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    validate_call_id(&call_id)?;
    if !state.seen_tool_result_ids.insert(call_id.clone()) {
        return Err(ParseRequestError::InvalidValue);
    }
    if !state.pending_tool_call_ids.remove(&call_id) && !state.has_remote_context {
        return Err(ParseRequestError::InvalidValue);
    }

    let content = match output {
        FunctionOutputWire::Text(text) => vec![state.add_text(text)?],
        FunctionOutputWire::Parts(parts) => {
            validate_parts_len(parts.len())?;
            parts
                .into_iter()
                .map(|part| convert_tool_output_content(part, state))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    state.add_block()?;
    Ok(Message::new(
        Role::Tool,
        vec![ContentBlock::ToolResult {
            tool_use_id: call_id,
            content,
            structured_content: None,
            is_error: false,
        }],
    ))
}

fn convert_tool_output_content(
    content: InputContentWire,
    state: &mut InputConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    match content {
        InputContentWire::Text(InputTextWire {
            text,
            prompt_cache_breakpoint,
        }) => {
            if field_is_present(&prompt_cache_breakpoint) {
                return Err(ParseRequestError::UnsupportedFeature);
            }
            state.add_text(text)
        }
        InputContentWire::OutputText(_) => Err(ParseRequestError::InvalidValue),
        InputContentWire::Image(image) => convert_image(image, state),
        InputContentWire::File(file) => {
            let InputFileWire {
                detail,
                file_data,
                file_id,
                file_url,
                filename,
                prompt_cache_breakpoint,
            } = file;
            let _ = (
                detail,
                file_data,
                file_id,
                file_url,
                filename,
                prompt_cache_breakpoint,
            );
            Err(ParseRequestError::UnsupportedFeature)
        }
    }
}

fn convert_image(
    image: InputImageWire,
    state: &mut InputConvertState,
) -> Result<ContentBlock, ParseRequestError> {
    let InputImageWire {
        detail,
        file_id,
        image_url,
        prompt_cache_breakpoint,
    } = image;
    if field_is_present(&file_id) || field_is_present(&prompt_cache_breakpoint) {
        return Err(ParseRequestError::UnsupportedFeature);
    }
    match detail {
        Field::Missing | Field::Value(ImageDetailWire::Auto) => {}
        Field::Value(ImageDetailWire::Low | ImageDetailWire::High | ImageDetailWire::Original) => {
            return Err(ParseRequestError::UnsupportedFeature);
        }
    }
    let image_url = match image_url {
        Field::Missing => return Err(ParseRequestError::InvalidValue),
        Field::Value(image_url) => image_url,
    };
    if image_url.chars().any(char::is_control) {
        return Err(ParseRequestError::InvalidValue);
    }

    let (source, mime_type) = if let Some(data_url) = image_url.strip_prefix("data:") {
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
        if image_url.len() > MAX_MEDIA_URL_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        let url = normalize_remote_media_url(&image_url)?;
        (MediaSource::Url(url), None)
    };
    state.add_block()?;
    Ok(ContentBlock::Image { source, mime_type })
}

fn validate_base64(encoded: &str, state: &mut InputConvertState) -> Result<(), ParseRequestError> {
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

fn normalize_remote_media_url(value: &str) -> Result<String, ParseRequestError> {
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

pub(super) fn validate_call_id(id: &str) -> Result<(), ParseRequestError> {
    if id.is_empty() || id.len() > MAX_TOOL_CALL_ID_BYTES || id.chars().any(char::is_control) {
        return Err(ParseRequestError::InvalidValue);
    }
    Ok(())
}

fn validate_parts_len(parts: usize) -> Result<(), ParseRequestError> {
    if parts == 0 || parts > MAX_PARTS_PER_MESSAGE {
        Err(ParseRequestError::StructureLimitExceeded)
    } else {
        Ok(())
    }
}

fn map_argument_error(error: BoundedJsonError) -> ParseRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseRequestError::InvalidValue,
        BoundedJsonError::DuplicateKey => ParseRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseRequestError::StructureLimitExceeded,
    }
}

fn field_is_present<T>(field: &Field<T>) -> bool {
    matches!(field, Field::Value(_))
}

fn field_has_items<T>(field: &Field<Vec<T>>) -> bool {
    matches!(field, Field::Value(items) if !items.is_empty())
}

struct InputConvertState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    seen_tool_call_ids: HashSet<String>,
    seen_tool_result_ids: HashSet<String>,
    seen_compaction_ids: HashSet<String>,
    pending_tool_call_ids: HashSet<String>,
    has_remote_context: bool,
}

impl InputConvertState {
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
            seen_compaction_ids: HashSet::new(),
            pending_tool_call_ids: HashSet::new(),
            has_remote_context,
        }
    }

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
        self.add_text_bytes(text.len())?;
        Ok(ContentBlock::Text(text))
    }

    fn add_thinking(
        &mut self,
        text: String,
        signature: String,
    ) -> Result<ContentBlock, ParseRequestError> {
        self.add_text_bytes(text.len())?;
        Ok(ContentBlock::Thinking {
            text,
            signature: Some(signature),
        })
    }

    fn add_text_bytes(&mut self, bytes: usize) -> Result<(), ParseRequestError> {
        if bytes > MAX_TEXT_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(bytes)
            .ok_or(ParseRequestError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(ParseRequestError::StructureLimitExceeded);
        }
        self.add_block()
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
