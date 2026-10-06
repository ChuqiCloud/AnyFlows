use std::{collections::HashSet, fmt};

use af_domain::{Role, UpstreamError, UpstreamServerStatus};
use serde_json::Value;

use super::{
    ParseStreamError,
    wire::{
        ContentBlockDeltaWire, ContentBlockStartWire, MessageDeltaUsageWire, MessageDeltaWire,
        MessageStartWire, StreamErrorTypeWire, StreamEventWire,
    },
};
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource,
    anthropic::{
        ParseRequestError, ParseResponseError,
        convert::{
            MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_STOP_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
            MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES,
            validate_call_id, validate_model, validate_tool_name, validate_value_shape,
        },
        parse_response::{convert_usage, validate_response_id},
    },
    bounded_json::{BoundedJsonError, JsonLimits, parse_value},
    sse::{SseEvent, SseParseError, SseParser},
};

const ARGUMENT_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

/// 将 Anthropic Messages SSE 增量解码为有序 Canonical 事件。
///
/// `message_start` 的初始用量只作为累计快照缓存；decoder 在 `message_delta`
/// 合并最终快照后只发送一次 Canonical `Usage`，调用方不得把两个快照相加。
pub struct AnthropicMessagesStreamDecoder {
    parser: SseParser,
    initial_usage: Option<Usage>,
    initial_service_tier: Option<&'static str>,
    active_block: Option<ActiveBlock>,
    seen_tool_ids: HashSet<String>,
    totals: DecodeTotals,
    next_block_index: u32,
    started: bool,
    message_delta_seen: bool,
    has_tool_use: bool,
    done: bool,
    failed: bool,
}

