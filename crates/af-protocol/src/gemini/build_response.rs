use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_MEDIA_BYTES, MAX_PARTS_PER_CONTENT,
        MAX_SIGNATURE_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES,
        MAX_TOTAL_JSON_NODES, MAX_TOTAL_SIGNATURE_BYTES, MAX_TOTAL_TEXT_BYTES, TOOL_PAYLOAD_LIMITS,
        decode_base64, normalize_mime_type, validate_call_id, validate_json_object, validate_model,
        validate_tool_name,
    },
    parse_response::{
        MAX_FINISH_MESSAGE_BYTES, MAX_RESPONSE_CHOICES, ParseResponseError, convert_usage_metadata,
        validate_model_status, validate_prompt_feedback, validate_response_id,
        validate_safety_ratings,
    },
    response_wire::{CandidateWire, FinishReasonWire, ModelStatusWire, PromptFeedbackWire},
    wire::Field,
};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, MediaSource, ResponseChoice,
    UnsupportedCapability, Usage, bounded_json::validate_object, validate_response_capabilities,
};

/// 将 Canonical 响应构造为 Gemini `generateContent` 非流式 JSON。
///
/// Gemini 不提供创建时间，合法的 `created_at` 不写入 wire。所有候选、usage 与
/// 同源元数据都会在出站信任边界重新校验，无法无损表达的内容明确失败。
pub fn build_response(response: &CanonicalResponse) -> Result<Value, BuildResponseError> {
    if response.operation != Operation::Chat {
        return Err(BuildResponseError::UnsupportedOperation);
    }
    validate_response_capabilities(Protocol::Gemini, response)
        .map_err(BuildResponseError::UnsupportedCapability)?;
    validate_response_id(&response.id).map_err(map_response_validation_error)?;
    validate_model(&response.model).map_err(map_request_validation_error)?;
    if response.created_at.is_some_and(|created_at| created_at < 0) {
        return Err(BuildResponseError::InvalidValue);
    }
    if response.choices.len() > MAX_RESPONSE_CHOICES {
        return Err(BuildResponseError::StructureLimitExceeded);
    }

    let mut raw = validate_raw(response)?;
    if response.choices.is_empty() != raw.prompt_blocked {
        return Err(BuildResponseError::InvalidValue);
    }
    let (candidates, candidate_token_counts) = build_candidates(response, &mut raw)?;
    if !raw.candidate_metadata.is_empty() {
        return Err(BuildResponseError::InvalidValue);
    }

    let usage = match &response.usage {
        Some(usage) => {
            let usage = build_usage(usage, &raw.usage_metadata)?;
            validate_candidate_token_counts(&candidate_token_counts, &usage)?;
            Some(Value::Object(usage))
        }
        None if raw.usage_metadata.is_empty() => None,
        None => return Err(BuildResponseError::InvalidValue),
    };

    let mut root = Map::new();
    root.insert("responseId".to_owned(), Value::String(response.id.clone()));
    root.insert(
        "modelVersion".to_owned(),
        Value::String(response.model.clone()),
    );
    root.insert("candidates".to_owned(), Value::Array(candidates));
    if let Some(usage) = usage {
        root.insert("usageMetadata".to_owned(), usage);
    }
    if let Some(prompt_feedback) = raw.prompt_feedback {
        root.insert("promptFeedback".to_owned(), prompt_feedback);
    }
    if let Some(model_status) = raw.model_status {
        root.insert("modelStatus".to_owned(), model_status);
    }
    validate_object(&root, super::REQUEST_JSON_LIMITS, super::MAX_BODY_BYTES)
        .map_err(|_| BuildResponseError::StructureLimitExceeded)?;
    Ok(Value::Object(root))
}

