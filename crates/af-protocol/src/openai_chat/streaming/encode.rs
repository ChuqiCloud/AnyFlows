use std::{collections::BTreeMap, fmt};

use af_domain::{Protocol, Role};

use super::{
    EncodeStreamError, MAX_SSE_EVENT_BYTES,
    wire::{
        EncodedChoice, EncodedChunk, EncodedCompletionDetails, EncodedDelta, EncodedFunction,
        EncodedPromptDetails, EncodedToolCall, EncodedUsage,
    },
};
use crate::bounded_json::{BoundedJsonError, parse_value};
use crate::openai_chat::{
    convert::{
        ARGUMENT_JSON_LIMITS, MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_TEXT_BYTES,
        MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES,
        validate_call_id, validate_model, validate_tool_name, validate_value_shape,
    },
    parse_response::{MAX_RESPONSE_CHOICES, validate_response_id},
};
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason, Usage, validate_stream_event_capabilities,
};

/// 将 Canonical 流事件编码为 OpenAI Chat Completions SSE。
///
/// encoder 校验每个 choice 的 role、内容索引、工具生命周期与 finish 顺序；任一错误后
/// 进入终止状态，避免继续输出基于部分状态的响应。
pub struct OpenAiChatStreamEncoder {
    identity: EncodeIdentity,
    choices: BTreeMap<u32, ChoiceState>,
    totals: EncodeTotals,
    usage_seen: bool,
    include_usage_null: bool,
    done: bool,
    failed: bool,
}

impl OpenAiChatStreamEncoder {
    /// 使用已校验的响应标识、模型名与 Unix 秒时间戳创建 encoder。
    pub fn new(
        response_id: impl Into<String>,
        model: impl Into<String>,
        created: i64,
    ) -> Result<Self, EncodeStreamError> {
        let identity = EncodeIdentity::new(response_id.into(), model.into(), created)?;
        Ok(Self {
            identity,
            choices: BTreeMap::new(),
            totals: EncodeTotals::default(),
            usage_seen: false,
            include_usage_null: false,
            done: false,
            failed: false,
        })
    }

    /// 控制普通 JSON chunk 是否显式输出 `usage: null`。
    ///
    /// OpenAI 的 `stream_options.include_usage` 契约要求最终 usage 块之前的其他
    /// JSON chunk 均携带空 usage；未请求时保持字段缺省。
    #[must_use]
    pub const fn with_usage_null_fields(mut self, enabled: bool) -> Self {
        self.include_usage_null = enabled;
        self
    }

