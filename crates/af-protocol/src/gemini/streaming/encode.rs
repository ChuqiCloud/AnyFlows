use std::{
    collections::{BTreeMap, HashSet, btree_map::Entry},
    fmt,
};

use af_domain::{Protocol, Role};
use serde_json::{Map, Value};

use super::{EncodeStreamError, MAX_SSE_EVENT_BYTES};
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason,
    bounded_json::{BoundedJsonError, parse_value},
    gemini::{
        BuildResponseError, ParseRequestError, ParseResponseError,
        build_response::build_usage,
        convert::{
            MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_MEDIA_BYTES, MAX_SIGNATURE_BYTES,
            MAX_TEXT_BYTES, MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_JSON_NODES,
            MAX_TOTAL_SIGNATURE_BYTES, MAX_TOTAL_TEXT_BYTES, TOOL_PAYLOAD_LIMITS, decode_base64,
            normalize_mime_type, validate_call_id, validate_json_object, validate_model,
            validate_tool_name,
        },
        parse_response::validate_response_id,
    },
    validate_stream_event_capabilities,
};

const MAX_SIGNATURE_ENCODED_BYTES: usize = (MAX_SIGNATURE_BYTES / 3 + 1) * 4;
const MAX_TOTAL_SIGNATURE_ENCODED_BYTES: usize = (MAX_TOTAL_SIGNATURE_BYTES / 3 + 1) * 4;

/// 将 Canonical 流事件编码为 Gemini `streamGenerateContent` SSE。
///
/// `StreamEnd` 只关闭状态且不输出 `[DONE]`，调用方随后应结束 HTTP 正文，这与 Gemini
/// 官方 SDK 以传输 EOF 识别终点的行为一致。
pub struct GeminiGenerateContentStreamEncoder {
    identity: EncodeIdentity,
    choices: BTreeMap<u32, ChoiceState>,
    totals: EncodeTotals,
    usage_seen: bool,
    prompt_blocked: bool,
    done: bool,
    failed: bool,
}

impl GeminiGenerateContentStreamEncoder {
    /// 使用已校验的响应标识与模型版本创建 encoder。
    pub fn new(
        response_id: impl Into<String>,
        model_version: impl Into<String>,
    ) -> Result<Self, EncodeStreamError> {
        Ok(Self {
            identity: EncodeIdentity::new(response_id.into(), model_version.into())?,
            choices: BTreeMap::new(),
            totals: EncodeTotals::default(),
            usage_seen: false,
            prompt_blocked: false,
            done: false,
            failed: false,
        })
    }

    /// 编码一个 Canonical 事件；无对应 Gemini wire 块的状态事件会返回空字节。
    pub fn encode(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, EncodeStreamError> {
        if self.failed {
            return Err(EncodeStreamError::EncoderFailed);
        }
        if self.done {
            self.failed = true;
            return Err(EncodeStreamError::InvalidSequence);
        }
        let result = validate_stream_event_capabilities(Protocol::Gemini, &event)
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
            CanonicalStreamEvent::ToolCallSignature {
                choice_index,
                tool_index,
                signature,
            } => self.encode_tool_signature(choice_index, tool_index, signature),
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
            CanonicalStreamEvent::PromptBlocked => self.encode_prompt_blocked(),
            CanonicalStreamEvent::Usage(usage) => self.encode_usage(usage),
            CanonicalStreamEvent::Ping => Ok(b": ping\n\n".to_vec()),
            CanonicalStreamEvent::StreamEnd => self.encode_stream_end(),
            CanonicalStreamEvent::Error(_) => Err(EncodeStreamError::UnsupportedEvent),
        }
    }

    fn encode_message_start(
        &mut self,
        choice_index: u32,
        role: Role,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        if self.prompt_blocked
            || role != Role::Assistant
            || self.choices.contains_key(&choice_index)
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        if self.choices.len() >= super::super::parse_response::MAX_RESPONSE_CHOICES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        self.choices.insert(choice_index, ChoiceState::default());
        Ok(Vec::new())
    }

