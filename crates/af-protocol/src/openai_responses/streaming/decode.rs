use std::fmt;

use af_domain::{Operation, Protocol, Role, UpstreamError, UpstreamServerStatus};
use serde_json::{Map, Value};

use super::{
    ParseStreamError,
    decode_state::OutputDecodeState,
    wire::{
        StreamEventWire, is_known_unsupported_event_type, is_supported_event_type,
        uses_known_unsupported_payload,
    },
};
use crate::bounded_json::{BoundedJsonError, parse_value};
use crate::openai_responses::{
    parse_response,
    parse_response::{ParseResponseError, validate_response_id},
    response_raw::{ResponseRawError, validate_raw_fields},
    response_wire::{ResponseObjectWire, ResponseStatusWire, ResponsesResponseWire},
};
use crate::sse::{SseEvent, SseParseError, SseParser};
use crate::{CanonicalResponse, CanonicalStreamEvent, FinishReason, Usage};

/// 将 OpenAI Responses SSE 解码为严格有序的 Canonical 流事件。
///
/// decoder 校验官方事件序号、Response 身份、Item/Part 生命周期和终态冗余快照；
/// 任一错误后实例进入失败状态，调用方必须丢弃。
pub struct OpenAiResponsesStreamDecoder {
    parser: SseParser,
    phase: LifecyclePhase,
    identity: Option<StreamIdentity>,
    outputs: OutputDecodeState,
    next_sequence: u64,
    done: bool,
    compatibility_done_seen: bool,
    failed: bool,
}

impl OpenAiResponsesStreamDecoder {
    /// 使用默认 SSE 单事件上限创建 decoder。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用调用方收紧后的 SSE 单事件上限创建 decoder。
    pub fn with_max_event_bytes(max_event_bytes: usize) -> Result<Self, SseParseError> {
        Ok(Self {
            parser: SseParser::with_max_event_bytes(max_event_bytes)?,
            phase: LifecyclePhase::AwaitingCreated,
            identity: None,
            outputs: OutputDecodeState::default(),
            next_sequence: 0,
            done: false,
            compatibility_done_seen: false,
            failed: false,
        })
    }

    /// 推入任意上游字节分片，并返回本次完成解码的 Canonical 事件。
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