    /// 编码一个 Canonical 事件；工具结束事件只更新状态，因此可能返回空字节。
    pub fn encode(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, EncodeStreamError> {
        if self.failed {
            return Err(EncodeStreamError::EncoderFailed);
        }
        if self.done {
            self.failed = true;
            return Err(EncodeStreamError::InvalidSequence);
        }
        let result = validate_stream_event_capabilities(Protocol::OpenAiChat, &event)
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
                if role != Role::Assistant || self.choices.contains_key(&choice_index) {
                    return Err(EncodeStreamError::InvalidSequence);
                }
                if self.choices.len() >= MAX_RESPONSE_CHOICES {
                    return Err(EncodeStreamError::StructureLimitExceeded);
                }
                let bytes = encode_choice_chunk(
                    &self.identity,
                    choice_index,
                    EncodedDelta {
                        role: Some("assistant"),
                        ..EncodedDelta::default()
                    },
                    None,
                    self.include_usage_null,
                )?;
                self.choices.insert(choice_index, ChoiceState::default());
                Ok(bytes)
            }
            CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index,
                delta,
            } => {
                let ContentDelta::Text(text) = delta else {
                    return Err(EncodeStreamError::UnsupportedEvent);
                };
                add_text(&mut self.totals, &text)?;
                let state = active_choice(&mut self.choices, choice_index)?;
                bind_content_index(
                    &mut state.text_index,
                    state.reasoning_index,
                    content_index,
                    &mut self.totals,
                )?;
                encode_choice_chunk(
                    &self.identity,
                    choice_index,
                    EncodedDelta {
                        content: Some(&text),
                        ..EncodedDelta::default()
                    },
                    None,
                    self.include_usage_null,
                )
            }
            CanonicalStreamEvent::ReasoningDelta {
                choice_index,
                content_index,
                text,
                signature,
            } => {
                if signature.is_some() {
                    return Err(EncodeStreamError::UnsupportedEvent);
                }
                add_text(&mut self.totals, &text)?;
                let state = active_choice(&mut self.choices, choice_index)?;
                bind_content_index(
                    &mut state.reasoning_index,
                    state.text_index,
                    content_index,
                    &mut self.totals,
                )?;
                encode_choice_chunk(
                    &self.identity,
                    choice_index,
                    EncodedDelta {
                        reasoning_content: Some(&text),
                        ..EncodedDelta::default()
                    },
                    None,
                    self.include_usage_null,
                )
            }
            CanonicalStreamEvent::CompactionStart { .. }
            | CanonicalStreamEvent::CompactionEnd { .. } => {
                Err(EncodeStreamError::UnsupportedEvent)
            }
            CanonicalStreamEvent::ToolCallStart {
                choice_index,
                tool_index,
                id,
                name,
            } => {
                validate_call_id(&id).map_err(|_| EncodeStreamError::InvalidSequence)?;
                validate_tool_name(&name).map_err(|_| EncodeStreamError::InvalidSequence)?;
                if self.totals.tool_calls >= MAX_TOOL_CALLS {
                    return Err(EncodeStreamError::StructureLimitExceeded);
                }
                let state = active_choice(&mut self.choices, choice_index)?;
                if state.tools.contains_key(&tool_index) {
                    return Err(EncodeStreamError::InvalidSequence);
                }
                let bytes = encode_choice_chunk(
                    &self.identity,
                    choice_index,
                    EncodedDelta {
                        tool_calls: vec![EncodedToolCall {
                            index: tool_index,
                            id: Some(&id),
                            kind: Some("function"),
                            function: EncodedFunction {
                                name: Some(&name),
                                arguments: Some(""),
                            },
                        }],
                        ..EncodedDelta::default()
                    },
                    None,
                    self.include_usage_null,
                )?;
                state.tools.insert(tool_index, ToolState::default());
                self.totals.tool_calls += 1;
                Ok(bytes)
            }
            CanonicalStreamEvent::ToolCallSignature { .. } => {
                Err(EncodeStreamError::UnsupportedEvent)
            }
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index,
                tool_index,
                partial_json,
            } => {
                let state = active_choice(&mut self.choices, choice_index)?;
                let tool = active_tool(state, tool_index)?;
                let next_tool_bytes = tool
                    .arguments
                    .len()
                    .checked_add(partial_json.len())
                    .ok_or(EncodeStreamError::StructureLimitExceeded)?;
                let next_total_bytes = self
                    .totals
                    .argument_bytes
                    .checked_add(partial_json.len())
                    .ok_or(EncodeStreamError::StructureLimitExceeded)?;
                if next_tool_bytes > MAX_ARGUMENT_BYTES
                    || next_total_bytes > MAX_TOTAL_ARGUMENT_BYTES
                {
                    return Err(EncodeStreamError::StructureLimitExceeded);
                }
                let bytes = encode_choice_chunk(
                    &self.identity,
                    choice_index,
                    EncodedDelta {
                        tool_calls: vec![EncodedToolCall {
                            index: tool_index,
                            id: None,
                            kind: None,
                            function: EncodedFunction {
                                name: None,
                                arguments: Some(&partial_json),
                            },
                        }],
                        ..EncodedDelta::default()
                    },
                    None,
                    self.include_usage_null,
                )?;
                tool.arguments.push_str(&partial_json);
                self.totals.argument_bytes = next_total_bytes;
                Ok(bytes)
            }
            CanonicalStreamEvent::ToolCallEnd {
                choice_index,
                tool_index,
            } => {
                let state = active_choice(&mut self.choices, choice_index)?;
                let tool = active_tool(state, tool_index)?;
                let value = parse_value(tool.arguments.as_bytes(), ARGUMENT_JSON_LIMITS)
                    .map_err(map_argument_error)?;
                if !value.is_object() {
                    return Err(EncodeStreamError::UnsupportedEvent);
                }
                let nodes = validate_value_shape(&value, 16, 4_096, 1_024)
                    .map_err(|_| EncodeStreamError::InvalidSequence)?;
                let next_nodes = self
                    .totals
                    .argument_nodes
                    .checked_add(nodes)
                    .ok_or(EncodeStreamError::StructureLimitExceeded)?;
                if next_nodes > MAX_TOTAL_ARGUMENT_NODES {
                    return Err(EncodeStreamError::StructureLimitExceeded);
                }
                tool.ended = true;
                self.totals.argument_nodes = next_nodes;
                Ok(Vec::new())
            }
            CanonicalStreamEvent::Finish {
                choice_index,
                reason,
                stop_sequence,
            } => {
                if stop_sequence.is_some() {
                    return Err(EncodeStreamError::UnsupportedEvent);
                }
                let state = active_choice(&mut self.choices, choice_index)?;
                if state.tools.values().any(|tool| !tool.ended)
                    || (reason == FinishReason::ToolCalls) != !state.tools.is_empty()
                {
                    return Err(EncodeStreamError::InvalidSequence);
                }
                let bytes = encode_choice_chunk(
                    &self.identity,
                    choice_index,
                    EncodedDelta::default(),
                    Some(finish_reason(reason)),
                    self.include_usage_null,
                )?;
                state.finished = true;
                Ok(bytes)
            }
            CanonicalStreamEvent::Usage(usage) => {
                if self.usage_seen || self.choices.is_empty() || !self.all_choices_finished() {
                    return Err(EncodeStreamError::InvalidSequence);
                }
                let usage = encode_usage(&usage)?;
                let bytes = encode_usage_chunk(&self.identity, usage)?;
                self.usage_seen = true;
                Ok(bytes)
            }
            CanonicalStreamEvent::PromptBlocked => Err(EncodeStreamError::UnsupportedEvent),
            CanonicalStreamEvent::Ping => Ok(b": ping\n\n".to_vec()),
            CanonicalStreamEvent::StreamEnd => {
                if self.choices.is_empty() || !self.all_choices_finished() {
                    return Err(EncodeStreamError::InvalidSequence);
                }
                self.done = true;
                Ok(b"data: [DONE]\n\n".to_vec())
            }
            CanonicalStreamEvent::Error(_) => Err(EncodeStreamError::UnsupportedEvent),
        }
    }

    fn all_choices_finished(&self) -> bool {
        self.choices.values().all(|choice| choice.finished)
    }
}

