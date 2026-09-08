use std::{collections::HashSet, fmt};

use af_domain::{Protocol, Role};
use serde::Serialize;
use serde_json::{Map, Value};

use super::{
    EncodeStreamError, MAX_SSE_EVENT_BYTES,
    wire::{
        EncodedContentBlock, EncodedContentBlockDelta, EncodedContentBlockStart,
        EncodedContentBlockStop, EncodedContentDelta, EncodedDirectCaller, EncodedMessage,
        EncodedMessageDelta, EncodedMessageDeltaState, EncodedMessageStart, EncodedSimpleEvent,
    },
};
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason, Usage, UsageSemantics,
    anthropic::{
        build_response::build_usage,
        convert::{
            MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_STOP_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
            MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES,
            validate_call_id, validate_model, validate_tool_name, validate_value_shape,
        },
        parse_response::validate_response_id,
    },
    bounded_json::{BoundedJsonError, JsonLimits, parse_value},
    validate_stream_event_capabilities,
};

const ARGUMENT_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

/// 将 Canonical 流事件编码为 Anthropic Messages SSE。
///
/// 构造器要求调用方显式提供 `message_start` 的初始累计用量；encoder 不会用零值
/// 代替未知用量。最终 `Usage` 必须不早于 `Finish`，两者合并为官方 `message_delta`。
pub struct AnthropicMessagesStreamEncoder {
    identity: EncodeIdentity,
    initial_usage: UsageParts,
    initial_usage_value: Value,
    active_block: Option<ActiveBlock>,
    seen_blocks: HashSet<BlockKey>,
    totals: EncodeTotals,
    next_output_index: u32,
    pending_finish: Option<PendingFinish>,
    started: bool,
    has_tool_use: bool,
    usage_seen: bool,
    done: bool,
    failed: bool,
}

impl AnthropicMessagesStreamEncoder {
    /// 使用已校验的响应标识、模型名与初始累计用量创建 encoder。
    pub fn new(
        response_id: impl Into<String>,
        model: impl Into<String>,
        initial_usage: Usage,
    ) -> Result<Self, EncodeStreamError> {
        let identity = EncodeIdentity::new(response_id.into(), model.into())?;
        let initial_usage_value = Value::Object(
            build_usage(&initial_usage).map_err(|_| EncodeStreamError::InvalidMetadata)?,
        );
        let initial_usage = usage_parts(&initial_usage)?;
        Ok(Self {
            identity,
            initial_usage,
            initial_usage_value,
            active_block: None,
            seen_blocks: HashSet::new(),
            totals: EncodeTotals::default(),
            next_output_index: 0,
            pending_finish: None,
            started: false,
            has_tool_use: false,
            usage_seen: false,
            done: false,
            failed: false,
        })
    }