    /// 在传输 EOF 时确认 framing 空闲且已收到官方终态事件。
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
            if !self.compatibility_done_seen && is_done_sentinel(&frame) {
                self.compatibility_done_seen = true;
                return Ok(());
            }
            return Err(ParseStreamError::DataAfterDone);
        }
        if frame.is_comment() {
            output.push(CanonicalStreamEvent::Ping);
            return Ok(());
        }
        if is_done_sentinel(&frame) {
            return Err(ParseStreamError::InvalidSequence);
        }

        let value =
            parse_value(frame.data(), super::super::REQUEST_JSON_LIMITS).map_err(map_json_error)?;
        let kind = value
            .as_object()
            .and_then(|event| event.get("type"))
            .and_then(Value::as_str)
            .ok_or(ParseStreamError::InvalidValue)?
            .to_owned();
        if is_known_unsupported_event_type(&kind) {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        if !is_supported_event_type(&kind) {
            return Err(ParseStreamError::InvalidValue);
        }
        if frame.event_name() != Some(kind.as_str()) {
            return Err(ParseStreamError::InvalidSequence);
        }
        if uses_known_unsupported_payload(&kind, &value) {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        let event: StreamEventWire =
            serde_json::from_value(value).map_err(|_| ParseStreamError::InvalidValue)?;
        self.accept_sequence(event.sequence_number())?;
        self.decode_event(event, output)
    }

    fn decode_event(
        &mut self,
        event: StreamEventWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        match event {
            StreamEventWire::ResponseCreated { response, .. } => {
                self.decode_created(response, output)
            }
            StreamEventWire::ResponseInProgress { response, .. } => {
                self.decode_in_progress(response)
            }
            StreamEventWire::OutputItemAdded {
                output_index, item, ..
            } => {
                self.require_in_progress()?;
                self.outputs.item_added(output_index, item, output)
            }
            StreamEventWire::OutputItemDone {
                output_index, item, ..
            } => {
                self.require_in_progress()?;
                self.outputs.item_done(output_index, item, output)
            }
            StreamEventWire::ContentPartAdded {
                item_id,
                output_index,
                content_index,
                part,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .content_part_added(&item_id, output_index, content_index, part, output)
            }
            StreamEventWire::ContentPartDone {
                item_id,
                output_index,
                content_index,
                part,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .content_part_done(&item_id, output_index, content_index, part, output)
            }
            StreamEventWire::OutputTextDelta {
                item_id,
                output_index,
                content_index,
                delta,
                logprobs,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs.output_text_delta(
                    &item_id,
                    output_index,
                    content_index,
                    delta,
                    &logprobs,
                    output,
                )
            }
            StreamEventWire::OutputTextDone {
                item_id,
                output_index,
                content_index,
                text,
                logprobs,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs.output_text_done(
                    &item_id,
                    output_index,
                    content_index,
                    &text,
                    &logprobs,
                )
            }
            StreamEventWire::FunctionArgumentsDelta {
                item_id,
                output_index,
                delta,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .function_arguments_delta(&item_id, output_index, delta, output)
            }
            StreamEventWire::FunctionArgumentsDone {
                item_id,
                output_index,
                arguments,
                name,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .function_arguments_done(&item_id, output_index, &arguments, &name)
            }
            StreamEventWire::ReasoningSummaryPartAdded {
                item_id,
                output_index,
                summary_index,
                part,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .reasoning_part_added(&item_id, output_index, summary_index, part)
            }
            StreamEventWire::ReasoningSummaryPartDone {
                item_id,
                output_index,
                summary_index,
                part,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .reasoning_part_done(&item_id, output_index, summary_index, part)
            }
            StreamEventWire::ReasoningSummaryTextDelta {
                item_id,
                output_index,
                summary_index,
                delta,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs.reasoning_text_delta(
                    &item_id,
                    output_index,
                    summary_index,
                    delta,
                    output,
                )
            }
            StreamEventWire::ReasoningSummaryTextDone {
                item_id,
                output_index,
                summary_index,
                text,
                ..
            } => {
                self.require_in_progress()?;
                self.outputs
                    .reasoning_text_done(&item_id, output_index, summary_index, &text)
            }
            StreamEventWire::ResponseCompleted { response, .. } => {
                self.decode_terminal(response, false, output)
            }
            StreamEventWire::ResponseIncomplete { response, .. } => {
                self.decode_terminal(response, true, output)
            }
            StreamEventWire::ResponseFailed { response, .. } => {
                self.decode_failed(response, output)
            }
            StreamEventWire::Error { code, .. } => {
                let error = map_upstream_error(code.as_deref());
                self.finish_with_error(error, output)
            }
        }
    }

    fn decode_created(
        &mut self,
        response: Value,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.phase != LifecyclePhase::AwaitingCreated {
            return Err(ParseStreamError::InvalidSequence);
        }
        let identity = parse_lifecycle_snapshot(response, ResponseStatusWire::InProgress)?;
        self.identity = Some(identity);
        self.phase = LifecyclePhase::Created;
        output.push(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        });
        Ok(())
    }

    fn decode_in_progress(&mut self, response: Value) -> Result<(), ParseStreamError> {
        if self.phase != LifecyclePhase::Created {
            return Err(ParseStreamError::InvalidSequence);
        }
        let actual = parse_lifecycle_snapshot(response, ResponseStatusWire::InProgress)?;
        if self.identity.as_ref() != Some(&actual) {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.phase = LifecyclePhase::InProgress;
        Ok(())
    }

    fn decode_terminal(
        &mut self,
        response: Value,
        incomplete: bool,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        self.require_in_progress()?;
        if self.outputs.has_active_item() {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.outputs.validate_terminal_item_ids(&response)?;
        let canonical = parse_terminal_response(&response)?;
        self.validate_terminal_canonical(&canonical)?;
        let choice = canonical
            .choices
            .first()
            .ok_or(ParseStreamError::InvalidSequence)?;
        if incomplete
            != matches!(
                choice.finish_reason,
                FinishReason::Length | FinishReason::ContentFilter
            )
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        output.push(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: choice.finish_reason,
            stop_sequence: None,
        });
        if let Some(usage) = canonical.usage {
            output.push(CanonicalStreamEvent::Usage(usage));
        }
        self.complete_stream(output);
        Ok(())
    }

    fn decode_failed(
        &mut self,
        response: Value,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if !matches!(
            self.phase,
            LifecyclePhase::Created | LifecyclePhase::InProgress
        ) || self.outputs.has_active_item()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        let code = validate_failed_snapshot(&response, self.identity.as_ref(), &self.outputs)?;
        let normalized = normalize_failed_response(response)?;
        let canonical = parse_terminal_response(&normalized)?;
        self.validate_terminal_canonical(&canonical)?;
        if let Some(usage) = canonical.usage {
            output.push(CanonicalStreamEvent::Usage(usage));
        }
        output.push(CanonicalStreamEvent::Error(map_upstream_error(
            code.as_deref(),
        )));
        self.complete_stream(output);
        Ok(())
    }

    fn validate_terminal_canonical(
        &self,
        response: &CanonicalResponse,
    ) -> Result<(), ParseStreamError> {
        let identity = self
            .identity
            .as_ref()
            .ok_or(ParseStreamError::InvalidSequence)?;
        if response.operation != Operation::Responses
            || response.id != identity.id
            || response.model != identity.model
            || response.created_at != Some(identity.created_at)
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        let [choice] = response.choices.as_slice() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if choice.index != 0
            || choice.message.role != Role::Assistant
            || choice.stop_sequence.is_some()
            || choice.message.content != self.outputs.content()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        let raw = response
            .raw_passthrough()
            .map(|raw| raw.fields_for_protocol(Protocol::OpenAiResponses))
            .transpose()
            .map_err(|_| ParseStreamError::InvalidSequence)?;
        let stable = stable_raw(raw.cloned().unwrap_or_default());
        if stable != identity.stable_raw {
            return Err(ParseStreamError::InvalidSequence);
        }
        Ok(())
    }

    fn finish_with_error(
        &mut self,
        error: UpstreamError,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.phase == LifecyclePhase::Done {
            return Err(ParseStreamError::DataAfterDone);
        }
        output.push(CanonicalStreamEvent::Error(error));
        self.complete_stream(output);
        Ok(())
    }

    fn complete_stream(&mut self, output: &mut Vec<CanonicalStreamEvent>) {
        self.phase = LifecyclePhase::Done;
        self.done = true;
        output.push(CanonicalStreamEvent::StreamEnd);
    }

    fn require_in_progress(&self) -> Result<(), ParseStreamError> {
        if self.phase == LifecyclePhase::InProgress {
            Ok(())
        } else {
            Err(ParseStreamError::InvalidSequence)
        }
    }

    fn accept_sequence(&mut self, sequence: u64) -> Result<(), ParseStreamError> {
        if sequence != self.next_sequence {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        Ok(())
    }
}

impl Default for OpenAiResponsesStreamDecoder {
    fn default() -> Self {
        Self {
            parser: SseParser::new(),
            phase: LifecyclePhase::AwaitingCreated,
            identity: None,
            outputs: OutputDecodeState::default(),
            next_sequence: 0,
            done: false,
            compatibility_done_seen: false,
            failed: false,
        }
    }
}

impl fmt::Debug for OpenAiResponsesStreamDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesStreamDecoder")
            .field("has_identity", &self.identity.is_some())
            .field("next_sequence", &self.next_sequence)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum LifecyclePhase {
    #[default]
    AwaitingCreated,
    Created,
    InProgress,
    Done,
}

#[derive(Eq, PartialEq)]
struct StreamIdentity {
    id: String,
    model: String,
    created_at: i64,
    stable_raw: Map<String, Value>,
}

fn parse_lifecycle_snapshot(
    response: Value,
    expected_status: ResponseStatusWire,
) -> Result<StreamIdentity, ParseStreamError> {
    let wire: ResponsesResponseWire =
        serde_json::from_value(response).map_err(|_| ParseStreamError::InvalidValue)?;
    let ResponsesResponseWire {
        id,
        object,
        created_at,
        status,
        error,
        incomplete_details,
        model,
        output,
        output_text,
        moderation,
        usage,
        extra,
    } = wire;
    if !matches!(object, ResponseObjectWire::Response)
        || status != expected_status
        || error.is_some()
        || incomplete_details.is_some()
        || !output.is_empty()
        || output_text.is_some()
        || moderation.is_some()
        || usage.is_some()
    {
        return Err(ParseStreamError::InvalidSequence);
    }
    validate_response_id(&id).map_err(map_response_error)?;
    super::super::convert::validate_model(&model).map_err(|_| ParseStreamError::InvalidValue)?;
    if created_at < 0 {
        return Err(ParseStreamError::InvalidValue);
    }
    validate_raw_fields(&extra, status, created_at).map_err(map_raw_error)?;
    Ok(StreamIdentity {
        id,
        model,
        created_at,
        stable_raw: stable_raw(extra),
    })
}

fn parse_terminal_response(response: &Value) -> Result<CanonicalResponse, ParseStreamError> {
    let bytes = serde_json::to_vec(response).map_err(|_| ParseStreamError::InvalidValue)?;
    parse_response(&bytes).map_err(map_response_error)
}

fn validate_failed_snapshot(
    response: &Value,
    identity: Option<&StreamIdentity>,
    outputs: &OutputDecodeState,
) -> Result<Option<String>, ParseStreamError> {
    let root = response.as_object().ok_or(ParseStreamError::InvalidValue)?;
    if root
        .get("output")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.as_object()
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    .is_some_and(|kind| !matches!(kind, "message" | "function_call" | "reasoning"))
            })
        })
    {
        return Err(ParseStreamError::UnsupportedFeature);
    }
    let wire: ResponsesResponseWire =
        serde_json::from_value(response.clone()).map_err(|_| ParseStreamError::InvalidValue)?;
    let error = wire.error.as_ref().ok_or(ParseStreamError::InvalidValue)?;
    let code = validate_error_value(error)?;
    if wire.status != ResponseStatusWire::Failed
        || !matches!(wire.object, ResponseObjectWire::Response)
        || wire.incomplete_details.is_some()
        || wire.output_text.is_some()
        || wire.moderation.is_some()
    {
        return Err(ParseStreamError::InvalidSequence);
    }
    validate_response_id(&wire.id).map_err(map_response_error)?;
    super::super::convert::validate_model(&wire.model)
        .map_err(|_| ParseStreamError::InvalidValue)?;
    if wire.created_at < 0 {
        return Err(ParseStreamError::InvalidValue);
    }
    validate_raw_fields(&wire.extra, wire.status, wire.created_at).map_err(map_raw_error)?;
    let expected = identity.ok_or(ParseStreamError::InvalidSequence)?;
    if wire.id != expected.id
        || wire.model != expected.model
        || wire.created_at != expected.created_at
        || stable_raw(wire.extra) != expected.stable_raw
    {
        return Err(ParseStreamError::InvalidSequence);
    }
    outputs.validate_terminal_item_ids(response)?;
    Ok(code)
}