impl fmt::Debug for OpenAiChatStreamEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatStreamEncoder")
            .field("metadata", &"<已脱敏>")
            .field("choice_count", &self.choices.len())
            .field("usage_seen", &self.usage_seen)
            .field("include_usage_null", &self.include_usage_null)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

struct EncodeIdentity {
    id: String,
    model: String,
    created: i64,
}

impl EncodeIdentity {
    fn new(id: String, model: String, created: i64) -> Result<Self, EncodeStreamError> {
        validate_response_id(&id).map_err(|_| EncodeStreamError::InvalidMetadata)?;
        validate_model(&model).map_err(|_| EncodeStreamError::InvalidMetadata)?;
        if created < 0 {
            return Err(EncodeStreamError::InvalidMetadata);
        }
        Ok(Self { id, model, created })
    }
}

#[derive(Default)]
struct ChoiceState {
    finished: bool,
    text_index: Option<u32>,
    reasoning_index: Option<u32>,
    tools: BTreeMap<u32, ToolState>,
}

#[derive(Default)]
struct ToolState {
    arguments: String,
    ended: bool,
}

#[derive(Default)]
struct EncodeTotals {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

fn active_choice(
    choices: &mut BTreeMap<u32, ChoiceState>,
    choice_index: u32,
) -> Result<&mut ChoiceState, EncodeStreamError> {
    let state = choices
        .get_mut(&choice_index)
        .ok_or(EncodeStreamError::InvalidSequence)?;
    if state.finished {
        return Err(EncodeStreamError::InvalidSequence);
    }
    Ok(state)
}

fn active_tool(
    choice: &mut ChoiceState,
    tool_index: u32,
) -> Result<&mut ToolState, EncodeStreamError> {
    let tool = choice
        .tools
        .get_mut(&tool_index)
        .ok_or(EncodeStreamError::InvalidSequence)?;
    if tool.ended {
        return Err(EncodeStreamError::InvalidSequence);
    }
    Ok(tool)
}

fn bind_content_index(
    current: &mut Option<u32>,
    other: Option<u32>,
    content_index: u32,
    totals: &mut EncodeTotals,
) -> Result<(), EncodeStreamError> {
    match *current {
        Some(expected) if expected == content_index => Ok(()),
        Some(_) => Err(EncodeStreamError::InvalidSequence),
        None if other == Some(content_index) => Err(EncodeStreamError::InvalidSequence),
        None => {
            totals.content_blocks = totals
                .content_blocks
                .checked_add(1)
                .ok_or(EncodeStreamError::StructureLimitExceeded)?;
            if totals.content_blocks > MAX_CONTENT_BLOCKS {
                return Err(EncodeStreamError::StructureLimitExceeded);
            }
            *current = Some(content_index);
            Ok(())
        }
    }
}

fn add_text(totals: &mut EncodeTotals, text: &str) -> Result<(), EncodeStreamError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(EncodeStreamError::StructureLimitExceeded);
    }
    totals.text_bytes = totals
        .text_bytes
        .checked_add(text.len())
        .ok_or(EncodeStreamError::StructureLimitExceeded)?;
    if totals.text_bytes > MAX_TOTAL_TEXT_BYTES {
        return Err(EncodeStreamError::StructureLimitExceeded);
    }
    Ok(())
}

