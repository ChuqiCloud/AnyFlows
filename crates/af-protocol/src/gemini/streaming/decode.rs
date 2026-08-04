use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

use af_domain::{Role, UpstreamError, UpstreamServerStatus};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::ParseStreamError;
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount,
    bounded_json::{BoundedJsonError, parse_value},
    gemini::{
        ParseRequestError, ParseResponseError,
        convert::{
            MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_MEDIA_BYTES, MAX_PARTS_PER_CONTENT,
            MAX_SIGNATURE_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES,
            MAX_TOTAL_JSON_NODES, MAX_TOTAL_SIGNATURE_BYTES, MAX_TOTAL_TEXT_BYTES,
            TOOL_PAYLOAD_LIMITS, decode_base64, normalize_mime_type, validate_call_id,
            validate_json_object, validate_model, validate_tool_name,
        },
        parse_response::{
            ConvertedUsage, MAX_FINISH_MESSAGE_BYTES, MAX_RESPONSE_CHOICES, convert_usage_metadata,
            uses_known_unsupported_feature, validate_model_status, validate_prompt_feedback,
            validate_response_id, validate_safety_ratings,
        },
        response_wire::{CandidateWire, FinishReasonWire, GenerateContentResponseWire},
        wire::{BlobWire, ContentWire, Field, FunctionCallWire, PartWire},
    },
    sse::{SseEvent, SseParseError, SseParser},
};

const MAX_ERROR_MESSAGE_BYTES: usize = 16 * 1024;

/// 将 Gemini `streamGenerateContent` SSE 解码为有序 Canonical 事件。
///
/// Gemini 以传输 EOF 作为官方流终点；兼容上游附加的 `[DONE]` 也会被接受。usage
/// 按累计快照校验并只在逻辑终点发送一次，调用方不得把中间快照相加。
pub struct GeminiGenerateContentStreamDecoder {
    parser: SseParser,
    identity: Option<StreamIdentity>,
    choices: BTreeMap<u32, ChoiceState>,
    totals: DecodeTotals,
    usage: Option<ConvertedUsage>,
    prompt_blocked: bool,
    data_seen: bool,
    done: bool,
    failed: bool,
}

impl GeminiGenerateContentStreamDecoder {
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
            usage: None,
            prompt_blocked: false,
            data_seen: false,
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