    fn encode_content(
        &mut self,
        choice_index: u32,
        content_index: u32,
        delta: ContentDelta,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        let (kind, part) = match delta {
            ContentDelta::Text(text) => {
                self.totals.add_text(&text)?;
                (
                    ContentKind::Text,
                    object_value([("text", Value::String(text))]),
                )
            }
            ContentDelta::Image { data, mime_type } => {
                let mime_type = validate_media(&data, &mime_type, "image/", &mut self.totals)?;
                (ContentKind::Image, inline_data_part(data, mime_type))
            }
            ContentDelta::Audio { data, mime_type } => {
                let mime_type = validate_media(&data, &mime_type, "audio/", &mut self.totals)?;
                (ContentKind::Audio, inline_data_part(data, mime_type))
            }
        };
        let choice = active_choice(&mut self.choices, choice_index)?;
        bind_content(choice, content_index, kind, &mut self.totals)?;
        choice.has_content = true;
        encode_candidate_part(&self.identity, choice_index, part)
    }

    fn encode_reasoning(
        &mut self,
        choice_index: u32,
        content_index: u32,
        text: String,
        signature: Option<String>,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        self.totals.add_text(&text)?;
        let choice = active_choice(&mut self.choices, choice_index)?;
        choice.has_content = true;
        let content = bind_content(
            choice,
            content_index,
            ContentKind::Reasoning,
            &mut self.totals,
        )?;
        let ContentState::Reasoning {
            signature: ref mut complete_signature,
            ref signature_flushed,
        } = *content
        else {
            unreachable!("内容类型已由 bind_content 校验")
        };
        if *signature_flushed {
            return Err(EncodeStreamError::InvalidSequence);
        }
        if let Some(signature) = signature {
            self.totals
                .add_encoded_signature(signature.len(), complete_signature.len())?;
            complete_signature.push_str(&signature);
        }
        if text.is_empty() && !complete_signature.is_empty() {
            return Ok(Vec::new());
        }
        encode_candidate_part(
            &self.identity,
            choice_index,
            Value::Object(Map::from_iter([
                ("text".to_owned(), Value::String(text)),
                ("thought".to_owned(), Value::Bool(true)),
            ])),
        )
    }