fn normalize_failed_response(mut response: Value) -> Result<Value, ParseStreamError> {
    let root = response
        .as_object_mut()
        .ok_or(ParseStreamError::InvalidValue)?;
    root.insert("status".to_owned(), Value::String("completed".to_owned()));
    root.insert("error".to_owned(), Value::Null);
    root.insert("incomplete_details".to_owned(), Value::Null);
    let items = root
        .get_mut("output")
        .and_then(Value::as_array_mut)
        .ok_or(ParseStreamError::InvalidValue)?;
    for item in items {
        let item = item.as_object_mut().ok_or(ParseStreamError::InvalidValue)?;
        if item.contains_key("status") {
            item.insert("status".to_owned(), Value::String("completed".to_owned()));
        }
    }
    Ok(response)
}

fn validate_error_value(error: &Value) -> Result<Option<String>, ParseStreamError> {
    let error = error.as_object().ok_or(ParseStreamError::InvalidValue)?;
    if error
        .keys()
        .any(|key| !matches!(key.as_str(), "code" | "message" | "param"))
        || !error.get("message").is_some_and(Value::is_string)
        || error
            .get("param")
            .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        return Err(ParseStreamError::InvalidValue);
    }
    match error.get("code") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(code)) if !code.is_empty() && !code.chars().any(char::is_control) => {
            Ok(Some(code.clone()))
        }
        _ => Err(ParseStreamError::InvalidValue),
    }
}