    /// 编码一个 Canonical 事件；部分状态事件只更新生命周期，可能返回空字节。
    pub fn encode(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, EncodeStreamError> {
        if self.failed {
            return Err(EncodeStreamError::EncoderFailed);
        }
        if self.done {
            self.failed = true;
            return Err(EncodeStreamError::InvalidSequence);
        }
        let result = validate_stream_event_capabilities(Protocol::Anthropic, &event)
            .map_err(EncodeStreamError::UnsupportedCapability)
            .and_then(|()| self.encode_inner(event));
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn encode_inner(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, EncodeStreamError> {
        match event {
            CanonicalStreamEvent::MessageStart { choice_index, role } => {
                self.encode_message_start(choice_index, role)
            }
            CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index,
                delta,
            } => self.encode_content(choice_index, content_index, delta),
            CanonicalStreamEvent::ReasoningDelta {
                choice_index,
                content_index,
                text,
                signature,
            } => self.encode_reasoning(choice_index, content_index, text, signature),
            CanonicalStreamEvent::ToolCallStart {
                choice_index,
                tool_index,
                id,
                name,
            } => self.encode_tool_start(choice_index, tool_index, id, name),
            CanonicalStreamEvent::ToolCallSignature { .. } => {
                Err(EncodeStreamError::UnsupportedEvent)
            }
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index,
                tool_index,
                partial_json,
            } => self.encode_tool_arguments(choice_index, tool_index, partial_json),
            CanonicalStreamEvent::ToolCallEnd {
                choice_index,
                tool_index,
            } => self.encode_tool_end(choice_index, tool_index),
            CanonicalStreamEvent::CompactionStart { .. }
            | CanonicalStreamEvent::CompactionEnd { .. } => {
                Err(EncodeStreamError::UnsupportedEvent)
            }
            CanonicalStreamEvent::Finish {
                choice_index,
                reason,
                stop_sequence,
            } => self.encode_finish(choice_index, reason, stop_sequence),
            CanonicalStreamEvent::Usage(usage) => self.encode_usage(usage),
            CanonicalStreamEvent::PromptBlocked => Err(EncodeStreamError::UnsupportedEvent),
            CanonicalStreamEvent::Ping => encode_sse("ping", &EncodedSimpleEvent { kind: "ping" }),
            CanonicalStreamEvent::StreamEnd => self.encode_stream_end(),
            CanonicalStreamEvent::Error(_) => Err(EncodeStreamError::UnsupportedEvent),
        }
    }

    fn encode_message_start(
        &mut self,
        choice_index: u32,
        role: Role,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        if self.started || choice_index != 0 || role != Role::Assistant {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let bytes = encode_sse(
            "message_start",
            &EncodedMessageStart {
                kind: "message_start",
                message: EncodedMessage {
                    id: &self.identity.id,
                    kind: "message",
                    role: "assistant",
                    content: Vec::new(),
                    model: &self.identity.model,
                    container: None,
                    stop_reason: None,
                    stop_sequence: None,
                    stop_details: None,
                    usage: self.initial_usage_value.clone(),
                },
            },
        )?;
        self.started = true;
        Ok(bytes)
    }

    fn encode_content(
        &mut self,
        choice_index: u32,
        content_index: u32,
        delta: ContentDelta,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.require_content_phase(choice_index)?;
        let ContentDelta::Text(text) = delta else {
            return Err(EncodeStreamError::UnsupportedEvent);
        };
        let mut output = Vec::new();
        let output_index = self.ensure_text_block(content_index, &mut output)?;
        let active = self
            .active_block
            .as_mut()
            .expect("文本块创建成功后必须处于活动状态");
        let ActiveBlockKind::Text { text_bytes } = &mut active.kind else {
            unreachable!("活动块类型已由 ensure_text_block 校验")
        };
        self.totals.add_text(&text, text_bytes)?;
        output.extend(encode_sse(
            "content_block_delta",
            &EncodedContentBlockDelta {
                kind: "content_block_delta",
                index: output_index,
                delta: EncodedContentDelta::Text { text: &text },
            },
        )?);
        Ok(output)
    }

    fn encode_reasoning(
        &mut self,
        choice_index: u32,
        content_index: u32,
        text: String,
        signature: Option<String>,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.require_content_phase(choice_index)?;
        if text.is_empty() && signature.as_ref().is_none_or(String::is_empty) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let mut output = Vec::new();
        let output_index = self.ensure_thinking_block(content_index, &mut output)?;
        let active = self
            .active_block
            .as_mut()
            .expect("思考块创建成功后必须处于活动状态");
        let ActiveBlockKind::Thinking {
            thinking_bytes,
            signature_bytes,
            signature: complete_signature,
        } = &mut active.kind
        else {
            unreachable!("活动块类型已由 ensure_thinking_block 校验")
        };
        if !text.is_empty() {
            self.totals.add_text(&text, thinking_bytes)?;
            output.extend(encode_sse(
                "content_block_delta",
                &EncodedContentBlockDelta {
                    kind: "content_block_delta",
                    index: output_index,
                    delta: EncodedContentDelta::Thinking { thinking: &text },
                },
            )?);
        }
        if let Some(signature) = signature.filter(|value| !value.is_empty()) {
            self.totals.add_text(&signature, signature_bytes)?;
            complete_signature.push_str(&signature);
            output.extend(encode_sse(
                "content_block_delta",
                &EncodedContentBlockDelta {
                    kind: "content_block_delta",
                    index: output_index,
                    delta: EncodedContentDelta::Signature {
                        signature: &signature,
                    },
                },
            )?);
        }
        Ok(output)
    }

    fn encode_tool_start(
        &mut self,
        choice_index: u32,
        tool_index: u32,
        id: String,
        name: String,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.require_content_phase(choice_index)?;
        validate_call_id(&id).map_err(map_request_error)?;
        validate_tool_name(&name).map_err(map_request_error)?;
        if self.totals.tool_calls >= MAX_TOOL_CALLS {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }

        let mut output = Vec::new();
        self.close_non_tool_block(&mut output)?;
        let key = BlockKey::Tool(tool_index);
        if !self.seen_blocks.insert(key) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let output_index = self.allocate_output_index()?;
        output.extend(encode_sse(
            "content_block_start",
            &EncodedContentBlockStart {
                kind: "content_block_start",
                index: output_index,
                content_block: EncodedContentBlock::ToolUse {
                    id: &id,
                    name: &name,
                    input: Value::Object(Map::new()),
                    caller: EncodedDirectCaller { kind: "direct" },
                },
            },
        )?);
        self.active_block = Some(ActiveBlock {
            key,
            output_index,
            kind: ActiveBlockKind::Tool {
                arguments: String::new(),
            },
        });
        self.totals.tool_calls += 1;
        self.has_tool_use = true;
        Ok(output)
    }

    fn encode_tool_arguments(
        &mut self,
        choice_index: u32,
        tool_index: u32,
        partial_json: String,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.require_content_phase(choice_index)?;
        let active = self
            .active_block
            .as_mut()
            .filter(|block| block.key == BlockKey::Tool(tool_index))
            .ok_or(EncodeStreamError::InvalidSequence)?;
        let ActiveBlockKind::Tool { arguments } = &mut active.kind else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        self.totals.add_arguments(&partial_json, arguments.len())?;
        arguments.push_str(&partial_json);
        encode_sse(
            "content_block_delta",
            &EncodedContentBlockDelta {
                kind: "content_block_delta",
                index: active.output_index,
                delta: EncodedContentDelta::InputJson {
                    partial_json: &partial_json,
                },
            },
        )
    }

    fn encode_tool_end(
        &mut self,
        choice_index: u32,
        tool_index: u32,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.require_content_phase(choice_index)?;
        let active = self
            .active_block
            .take()
            .filter(|block| block.key == BlockKey::Tool(tool_index))
            .ok_or(EncodeStreamError::InvalidSequence)?;
        let ActiveBlockKind::Tool { arguments } = active.kind else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        let value =
            parse_value(arguments.as_bytes(), ARGUMENT_JSON_LIMITS).map_err(map_argument_error)?;
        if !value.is_object() {
            return Err(EncodeStreamError::UnsupportedEvent);
        }
        let nodes = validate_value_shape(&value, 16, 4_096, 1_024).map_err(map_request_error)?;
        self.totals.add_argument_nodes(nodes)?;
        encode_sse(
            "content_block_stop",
            &EncodedContentBlockStop {
                kind: "content_block_stop",
                index: active.output_index,
            },
        )
    }

    fn encode_finish(
        &mut self,
        choice_index: u32,
        reason: FinishReason,
        stop_sequence: Option<String>,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.require_content_phase(choice_index)?;
        if (reason == FinishReason::ToolCalls) != self.has_tool_use {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let (stop_reason, stop_sequence) = convert_finish(reason, stop_sequence)?;
        let mut output = Vec::new();
        self.close_non_tool_block(&mut output)?;
        self.pending_finish = Some(PendingFinish {
            stop_reason,
            stop_sequence,
        });
        Ok(output)
    }

    fn encode_usage(&mut self, usage: Usage) -> Result<Vec<u8>, EncodeStreamError> {
        if !self.started || self.usage_seen || self.active_block.is_some() {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let finish = self
            .pending_finish
            .as_ref()
            .ok_or(EncodeStreamError::InvalidSequence)?;
        let final_parts = usage_parts(&usage)?;
        validate_final_usage(self.initial_usage, final_parts)?;
        let usage = message_delta_usage(&usage)?;
        let bytes = encode_sse(
            "message_delta",
            &EncodedMessageDelta {
                kind: "message_delta",
                delta: EncodedMessageDeltaState {
                    container: None,
                    stop_details: None,
                    stop_reason: finish.stop_reason,
                    stop_sequence: finish.stop_sequence.as_deref(),
                },
                usage,
            },
        )?;
        self.usage_seen = true;
        Ok(bytes)
    }

    fn encode_stream_end(&mut self) -> Result<Vec<u8>, EncodeStreamError> {
        if !self.started
            || self.pending_finish.is_none()
            || !self.usage_seen
            || self.active_block.is_some()
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let bytes = encode_sse(
            "message_stop",
            &EncodedSimpleEvent {
                kind: "message_stop",
            },
        )?;
        self.done = true;
        Ok(bytes)
    }

    fn ensure_text_block(
        &mut self,
        content_index: u32,
        output: &mut Vec<u8>,
    ) -> Result<u32, EncodeStreamError> {
        let key = BlockKey::Text(content_index);
        if let Some(active) = &self.active_block
            && active.key == key
        {
            return Ok(active.output_index);
        }
        self.close_non_tool_block(output)?;
        if !self.seen_blocks.insert(key) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let output_index = self.allocate_output_index()?;
        output.extend(encode_sse(
            "content_block_start",
            &EncodedContentBlockStart {
                kind: "content_block_start",
                index: output_index,
                content_block: EncodedContentBlock::Text {
                    text: "",
                    citations: None,
                },
            },
        )?);
        self.active_block = Some(ActiveBlock {
            key,
            output_index,
            kind: ActiveBlockKind::Text { text_bytes: 0 },
        });
        Ok(output_index)
    }

    fn ensure_thinking_block(
        &mut self,
        content_index: u32,
        output: &mut Vec<u8>,
    ) -> Result<u32, EncodeStreamError> {
        let key = BlockKey::Thinking(content_index);
        if let Some(active) = &self.active_block
            && active.key == key
        {
            return Ok(active.output_index);
        }
        self.close_non_tool_block(output)?;
        if !self.seen_blocks.insert(key) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let output_index = self.allocate_output_index()?;
        output.extend(encode_sse(
            "content_block_start",
            &EncodedContentBlockStart {
                kind: "content_block_start",
                index: output_index,
                content_block: EncodedContentBlock::Thinking {
                    thinking: "",
                    signature: "",
                },
            },
        )?);
        self.active_block = Some(ActiveBlock {
            key,
            output_index,
            kind: ActiveBlockKind::Thinking {
                thinking_bytes: 0,
                signature_bytes: 0,
                signature: String::new(),
            },
        });
        Ok(output_index)
    }

    fn close_non_tool_block(&mut self, output: &mut Vec<u8>) -> Result<(), EncodeStreamError> {
        if self
            .active_block
            .as_ref()
            .is_some_and(|block| matches!(&block.kind, ActiveBlockKind::Tool { .. }))
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let Some(active) = self.active_block.take() else {
            return Ok(());
        };
        if let ActiveBlockKind::Thinking { signature, .. } = &active.kind
            && (signature.is_empty() || signature.chars().any(char::is_control))
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        output.extend(encode_sse(
            "content_block_stop",
            &EncodedContentBlockStop {
                kind: "content_block_stop",
                index: active.output_index,
            },
        )?);
        Ok(())
    }

    fn allocate_output_index(&mut self) -> Result<u32, EncodeStreamError> {
        self.totals.add_block()?;
        let index = self.next_output_index;
        self.next_output_index = self
            .next_output_index
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        Ok(index)
    }

    fn require_content_phase(&self, choice_index: u32) -> Result<(), EncodeStreamError> {
        if !self.started || choice_index != 0 || self.pending_finish.is_some() {
            return Err(EncodeStreamError::InvalidSequence);
        }
        Ok(())
    }
}

impl fmt::Debug for AnthropicMessagesStreamEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicMessagesStreamEncoder")
            .field("metadata", &"<已脱敏>")
            .field("next_output_index", &self.next_output_index)
            .field("has_active_block", &self.active_block.is_some())
            .field("has_pending_finish", &self.pending_finish.is_some())
            .field("usage_seen", &self.usage_seen)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

struct EncodeIdentity {
    id: String,
    model: String,
}

impl EncodeIdentity {
    fn new(id: String, model: String) -> Result<Self, EncodeStreamError> {
        validate_response_id(&id).map_err(|_| EncodeStreamError::InvalidMetadata)?;
        validate_model(&model).map_err(|_| EncodeStreamError::InvalidMetadata)?;
        Ok(Self { id, model })
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum BlockKey {
    Text(u32),
    Thinking(u32),
    Tool(u32),
}

struct ActiveBlock {
    key: BlockKey,
    output_index: u32,
    kind: ActiveBlockKind,
}

enum ActiveBlockKind {
    Text {
        text_bytes: usize,
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

struct PendingFinish {
    stop_reason: &'static str,
    stop_sequence: Option<String>,
}

#[derive(Default)]
struct EncodeTotals {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

impl EncodeTotals {
    fn add_block(&mut self) -> Result<(), EncodeStreamError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text(
        &mut self,
        fragment: &str,
        block_bytes: &mut usize,
    ) -> Result<(), EncodeStreamError> {
        *block_bytes = block_bytes
            .checked_add(fragment.len())
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        self.text_bytes = self
            .text_bytes
            .checked_add(fragment.len())
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if *block_bytes > MAX_TEXT_BYTES || self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_arguments(
        &mut self,
        fragment: &str,
        current_block_bytes: usize,
    ) -> Result<(), EncodeStreamError> {
        let block_bytes = current_block_bytes
            .checked_add(fragment.len())
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(fragment.len())
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if block_bytes > MAX_ARGUMENT_BYTES || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_argument_nodes(&mut self, nodes: usize) -> Result<(), EncodeStreamError> {
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct UsageParts {
    input_tokens: i64,
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    output_tokens: i64,
    reasoning: i64,
}

fn usage_parts(usage: &Usage) -> Result<UsageParts, EncodeStreamError> {
    usage
        .checked_total_tokens()
        .map_err(|_| EncodeStreamError::InvalidUsage)?;
    let details = usage.details();
    if details.audio_input().get() != 0 || details.audio_output().get() != 0 {
        return Err(EncodeStreamError::UnsupportedEvent);
    }
    let cache_total = details
        .cache_read()
        .get()
        .checked_add(details.cache_creation_5m().get())
        .and_then(|value| value.checked_add(details.cache_creation_1h().get()))
        .ok_or(EncodeStreamError::InvalidUsage)?;
    let input_tokens = match usage.semantics() {
        UsageSemantics::CacheSeparated => usage.input_tokens().get(),
        UsageSemantics::Inclusive => usage
            .input_tokens()
            .get()
            .checked_sub(cache_total)
            .ok_or(EncodeStreamError::InvalidUsage)?,
    };
    Ok(UsageParts {
        input_tokens,
        cache_read: details.cache_read().get(),
        cache_creation_5m: details.cache_creation_5m().get(),
        cache_creation_1h: details.cache_creation_1h().get(),
        output_tokens: usage.output_tokens().get(),
        reasoning: details.reasoning().get(),
    })
}

fn validate_final_usage(
    initial: UsageParts,
    final_usage: UsageParts,
) -> Result<(), EncodeStreamError> {
    if final_usage.input_tokens != initial.input_tokens
        || final_usage.cache_read != initial.cache_read
        || final_usage.cache_creation_5m != initial.cache_creation_5m
        || final_usage.cache_creation_1h != initial.cache_creation_1h
        || final_usage.output_tokens < initial.output_tokens
        || final_usage.reasoning < initial.reasoning
    {
        return Err(EncodeStreamError::InvalidUsage);
    }
    Ok(())
}

fn message_delta_usage(usage: &Usage) -> Result<Value, EncodeStreamError> {
    let mut full = build_usage(usage).map_err(|_| EncodeStreamError::InvalidUsage)?;
    let mut delta = Map::new();
    for key in [
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "input_tokens",
        "output_tokens",
        "output_tokens_details",
        "server_tool_use",
    ] {
        let value = full.remove(key).ok_or(EncodeStreamError::Serialization)?;
        delta.insert(key.to_owned(), value);
    }
    Ok(Value::Object(delta))
}

fn convert_finish(
    reason: FinishReason,
    stop_sequence: Option<String>,
) -> Result<(&'static str, Option<String>), EncodeStreamError> {
    match (reason, stop_sequence) {
        (FinishReason::Stop, Some(sequence))
            if !sequence.is_empty() && sequence.len() <= MAX_STOP_BYTES =>
        {
            Ok(("stop_sequence", Some(sequence)))
        }
        (FinishReason::Stop, None) => Ok(("end_turn", None)),
        (FinishReason::Length, None) => Ok(("max_tokens", None)),
        (FinishReason::ToolCalls, None) => Ok(("tool_use", None)),
        (FinishReason::ContentFilter, None) => Ok(("refusal", None)),
        _ => Err(EncodeStreamError::InvalidSequence),
    }
}

fn encode_sse(
    event_name: &'static str,
    payload: &impl Serialize,
) -> Result<Vec<u8>, EncodeStreamError> {
    let data = serde_json::to_vec(payload).map_err(|_| EncodeStreamError::Serialization)?;
    let frame_bytes = event_name
        .len()
        .checked_add(data.len())
        .and_then(|value| value.checked_add(16))
        .ok_or(EncodeStreamError::StructureLimitExceeded)?;
    if frame_bytes > MAX_SSE_EVENT_BYTES {
        return Err(EncodeStreamError::StructureLimitExceeded);
    }
    let mut output = Vec::with_capacity(frame_bytes);
    output.extend_from_slice(b"event: ");
    output.extend_from_slice(event_name.as_bytes());
    output.extend_from_slice(b"\ndata: ");
    output.extend_from_slice(&data);
    output.extend_from_slice(b"\n\n");
    Ok(output)
}

fn map_request_error(error: super::super::ParseRequestError) -> EncodeStreamError {
    match error {
        super::super::ParseRequestError::BodyTooLarge
        | super::super::ParseRequestError::StructureLimitExceeded => {
            EncodeStreamError::StructureLimitExceeded
        }
        super::super::ParseRequestError::UnsupportedFeature => EncodeStreamError::UnsupportedEvent,
        super::super::ParseRequestError::InvalidJson
        | super::super::ParseRequestError::DuplicateKey
        | super::super::ParseRequestError::InvalidValue => EncodeStreamError::InvalidSequence,
    }
}

fn map_argument_error(error: BoundedJsonError) -> EncodeStreamError {
    match error {
        BoundedJsonError::InvalidJson | BoundedJsonError::DuplicateKey => {
            EncodeStreamError::InvalidSequence
        }
        BoundedJsonError::LimitExceeded => EncodeStreamError::StructureLimitExceeded,
    }
}