    fn encode_tool_start(
        &mut self,
        choice_index: u32,
        tool_index: u32,
        id: String,
        name: String,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        validate_call_id(&id).map_err(map_request_error)?;
        validate_tool_name(&name).map_err(map_request_error)?;
        if self.totals.tool_calls >= MAX_TOOL_CALLS {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        let choice = active_choice(&mut self.choices, choice_index)?;
        if choice.tools.contains_key(&tool_index) || !choice.seen_call_ids.insert(id.clone()) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.totals.add_block()?;
        self.totals.tool_calls += 1;
        choice.has_content = true;
        choice.tools.insert(
            tool_index,
            ToolState {
                id,
                name,
                signature: None,
                arguments: String::new(),
                ended: false,
            },
        );
        Ok(Vec::new())
    }

    fn encode_tool_signature(
        &mut self,
        choice_index: u32,
        tool_index: u32,
        signature: String,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        let decoded = validate_signature(&signature)?;
        let choice = active_choice(&mut self.choices, choice_index)?;
        let tool = active_tool(choice, tool_index)?;
        if tool.signature.is_some() {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.totals.add_signature(decoded)?;
        tool.signature = Some(signature);
        Ok(Vec::new())
    }

    fn encode_tool_arguments(
        &mut self,
        choice_index: u32,
        tool_index: u32,
        partial_json: String,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        let choice = active_choice(&mut self.choices, choice_index)?;
        let tool = active_tool(choice, tool_index)?;
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
        if next_tool_bytes > MAX_ARGUMENT_BYTES || next_total_bytes > MAX_TOTAL_ARGUMENT_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        tool.arguments.push_str(&partial_json);
        self.totals.argument_bytes = next_total_bytes;
        Ok(Vec::new())
    }

    fn encode_tool_end(
        &mut self,
        choice_index: u32,
        tool_index: u32,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        let choice = active_choice(&mut self.choices, choice_index)?;
        let tool = active_tool(choice, tool_index)?;
        let input = parse_value(tool.arguments.as_bytes(), TOOL_PAYLOAD_LIMITS)
            .map_err(map_argument_error)?;
        let (_, nodes) = validate_json_object(&input, TOOL_PAYLOAD_LIMITS, MAX_ARGUMENT_BYTES)
            .map_err(map_request_error)?;
        self.totals.argument_nodes = self
            .totals
            .argument_nodes
            .checked_add(nodes)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if self.totals.argument_nodes > MAX_TOTAL_JSON_NODES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }

        let mut part = Map::from_iter([(
            "functionCall".to_owned(),
            object_value([
                ("id", Value::String(tool.id.clone())),
                ("name", Value::String(tool.name.clone())),
                ("args", input),
            ]),
        )]);
        if let Some(signature) = &tool.signature {
            part.insert(
                "thoughtSignature".to_owned(),
                Value::String(signature.clone()),
            );
        }
        tool.ended = true;
        encode_candidate_part(&self.identity, choice_index, Value::Object(part))
    }

    fn encode_finish(
        &mut self,
        choice_index: u32,
        reason: FinishReason,
        stop_sequence: Option<String>,
    ) -> Result<Vec<u8>, EncodeStreamError> {
        if stop_sequence.is_some() {
            return Err(EncodeStreamError::UnsupportedEvent);
        }
        let choice = active_choice(&mut self.choices, choice_index)?;
        if choice.tools.values().any(|tool| !tool.ended)
            || (reason == FinishReason::ToolCalls) != !choice.tools.is_empty()
            || (!choice.has_content && reason != FinishReason::ContentFilter)
        {
            return Err(EncodeStreamError::InvalidSequence);
        }

        let mut output =
            flush_reasoning_signatures(&self.identity, choice_index, choice, &mut self.totals)?;
        output.extend(encode_candidate_finish(
            &self.identity,
            choice_index,
            finish_reason(reason),
        )?);
        choice.finished = true;
        Ok(output)
    }

    fn encode_prompt_blocked(&mut self) -> Result<Vec<u8>, EncodeStreamError> {
        if self.prompt_blocked || !self.choices.is_empty() || self.usage_seen {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.prompt_blocked = true;
        let mut root = response_root(&self.identity);
        root.insert(
            "promptFeedback".to_owned(),
            object_value([("blockReason", Value::String("SAFETY".to_owned()))]),
        );
        encode_sse(Value::Object(root))
    }

    fn encode_usage(&mut self, usage: crate::Usage) -> Result<Vec<u8>, EncodeStreamError> {
        if self.usage_seen
            || (!self.prompt_blocked
                && (self.choices.is_empty()
                    || self.choices.values().any(|choice| !choice.finished)))
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let usage = build_usage(&usage, &Map::new()).map_err(map_build_response_error)?;
        let mut root = response_root(&self.identity);
        root.insert("usageMetadata".to_owned(), Value::Object(usage));
        self.usage_seen = true;
        encode_sse(Value::Object(root))
    }

    fn encode_stream_end(&mut self) -> Result<Vec<u8>, EncodeStreamError> {
        if !self.prompt_blocked
            && (self.choices.is_empty() || self.choices.values().any(|choice| !choice.finished))
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.done = true;
        Ok(Vec::new())
    }
}

impl fmt::Debug for GeminiGenerateContentStreamEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiGenerateContentStreamEncoder")
            .field("metadata", &"<已脱敏>")
            .field("choice_count", &self.choices.len())
            .field("usage_seen", &self.usage_seen)
            .field("prompt_blocked", &self.prompt_blocked)
            .field("done", &self.done)
            .field("failed", &self.failed)
            .finish()
    }
}

struct EncodeIdentity {
    response_id: String,
    model_version: String,
}

impl EncodeIdentity {
    fn new(response_id: String, model_version: String) -> Result<Self, EncodeStreamError> {
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
    finished: bool,
    has_content: bool,
    contents: BTreeMap<u32, ContentState>,
    tools: BTreeMap<u32, ToolState>,
    seen_call_ids: HashSet<String>,
}

enum ContentState {
    Text,
    Reasoning {
        signature: String,
        signature_flushed: bool,
    },
    Image,
    Audio,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ContentKind {
    Text,
    Reasoning,
    Image,
    Audio,
}

impl ContentState {
    const fn kind(&self) -> ContentKind {
        match self {
            Self::Text => ContentKind::Text,
            Self::Reasoning { .. } => ContentKind::Reasoning,
            Self::Image => ContentKind::Image,
            Self::Audio => ContentKind::Audio,
        }
    }