fn build_candidates(
    response: &CanonicalResponse,
    raw: &mut ValidatedRawResponse,
) -> Result<(Vec<Value>, Vec<Option<i64>>), BuildResponseError> {
    let mut state = ResponseBuildState::default();
    let mut seen_indexes = HashSet::with_capacity(response.choices.len());
    let mut candidates = Vec::with_capacity(response.choices.len());
    let mut token_counts = Vec::with_capacity(response.choices.len());
    for choice in &response.choices {
        if i64::from(choice.index) > i64::from(i32::MAX) || !seen_indexes.insert(choice.index) {
            return Err(BuildResponseError::InvalidValue);
        }
        validate_choice(choice)?;
        let parts = build_content(choice, &mut state)?;
        let has_tool_use = choice
            .message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse { .. }));
        if has_tool_use != (choice.finish_reason == FinishReason::ToolCalls) {
            return Err(BuildResponseError::InvalidValue);
        }
        if parts.is_empty() && choice.finish_reason != FinishReason::ContentFilter {
            return Err(BuildResponseError::InvalidValue);
        }

        let mut candidate = Map::new();
        candidate.insert(
            "index".to_owned(),
            Value::Number(i64::from(choice.index).into()),
        );
        candidate.insert(
            "content".to_owned(),
            object_value([
                ("role", Value::String("model".to_owned())),
                ("parts", Value::Array(parts)),
            ]),
        );
        let mut metadata = raw.candidate_metadata.remove(&choice.index);
        let finish_reason = build_finish_reason(choice, metadata.as_ref())?;
        candidate.insert(
            "finishReason".to_owned(),
            Value::String(finish_reason.to_owned()),
        );
        if let Some(metadata) = metadata.as_mut() {
            metadata.remove("finishReason");
        }
        if let Some(metadata) = metadata {
            for (key, value) in metadata {
                if candidate.insert(key, value).is_some() {
                    return Err(BuildResponseError::InvalidValue);
                }
            }
        }
        token_counts.push(candidate.get("tokenCount").and_then(Value::as_i64));
        candidates.push(Value::Object(candidate));
    }
    Ok((candidates, token_counts))
}

fn validate_choice(choice: &ResponseChoice) -> Result<(), BuildResponseError> {
    if choice.message.role != Role::Assistant || choice.stop_sequence.is_some() {
        return Err(BuildResponseError::InvalidValue);
    }
    if choice.message.content.len() > MAX_PARTS_PER_CONTENT {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    Ok(())
}

fn build_finish_reason(
    choice: &ResponseChoice,
    metadata: Option<&Map<String, Value>>,
) -> Result<&'static str, BuildResponseError> {
    let raw_reason = metadata
        .and_then(|metadata| metadata.get("finishReason"))
        .and_then(Value::as_str);
    match choice.finish_reason {
        FinishReason::Stop if raw_reason.is_none() => Ok("STOP"),
        FinishReason::Length if raw_reason.is_none() => Ok("MAX_TOKENS"),
        FinishReason::ToolCalls if raw_reason.is_none() => Ok("STOP"),
        FinishReason::ContentFilter => match raw_reason {
            None => Ok("SAFETY"),
            Some(reason) => {
                let reason = parse_finish_reason(reason)?;
                if !reason.is_content_filter() {
                    return Err(BuildResponseError::InvalidValue);
                }
                Ok(reason.as_str())
            }
        },
        FinishReason::Stop | FinishReason::Length | FinishReason::ToolCalls => {
            Err(BuildResponseError::InvalidValue)
        }
    }
}

fn build_content(
    choice: &ResponseChoice,
    state: &mut ResponseBuildState,
) -> Result<Vec<Value>, BuildResponseError> {
    let mut seen_call_ids = HashSet::new();
    choice
        .message
        .content
        .iter()
        .map(|block| build_content_block(block, state, &mut seen_call_ids))
        .collect()
}

fn build_content_block(
    block: &ContentBlock,
    state: &mut ResponseBuildState,
    seen_call_ids: &mut HashSet<String>,
) -> Result<Value, BuildResponseError> {
    match block {
        ContentBlock::Text(text) => {
            state.add_text(text)?;
            Ok(object_value([("text", Value::String(text.clone()))]))
        }
        ContentBlock::Thinking { text, signature } => {
            state.add_text(text)?;
            let mut part = Map::from_iter([
                ("text".to_owned(), Value::String(text.clone())),
                ("thought".to_owned(), Value::Bool(true)),
            ]);
            if let Some(signature) = signature {
                validate_signature(signature, state)?;
                part.insert(
                    "thoughtSignature".to_owned(),
                    Value::String(signature.clone()),
                );
            }
            Ok(Value::Object(part))
        }
        ContentBlock::Image { source, mime_type } => {
            build_image_part(source, mime_type.as_deref(), state)
        }
        ContentBlock::Audio { source, mime_type } => build_audio_part(source, mime_type, state),
        ContentBlock::ToolUse {
            id,
            name,
            input,
            signature,
        } => build_tool_use_part(id, name, input, signature.as_deref(), state, seen_call_ids),
        ContentBlock::ToolResult { .. }
        | ContentBlock::CacheControl(_)
        | ContentBlock::Compaction(_) => Err(BuildResponseError::UnsupportedFeature),
    }
}