    /// 在传输 EOF 时关闭 parser，并返回最终 usage 与 `StreamEnd`。
    ///
    /// 若兼容上游已经发送 `[DONE]`，这里仅确认 framing 完整并返回空事件列表。
    pub fn finish(&mut self) -> Result<Vec<CanonicalStreamEvent>, ParseStreamError> {
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

    fn finish_inner(&mut self) -> Result<Vec<CanonicalStreamEvent>, ParseStreamError> {
        self.parser.finish()?;
        if self.done {
            return Ok(Vec::new());
        }
        self.finalize_stream()
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
        if !matches!(frame.event_name(), None | Some("message")) {
            return Err(ParseStreamError::UnsupportedFeature);
        }

        let data = frame.data();
        if data.trim_ascii() == b"[DONE]" {
            output.extend(self.finalize_stream()?);
            return Ok(());
        }

        let value = parse_value(data, super::super::REQUEST_JSON_LIMITS).map_err(map_json_error)?;
        if value.get("error").is_some() {
            self.decode_error(value, output)?;
            return Ok(());
        }
        if uses_known_unsupported_feature(&value) {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        let chunk: GenerateContentResponseWire =
            serde_json::from_value(value).map_err(|_| ParseStreamError::InvalidValue)?;
        self.decode_chunk(chunk, output)
    }

    fn decode_error(
        &mut self,
        value: Value,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let envelope: ErrorEnvelope =
            serde_json::from_value(value).map_err(|_| ParseStreamError::InvalidValue)?;
        if envelope.error.message.len() > MAX_ERROR_MESSAGE_BYTES
            || envelope.error.message.chars().any(char::is_control)
            || !envelope.error.details.is_empty()
        {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        let error = map_upstream_error(envelope.error.code, &envelope.error.status)?;
        self.done = true;
        output.push(CanonicalStreamEvent::Error(error));
        Ok(())
    }

    fn decode_chunk(
        &mut self,
        chunk: GenerateContentResponseWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let prompt_was_blocked = self.prompt_blocked;
        let GenerateContentResponseWire {
            candidates,
            prompt_feedback,
            model_version,
            response_id,
            usage_metadata,
            model_status,
        } = chunk;

        let response_id = field_value(response_id).ok_or(ParseStreamError::UnsupportedFeature)?;
        let model_version =
            field_value(model_version).ok_or(ParseStreamError::UnsupportedFeature)?;
        let identity = StreamIdentity::new(response_id, model_version)?;
        match &self.identity {
            None => self.identity = Some(identity),
            Some(expected) if expected == &identity => {}
            Some(_) => return Err(ParseStreamError::InvalidSequence),
        }

        let prompt_feedback_present = matches!(&prompt_feedback, Field::Value(_));
        let model_status_present = matches!(&model_status, Field::Value(_));
        if prompt_feedback_present
            && (self.data_seen
                || !validate_prompt_feedback(prompt_feedback).map_err(map_response_error)?)
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        validate_model_status(model_status).map_err(map_response_error)?;

        let candidates = field_value(candidates).unwrap_or_default();
        if candidates.len() > MAX_RESPONSE_CHOICES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        if prompt_was_blocked
            && (prompt_feedback_present || model_status_present || !candidates.is_empty())
        {
            return Err(ParseStreamError::DataAfterDone);
        }
        if prompt_feedback_present {
            if !candidates.is_empty() {
                return Err(ParseStreamError::InvalidSequence);
            }
            self.prompt_blocked = true;
            output.push(CanonicalStreamEvent::PromptBlocked);
        } else if !prompt_was_blocked {
            let mut seen = HashSet::with_capacity(candidates.len());
            for candidate in candidates {
                let index = candidate_index(&candidate)?;
                if !seen.insert(index) {
                    return Err(ParseStreamError::InvalidValue);
                }
                self.decode_candidate(index, candidate, output)?;
            }
        }

        let usage_present = matches!(&usage_metadata, Field::Value(_));
        if let Some(usage) = field_value(usage_metadata) {
            let converted = convert_usage_metadata(usage).map_err(map_response_error)?;
            if self
                .usage
                .as_ref()
                .is_some_and(|previous| !usage_is_monotonic(previous, &converted))
            {
                return Err(ParseStreamError::InvalidSequence);
            }
            self.usage = Some(converted);
        }

        if !prompt_feedback_present
            && !model_status_present
            && self.choices.is_empty()
            && !usage_present
        {
            return Err(ParseStreamError::InvalidValue);
        }
        self.data_seen = true;
        Ok(())
    }

    fn decode_candidate(
        &mut self,
        index: u32,
        candidate: CandidateWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let CandidateWire {
            index: _,
            content,
            finish_reason,
            finish_message,
            safety_ratings,
            token_count,
            grounding_metadata,
            logprobs_result,
            avg_logprobs,
            url_context_metadata,
            grounding_attributions,
            citation_metadata,
        } = candidate;
        if matches!(grounding_metadata, Field::Value(_))
            || matches!(logprobs_result, Field::Value(_))
            || matches!(avg_logprobs, Field::Value(_))
            || matches!(url_context_metadata, Field::Value(_))
            || matches!(citation_metadata, Field::Value(_))
            || matches!(grounding_attributions, Field::Value(values) if !values.is_empty())
        {
            return Err(ParseStreamError::UnsupportedFeature);
        }
        validate_safety_ratings(safety_ratings).map_err(map_response_error)?;
        if let Some(message) = field_value(finish_message) {
            if message.len() > MAX_FINISH_MESSAGE_BYTES {
                return Err(ParseStreamError::StructureLimitExceeded);
            }
            if message.chars().any(char::is_control) || matches!(&finish_reason, Field::Missing) {
                return Err(ParseStreamError::InvalidValue);
            }
        }

        if !self.choices.contains_key(&index) {
            if self.choices.len() >= MAX_RESPONSE_CHOICES {
                return Err(ParseStreamError::StructureLimitExceeded);
            }
            self.choices.insert(index, ChoiceState::default());
        }
        let (choices, totals) = (&mut self.choices, &mut self.totals);
        let state = choices
            .get_mut(&index)
            .expect("已插入的 Gemini 候选状态必须存在");
        if state.finished {
            return Err(ParseStreamError::InvalidSequence);
        }
        if !state.started {
            state.started = true;
            output.push(CanonicalStreamEvent::MessageStart {
                choice_index: index,
                role: Role::Assistant,
            });
        }

        let mut emitted = false;
        if let Some(content) = field_value(content) {
            emitted = decode_content(index, content, state, totals, output)?;
        }
        if let Some(count) = field_value(token_count) {
            let count = wire_token(count)?;
            if state
                .candidate_tokens
                .is_some_and(|previous| count < previous)
            {
                return Err(ParseStreamError::InvalidSequence);
            }
            state.candidate_tokens = Some(count);
        }
        if let Some(reason) = field_value(finish_reason) {
            finish_candidate(index, reason, state, output)?;
            emitted = true;
        }
        if !emitted {
            return Err(ParseStreamError::InvalidValue);
        }
        Ok(())
    }

    fn finalize_stream(&mut self) -> Result<Vec<CanonicalStreamEvent>, ParseStreamError> {
        if self.done {
            return Err(ParseStreamError::DataAfterDone);
        }
        if self.identity.is_none() || !self.data_seen {
            return Err(ParseStreamError::UnexpectedEof);
        }
        if self.prompt_blocked {
            if !self.choices.is_empty() {
                return Err(ParseStreamError::InvalidSequence);
            }
        } else if self.choices.is_empty() || self.choices.values().any(|choice| !choice.finished) {
            return Err(ParseStreamError::UnexpectedEof);
        }
        validate_candidate_token_counts(&self.choices, self.usage.as_ref())?;

        let mut output = Vec::with_capacity(2);
        if let Some(usage) = self.usage.take() {
            output.push(CanonicalStreamEvent::Usage(usage.usage));
        }
        output.push(CanonicalStreamEvent::StreamEnd);
        self.done = true;
        Ok(output)
    }
}

impl Default for GeminiGenerateContentStreamDecoder {
    fn default() -> Self {
        Self {
            parser: SseParser::new(),
            identity: None,
            choices: BTreeMap::new(),
            totals: DecodeTotals::default(),
            usage: None,
            prompt_blocked: false,
            data_seen: false,
            done: false,
            failed: false,
        }
    }
}

impl fmt::Debug for GeminiGenerateContentStreamDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiGenerateContentStreamDecoder")
            .field("has_identity", &self.identity.is_some())
            .field("choice_count", &self.choices.len())
            .field("has_usage", &self.usage.is_some())
            .field("prompt_blocked", &self.prompt_blocked)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
struct StreamIdentity {
    response_id: String,
    model_version: String,
}

impl StreamIdentity {
    fn new(response_id: String, model_version: String) -> Result<Self, ParseStreamError> {
        validate_response_id(&response_id).map_err(map_response_error)?;
        validate_model(&model_version).map_err(map_request_error)?;
        Ok(Self {
            response_id,
            model_version,
        })
    }
}

#[derive(Default)]
struct ChoiceState {
    started: bool,
    finished: bool,
    has_content: bool,
    has_tool_use: bool,
    next_content_index: u32,
    text_index: Option<u32>,
    reasoning_index: Option<u32>,
    candidate_tokens: Option<TokenCount>,
    seen_call_ids: HashSet<String>,
}

#[derive(Default)]
struct DecodeTotals {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    signature_bytes: usize,
    tool_calls: usize,
}

fn candidate_index(candidate: &CandidateWire) -> Result<u32, ParseStreamError> {
    let Field::Value(index) = &candidate.index else {
        return Err(ParseStreamError::UnsupportedFeature);
    };
    if !(0..=i64::from(i32::MAX)).contains(index) {
        return Err(ParseStreamError::InvalidValue);
    }
    u32::try_from(*index).map_err(|_| ParseStreamError::InvalidValue)
}

fn decode_content(
    choice_index: u32,
    content: ContentWire,
    state: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<bool, ParseStreamError> {
    match field_value(content.role).as_deref() {
        None | Some("model") => {}
        Some(_) => return Err(ParseStreamError::InvalidValue),
    }
    if content.parts.len() > MAX_PARTS_PER_CONTENT {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    let emitted = !content.parts.is_empty();
    for part in content.parts {
        decode_part(choice_index, part, state, totals, output)?;
    }
    Ok(emitted)
}

fn decode_part(
    choice_index: u32,
    part: PartWire,
    state: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<(), ParseStreamError> {
    let PartWire {
        text,
        inline_data,
        function_call,
        function_response,
        thought,
        thought_signature,
    } = part;
    let primary_fields = usize::from(matches!(&text, Field::Value(_)))
        + usize::from(matches!(&inline_data, Field::Value(_)))
        + usize::from(matches!(&function_call, Field::Value(_)))
        + usize::from(matches!(&function_response, Field::Value(_)));
    if primary_fields != 1 {
        return Err(ParseStreamError::InvalidValue);
    }
    let thought = field_value(thought).unwrap_or(false);
    let signature = field_value(thought_signature);
    match (text, inline_data, function_call, function_response) {
        (Field::Value(text), Field::Missing, Field::Missing, Field::Missing) if thought => {
            totals.add_text(&text)?;
            let signature = validate_optional_signature(signature, totals)?;
            let content_index = assign_content_index(
                &mut state.reasoning_index,
                &mut state.next_content_index,
                totals,
            )?;
            state.has_content = true;
            output.push(CanonicalStreamEvent::ReasoningDelta {
                choice_index,
                content_index,
                text,
                signature,
            });
            Ok(())
        }
        (Field::Value(text), Field::Missing, Field::Missing, Field::Missing) => {
            if signature.is_some() {
                return Err(ParseStreamError::UnsupportedFeature);
            }
            totals.add_text(&text)?;
            let content_index =
                assign_content_index(&mut state.text_index, &mut state.next_content_index, totals)?;
            state.has_content = true;
            output.push(CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index,
                delta: ContentDelta::Text(text),
            });
            Ok(())
        }
        (Field::Missing, Field::Value(media), Field::Missing, Field::Missing) => {
            if thought || signature.is_some() {
                return Err(ParseStreamError::UnsupportedFeature);
            }
            let (delta, decoded_bytes) = convert_media(media)?;
            totals.add_media(decoded_bytes)?;
            let content_index = allocate_content_index(state, totals)?;
            state.has_content = true;
            output.push(CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index,
                delta,
            });
            Ok(())
        }
        (Field::Missing, Field::Missing, Field::Value(call), Field::Missing) => {
            if thought {
                return Err(ParseStreamError::UnsupportedFeature);
            }
            decode_function_call(choice_index, call, signature, state, totals, output)
        }
        (Field::Missing, Field::Missing, Field::Missing, Field::Value(_)) => {
            Err(ParseStreamError::UnsupportedFeature)
        }
        _ => Err(ParseStreamError::InvalidValue),
    }
}

fn decode_function_call(
    choice_index: u32,
    call: FunctionCallWire,
    signature: Option<String>,
    state: &mut ChoiceState,
    totals: &mut DecodeTotals,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<(), ParseStreamError> {
    let id = field_value(call.id).ok_or(ParseStreamError::UnsupportedFeature)?;
    validate_call_id(&id).map_err(map_request_error)?;
    validate_tool_name(&call.name).map_err(map_request_error)?;
    if !state.seen_call_ids.insert(id.clone()) {
        return Err(ParseStreamError::InvalidValue);
    }
    let input = field_value(call.args).unwrap_or_else(|| Value::Object(Map::new()));
    let (bytes, nodes) = validate_json_object(&input, TOOL_PAYLOAD_LIMITS, MAX_ARGUMENT_BYTES)
        .map_err(map_request_error)?;
    totals.add_arguments(bytes, nodes)?;
    let signature = validate_optional_signature(signature, totals)?;
    let tool_index = allocate_content_index(state, totals)?;
    state.has_content = true;
    state.has_tool_use = true;

    output.push(CanonicalStreamEvent::ToolCallStart {
        choice_index,
        tool_index,
        id,
        name: call.name,
    });
    if let Some(signature) = signature {
        output.push(CanonicalStreamEvent::ToolCallSignature {
            choice_index,
            tool_index,
            signature,
        });
    }
    let partial_json = serde_json::to_string(&input).map_err(|_| ParseStreamError::InvalidValue)?;
    output.push(CanonicalStreamEvent::ToolCallArgsDelta {
        choice_index,
        tool_index,
        partial_json,
    });
    output.push(CanonicalStreamEvent::ToolCallEnd {
        choice_index,
        tool_index,
    });
    Ok(())
}

fn convert_media(media: BlobWire) -> Result<(ContentDelta, usize), ParseStreamError> {
    let mime_type = normalize_mime_type(&media.mime_type).map_err(map_request_error)?;
    let decoded = decode_base64(&media.data).ok_or(ParseStreamError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_MEDIA_BYTES {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    let bytes = decoded.len();
    if mime_type.starts_with("image/") {
        Ok((
            ContentDelta::Image {
                data: media.data,
                mime_type,
            },
            bytes,
        ))
    } else if mime_type.starts_with("audio/") {
        Ok((
            ContentDelta::Audio {
                data: media.data,
                mime_type,
            },
            bytes,
        ))
    } else {
        Err(ParseStreamError::UnsupportedFeature)
    }
}

fn finish_candidate(
    choice_index: u32,
    reason: FinishReasonWire,
    state: &mut ChoiceState,
    output: &mut Vec<CanonicalStreamEvent>,
) -> Result<(), ParseStreamError> {
    if reason == FinishReasonWire::Unspecified || state.finished {
        return Err(ParseStreamError::InvalidSequence);
    }
    let reason = match reason {
        FinishReasonWire::Stop if state.has_tool_use => FinishReason::ToolCalls,
        FinishReasonWire::Stop => FinishReason::Stop,
        FinishReasonWire::MaxTokens if !state.has_tool_use => FinishReason::Length,
        reason if reason.is_content_filter() && !state.has_tool_use => FinishReason::ContentFilter,
        _ => return Err(ParseStreamError::InvalidSequence),
    };
    if !state.has_content && reason != FinishReason::ContentFilter {
        return Err(ParseStreamError::InvalidSequence);
    }
    state.finished = true;
    output.push(CanonicalStreamEvent::Finish {
        choice_index,
        reason,
        stop_sequence: None,
    });
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
    totals.add_block()?;
    let index = *next;
    *next = next
        .checked_add(1)
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    *current = Some(index);
    Ok(index)
}

fn allocate_content_index(
    state: &mut ChoiceState,
    totals: &mut DecodeTotals,
) -> Result<u32, ParseStreamError> {
    totals.add_block()?;
    let index = state.next_content_index;
    state.next_content_index = state
        .next_content_index
        .checked_add(1)
        .ok_or(ParseStreamError::StructureLimitExceeded)?;
    Ok(index)
}

fn validate_optional_signature(
    signature: Option<String>,
    totals: &mut DecodeTotals,
) -> Result<Option<String>, ParseStreamError> {
    let Some(signature) = signature else {
        return Ok(None);
    };
    let decoded = decode_base64(&signature).ok_or(ParseStreamError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_SIGNATURE_BYTES {
        return Err(ParseStreamError::StructureLimitExceeded);
    }
    totals.add_signature(decoded.len())?;
    Ok(Some(signature))
}

fn validate_candidate_token_counts(
    choices: &BTreeMap<u32, ChoiceState>,
    usage: Option<&ConvertedUsage>,
) -> Result<(), ParseStreamError> {
    let Some(expected) = usage.and_then(|usage| usage.candidates_token_count) else {
        return Ok(());
    };
    if choices
        .values()
        .any(|choice| choice.candidate_tokens.is_none())
    {
        return Ok(());
    }
    let sum = choices.values().try_fold(0_i64, |sum, choice| {
        let count = choice
            .candidate_tokens
            .ok_or(ParseStreamError::InvalidSequence)?;
        sum.checked_add(count.get())
            .ok_or(ParseStreamError::InvalidValue)
    })?;
    if sum != expected.get() {
        return Err(ParseStreamError::InvalidValue);
    }
    Ok(())
}

fn usage_is_monotonic(previous: &ConvertedUsage, current: &ConvertedUsage) -> bool {
    let previous_usage = &previous.usage;
    let current_usage = &current.usage;
    let previous_details = previous_usage.details();
    let current_details = current_usage.details();
    previous_usage.input_tokens() <= current_usage.input_tokens()
        && previous_usage.output_tokens() <= current_usage.output_tokens()
        && previous_details.cache_read() <= current_details.cache_read()
        && previous_details.reasoning() <= current_details.reasoning()
        && previous_details.audio_input() <= current_details.audio_input()
        && previous_details.audio_output() <= current_details.audio_output()
        && match (
            previous.candidates_token_count,
            current.candidates_token_count,
        ) {
            (Some(previous), Some(current)) => previous <= current,
            (Some(_), None) => false,
            (None, _) => true,
        }
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

    fn add_text(&mut self, text: &str) -> Result<(), ParseStreamError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_media(&mut self, bytes: usize) -> Result<(), ParseStreamError> {
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_arguments(&mut self, bytes: usize, nodes: usize) -> Result<(), ParseStreamError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.argument_nodes > MAX_TOTAL_JSON_NODES
        {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_signature(&mut self, bytes: usize) -> Result<(), ParseStreamError> {
        self.signature_bytes = self
            .signature_bytes
            .checked_add(bytes)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        if self.signature_bytes > MAX_TOTAL_SIGNATURE_BYTES {
            return Err(ParseStreamError::StructureLimitExceeded);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorEnvelope {
    error: ErrorWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorWire {
    code: u16,
    message: String,
    status: String,
    #[serde(default)]
    details: Vec<Value>,
}

fn map_upstream_error(code: u16, status: &str) -> Result<UpstreamError, ParseStreamError> {
    let mapped = match (code, status) {
        (400, "INVALID_ARGUMENT") => UpstreamError::BadRequest,
        (401, "UNAUTHENTICATED") => UpstreamError::AuthExpired,
        (403, "PERMISSION_DENIED") => UpstreamError::ProtocolError,
        (404, "NOT_FOUND") => UpstreamError::ModelUnsupported,
        (429, "RESOURCE_EXHAUSTED") => {
            UpstreamError::rate_limited(af_domain::RateLimitScope::Unknown)
        }
        (503, "UNAVAILABLE") => UpstreamError::overloaded(),
        (500..=599, _) => UpstreamError::ServerError {
            status: UpstreamServerStatus::new(code).ok_or(ParseStreamError::InvalidValue)?,
        },
        _ => return Err(ParseStreamError::InvalidValue),
    };
    Ok(mapped)
}

fn field_value<T>(field: Field<T>) -> Option<T> {
    match field {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    }
}

fn wire_token(value: i64) -> Result<TokenCount, ParseStreamError> {
    if value > i64::from(i32::MAX) {
        return Err(ParseStreamError::InvalidValue);
    }
    TokenCount::new(value).map_err(|_| ParseStreamError::InvalidValue)
}

fn map_json_error(error: BoundedJsonError) -> ParseStreamError {
    match error {
        BoundedJsonError::InvalidJson => ParseStreamError::InvalidJson,
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