fn encode_choice_chunk(
    identity: &EncodeIdentity,
    choice_index: u32,
    delta: EncodedDelta<'_>,
    finish_reason: Option<&'static str>,
    include_usage_null: bool,
) -> Result<Vec<u8>, EncodeStreamError> {
    encode_chunk(EncodedChunk {
        id: &identity.id,
        object: "chat.completion.chunk",
        created: identity.created,
        model: &identity.model,
        choices: vec![EncodedChoice {
            index: choice_index,
            delta,
            logprobs: None,
            finish_reason,
        }],
        usage: include_usage_null.then_some(None),
    })
}

fn encode_usage_chunk(
    identity: &EncodeIdentity,
    usage: EncodedUsage,
) -> Result<Vec<u8>, EncodeStreamError> {
    encode_chunk(EncodedChunk {
        id: &identity.id,
        object: "chat.completion.chunk",
        created: identity.created,
        model: &identity.model,
        choices: Vec::new(),
        usage: Some(Some(usage)),
    })
}

fn encode_chunk(chunk: EncodedChunk<'_>) -> Result<Vec<u8>, EncodeStreamError> {
    let data = serde_json::to_vec(&chunk).map_err(|_| EncodeStreamError::Serialization)?;
    let total = b"data: "
        .len()
        .checked_add(data.len())
        .and_then(|value| value.checked_add(2))
        .ok_or(EncodeStreamError::StructureLimitExceeded)?;
    if total > MAX_SSE_EVENT_BYTES {
        return Err(EncodeStreamError::StructureLimitExceeded);
    }
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(b"data: ");
    output.extend_from_slice(&data);
    output.extend_from_slice(b"\n\n");
    Ok(output)
}

fn encode_usage(usage: &Usage) -> Result<EncodedUsage, EncodeStreamError> {
    let details = usage.details();
    if details.cache_creation_5m().get() != 0 || details.cache_creation_1h().get() != 0 {
        return Err(EncodeStreamError::InvalidUsage);
    }
    let prompt_tokens = usage
        .checked_input_tokens()
        .map_err(|_| EncodeStreamError::InvalidUsage)?
        .get();
    let completion_tokens = usage.output_tokens().get();
    let total_tokens = prompt_tokens
        .checked_add(completion_tokens)
        .ok_or(EncodeStreamError::InvalidUsage)?;
    let cached_tokens = details.cache_read().get();
    let audio_input = details.audio_input().get();
    let reasoning = details.reasoning().get();
    let audio_output = details.audio_output().get();
    Ok(EncodedUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        prompt_tokens_details: (cached_tokens != 0 || audio_input != 0).then_some(
            EncodedPromptDetails {
                cached_tokens,
                audio_tokens: audio_input,
            },
        ),
        completion_tokens_details: (reasoning != 0 || audio_output != 0).then_some(
            EncodedCompletionDetails {
                reasoning_tokens: reasoning,
                audio_tokens: audio_output,
            },
        ),
    })
}

const fn finish_reason(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::ContentFilter => "content_filter",
    }
}

fn map_argument_error(error: BoundedJsonError) -> EncodeStreamError {
    match error {
        BoundedJsonError::LimitExceeded => EncodeStreamError::StructureLimitExceeded,
        BoundedJsonError::InvalidJson | BoundedJsonError::DuplicateKey => {
            EncodeStreamError::InvalidSequence
        }
    }
}