impl AnthropicMessagesStreamDecoder {
    /// 使用默认 SSE 事件上限创建 decoder。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用调用方收紧后的 SSE 单事件上限创建 decoder。
    pub fn with_max_event_bytes(max_event_bytes: usize) -> Result<Self, SseParseError> {
        Ok(Self {
            parser: SseParser::with_max_event_bytes(max_event_bytes)?,
            initial_usage: None,
            initial_service_tier: None,
            active_block: None,
            seen_tool_ids: HashSet::new(),
            totals: DecodeTotals::default(),
            next_block_index: 0,
            started: false,
            message_delta_seen: false,
            has_tool_use: false,
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

    /// 在传输 EOF 时确认 parser 空闲且已经收到逻辑终点。
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

    /// 返回已经由 `message_start` 验证的初始累计 usage，尚未开始消息时返回 `None`。
    #[must_use]
    pub const fn initial_usage(&self) -> Option<Usage> {
        self.initial_usage
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
        if frame.is_comment() {
            output.push(CanonicalStreamEvent::Ping);
            return Ok(());
        }

        let value =
            parse_value(frame.data(), super::super::REQUEST_JSON_LIMITS).map_err(map_json_error)?;
        let event_name = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or(ParseStreamError::InvalidValue)?;
        if frame.event_name() != Some(event_name) {
            return Err(ParseStreamError::InvalidSequence);
        }
        if uses_known_unsupported_feature(&value) {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        let event: StreamEventWire =
            serde_json::from_value(value).map_err(|_| ParseStreamError::InvalidValue)?;
        self.decode_event(event, output)
    }

    fn decode_event(
        &mut self,
        event: StreamEventWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        match event {
            StreamEventWire::MessageStart { message } => self.decode_message_start(message, output),
            StreamEventWire::ContentBlockStart {
                index,
                content_block,
            } => self.decode_content_start(index, content_block, output),
            StreamEventWire::ContentBlockDelta { index, delta } => {
                self.decode_content_delta(index, delta, output)
            }
            StreamEventWire::ContentBlockStop { index } => self.decode_content_stop(index, output),
            StreamEventWire::MessageDelta { delta, usage } => {
                self.decode_message_delta(delta, usage, output)
            }
            StreamEventWire::MessageStop => self.decode_message_stop(output),
            StreamEventWire::Ping => {
                output.push(CanonicalStreamEvent::Ping);
                Ok(())
            }
            StreamEventWire::Error { error } => {
                self.done = true;
                output.push(CanonicalStreamEvent::Error(map_stream_error(error.kind)));
                Ok(())
            }
        }
    }

    fn decode_message_start(
        &mut self,
        message: MessageStartWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.started || self.message_delta_seen || self.active_block.is_some() {
            return Err(ParseStreamError::InvalidSequence);
        }
        let MessageStartWire {
            id,
            _kind: _,
            _role: _,
            content,
            model,
            container,
            stop_reason,
            stop_sequence,
            stop_details,
            usage,
        } = message;
        if !content.is_empty()
            || stop_reason.is_some()
            || stop_sequence.is_some()
            || stop_details.is_some()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        if container.is_some() {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        validate_response_id(&id).map_err(map_response_error)?;
        validate_model(&model).map_err(map_request_error)?;
        let service_tier = usage.service_tier.as_ref().map(|tier| tier.as_str());
        // service_tier 等字段已由 UsageWire 严格验证，但当前流式 Canonical 事件没有
        // 对应的元数据载体；丢弃这些 raw 元数据，避免合法上游响应被误判为协议错误。
        let (usage, _) = convert_usage(usage).map_err(map_response_error)?;

        self.initial_usage = Some(usage);
        self.initial_service_tier = service_tier;
        self.started = true;
        output.push(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        });
        Ok(())
    }

    fn decode_content_start(
        &mut self,
        index: u32,
        content_block: ContentBlockStartWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        self.require_content_phase()?;
        if self.active_block.is_some() || index != self.next_block_index {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.next_block_index = self
            .next_block_index
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        self.totals.add_block()?;

        let kind = match content_block {
            ContentBlockStartWire::Text { text, citations } => {
                if citations.is_some_and(|citations| !citations.is_empty()) {
                    return Err(ParseStreamError::UnsupportedFeature);
                }
                let mut text_bytes = 0;
                let mut emitted = false;
                if !text.is_empty() {
                    self.totals.add_text(&text, &mut text_bytes)?;
                    output.push(CanonicalStreamEvent::ContentDelta {
                        choice_index: 0,
                        content_index: index,
                        delta: ContentDelta::Text(text),
                    });
                    emitted = true;
                }
                ActiveBlockKind::Text {
                    text_bytes,
                    emitted,
                }
            }
            ContentBlockStartWire::Thinking {
                thinking,
                signature,
            } => {
                let mut thinking_bytes = 0;
                let mut signature_bytes = 0;
                if !thinking.is_empty() {
                    self.totals.add_text(&thinking, &mut thinking_bytes)?;
                    output.push(CanonicalStreamEvent::ReasoningDelta {
                        choice_index: 0,
                        content_index: index,
                        text: thinking,
                        signature: None,
                    });
                }
                if !signature.is_empty() {
                    self.totals.add_text(&signature, &mut signature_bytes)?;
                    output.push(CanonicalStreamEvent::ReasoningDelta {
                        choice_index: 0,
                        content_index: index,
                        text: String::new(),
                        signature: Some(signature.clone()),
                    });
                }
                ActiveBlockKind::Thinking {
                    thinking_bytes,
                    signature_bytes,
                    signature,
                }
            }
            ContentBlockStartWire::ToolUse {
                id,
                name,
                input,
                _caller: _,
            } => {
                if self.totals.tool_calls >= MAX_TOOL_CALLS {
                    return Err(ParseStreamError::StructureLimitExceeded);
                }
                if !self.seen_tool_ids.insert(id.clone()) {
                    return Err(ParseStreamError::InvalidValue);
                }
                validate_call_id(&id).map_err(map_request_error)?;
                validate_tool_name(&name).map_err(map_request_error)?;
                let input = input
                    .as_object()
                    .ok_or(ParseStreamError::UnsupportedFeature)?;
                self.totals.tool_calls += 1;
                self.has_tool_use = true;
                output.push(CanonicalStreamEvent::ToolCallStart {
                    choice_index: 0,
                    tool_index: index,
                    id,
                    name,
                });

                let mut arguments = String::new();
                if !input.is_empty() {
                    arguments =
                        serde_json::to_string(input).map_err(|_| ParseStreamError::InvalidValue)?;
                    self.totals.add_arguments(&arguments, 0)?;
                    output.push(CanonicalStreamEvent::ToolCallArgsDelta {
                        choice_index: 0,
                        tool_index: index,
                        partial_json: arguments.clone(),
                    });
                }
                ActiveBlockKind::Tool { arguments }
            }
        };
        self.active_block = Some(ActiveBlock { index, kind });
        Ok(())
    }

    fn decode_content_delta(
        &mut self,
        index: u32,
        delta: ContentBlockDeltaWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        self.require_content_phase()?;
        let active = self
            .active_block
            .as_mut()
            .filter(|block| block.index == index)
            .ok_or(ParseStreamError::InvalidSequence)?;
        match (&mut active.kind, delta) {
            (
                ActiveBlockKind::Text {
                    text_bytes,
                    emitted,
                },
                ContentBlockDeltaWire::Text { text },
            ) => {
                if !text.is_empty() {
                    self.totals.add_text(&text, text_bytes)?;
                    *emitted = true;
                    output.push(CanonicalStreamEvent::ContentDelta {
                        choice_index: 0,
                        content_index: index,
                        delta: ContentDelta::Text(text),
                    });
                }
            }
            (
                ActiveBlockKind::Thinking { thinking_bytes, .. },
                ContentBlockDeltaWire::Thinking { thinking },
            ) => {
                if !thinking.is_empty() {
                    self.totals.add_text(&thinking, thinking_bytes)?;
                    output.push(CanonicalStreamEvent::ReasoningDelta {
                        choice_index: 0,
                        content_index: index,
                        text: thinking,
                        signature: None,
                    });
                }
            }
            (
                ActiveBlockKind::Thinking {
                    signature_bytes,
                    signature: complete_signature,
                    ..
                },
                ContentBlockDeltaWire::Signature { signature },
            ) => {
                if !signature.is_empty() {
                    self.totals.add_text(&signature, signature_bytes)?;
                    complete_signature.push_str(&signature);
                    output.push(CanonicalStreamEvent::ReasoningDelta {
                        choice_index: 0,
                        content_index: index,
                        text: String::new(),
                        signature: Some(signature),
                    });
                }
            }
            (
                ActiveBlockKind::Tool { arguments },
                ContentBlockDeltaWire::InputJson { partial_json },
            ) => {
                if !partial_json.is_empty() {
                    self.totals.add_arguments(&partial_json, arguments.len())?;
                    arguments.push_str(&partial_json);
                    output.push(CanonicalStreamEvent::ToolCallArgsDelta {
                        choice_index: 0,
                        tool_index: index,
                        partial_json,
                    });
                }
            }
            _ => return Err(ParseStreamError::InvalidSequence),
        }
        Ok(())
    }

    fn decode_content_stop(
        &mut self,
        index: u32,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        self.require_content_phase()?;
        let active = self
            .active_block
            .take()
            .filter(|block| block.index == index)
            .ok_or(ParseStreamError::InvalidSequence)?;
        match active.kind {
            ActiveBlockKind::Text { emitted, .. } => {
                if !emitted {
                    output.push(CanonicalStreamEvent::ContentDelta {
                        choice_index: 0,
                        content_index: index,
                        delta: ContentDelta::Text(String::new()),
                    });
                }
            }
            ActiveBlockKind::Thinking { signature, .. } => {
                if signature.is_empty() || signature.chars().any(char::is_control) {
                    return Err(ParseStreamError::InvalidValue);
                }
            }
            ActiveBlockKind::Tool { mut arguments } => {
                if arguments.is_empty() {
                    arguments.push_str("{}");
                    self.totals.add_arguments("{}", 0)?;
                    output.push(CanonicalStreamEvent::ToolCallArgsDelta {
                        choice_index: 0,
                        tool_index: index,
                        partial_json: "{}".to_owned(),
                    });
                }
                let value = parse_value(arguments.as_bytes(), ARGUMENT_JSON_LIMITS)
                    .map_err(map_argument_error)?;
                if !value.is_object() {
                    return Err(ParseStreamError::UnsupportedFeature);
                }
                let nodes =
                    validate_value_shape(&value, 16, 4_096, 1_024).map_err(map_request_error)?;
                self.totals.add_argument_nodes(nodes)?;
                output.push(CanonicalStreamEvent::ToolCallEnd {
                    choice_index: 0,
                    tool_index: index,
                });
            }
        }
        Ok(())
    }

    fn decode_message_delta(
        &mut self,
        delta: MessageDeltaWire,
        usage: MessageDeltaUsageWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        self.require_content_phase()?;
        if self.active_block.is_some() || self.message_delta_seen {
            return Err(ParseStreamError::InvalidSequence);
        }
        if delta.container.is_some() || delta.stop_details.is_some() {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        if let (Some(initial), Some(final_tier)) = (
            self.initial_service_tier,
            usage.service_tier.as_ref().map(|tier| tier.as_str()),
        ) && initial != final_tier
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        let (reason, stop_sequence) = convert_stop(delta.stop_reason, delta.stop_sequence)?;
        if (reason == FinishReason::ToolCalls) != self.has_tool_use {
            return Err(ParseStreamError::InvalidSequence);
        }
        let final_usage = merge_usage(
            self.initial_usage
                .ok_or(ParseStreamError::InvalidSequence)?,
            usage,
        )?;

        self.message_delta_seen = true;
        output.push(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason,
            stop_sequence,
        });
        output.push(CanonicalStreamEvent::Usage(final_usage));
        Ok(())
    }

    fn decode_message_stop(
        &mut self,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if !self.started || !self.message_delta_seen || self.active_block.is_some() {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.done = true;
        output.push(CanonicalStreamEvent::StreamEnd);
        Ok(())
    }

    fn require_content_phase(&self) -> Result<(), ParseStreamError> {
        if !self.started || self.message_delta_seen {
            return Err(ParseStreamError::InvalidSequence);
        }
        Ok(())
    }
}

impl Default for AnthropicMessagesStreamDecoder {
    fn default() -> Self {
        Self::with_max_event_bytes(crate::sse::DEFAULT_MAX_SSE_EVENT_BYTES)
            .expect("默认 SSE 事件上限必须有效")
    }
}

impl fmt::Debug for AnthropicMessagesStreamDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicMessagesStreamDecoder")
            .field("has_initial_usage", &self.initial_usage.is_some())
            .field("next_block_index", &self.next_block_index)
            .field("has_active_block", &self.active_block.is_some())
            .field("message_delta_seen", &self.message_delta_seen)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

struct ActiveBlock {
    index: u32,
    kind: ActiveBlockKind,
}

enum ActiveBlockKind {
    Text {
        text_bytes: usize,
        emitted: bool,
    },
    Thinking {
        thinking_bytes: usize,
        signature_bytes: usize,
        signature: String,
    },
    Tool {
        arguments: String,
    },
}

#[derive(Default)]
struct DecodeTotals {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

impl DecodeTotals {
    fn add_block(&mut self) -> Result<(), ParseStreamError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text(
        &mut self,
        fragment: &str,
        block_bytes: &mut usize,
    ) -> Result<(), ParseStreamError> {
        *block_bytes = block_bytes
            .checked_add(fragment.len())
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        self.text_bytes = self
            .text_bytes
            .checked_add(fragment.len())
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if *block_bytes > MAX_TEXT_BYTES || self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_arguments(
        &mut self,
        fragment: &str,
        current_block_bytes: usize,
    ) -> Result<(), ParseStreamError> {
        let block_bytes = current_block_bytes
            .checked_add(fragment.len())
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(fragment.len())
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if block_bytes > MAX_ARGUMENT_BYTES || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_argument_nodes(&mut self, nodes: usize) -> Result<(), ParseStreamError> {
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }
}

fn convert_stop(
    reason: super::super::response_wire::StopReasonWire,
    stop_sequence: Option<String>,
) -> Result<(FinishReason, Option<String>), ParseStreamError> {
    use super::super::response_wire::StopReasonWire;

    match (reason, stop_sequence) {
        (StopReasonWire::EndTurn, None) => Ok((FinishReason::Stop, None)),
        (StopReasonWire::StopSequence, Some(sequence))
            if !sequence.is_empty() && sequence.len() <= MAX_STOP_BYTES =>
        {
            Ok((FinishReason::Stop, Some(sequence)))
        }
        (StopReasonWire::MaxTokens | StopReasonWire::ModelContextWindowExceeded, None) => {
            Ok((FinishReason::Length, None))
        }
        (StopReasonWire::ToolUse, None) => Ok((FinishReason::ToolCalls, None)),
        (StopReasonWire::Refusal, None) => Ok((FinishReason::ContentFilter, None)),
        (StopReasonWire::PauseTurn, _) => Err(ParseStreamError::UnsupportedFeature),
        _ => Err(ParseStreamError::InvalidValue),
    }
}

fn merge_usage(initial: Usage, delta: MessageDeltaUsageWire) -> Result<Usage, ParseStreamError> {
    reject_server_tool_usage(delta.server_tool_use)?;
    let details = initial.details();
    let cache_creation = details
        .cache_creation_5m()
        .get()
        .checked_add(details.cache_creation_1h().get())
        .ok_or(ParseStreamError::InvalidValue)?;
    require_same_optional(delta.input_tokens, initial.input_tokens().get())?;
    require_same_optional(delta.cache_read_input_tokens, details.cache_read().get())?;
    require_same_optional(delta.cache_creation_input_tokens, cache_creation)?;

    let output_tokens = token_count(delta.output_tokens)?;
    if output_tokens < initial.output_tokens() {
        return Err(ParseStreamError::InvalidSequence);
    }
    let reasoning = delta
        .output_tokens_details
        .map_or(Ok(details.reasoning()), |details| {
            token_count(details.thinking_tokens)
        })?;
    if reasoning < details.reasoning() {
        return Err(ParseStreamError::InvalidSequence);
    }

    let usage = Usage::new(
        initial.input_tokens(),
        output_tokens,
        UsageDetails::new(
            details.cache_read(),
            details.cache_creation_5m(),
            details.cache_creation_1h(),
            reasoning,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::CacheSeparated,
    )
    .map_err(|_| ParseStreamError::InvalidValue)?;
    usage
        .checked_total_tokens()
        .map_err(|_| ParseStreamError::InvalidValue)?;
    Ok(usage)
}

fn require_same_optional(value: Option<i64>, expected: i64) -> Result<(), ParseStreamError> {
    if let Some(value) = value {
        if value < 0 {
            return Err(ParseStreamError::InvalidValue);
        }
        if value != expected {
            return Err(ParseStreamError::InvalidSequence);
        }
    }
    Ok(())
}

fn reject_server_tool_usage(
    usage: Option<super::super::response_wire::ServerToolUsageWire>,
) -> Result<(), ParseStreamError> {
    let Some(usage) = usage else {
        return Ok(());
    };
    for value in [usage.web_fetch_requests, usage.web_search_requests]
        .into_iter()
        .flatten()
    {
        if value < 0 {
            return Err(ParseStreamError::InvalidValue);
        }
        if value != 0 {
            return Err(ParseStreamError::UnsupportedFeature);
        }
    }
    Ok(())
}

fn token_count(value: i64) -> Result<TokenCount, ParseStreamError> {
    TokenCount::new(value).map_err(|_| ParseStreamError::InvalidValue)
}

fn map_stream_error(kind: StreamErrorTypeWire) -> UpstreamError {
    match kind {
        StreamErrorTypeWire::InvalidRequestError | StreamErrorTypeWire::RequestTooLarge => {
            UpstreamError::BadRequest
        }
        StreamErrorTypeWire::AuthenticationError => UpstreamError::AuthExpired,
        StreamErrorTypeWire::BillingError => UpstreamError::QuotaExhausted,
        StreamErrorTypeWire::PermissionError => UpstreamError::ProtocolError,
        StreamErrorTypeWire::NotFoundError => UpstreamError::ModelUnsupported,
        StreamErrorTypeWire::RateLimitError => {
            UpstreamError::rate_limited(af_domain::RateLimitScope::Window)
        }
        StreamErrorTypeWire::OverloadedError => UpstreamError::overloaded(),
        StreamErrorTypeWire::GatewayTimeout => UpstreamError::ServerError {
            status: UpstreamServerStatus::new(504).expect("504 必须是有效上游服务器状态"),
        },
        StreamErrorTypeWire::ApiError => UpstreamError::ServerError {
            status: UpstreamServerStatus::new(500).expect("500 必须是有效上游服务器状态"),
        },
    }
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(event) = value.as_object() else {
        return false;
    };
    match event.get("type").and_then(Value::as_str) {
        Some("content_block_start") => event
            .get("content_block")
            .and_then(Value::as_object)
            .is_some_and(|block| {
                matches!(
                    block.get("type").and_then(Value::as_str),
                    Some(
                        "redacted_thinking"
                            | "server_tool_use"
                            | "web_search_tool_result"
                            | "web_fetch_tool_result"
                            | "code_execution_tool_result"
                            | "bash_code_execution_tool_result"
                            | "text_editor_code_execution_tool_result"
                            | "tool_search_tool_result"
                            | "container_upload"
                    )
                ) || block.get("caller").is_some_and(|caller| {
                    !caller.is_null()
                        && caller.get("type").and_then(Value::as_str) != Some("direct")
                })
            }),
        Some("content_block_delta") => {
            event
                .get("delta")
                .and_then(Value::as_object)
                .and_then(|delta| delta.get("type"))
                .and_then(Value::as_str)
                == Some("citations_delta")
        }
        Some("message_start") => {
            event
                .get("message")
                .and_then(Value::as_object)
                .is_some_and(|message| {
                    message
                        .get("container")
                        .is_some_and(|value| !value.is_null())
                })
        }
        Some("message_delta") => {
            event
                .get("delta")
                .and_then(Value::as_object)
                .is_some_and(|delta| {
                    delta.get("container").is_some_and(|value| !value.is_null())
                        || delta
                            .get("stop_details")
                            .is_some_and(|value| !value.is_null())
                })
        }
        _ => false,
    }
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
        BoundedJsonError::InvalidJson => ParseStreamError::InvalidValue,
        BoundedJsonError::DuplicateKey => ParseStreamError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseStreamError::StructureLimitExceeded,
    }
}

fn map_request_error(error: ParseRequestError) -> ParseStreamError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            ParseStreamError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => ParseStreamError::UnsupportedFeature,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => ParseStreamError::InvalidValue,
    }
}

fn map_response_error(error: ParseResponseError) -> ParseStreamError {
    match error {
        ParseResponseError::BodyTooLarge | ParseResponseError::StructureLimitExceeded => {
            ParseStreamError::StructureLimitExceeded
        }
        ParseResponseError::UnsupportedFeature => ParseStreamError::UnsupportedFeature,
        ParseResponseError::InvalidJson
        | ParseResponseError::DuplicateKey
        | ParseResponseError::InvalidValue => ParseStreamError::InvalidValue,
    }
}