fn build_tool_use_part(
    id: &str,
    name: &str,
    input: &Value,
    signature: Option<&str>,
    state: &mut ResponseBuildState,
    seen_call_ids: &mut HashSet<String>,
) -> Result<Value, BuildResponseError> {
    validate_call_id(id).map_err(map_request_validation_error)?;
    validate_tool_name(name).map_err(map_request_validation_error)?;
    if !seen_call_ids.insert(id.to_owned()) {
        return Err(BuildResponseError::InvalidValue);
    }
    let (bytes, nodes) = validate_json_object(input, TOOL_PAYLOAD_LIMITS, MAX_ARGUMENT_BYTES)
        .map_err(map_request_validation_error)?;
    state.add_argument(bytes, nodes)?;
    state.add_block()?;
    if let Some(signature) = signature {
        validate_signature(signature, state)?;
    }

    let mut part = Map::from_iter([(
        "functionCall".to_owned(),
        object_value([
            ("id", Value::String(id.to_owned())),
            ("name", Value::String(name.to_owned())),
            ("args", input.clone()),
        ]),
    )]);
    if let Some(signature) = signature {
        part.insert(
            "thoughtSignature".to_owned(),
            Value::String(signature.to_owned()),
        );
    }
    Ok(Value::Object(part))
}

fn build_image_part(
    source: &MediaSource,
    mime_type: Option<&str>,
    state: &mut ResponseBuildState,
) -> Result<Value, BuildResponseError> {
    let MediaSource::Base64(data) = source else {
        return Err(BuildResponseError::UnsupportedFeature);
    };
    let mime_type = mime_type.ok_or(BuildResponseError::InvalidValue)?;
    let mime_type = normalize_mime_type(mime_type).map_err(map_request_validation_error)?;
    if !mime_type.starts_with("image/") {
        return Err(BuildResponseError::InvalidValue);
    }
    build_inline_data_part(data, mime_type, state)
}

fn build_audio_part(
    source: &MediaSource,
    mime_type: &str,
    state: &mut ResponseBuildState,
) -> Result<Value, BuildResponseError> {
    let MediaSource::Base64(data) = source else {
        return Err(BuildResponseError::UnsupportedFeature);
    };
    let mime_type = normalize_mime_type(mime_type).map_err(map_request_validation_error)?;
    if !mime_type.starts_with("audio/") {
        return Err(BuildResponseError::InvalidValue);
    }
    build_inline_data_part(data, mime_type, state)
}