    fn new(kind: ContentKind) -> Self {
        match kind {
            ContentKind::Text => Self::Text,
            ContentKind::Reasoning => Self::Reasoning {
                signature: String::new(),
                signature_flushed: false,
            },
            ContentKind::Image => Self::Image,
            ContentKind::Audio => Self::Audio,
        }
    }
}

struct ToolState {
    id: String,
    name: String,
    signature: Option<String>,
    arguments: String,
    ended: bool,
}

#[derive(Default)]
struct EncodeTotals {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    signature_bytes: usize,
    signature_encoded_bytes: usize,
}

fn active_choice(
    choices: &mut BTreeMap<u32, ChoiceState>,
    choice_index: u32,
) -> Result<&mut ChoiceState, EncodeStreamError> {
    let choice = choices
        .get_mut(&choice_index)
        .ok_or(EncodeStreamError::InvalidSequence)?;
    if choice.finished {
        return Err(EncodeStreamError::InvalidSequence);
    }
    Ok(choice)
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

fn bind_content<'a>(
    choice: &'a mut ChoiceState,
    content_index: u32,
    kind: ContentKind,
    totals: &mut EncodeTotals,
) -> Result<&'a mut ContentState, EncodeStreamError> {
    match choice.contents.entry(content_index) {
        Entry::Vacant(entry) => {
            totals.add_block()?;
            Ok(entry.insert(ContentState::new(kind)))
        }
        Entry::Occupied(entry) => {
            if entry.get().kind() != kind || matches!(kind, ContentKind::Image | ContentKind::Audio)
            {
                return Err(EncodeStreamError::InvalidSequence);
            }
            Ok(entry.into_mut())
        }
    }
}

fn flush_reasoning_signatures(
    identity: &EncodeIdentity,
    choice_index: u32,
    choice: &mut ChoiceState,
    totals: &mut EncodeTotals,
) -> Result<Vec<u8>, EncodeStreamError> {
    let mut output = Vec::new();
    for content in choice.contents.values_mut() {
        let ContentState::Reasoning {
            signature,
            signature_flushed,
        } = content
        else {
            continue;
        };
        if *signature_flushed || signature.is_empty() {
            continue;
        }
        let decoded = validate_signature(signature)?;
        totals.add_signature(decoded)?;
        output.extend(encode_candidate_part(
            identity,
            choice_index,
            Value::Object(Map::from_iter([
                ("text".to_owned(), Value::String(String::new())),
                ("thought".to_owned(), Value::Bool(true)),
                (
                    "thoughtSignature".to_owned(),
                    Value::String(signature.clone()),
                ),
            ])),
        )?);
        *signature_flushed = true;
    }
    Ok(output)
}

fn validate_media(
    data: &str,
    mime_type: &str,
    expected_prefix: &str,
    totals: &mut EncodeTotals,
) -> Result<String, EncodeStreamError> {
    let mime_type = normalize_mime_type(mime_type).map_err(map_request_error)?;
    if !mime_type.starts_with(expected_prefix) {
        return Err(EncodeStreamError::InvalidSequence);
    }
    let decoded = decode_base64(data).ok_or(EncodeStreamError::InvalidSequence)?;
    if decoded.is_empty() || decoded.len() > MAX_MEDIA_BYTES {
        return Err(EncodeStreamError::StructureLimitExceeded);
    }
    totals.add_media(decoded.len())?;
    Ok(mime_type)
}

fn validate_signature(signature: &str) -> Result<usize, EncodeStreamError> {
    let decoded = decode_base64(signature).ok_or(EncodeStreamError::InvalidSequence)?;
    if decoded.is_empty() || decoded.len() > MAX_SIGNATURE_BYTES {
        return Err(EncodeStreamError::StructureLimitExceeded);
    }
    Ok(decoded.len())
}

fn inline_data_part(data: String, mime_type: String) -> Value {
    object_value([(
        "inlineData",
        object_value([
            ("mimeType", Value::String(mime_type)),
            ("data", Value::String(data)),
        ]),
    )])
}

fn encode_candidate_part(
    identity: &EncodeIdentity,
    choice_index: u32,
    part: Value,
) -> Result<Vec<u8>, EncodeStreamError> {
    let candidate = object_value([
        ("index", Value::Number(i64::from(choice_index).into())),
        (
            "content",
            object_value([
                ("role", Value::String("model".to_owned())),
                ("parts", Value::Array(vec![part])),
            ]),
        ),
    ]);
    let mut root = response_root(identity);
    root.insert("candidates".to_owned(), Value::Array(vec![candidate]));
    encode_sse(Value::Object(root))
}

fn encode_candidate_finish(
    identity: &EncodeIdentity,
    choice_index: u32,
    reason: &'static str,
) -> Result<Vec<u8>, EncodeStreamError> {
    let candidate = object_value([
        ("index", Value::Number(i64::from(choice_index).into())),
        ("finishReason", Value::String(reason.to_owned())),
    ]);
    let mut root = response_root(identity);
    root.insert("candidates".to_owned(), Value::Array(vec![candidate]));
    encode_sse(Value::Object(root))
}

fn response_root(identity: &EncodeIdentity) -> Map<String, Value> {
    Map::from_iter([
        (
            "responseId".to_owned(),
            Value::String(identity.response_id.clone()),
        ),
        (
            "modelVersion".to_owned(),
            Value::String(identity.model_version.clone()),
        ),
    ])
}

fn encode_sse(value: Value) -> Result<Vec<u8>, EncodeStreamError> {
    let data = serde_json::to_vec(&value).map_err(|_| EncodeStreamError::Serialization)?;
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

const fn finish_reason(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop | FinishReason::ToolCalls => "STOP",
        FinishReason::Length => "MAX_TOKENS",
        FinishReason::ContentFilter => "SAFETY",
    }
}

fn object_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(Map::from_iter(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    ))
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

