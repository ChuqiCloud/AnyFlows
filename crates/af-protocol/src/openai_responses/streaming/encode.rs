use std::fmt;

use af_domain::{Operation, Protocol, Role, UpstreamError};
use serde_json::{Map, Value};

use super::{EncodeStreamError, encode_state::OutputEncodeState, wire::PendingEvent};
use crate::openai_responses::{
    build_response, build_response::BuildResponseError, convert::validate_model,
    parse_response::validate_response_id,
};
use crate::sse::MAX_SSE_EVENT_BYTES;
use crate::{
    CanonicalResponse, CanonicalStreamEvent, Message, ResponseChoice, Usage,
    validate_stream_event_capabilities,
};

/// 将单候选 Canonical 流编码为 OpenAI Responses 官方语义 SSE。
///
/// encoder 为每个事件分配从零开始的连续 `sequence_number`，并在 `StreamEnd`
/// 生成包含最终输出与 usage 的完整终态 Response 快照。
pub struct OpenAiResponsesStreamEncoder {
    identity: EncodeIdentity,
    outputs: OutputEncodeState,
    next_sequence: u64,
    started: bool,
    usage: Option<Usage>,
    usage_seen: bool,
    error_seen: bool,
    done: bool,
    failed: bool,
}

impl OpenAiResponsesStreamEncoder {
    /// 使用响应标识、模型名与 Unix 秒创建时间构造 encoder。
    pub fn new(
        response_id: impl Into<String>,
        model: impl Into<String>,
        created_at: i64,
    ) -> Result<Self, EncodeStreamError> {
        Ok(Self {
            identity: EncodeIdentity::new(response_id.into(), model.into(), created_at)?,
            outputs: OutputEncodeState::default(),
            next_sequence: 0,
            started: false,
            usage: None,
            usage_seen: false,
            error_seen: false,
            done: false,
            failed: false,
        })
    }