fn build_inline_data_part(
    data: &str,
    mime_type: String,
    state: &mut ResponseBuildState,
) -> Result<Value, BuildResponseError> {
    let decoded = decode_base64(data).ok_or(BuildResponseError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_MEDIA_BYTES {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    state.add_media(decoded.len())?;
    Ok(object_value([(
        "inlineData",
        object_value([
            ("mimeType", Value::String(mime_type)),
            ("data", Value::String(data.to_owned())),
        ]),
    )]))
}

fn validate_signature(
    signature: &str,
    state: &mut ResponseBuildState,
) -> Result<(), BuildResponseError> {
    let decoded = decode_base64(signature).ok_or(BuildResponseError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_SIGNATURE_BYTES {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    state.add_signature(decoded.len())
}

pub(super) fn build_usage(
    usage: &Usage,
    raw: &Map<String, Value>,
) -> Result<Map<String, Value>, BuildResponseError> {
    let details = usage.details();
    if details.cache_creation_5m().get() != 0 || details.cache_creation_1h().get() != 0 {
        return Err(BuildResponseError::UnsupportedFeature);
    }
    let input = usage
        .checked_input_tokens()
        .map_err(|_| BuildResponseError::InvalidValue)?
        .get();
    let output = usage.output_tokens().get();
    let reasoning = details.reasoning().get();
    let candidates = output
        .checked_sub(reasoning)
        .ok_or(BuildResponseError::InvalidValue)?;
    ensure_wire_token(input)?;
    ensure_wire_token(output)?;
    ensure_wire_token(reasoning)?;
    ensure_wire_token(candidates)?;

    let tool_prompt = raw
        .get("toolUsePromptTokenCount")
        .map(parse_raw_token)
        .transpose()?
        .unwrap_or(0);
    let prompt = input
        .checked_sub(tool_prompt)
        .ok_or(BuildResponseError::InvalidValue)?;
    let total = input
        .checked_add(output)
        .ok_or(BuildResponseError::InvalidValue)?;
    ensure_wire_token(prompt)?;
    ensure_wire_token(total)?;

    let mut encoded = Map::from_iter([
        ("promptTokenCount".to_owned(), Value::Number(prompt.into())),
        (
            "candidatesTokenCount".to_owned(),
            Value::Number(candidates.into()),
        ),
        ("totalTokenCount".to_owned(), Value::Number(total.into())),
    ]);
    let cache_read = details.cache_read().get();
    ensure_wire_token(cache_read)?;
    if cache_read != 0 {
        encoded.insert(
            "cachedContentTokenCount".to_owned(),
            Value::Number(cache_read.into()),
        );
    }
    if reasoning != 0 {
        encoded.insert(
            "thoughtsTokenCount".to_owned(),
            Value::Number(reasoning.into()),
        );
    }
    if details.audio_input().get() != 0 && !raw.contains_key("promptTokensDetails") {
        encoded.insert(
            "promptTokensDetails".to_owned(),
            Value::Array(vec![modality_count("AUDIO", details.audio_input().get())]),
        );
    }
    if details.audio_output().get() != 0 && !raw.contains_key("candidatesTokensDetails") {
        encoded.insert(
            "candidatesTokensDetails".to_owned(),
            Value::Array(vec![modality_count("AUDIO", details.audio_output().get())]),
        );
    }
    for (key, value) in raw {
        if encoded.insert(key.clone(), value.clone()).is_some() {
            return Err(BuildResponseError::InvalidValue);
        }
    }

    let wire = serde_json::from_value(Value::Object(encoded.clone()))
        .map_err(|_| BuildResponseError::InvalidValue)?;
    let converted = convert_usage_metadata(wire).map_err(map_response_validation_error)?;
    if !usage_equivalent(usage, &converted.usage) {
        return Err(BuildResponseError::InvalidValue);
    }
    Ok(encoded)
}

fn usage_equivalent(left: &Usage, right: &Usage) -> bool {
    let Ok(left_input) = left.checked_input_tokens() else {
        return false;
    };
    let Ok(right_input) = right.checked_input_tokens() else {
        return false;
    };
    left_input == right_input
        && left.output_tokens() == right.output_tokens()
        && left.details().cache_read() == right.details().cache_read()
        && left.details().cache_creation_5m() == right.details().cache_creation_5m()
        && left.details().cache_creation_1h() == right.details().cache_creation_1h()
        && left.details().reasoning() == right.details().reasoning()
        && left.details().audio_input() == right.details().audio_input()
        && left.details().audio_output() == right.details().audio_output()
}

fn modality_count(modality: &str, count: i64) -> Value {
    object_value([
        ("modality", Value::String(modality.to_owned())),
        ("tokenCount", Value::Number(count.into())),
    ])
}

fn validate_candidate_token_counts(
    counts: &[Option<i64>],
    usage: &Map<String, Value>,
) -> Result<(), BuildResponseError> {
    if counts.iter().any(Option::is_none) {
        return Ok(());
    }
    let expected = usage
        .get("candidatesTokenCount")
        .and_then(Value::as_i64)
        .ok_or(BuildResponseError::InvalidValue)?;
    let sum = counts.iter().try_fold(0_i64, |sum, count| {
        let Some(count) = count else {
            return Err(BuildResponseError::InvalidValue);
        };
        sum.checked_add(*count)
            .ok_or(BuildResponseError::InvalidValue)
    })?;
    if sum != expected {
        return Err(BuildResponseError::InvalidValue);
    }
    Ok(())
}

#[derive(Default)]
struct ValidatedRawResponse {
    prompt_feedback: Option<Value>,
    model_status: Option<Value>,
    candidate_metadata: HashMap<u32, Map<String, Value>>,
    usage_metadata: Map<String, Value>,
    prompt_blocked: bool,
}

fn validate_raw(response: &CanonicalResponse) -> Result<ValidatedRawResponse, BuildResponseError> {
    let Some(raw) = response.raw_passthrough() else {
        return Ok(ValidatedRawResponse::default());
    };
    let fields = raw
        .fields_for_protocol(Protocol::Gemini)
        .map_err(|_| BuildResponseError::RawProtocolMismatch)?;
    let mut validated = ValidatedRawResponse::default();
    for (key, value) in fields {
        match key.as_str() {
            "promptFeedback" => {
                let wire: PromptFeedbackWire = serde_json::from_value(value.clone())
                    .map_err(|_| BuildResponseError::InvalidValue)?;
                validated.prompt_blocked = validate_prompt_feedback(Field::Value(wire))
                    .map_err(map_response_validation_error)?;
                validated.prompt_feedback = Some(value.clone());
            }
            "modelStatus" => {
                let wire: ModelStatusWire = serde_json::from_value(value.clone())
                    .map_err(|_| BuildResponseError::InvalidValue)?;
                validate_model_status(Field::Value(wire)).map_err(map_response_validation_error)?;
                validated.model_status = Some(value.clone());
            }
            "candidates" => validate_raw_candidates(value, &mut validated.candidate_metadata)?,
            "usageMetadata" => validate_raw_usage(value, &mut validated.usage_metadata)?,
            _ => return Err(BuildResponseError::UnsupportedFeature),
        }
    }
    Ok(validated)
}

fn validate_raw_candidates(
    value: &Value,
    output: &mut HashMap<u32, Map<String, Value>>,
) -> Result<(), BuildResponseError> {
    let candidates = value.as_array().ok_or(BuildResponseError::InvalidValue)?;
    if candidates.len() > MAX_RESPONSE_CHOICES {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    for candidate in candidates {
        let object = candidate
            .as_object()
            .ok_or(BuildResponseError::InvalidValue)?;
        let wire: CandidateWire = serde_json::from_value(candidate.clone())
            .map_err(|_| BuildResponseError::InvalidValue)?;
        let CandidateWire {
            index,
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
        } = wire;
        if !matches!(content, Field::Missing)
            || !matches!(grounding_metadata, Field::Missing)
            || !matches!(logprobs_result, Field::Missing)
            || !matches!(avg_logprobs, Field::Missing)
            || !matches!(url_context_metadata, Field::Missing)
            || !matches!(grounding_attributions, Field::Missing)
            || !matches!(citation_metadata, Field::Missing)
        {
            return Err(BuildResponseError::UnsupportedFeature);
        }
        validate_safety_ratings(safety_ratings).map_err(map_response_validation_error)?;
        if let Field::Value(message) = finish_message {
            if message.len() > MAX_FINISH_MESSAGE_BYTES {
                return Err(BuildResponseError::StructureLimitExceeded);
            }
            if message.chars().any(char::is_control) {
                return Err(BuildResponseError::InvalidValue);
            }
        }
        if let Field::Value(reason) = finish_reason
            && (reason == FinishReasonWire::Unspecified || !reason.is_content_filter())
        {
            return Err(BuildResponseError::InvalidValue);
        }
        if let Field::Value(token_count) = token_count {
            ensure_wire_token(token_count)?;
        }
        let Field::Value(index) = index else {
            return Err(BuildResponseError::InvalidValue);
        };
        if !(0..=i64::from(i32::MAX)).contains(&index) {
            return Err(BuildResponseError::InvalidValue);
        }
        let index = u32::try_from(index).map_err(|_| BuildResponseError::InvalidValue)?;
        let mut metadata = object.clone();
        metadata.remove("index");
        if metadata.is_empty() || output.insert(index, metadata).is_some() {
            return Err(BuildResponseError::InvalidValue);
        }
    }
    Ok(())
}

fn validate_raw_usage(
    value: &Value,
    output: &mut Map<String, Value>,
) -> Result<(), BuildResponseError> {
    let usage = value.as_object().ok_or(BuildResponseError::InvalidValue)?;
    for (key, value) in usage {
        if !matches!(
            key.as_str(),
            "promptTokensDetails"
                | "cacheTokensDetails"
                | "candidatesTokensDetails"
                | "toolUsePromptTokenCount"
                | "toolUsePromptTokensDetails"
                | "serviceTier"
        ) {
            return Err(BuildResponseError::UnsupportedFeature);
        }
        output.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn parse_finish_reason(value: &str) -> Result<FinishReasonWire, BuildResponseError> {
    serde_json::from_value(Value::String(value.to_owned()))
        .map_err(|_| BuildResponseError::InvalidValue)
}

fn parse_raw_token(value: &Value) -> Result<i64, BuildResponseError> {
    let value = value.as_i64().ok_or(BuildResponseError::InvalidValue)?;
    ensure_wire_token(value)?;
    Ok(value)
}

fn ensure_wire_token(value: i64) -> Result<(), BuildResponseError> {
    if !(0..=i64::from(i32::MAX)).contains(&value) {
        return Err(BuildResponseError::InvalidValue);
    }
    Ok(())
}

fn object_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(Map::from_iter(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    ))
}

#[derive(Default)]
struct ResponseBuildState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    argument_bytes: usize,
    json_nodes: usize,
    signature_bytes: usize,
    tool_calls: usize,
}

impl ResponseBuildState {
    fn add_block(&mut self) -> Result<(), BuildResponseError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text(&mut self, text: &str) -> Result<(), BuildResponseError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_media(&mut self, bytes: usize) -> Result<(), BuildResponseError> {
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_argument(&mut self, bytes: usize, nodes: usize) -> Result<(), BuildResponseError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        self.json_nodes = self
            .json_nodes
            .checked_add(nodes)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.json_nodes > MAX_TOTAL_JSON_NODES
        {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_signature(&mut self, bytes: usize) -> Result<(), BuildResponseError> {
        self.signature_bytes = self
            .signature_bytes
            .checked_add(bytes)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        if self.signature_bytes > MAX_TOTAL_SIGNATURE_BYTES {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        Ok(())
    }
}

fn map_request_validation_error(error: ParseRequestError) -> BuildResponseError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            BuildResponseError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => BuildResponseError::UnsupportedFeature,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => BuildResponseError::InvalidValue,
    }
}

fn map_response_validation_error(error: ParseResponseError) -> BuildResponseError {
    match error {
        ParseResponseError::BodyTooLarge | ParseResponseError::StructureLimitExceeded => {
            BuildResponseError::StructureLimitExceeded
        }
        ParseResponseError::UnsupportedFeature => BuildResponseError::UnsupportedFeature,
        ParseResponseError::InvalidJson
        | ParseResponseError::DuplicateKey
        | ParseResponseError::InvalidValue => BuildResponseError::InvalidValue,
    }
}

/// Gemini `generateContent` 非流式响应构造错误，不保留 Canonical 中的敏感值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResponseError {
    /// 当前构造器仅支持 Chat 操作。
    UnsupportedOperation,
    /// Canonical 字段、角色或关联关系无效。
    InvalidValue,
    /// 目标协议缺少一项已声明的响应能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 使用了 Gemini 响应无法表达的能力。
    UnsupportedFeature,
    /// 响应结构或序列化正文超过预算。
    StructureLimitExceeded,
    /// raw 字段来源不是 Gemini，禁止跨协议透传。
    RawProtocolMismatch,
}

impl fmt::Display for BuildResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation => formatter.write_str("响应操作类型不受支持"),
            Self::InvalidValue => formatter.write_str("响应字段值无效"),
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedFeature => formatter.write_str("响应包含 Gemini 无法表达的特性"),
            Self::StructureLimitExceeded => formatter.write_str("响应结构超过限制"),
            Self::RawProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
        }
    }
}

impl Error for BuildResponseError {}
