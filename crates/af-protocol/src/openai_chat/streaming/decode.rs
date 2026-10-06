use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

use af_domain::Role;
use serde_json::Value;

use super::{
    ParseStreamError, SseEvent, SseParseError, SseParser,
    wire::{
        ChatStreamChunkWire, ChatStreamObjectWire, StreamChoiceWire, StreamDeltaWire,
        StreamFunctionWire, StreamRoleWire, StreamToolCallWire, StreamToolTypeWire,
    },
};
use crate::bounded_json::{BoundedJsonError, JsonLimits, parse_value};
use crate::openai_chat::{
    convert::{
        ARGUMENT_JSON_LIMITS, MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_TEXT_BYTES,
        MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES,
        validate_call_id, validate_model, validate_tool_name, validate_value_shape,
    },
    parse_response::{
        MAX_RESPONSE_CHOICES, convert_finish_reason, convert_usage, validate_response_id,
    },
};
use crate::{CanonicalStreamEvent, ContentDelta, FinishReason};

const MAX_RESPONSE_FINGERPRINT_BYTES: usize = 256;
const STREAM_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: super::super::MAX_BODY_BYTES,
    max_key_bytes: 1_024,
};

/// 将 OpenAI Chat SSE 增量解码为有序 Canonical 事件。
///
/// decoder 持有每个 choice 的内容块与工具调用状态；任一错误后必须丢弃实例。
#[derive(Default)]
pub struct OpenAiChatStreamDecoder {
    parser: SseParser,
    identity: Option<StreamIdentity>,
    choices: BTreeMap<u32, ChoiceState>,
    totals: DecodeTotals,
    usage_seen: bool,
    done: bool,
    failed: bool,
}

impl OpenAiChatStreamDecoder {
    /// 使用默认 SSE 事件上限创建 decoder。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用调用方收紧后的 SSE 单事件上限创建 decoder。
    pub fn with_max_event_bytes(max_event_bytes: usize) -> Result<Self, SseParseError> {
        Ok(Self {
            parser: SseParser::with_max_event_bytes(max_event_bytes)?,
            identity: None,
            choices: BTreeMap::new(),
            totals: DecodeTotals::default(),
            usage_seen: false,
            done: false,
            failed: false,
        })
    }