    /// 编码一个 Canonical 事件；仅更新状态的事件可能返回空字节。
    pub fn encode(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, EncodeStreamError> {
        if self.failed {
            return Err(EncodeStreamError::EncoderFailed);
        }
        if self.done {
            self.failed = true;
            return Err(EncodeStreamError::InvalidSequence);
        }
        let result = validate_stream_event_capabilities(Protocol::OpenAiResponses, &event)
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
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs
                    .content_delta(&self.identity.id, content_index, delta, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::ReasoningDelta {
                choice_index,
                content_index,
                text,
                signature,
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs.reasoning_delta(
                    &self.identity.id,
                    content_index,
                    text,
                    signature,
                    &mut events,
                )?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::ToolCallStart {
                choice_index,
                tool_index,
                id,
                name,
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs
                    .tool_start(&self.identity.id, tool_index, id, name, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::ToolCallSignature { .. } => {
                Err(EncodeStreamError::UnsupportedEvent)
            }
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index,
                tool_index,
                partial_json,
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs
                    .tool_arguments_delta(tool_index, partial_json, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::ToolCallEnd {
                choice_index,
                tool_index,
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs.tool_end(tool_index, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::CompactionStart {
                choice_index, item, ..
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs.compaction(item, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::CompactionEnd {
                choice_index, item, ..
            } => {
                self.require_active_choice(choice_index)?;
                let mut events = Vec::new();
                self.outputs.compaction_end(item, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::Finish {
                choice_index,
                reason,
                stop_sequence,
            } => {
                self.require_active_choice(choice_index)?;
                if stop_sequence.is_some() {
                    return Err(EncodeStreamError::UnsupportedEvent);
                }
                let mut events = Vec::new();
                self.outputs.finish(reason, &mut events)?;
                self.encode_events(events)
            }
            CanonicalStreamEvent::Usage(usage) => self.encode_usage(usage),
            CanonicalStreamEvent::Ping => Ok(b": ping\n\n".to_vec()),
            CanonicalStreamEvent::StreamEnd => self.encode_stream_end(),
            CanonicalStreamEvent::Error(error) => self.encode_error(error),
            CanonicalStreamEvent::PromptBlocked => Err(EncodeStreamError::UnsupportedEvent),
        }
    }

    fn encode_message_start(
        &mut self,
        choice_index: u32,
        role: Role,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        if self.started || self.error_seen || choice_index != 0 || role != Role::Assistant {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.started = true;
        let response = lifecycle_response(&self.identity);
        self.encode_events(vec![
            PendingEvent::new("response.created", [("response", response.clone())]),
            PendingEvent::new("response.in_progress", [("response", response)]),
        ])
    }

    fn encode_usage(&mut self, usage: Usage) -> Result<Vec<u8>, EncodeStreamError> {
        if !self.started
            || self.error_seen
            || self.usage_seen
            || self.outputs.finish_reason().is_none()
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        validate_usage(&usage)?;
        self.usage = Some(usage);
        self.usage_seen = true;
        Ok(Vec::new())
    }

    fn encode_stream_end(&mut self) -> Result<Vec<u8>, EncodeStreamError> {
        if self.error_seen {
            self.done = true;
            return Ok(Vec::new());
        }
        if !self.started {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let reason = self
            .outputs
            .finish_reason()
            .ok_or(EncodeStreamError::InvalidSequence)?;
        let choice = ResponseChoice::new(
            0,
            Message::new(Role::Assistant, self.outputs.content().to_vec()),
            reason,
        );
        let response = CanonicalResponse::new(
            Operation::Responses,
            self.identity.id.clone(),
            self.identity.model.clone(),
            Some(self.identity.created_at),
            vec![choice],
            self.usage,
        );
        let mut response = build_response(&response).map_err(map_build_error)?;
        patch_item_statuses(&mut response, self.outputs.item_statuses())?;
        let kind = match response
            .as_object()
            .and_then(|response| response.get("status"))
            .and_then(Value::as_str)
        {
            Some("completed") => "response.completed",
            Some("incomplete") => "response.incomplete",
            _ => return Err(EncodeStreamError::InvalidSequence),
        };
        let bytes = self.encode_events(vec![PendingEvent::new(kind, [("response", response)])])?;
        self.done = true;
        Ok(bytes)
    }

    fn encode_error(&mut self, error: UpstreamError) -> Result<Vec<u8>, EncodeStreamError> {
        if self.error_seen || self.outputs.finish_reason().is_some() || self.usage_seen {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let (code, message) = encoded_error(error);
        let bytes = self.encode_events(vec![PendingEvent::new(
            "error",
            [
                ("code", Value::String(code.to_owned())),
                ("message", Value::String(message.to_owned())),
                ("param", Value::Null),
            ],
        )])?;
        self.error_seen = true;
        Ok(bytes)
    }

    fn encode_events(&mut self, events: Vec<PendingEvent>) -> Result<Vec<u8>, EncodeStreamError> {
        let mut output = Vec::new();
        for event in events {
            let kind = event.kind();
            let value = event.into_value(self.next_sequence);
            let data = serde_json::to_vec(&value).map_err(|_| EncodeStreamError::Serialization)?;
            let event_bytes = b"event: "
                .len()
                .checked_add(kind.len())
                .and_then(|value| value.checked_add(1))
                .and_then(|value| value.checked_add(b"data: ".len()))
                .and_then(|value| value.checked_add(data.len()))
                .and_then(|value| value.checked_add(2))
                .ok_or(EncodeStreamError::StructureLimitExceeded)?;
            if event_bytes > MAX_SSE_EVENT_BYTES {
                return Err(EncodeStreamError::StructureLimitExceeded);
            }
            output
                .try_reserve(event_bytes)
                .map_err(|_| EncodeStreamError::StructureLimitExceeded)?;
            output.extend_from_slice(b"event: ");
            output.extend_from_slice(kind.as_bytes());
            output.push(b'\n');
            output.extend_from_slice(b"data: ");
            output.extend_from_slice(&data);
            output.extend_from_slice(b"\n\n");
            self.next_sequence = self
                .next_sequence
                .checked_add(1)
                .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        }
        Ok(output)
    }

    fn require_active_choice(&self, choice_index: u32) -> Result<(), EncodeStreamError> {
        if self.started && !self.error_seen && choice_index == 0 {
            Ok(())
        } else {
            Err(EncodeStreamError::InvalidSequence)
        }
    }
}

impl fmt::Debug for OpenAiResponsesStreamEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesStreamEncoder")
            .field("metadata", &"<已脱敏>")
            .field("next_sequence", &self.next_sequence)
            .field("started", &self.started)
            .field("usage_seen", &self.usage_seen)
            .field("error_seen", &self.error_seen)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

struct EncodeIdentity {
    id: String,
    model: String,
    created_at: i64,
}

impl EncodeIdentity {
    fn new(id: String, model: String, created_at: i64) -> Result<Self, EncodeStreamError> {
        validate_response_id(&id).map_err(|_| EncodeStreamError::InvalidMetadata)?;
        validate_model(&model).map_err(|_| EncodeStreamError::InvalidMetadata)?;
        if created_at < 0 {
            return Err(EncodeStreamError::InvalidMetadata);
        }
        Ok(Self {
            id,
            model,
            created_at,
        })
    }
}

fn lifecycle_response(identity: &EncodeIdentity) -> Value {
    Value::Object(Map::from_iter([
        ("id".to_owned(), Value::String(identity.id.clone())),
        ("object".to_owned(), Value::String("response".to_owned())),
        (
            "created_at".to_owned(),
            Value::Number(identity.created_at.into()),
        ),
        ("status".to_owned(), Value::String("in_progress".to_owned())),
        ("error".to_owned(), Value::Null),
        ("incomplete_details".to_owned(), Value::Null),
        ("model".to_owned(), Value::String(identity.model.clone())),
        ("output".to_owned(), Value::Array(Vec::new())),
        ("usage".to_owned(), Value::Null),
    ]))
}

fn patch_item_statuses(response: &mut Value, statuses: &[&str]) -> Result<(), EncodeStreamError> {
    let items = response
        .as_object_mut()
        .and_then(|response| response.get_mut("output"))
        .and_then(Value::as_array_mut)
        .ok_or(EncodeStreamError::Serialization)?;
    if items.len() != statuses.len() {
        return Err(EncodeStreamError::InvalidSequence);
    }
    for (item, status) in items.iter_mut().zip(statuses) {
        item.as_object_mut()
            .ok_or(EncodeStreamError::Serialization)?
            .insert("status".to_owned(), Value::String((*status).to_owned()));
    }
    Ok(())
}

fn validate_usage(usage: &Usage) -> Result<(), EncodeStreamError> {
    let details = usage.details();
    if details.cache_creation_5m().get() != 0
        || details.cache_creation_1h().get() != 0
        || details.audio_input().get() != 0
        || details.audio_output().get() != 0
    {
        return Err(EncodeStreamError::InvalidUsage);
    }
    usage
        .checked_input_tokens()
        .and_then(|_| usage.checked_total_tokens())
        .map(|_| ())
        .map_err(|_| EncodeStreamError::InvalidUsage)
}

const fn encoded_error(error: UpstreamError) -> (&'static str, &'static str) {
    match error {
        UpstreamError::RateLimited { .. } => ("rate_limit_exceeded", "Request was rate limited."),
        UpstreamError::QuotaExhausted => ("insufficient_quota", "Quota is exhausted."),
        UpstreamError::ModelUnsupported => ("model_not_found", "Model is not available."),
        UpstreamError::BadRequest => ("invalid_prompt", "Request was rejected."),
        UpstreamError::Overloaded { .. }
        | UpstreamError::AuthExpired
        | UpstreamError::AuthRevoked
        | UpstreamError::AccountDisabled
        | UpstreamError::ProtocolError
        | UpstreamError::ServerError { .. }
        | UpstreamError::Network { .. } => ("server_error", "Upstream request failed."),
    }
}

fn map_build_error(error: BuildResponseError) -> EncodeStreamError {
    match error {
        BuildResponseError::StructureLimitExceeded => EncodeStreamError::StructureLimitExceeded,
        BuildResponseError::UnsupportedCapability(error) => {
            EncodeStreamError::UnsupportedCapability(error)
        }
        BuildResponseError::UnsupportedFeature => EncodeStreamError::UnsupportedEvent,
        BuildResponseError::UnsupportedOperation
        | BuildResponseError::InvalidValue
        | BuildResponseError::RawProtocolMismatch
        | BuildResponseError::FieldConflict => EncodeStreamError::InvalidSequence,
    }
}