fn stable_raw(mut fields: Map<String, Value>) -> Map<String, Value> {
    fields.remove("completed_at");
    fields
}

fn is_done_sentinel(frame: &SseEvent) -> bool {
    matches!(frame.event_name(), None | Some("message")) && frame.data().trim_ascii() == b"[DONE]"
}

fn map_upstream_error(code: Option<&str>) -> UpstreamError {
    match code {
        Some("rate_limit_exceeded" | "rate_limit_error") => UpstreamError::RateLimited,
        Some("insufficient_quota" | "billing_error") => UpstreamError::QuotaExhausted,
        Some("invalid_api_key" | "authentication_error") => UpstreamError::AuthExpired,
        Some("permission_denied" | "permission_error") => UpstreamError::AuthRevoked,
        Some("model_not_found" | "model_unsupported") => UpstreamError::ModelUnsupported,
        Some("invalid_prompt" | "invalid_request_error") => UpstreamError::BadRequest,
        Some("server_error" | "vector_store_timeout") => UpstreamError::ServerError {
            status: UpstreamServerStatus::new(500).expect("500 必须是合法服务器状态码"),
        },
        Some("overloaded_error") => UpstreamError::Overloaded,
        _ => UpstreamError::ProtocolError,
    }
}

fn map_json_error(error: BoundedJsonError) -> ParseStreamError {
    match error {
        BoundedJsonError::InvalidJson => ParseStreamError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseStreamError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseStreamError::StructureLimitExceeded,
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

fn map_raw_error(error: ResponseRawError) -> ParseStreamError {
    match error {
        ResponseRawError::InvalidValue => ParseStreamError::InvalidValue,
        ResponseRawError::UnsupportedFeature => ParseStreamError::UnsupportedFeature,
        ResponseRawError::StructureLimitExceeded => ParseStreamError::StructureLimitExceeded,
    }
}