    fn add_text(&mut self, text: &str) -> Result<(), EncodeStreamError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_media(&mut self, bytes: usize) -> Result<(), EncodeStreamError> {
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_encoded_signature(
        &mut self,
        bytes: usize,
        current_block_bytes: usize,
    ) -> Result<(), EncodeStreamError> {
        let next_block = current_block_bytes
            .checked_add(bytes)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        self.signature_encoded_bytes = self
            .signature_encoded_bytes
            .checked_add(bytes)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if next_block > MAX_SIGNATURE_ENCODED_BYTES
            || self.signature_encoded_bytes > MAX_TOTAL_SIGNATURE_ENCODED_BYTES
        {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_signature(&mut self, bytes: usize) -> Result<(), EncodeStreamError> {
        self.signature_bytes = self
            .signature_bytes
            .checked_add(bytes)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        if self.signature_bytes > MAX_TOTAL_SIGNATURE_BYTES {
            return Err(EncodeStreamError::StructureLimitExceeded);
        }
        Ok(())
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

fn map_request_error(error: ParseRequestError) -> EncodeStreamError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            EncodeStreamError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => EncodeStreamError::UnsupportedEvent,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => EncodeStreamError::InvalidSequence,
    }
}

fn map_response_error(error: ParseResponseError) -> EncodeStreamError {
    match error {
        ParseResponseError::BodyTooLarge | ParseResponseError::StructureLimitExceeded => {
            EncodeStreamError::StructureLimitExceeded
        }
        ParseResponseError::UnsupportedFeature => EncodeStreamError::UnsupportedEvent,
        ParseResponseError::InvalidJson
        | ParseResponseError::DuplicateKey
        | ParseResponseError::InvalidValue => EncodeStreamError::InvalidMetadata,
    }
}

fn map_build_response_error(error: BuildResponseError) -> EncodeStreamError {
    match error {
        BuildResponseError::StructureLimitExceeded => EncodeStreamError::StructureLimitExceeded,
        BuildResponseError::UnsupportedCapability(error) => {
            EncodeStreamError::UnsupportedCapability(error)
        }
        BuildResponseError::UnsupportedFeature | BuildResponseError::RawProtocolMismatch => {
            EncodeStreamError::UnsupportedEvent
        }
        BuildResponseError::UnsupportedOperation | BuildResponseError::InvalidValue => {
            EncodeStreamError::InvalidUsage
        }
    }
}