    /// 推入任意上游字节分片，并返回本次完整解出的 Canonical 事件。
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<CanonicalStreamEvent>, ParseStreamError> {
        if self.failed {
            return Err(ParseStreamError::DecoderFailed);
        }
        let result = self.push_inner(chunk);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// 在上游字节流结束时确认 parser 空闲且已经收到 `[DONE]`。
    pub fn finish(&mut self) -> Result<(), ParseStreamError> {
        if self.failed {
            return Err(ParseStreamError::DecoderFailed);
        }
        let result = self.finish_inner();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn push_inner(&mut self, chunk: &[u8]) -> Result<Vec<CanonicalStreamEvent>, ParseStreamError> {
        let frames = self.parser.push(chunk)?;
        let mut output = Vec::new();
        for frame in frames {
            self.decode_frame(frame, &mut output)?;
        }
        Ok(output)
    }

    fn finish_inner(&mut self) -> Result<(), ParseStreamError> {
        self.parser.finish()?;
        if !self.done {
            return Err(ParseStreamError::UnexpectedEof);
        }
        Ok(())
    }

    fn decode_frame(
        &mut self,
        frame: SseEvent,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.done {
            return Err(ParseStreamError::DataAfterDone);
        }
        if frame.is_comment() || frame.event_name() == Some("ping") {
            output.push(CanonicalStreamEvent::Ping);
            return Ok(());
        }
        if !matches!(frame.event_name(), None | Some("message")) {
            return Err(ParseStreamError::UnsupportedFeature);
        }

        let data = frame.data();
        if data.trim_ascii() == b"[DONE]" {
            return self.finish_stream(output);
        }

        let value = parse_value(data, STREAM_JSON_LIMITS).map_err(map_json_error)?;
        if uses_known_unsupported_feature(&value) {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        let chunk: ChatStreamChunkWire =
            serde_json::from_value(value).map_err(|_| ParseStreamError::InvalidValue)?;
        self.decode_chunk(chunk, output)
    }

    fn decode_chunk(
        &mut self,
        chunk: ChatStreamChunkWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let ChatStreamChunkWire {
            id,
            request_id,
            object,
            created,
            model,
            choices,
            service_tier,
            system_fingerprint,
            usage,
            moderation,
        } = chunk;
        let _ = request_id;
        if !matches!(object, ChatStreamObjectWire::ChatCompletionChunk) || moderation.is_some() {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        let identity = StreamIdentity::new(
            id,
            model,
            created,
            service_tier.map(|tier| tier.as_str().to_owned()),
            system_fingerprint,
        )?;
        match &self.identity {
            None => self.identity = Some(identity),
            Some(expected) if expected == &identity => {}
            Some(_) => return Err(ParseStreamError::InvalidSequence),
        }

        if choices.len() > MAX_RESPONSE_CHOICES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        if choices.is_empty() && usage.is_none() {
            return Err(ParseStreamError::InvalidValue);
        }
        let mut seen_choices = HashSet::with_capacity(choices.len());
        for choice in choices {
            if !seen_choices.insert(choice.index) {
                return Err(ParseStreamError::InvalidValue);
            }
            self.decode_choice(choice, output)?;
        }

        if let Some(usage) = convert_usage(usage).map_err(map_response_error)? {
            if self.usage_seen || self.choices.is_empty() || !self.all_choices_finished() {
                return Err(ParseStreamError::InvalidSequence);
            }
            self.usage_seen = true;
            output.push(CanonicalStreamEvent::Usage(usage));
        }
        Ok(())
    }

    fn decode_choice(
        &mut self,
        choice: StreamChoiceWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let StreamChoiceWire {
            index,
            delta,
            finish_reason,
            logprobs,
        } = choice;
        if logprobs.is_some() {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        if !self.choices.contains_key(&index) {
            if self.choices.len() >= MAX_RESPONSE_CHOICES {
                return Err(ParseStreamError::StructureLimitExceeded);
            }
            self.choices.insert(index, ChoiceState::default());
        }
        let state = self
            .choices
            .get_mut(&index)
            .expect("刚插入或已存在的 choice 必须可读取");
        if state.finished {
            return Err(ParseStreamError::InvalidSequence);
        }

        let mut emitted = decode_delta(index, delta, state, &mut self.totals, output)?;
        if let Some(reason) = finish_reason {
            let reason = convert_finish_reason(reason).map_err(map_response_error)?;
            finish_choice(index, reason, state, &mut self.totals, output)?;
            emitted = true;
        }
        if !emitted {
            return Err(ParseStreamError::InvalidValue);
        }
        Ok(())
    }

    fn finish_stream(
        &mut self,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.identity.is_none() || self.choices.is_empty() || !self.all_choices_finished() {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.done = true;
        output.push(CanonicalStreamEvent::StreamEnd);
        Ok(())
    }

    fn all_choices_finished(&self) -> bool {
        self.choices.values().all(|choice| choice.finished)
    }
}

impl fmt::Debug for OpenAiChatStreamDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatStreamDecoder")
            .field("has_identity", &self.identity.is_some())
            .field("choice_count", &self.choices.len())
            .field("usage_seen", &self.usage_seen)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
struct StreamIdentity {
    id: String,
    model: String,
    created: i64,
    service_tier: Option<String>,
    system_fingerprint: Option<String>,
}

impl StreamIdentity {
    fn new(
        id: String,
        model: String,
        created: i64,
        service_tier: Option<String>,
        system_fingerprint: Option<String>,
    ) -> Result<Self, ParseStreamError> {
        validate_response_id(&id).map_err(map_response_error)?;
        validate_model(&model).map_err(map_request_error)?;
        if created < 0 {
            return Err(ParseStreamError::InvalidValue);
        }
        if system_fingerprint.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > MAX_RESPONSE_FINGERPRINT_BYTES
                || value.chars().any(char::is_control)
        }) {
            return Err(ParseStreamError::InvalidValue);
        }
        Ok(Self {
            id,
            model,
            created,
            service_tier,
            system_fingerprint,
        })
    }
}

#[derive(Default)]
struct ChoiceState {
    started: bool,
    finished: bool,
    next_content_index: u32,
    text_index: Option<u32>,
    reasoning_index: Option<u32>,
    tools: BTreeMap<u32, ToolState>,
}

#[derive(Default)]
struct ToolState {
    id: String,
    name: String,
    arguments: String,
    ended: bool,
}

#[derive(Default)]
struct DecodeTotals {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

fn decode_delta(
    choice_index: u32,
    delta: StreamDeltaWire,
    state: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<bool, ParseStreamError> {
    let StreamDeltaWire {
        role,
        content,
        reasoning_content,
        reasoning,
        tool_calls,
        refusal,
        audio,
        function_call,
    } = delta;
    if refusal.is_some() || audio.is_some() || function_call.is_some() {
        return Err(ParseStreamError::UnsupportedFeature);
    }

    let mut emitted = false;
    if let Some(role) = role {
        if state.started || !matches!(role, StreamRoleWire::Assistant) {
            return Err(ParseStreamError::InvalidSequence);
        }
        state.started = true;
        output.push(CanonicalStreamEvent::MessageStart {
            choice_index,
            role: Role::Assistant,
        });
        emitted = true;
    }

    if reasoning_content.is_some() && reasoning.is_some() {
        return Err(ParseStreamError::InvalidValue);
    }
    if let Some(text) = reasoning_content.or(reasoning)
        && !text.is_empty()
    {
        require_started(state)?;
        add_text(totals, &text)?;
        let content_index = assign_content_index(
            &mut state.reasoning_index,
            &mut state.next_content_index,
            totals,
        )?;
        output.push(CanonicalStreamEvent::ReasoningDelta {
            choice_index,
            content_index,
            text,
            signature: None,
        });
        emitted = true;
    }
    if let Some(text) = content
        && !text.is_empty()
    {
        require_started(state)?;
        add_text(totals, &text)?;
        let content_index =
            assign_content_index(&mut state.text_index, &mut state.next_content_index, totals)?;
        output.push(CanonicalStreamEvent::ContentDelta {
            choice_index,
            content_index,
            delta: ContentDelta::Text(text),
        });
        emitted = true;
    }
    if let Some(tool_calls) = tool_calls {
        if tool_calls.is_empty() {
            return Err(ParseStreamError::InvalidValue);
        }
        require_started(state)?;
        let mut seen_tools = HashSet::with_capacity(tool_calls.len());
        for tool in tool_calls {
            if !seen_tools.insert(tool.index) {
                return Err(ParseStreamError::InvalidValue);
            }
            decode_tool_delta(choice_index, tool, state, totals, output)?;
            emitted = true;
        }
    }
    Ok(emitted)
}

fn decode_tool_delta(
    choice_index: u32,
    tool: StreamToolCallWire,
    choice: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<(), ParseStreamError> {
    let StreamToolCallWire {
        index,
        id,
        kind,
        function,
    } = tool;
    let arguments = match choice.tools.entry(index) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            if totals.tool_calls >= MAX_TOOL_CALLS {
                return Err(ParseStreamError::StructureLimitExceeded);
            }
            let id = id.ok_or(ParseStreamError::InvalidSequence)?;
            if !matches!(kind, Some(StreamToolTypeWire::Function)) {
                return Err(ParseStreamError::InvalidSequence);
            }
            let StreamFunctionWire { name, arguments } =
                function.ok_or(ParseStreamError::InvalidSequence)?;
            let name = name.ok_or(ParseStreamError::InvalidSequence)?;
            validate_call_id(&id).map_err(map_request_error)?;
            validate_tool_name(&name).map_err(map_request_error)?;
            totals.tool_calls += 1;
            entry.insert(ToolState {
                id: id.clone(),
                name: name.clone(),
                arguments: String::new(),
                ended: false,
            });
            output.push(CanonicalStreamEvent::ToolCallStart {
                choice_index,
                tool_index: index,
                id,
                name,
            });
            arguments
        }
        std::collections::btree_map::Entry::Occupied(entry) => {
            let existing = entry.get();
            if existing.ended {
                return Err(ParseStreamError::InvalidSequence);
            }
            if id.as_ref().is_some_and(|value| value != &existing.id)
                || !matches!(kind, None | Some(StreamToolTypeWire::Function))
            {
                return Err(ParseStreamError::InvalidSequence);
            }
            let Some(StreamFunctionWire { name, arguments }) = function else {
                return Err(ParseStreamError::InvalidValue);
            };
            if name.as_ref().is_some_and(|value| value != &existing.name) {
                return Err(ParseStreamError::InvalidSequence);
            }
            Some(arguments.ok_or(ParseStreamError::InvalidValue)?)
        }
    };
    if let Some(arguments) = arguments
        && !arguments.is_empty()
    {
        append_arguments(choice_index, index, arguments, choice, totals, output)?;
    }
    Ok(())
}

fn append_arguments(
    choice_index: u32,
    tool_index: u32,
    partial_json: String,
    choice: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<(), ParseStreamError> {
    let tool = choice
        .tools
        .get_mut(&tool_index)
        .expect("参数增量只会写入已开始的工具调用");
    let next_tool_bytes = tool
        .arguments
        .len()
        .checked_add(partial_json.len())
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    let next_total_bytes = totals
        .argument_bytes
        .checked_add(partial_json.len())
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    if next_tool_bytes > MAX_ARGUMENT_BYTES || next_total_bytes > MAX_TOTAL_ARGUMENT_BYTES {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    tool.arguments.push_str(&partial_json);
    totals.argument_bytes = next_total_bytes;
    output.push(CanonicalStreamEvent::ToolCallArgsDelta {
        choice_index,
        tool_index,
        partial_json,
    });
    Ok(())
}

fn finish_choice(
    choice_index: u32,
    reason: FinishReason,
    state: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<(), ParseStreamError> {
    require_started(state)?;
    if state.finished {
        return Err(ParseStreamError::InvalidSequence);
    }
    if (reason == FinishReason::ToolCalls) != !state.tools.is_empty() {
        return Err(ParseStreamError::InvalidSequence);
    }
    for (&tool_index, tool) in &mut state.tools {
        finish_tool(tool, totals)?;
        output.push(CanonicalStreamEvent::ToolCallEnd {
            choice_index,
            tool_index,
        });
    }
    state.finished = true;
    output.push(CanonicalStreamEvent::Finish {
        choice_index,
        reason,
        stop_sequence: None,
    });
    Ok(())
}

fn finish_tool(tool: &mut ToolState, totals: &mut DecodeTotals) -> Result<(), ParseStreamError> {
    if tool.ended {
        return Err(ParseStreamError::InvalidSequence);
    }
    let value =
        parse_value(tool.arguments.as_bytes(), ARGUMENT_JSON_LIMITS).map_err(map_argument_error)?;
    if !value.is_object() {
        return Err(ParseStreamError::UnsupportedFeature);
    }
    let nodes = validate_value_shape(&value, 16, 4_096, 1_024).map_err(map_request_error)?;
    totals.argument_nodes = totals
        .argument_nodes
        .checked_add(nodes)
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    if totals.argument_nodes > MAX_TOTAL_ARGUMENT_NODES {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    tool.ended = true;
    Ok(())
}

fn assign_content_index(
    current: &mut Option<u32>,
    next: &mut u32,
    totals: &mut DecodeTotals,
) -> Result<u32, ParseStreamError> {
    if let Some(index) = *current {
        return Ok(index);
    }
    totals.content_blocks = totals
        .content_blocks
        .checked_add(1)
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    if totals.content_blocks > MAX_CONTENT_BLOCKS {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    let index = *next;
    *next = next
        .checked_add(1)
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    *current = Some(index);
    Ok(index)
}

fn add_text(totals: &mut DecodeTotals, text: &str) -> Result<(), ParseStreamError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    totals.text_bytes = totals
        .text_bytes
        .checked_add(text.len())
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    if totals.text_bytes > MAX_TOTAL_TEXT_BYTES {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    Ok(())
}

fn require_started(state: &ChoiceState) -> Result<(), ParseStreamError> {
    if state.started {
        Ok(())
    } else {
        Err(ParseStreamError::InvalidSequence)
    }
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(root) = value.as_object() else {
        return false;
    };
    if root.get("moderation").is_some_and(|value| !value.is_null()) {
        return true;
    }
    root.get("choices")
        .and_then(Value::as_array)
        .is_some_and(|choices| {
            choices.iter().any(|choice| {
                let Some(choice) = choice.as_object() else {
                    return false;
                };
                if choice.get("logprobs").is_some_and(|value| !value.is_null())
                    || choice.get("finish_reason").and_then(Value::as_str) == Some("function_call")
                {
                    return true;
                }
                let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
                    return false;
                };
                if ["refusal", "audio", "function_call"]
                    .iter()
                    .any(|key| delta.get(*key).is_some_and(|value| !value.is_null()))
                {
                    return true;
                }
                delta
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .is_some_and(|calls| {
                        calls
                            .iter()
                            .any(|call| call.get("type").and_then(Value::as_str) == Some("custom"))
                    })
            })
        })
}

fn map_json_error(error: BoundedJsonError) -> ParseStreamError {
    match error {
        BoundedJsonError::InvalidJson => ParseStreamError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseStreamError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseStreamError::StructureLimitExceeded,
    }
}

fn map_argument_error(error: BoundedJsonError) -> ParseStreamError {
    match error {
        BoundedJsonError::InvalidJson | BoundedJsonError::DuplicateKey => {
            ParseStreamError::InvalidValue
        }
        BoundedJsonError::LimitExceeded => ParseStreamError::StructureLimitExceeded,
    }
}

fn map_response_error(error: super::super::ParseResponseError) -> ParseStreamError {
    match error {
        super::super::ParseResponseError::BodyTooLarge
        | super::super::ParseResponseError::StructureLimitExceeded => {
            ParseStreamError::StructureLimitExceeded
        }
        super::super::ParseResponseError::UnsupportedFeature => {
            ParseStreamError::UnsupportedFeature
        }
        _ => ParseStreamError::InvalidValue,
    }
}

fn map_request_error(error: super::super::ParseRequestError) -> ParseStreamError {
    match error {
        super::super::ParseRequestError::BodyTooLarge
        | super::super::ParseRequestError::StructureLimitExceeded => {
            ParseStreamError::StructureLimitExceeded
        }
        super::super::ParseRequestError::UnsupportedFeature => ParseStreamError::UnsupportedFeature,
        _ => ParseStreamError::InvalidValue,
    }
}
