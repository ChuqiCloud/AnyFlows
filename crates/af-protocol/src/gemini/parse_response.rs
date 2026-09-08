use std::{collections::HashSet, error::Error, fmt};

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
    response_wire::{
        BlockReasonWire, CandidateWire, FinishReasonWire, GenerateContentResponseWire,
        HarmProbabilityWire, ModalityTokenCountWire, ModalityWire, ModelStageWire, ModelStatusWire,
        PromptFeedbackWire, SafetyCategoryWire, SafetyRatingWire, ServiceTierWire,
        UsageMetadataWire,
    },
    wire::{BlobWire, ContentWire, Field, FunctionCallWire, PartWire},
};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, MediaSource, Message, ResponseChoice,
    TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
    bounded_json::{BoundedJsonError, parse_value},
};

pub(super) const MAX_RESPONSE_CHOICES: usize = 128;
pub(super) const MAX_RESPONSE_ID_BYTES: usize = 256;
pub(super) const MAX_FINISH_MESSAGE_BYTES: usize = 16 * 1024;
pub(super) const MAX_SAFETY_RATINGS: usize = 32;
pub(super) const MAX_MODEL_STATUS_MESSAGE_BYTES: usize = 16 * 1024;
pub(super) const MAX_MODEL_RETIREMENT_TIME_BYTES: usize = 128;
const MAX_MODALITY_DETAILS: usize = 16;

/// 解析 Gemini `generateContent` 非流式响应。
///
/// 响应先经过共享受限 JSON 边界，再按官方闭合类型转换。无法进入 Canonical 的
/// 安全与模型状态元数据仅保存在同源 raw；Grounding、引用和 logprobs 失败关闭。
pub fn parse_response(body: &[u8]) -> Result<CanonicalResponse, ParseResponseError> {
    if body.len() > super::MAX_BODY_BYTES {
        return Err(ParseResponseError::BodyTooLarge);
    }
    let value = parse_value(body, super::REQUEST_JSON_LIMITS).map_err(map_json_error)?;
    if uses_known_unsupported_feature(&value) {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let raw = extract_raw_fields(&value);
    let wire = serde_json::from_value(value).map_err(|_| ParseResponseError::InvalidValue)?;
    convert_response(wire, raw)
}

pub(super) fn uses_known_unsupported_feature(value: &Value) -> bool {
    value
        .get("candidates")
        .and_then(Value::as_array)
        .is_some_and(|candidates| candidates.iter().any(candidate_is_unsupported))
}

fn candidate_is_unsupported(candidate: &Value) -> bool {
    let Some(candidate) = candidate.as_object() else {
        return false;
    };
    if [
        "groundingMetadata",
        "logprobsResult",
        "avgLogprobs",
        "urlContextMetadata",
        "citationMetadata",
    ]
    .iter()
    .any(|key| candidate.get(*key).is_some_and(|value| !value.is_null()))
    {
        return true;
    }
    if candidate
        .get("groundingAttributions")
        .is_some_and(|value| value.as_array().is_none_or(|values| !values.is_empty()))
    {
        return true;
    }
    candidate
        .get("content")
        .and_then(Value::as_object)
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .is_some_and(|parts| parts.iter().any(response_part_is_unsupported))
}

fn response_part_is_unsupported(part: &Value) -> bool {
    let Some(part) = part.as_object() else {
        return false;
    };
    [
        "codeExecutionResult",
        "executableCode",
        "fileData",
        "functionResponse",
        "mediaResolution",
        "partMetadata",
        "toolCall",
        "toolResponse",
        "videoMetadata",
    ]
    .iter()
    .any(|key| part.get(*key).is_some_and(|value| !value.is_null()))
}

fn extract_raw_fields(value: &Value) -> Option<Map<String, Value>> {
    let response = value.as_object()?;
    let mut raw = Map::new();
    for key in ["promptFeedback", "modelStatus"] {
        if let Some(value) = response.get(key) {
            raw.insert(key.to_owned(), value.clone());
        }
    }

    let mut candidate_metadata = Vec::new();
    if let Some(candidates) = response.get("candidates").and_then(Value::as_array) {
        for candidate in candidates {
            let Some(candidate) = candidate.as_object() else {
                continue;
            };
            let mut metadata = Map::new();
            if let Some(index) = candidate.get("index") {
                metadata.insert("index".to_owned(), index.clone());
            }
            if candidate
                .get("finishReason")
                .and_then(Value::as_str)
                .is_some_and(|reason| !matches!(reason, "STOP" | "MAX_TOKENS" | "SAFETY"))
                && let Some(reason) = candidate.get("finishReason")
            {
                metadata.insert("finishReason".to_owned(), reason.clone());
            }
            for key in ["finishMessage", "safetyRatings", "tokenCount"] {
                if let Some(value) = candidate.get(key) {
                    metadata.insert(key.to_owned(), value.clone());
                }
            }
            if metadata.len() > 1 {
                candidate_metadata.push(Value::Object(metadata));
            }
        }
    }
    if !candidate_metadata.is_empty() {
        raw.insert("candidates".to_owned(), Value::Array(candidate_metadata));
    }

    if let Some(usage) = response.get("usageMetadata").and_then(Value::as_object) {
        let mut metadata = Map::new();
        for key in ["promptTokensDetails", "candidatesTokensDetails"] {
            if let Some(value) = usage.get(key)
                && modality_details_need_raw(value)
            {
                metadata.insert(key.to_owned(), value.clone());
            }
        }
        for key in [
            "cacheTokensDetails",
            "toolUsePromptTokenCount",
            "toolUsePromptTokensDetails",
            "serviceTier",
        ] {
            if let Some(value) = usage.get(key) {
                metadata.insert(key.to_owned(), value.clone());
            }
        }
        if !metadata.is_empty() {
            raw.insert("usageMetadata".to_owned(), Value::Object(metadata));
        }
    }
    (!raw.is_empty()).then_some(raw)
}

fn modality_details_need_raw(value: &Value) -> bool {
    value.as_array().is_some_and(|details| {
        details
            .iter()
            .any(|detail| detail.get("modality").and_then(Value::as_str) != Some("AUDIO"))
    })
}

fn convert_response(
    wire: GenerateContentResponseWire,
    raw: Option<Map<String, Value>>,
) -> Result<CanonicalResponse, ParseResponseError> {
    let GenerateContentResponseWire {
        candidates,
        prompt_feedback,
        model_version,
        response_id,
        usage_metadata,
        model_status,
    } = wire;
    let response_id = field_value(response_id).ok_or(ParseResponseError::UnsupportedFeature)?;
    let model_version = field_value(model_version).ok_or(ParseResponseError::UnsupportedFeature)?;
    validate_response_id(&response_id)?;
    validate_model(&model_version).map_err(map_request_validation_error)?;

    let prompt_blocked = validate_prompt_feedback(prompt_feedback)?;
    validate_model_status(model_status)?;
    let candidates = field_value(candidates).unwrap_or_default();
    if candidates.len() > MAX_RESPONSE_CHOICES {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    if candidates.is_empty() != prompt_blocked {
        return Err(ParseResponseError::InvalidValue);
    }

    let mut state = ResponseConvertState::default();
    let mut seen_indexes = HashSet::with_capacity(candidates.len());
    let mut choices = Vec::with_capacity(candidates.len());
    let mut candidate_token_counts = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let (choice, token_count) = convert_candidate(candidate, &mut state)?;
        if !seen_indexes.insert(choice.index) {
            return Err(ParseResponseError::InvalidValue);
        }
        choices.push(choice);
        candidate_token_counts.push(token_count);
    }

    let converted_usage = match field_value(usage_metadata) {
        None => None,
        Some(usage) => Some(convert_usage_metadata(usage)?),
    };
    validate_candidate_token_counts(&candidate_token_counts, converted_usage.as_ref())?;
    let usage = converted_usage.map(|usage| usage.usage);
    Ok(CanonicalResponse::new(
        Operation::Chat,
        response_id,
        model_version,
        None,
        choices,
        usage,
    )
    .with_validated_raw_passthrough(raw.map(|fields| (Protocol::Gemini, fields))))
}

pub(super) fn validate_response_id(id: &str) -> Result<(), ParseResponseError> {
    if id.is_empty()
        || id.len() > MAX_RESPONSE_ID_BYTES
        || id.trim() != id
        || id.chars().any(char::is_control)
    {
        return Err(ParseResponseError::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_prompt_feedback(
    feedback: Field<PromptFeedbackWire>,
) -> Result<bool, ParseResponseError> {
    let Some(feedback) = field_value(feedback) else {
        return Ok(false);
    };
    validate_safety_ratings(feedback.safety_ratings)?;
    match field_value(feedback.block_reason) {
        None => Ok(false),
        Some(BlockReasonWire::Unspecified) => Err(ParseResponseError::InvalidValue),
        Some(
            BlockReasonWire::Safety
            | BlockReasonWire::Other
            | BlockReasonWire::Blocklist
            | BlockReasonWire::ProhibitedContent
            | BlockReasonWire::ImageSafety,
        ) => Ok(true),
    }
}

pub(super) fn validate_model_status(
    status: Field<ModelStatusWire>,
) -> Result<(), ParseResponseError> {
    let Some(status) = field_value(status) else {
        return Ok(());
    };
    if matches!(
        field_value(status.model_stage),
        Some(ModelStageWire::Unspecified)
    ) {
        return Err(ParseResponseError::InvalidValue);
    }
    if let Some(retirement_time) = field_value(status.retirement_time)
        && (retirement_time.is_empty()
            || retirement_time.len() > MAX_MODEL_RETIREMENT_TIME_BYTES
            || retirement_time.trim() != retirement_time
            || retirement_time.chars().any(char::is_control))
    {
        return Err(ParseResponseError::InvalidValue);
    }
    if let Some(message) = field_value(status.message) {
        if message.len() > MAX_MODEL_STATUS_MESSAGE_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        if message.chars().any(char::is_control) {
            return Err(ParseResponseError::InvalidValue);
        }
    }
    Ok(())
}

pub(super) fn validate_safety_ratings(
    ratings: Field<Vec<SafetyRatingWire>>,
) -> Result<(), ParseResponseError> {
    let Some(ratings) = field_value(ratings) else {
        return Ok(());
    };
    if ratings.len() > MAX_SAFETY_RATINGS {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    let mut categories = HashSet::with_capacity(ratings.len());
    for rating in ratings {
        if rating.category == SafetyCategoryWire::Unspecified
            || rating.probability == HarmProbabilityWire::Unspecified
            || !categories.insert(rating.category)
        {
            return Err(ParseResponseError::InvalidValue);
        }
        let _ = field_value(rating.blocked);
    }
    Ok(())
}

fn convert_candidate(
    candidate: CandidateWire,
    state: &mut ResponseConvertState,
) -> Result<(ResponseChoice, Option<TokenCount>), ParseResponseError> {
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
    } = candidate;
    if matches!(grounding_metadata, Field::Value(_))
        || matches!(logprobs_result, Field::Value(_))
        || matches!(avg_logprobs, Field::Value(_))
        || matches!(url_context_metadata, Field::Value(_))
        || matches!(citation_metadata, Field::Value(_))
        || matches!(grounding_attributions, Field::Value(values) if !values.is_empty())
    {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    validate_safety_ratings(safety_ratings)?;
    if let Some(message) = field_value(finish_message) {
        if message.len() > MAX_FINISH_MESSAGE_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        if message.chars().any(char::is_control) {
            return Err(ParseResponseError::InvalidValue);
        }
    }

    let index = field_value(index).ok_or(ParseResponseError::UnsupportedFeature)?;
    if !(0..=i64::from(i32::MAX)).contains(&index) {
        return Err(ParseResponseError::InvalidValue);
    }
    let index = u32::try_from(index).map_err(|_| ParseResponseError::InvalidValue)?;
    let finish_reason = field_value(finish_reason).ok_or(ParseResponseError::InvalidValue)?;
    if finish_reason == FinishReasonWire::Unspecified {
        return Err(ParseResponseError::InvalidValue);
    }

    let mut seen_call_ids = HashSet::new();
    let content = match field_value(content) {
        None => Vec::new(),
        Some(content) => convert_content(content, state, &mut seen_call_ids)?,
    };
    let has_tool_use = content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { .. }));
    let canonical_reason = match finish_reason {
        FinishReasonWire::Stop if has_tool_use => FinishReason::ToolCalls,
        FinishReasonWire::Stop => FinishReason::Stop,
        FinishReasonWire::MaxTokens if !has_tool_use => FinishReason::Length,
        reason if reason.is_content_filter() && !has_tool_use => FinishReason::ContentFilter,
        _ => return Err(ParseResponseError::InvalidValue),
    };
    if content.is_empty() && canonical_reason != FinishReason::ContentFilter {
        return Err(ParseResponseError::InvalidValue);
    }

    let token_count = optional_wire_token(field_value(token_count))?;
    Ok((
        ResponseChoice::new(
            index,
            Message::new(Role::Assistant, content),
            canonical_reason,
        ),
        token_count,
    ))
}

fn convert_content(
    content: ContentWire,
    state: &mut ResponseConvertState,
    seen_call_ids: &mut HashSet<String>,
) -> Result<Vec<ContentBlock>, ParseResponseError> {
    match field_value(content.role).as_deref() {
        None | Some("model") => {}
        Some(_) => return Err(ParseResponseError::InvalidValue),
    }
    if content.parts.len() > MAX_PARTS_PER_CONTENT {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    content
        .parts
        .into_iter()
        .map(|part| convert_part(part, state, seen_call_ids))
        .collect()
}

fn convert_part(
    part: PartWire,
    state: &mut ResponseConvertState,
    seen_call_ids: &mut HashSet<String>,
) -> Result<ContentBlock, ParseResponseError> {
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
        return Err(ParseResponseError::InvalidValue);
    }
    let thought = field_value(thought).unwrap_or(false);
    let signature = field_value(thought_signature);
    match (text, inline_data, function_call, function_response) {
        (Field::Value(text), Field::Missing, Field::Missing, Field::Missing) => {
            if thought {
                let signature = validate_optional_signature(signature, state)?;
                state.add_text(&text)?;
                Ok(ContentBlock::Thinking { text, signature })
            } else {
                if signature.is_some() {
                    return Err(ParseResponseError::UnsupportedFeature);
                }
                state.add_text(&text)?;
                Ok(ContentBlock::Text(text))
            }
        }
        (Field::Missing, Field::Value(media), Field::Missing, Field::Missing) => {
            if thought || signature.is_some() {
                return Err(ParseResponseError::UnsupportedFeature);
            }
            convert_inline_media(media, state)
        }
        (Field::Missing, Field::Missing, Field::Value(call), Field::Missing) => {
            if thought {
                return Err(ParseResponseError::UnsupportedFeature);
            }
            convert_function_call(call, signature, state, seen_call_ids)
        }
        (Field::Missing, Field::Missing, Field::Missing, Field::Value(_)) => {
            Err(ParseResponseError::UnsupportedFeature)
        }
        _ => Err(ParseResponseError::InvalidValue),
    }
}

fn convert_function_call(
    call: FunctionCallWire,
    signature: Option<String>,
    state: &mut ResponseConvertState,
    seen_call_ids: &mut HashSet<String>,
) -> Result<ContentBlock, ParseResponseError> {
    let id = field_value(call.id).ok_or(ParseResponseError::UnsupportedFeature)?;
    validate_call_id(&id).map_err(map_request_validation_error)?;
    validate_tool_name(&call.name).map_err(map_request_validation_error)?;
    if !seen_call_ids.insert(id.clone()) {
        return Err(ParseResponseError::InvalidValue);
    }
    let input = field_value(call.args).unwrap_or_else(|| Value::Object(Map::new()));
    let (bytes, nodes) = validate_json_object(&input, TOOL_PAYLOAD_LIMITS, MAX_ARGUMENT_BYTES)
        .map_err(map_request_validation_error)?;
    state.add_argument(bytes, nodes)?;
    let signature = validate_optional_signature(signature, state)?;
    state.add_block()?;
    Ok(ContentBlock::ToolUse {
        id,
        name: call.name,
        input,
        signature,
    })
}

fn convert_inline_media(
    media: BlobWire,
    state: &mut ResponseConvertState,
) -> Result<ContentBlock, ParseResponseError> {
    let mime_type = normalize_mime_type(&media.mime_type).map_err(map_request_validation_error)?;
    let decoded = decode_base64(&media.data).ok_or(ParseResponseError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_MEDIA_BYTES {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    state.add_media(decoded.len())?;
    let source = MediaSource::Base64(media.data);
    if mime_type.starts_with("image/") {
        Ok(ContentBlock::Image {
            source,
            mime_type: Some(mime_type),
        })
    } else if mime_type.starts_with("audio/") {
        Ok(ContentBlock::Audio { source, mime_type })
    } else {
        Err(ParseResponseError::UnsupportedFeature)
    }
}

fn validate_optional_signature(
    signature: Option<String>,
    state: &mut ResponseConvertState,
) -> Result<Option<String>, ParseResponseError> {
    let Some(signature) = signature else {
        return Ok(None);
    };
    let decoded = decode_base64(&signature).ok_or(ParseResponseError::InvalidValue)?;
    if decoded.is_empty() || decoded.len() > MAX_SIGNATURE_BYTES {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    state.add_signature(decoded.len())?;
    Ok(Some(signature))
}

pub(super) struct ConvertedUsage {
    pub(super) usage: Usage,
    pub(super) candidates_token_count: Option<TokenCount>,
}

pub(super) fn convert_usage_metadata(
    usage: UsageMetadataWire,
) -> Result<ConvertedUsage, ParseResponseError> {
    let prompt = optional_wire_token(field_value(usage.prompt_token_count))?;
    let candidates = optional_wire_token(field_value(usage.candidates_token_count))?;
    let total = optional_wire_token(field_value(usage.total_token_count))?;
    let cache = optional_wire_token(field_value(usage.cached_content_token_count))?;
    let thoughts = optional_wire_token(field_value(usage.thoughts_token_count))?;
    let tool_prompt = optional_wire_token(field_value(usage.tool_use_prompt_token_count))?;

    let prompt_value = prompt.unwrap_or(TokenCount::ZERO);
    let candidate_value = candidates.unwrap_or(TokenCount::ZERO);
    let cache_value = cache.unwrap_or(TokenCount::ZERO);
    let thoughts_value = thoughts.unwrap_or(TokenCount::ZERO);
    let tool_prompt_value = tool_prompt.unwrap_or(TokenCount::ZERO);
    let input_value = checked_token_add(prompt_value, tool_prompt_value)?;
    let output_value = checked_token_add(candidate_value, thoughts_value)?;

    let prompt_audio = validate_modality_details(usage.prompt_tokens_details, prompt_value)?;
    let tool_audio =
        validate_modality_details(usage.tool_use_prompt_tokens_details, tool_prompt_value)?;
    let audio_input = checked_token_add(prompt_audio, tool_audio)?;
    let audio_output = validate_modality_details(usage.candidates_tokens_details, candidate_value)?;
    let _ = validate_modality_details(usage.cache_tokens_details, cache_value)?;
    if matches!(
        field_value(usage.service_tier),
        Some(ServiceTierWire::Unspecified)
    ) {
        return Err(ParseResponseError::InvalidValue);
    }

    let details = UsageDetails::new(
        cache_value,
        TokenCount::ZERO,
        TokenCount::ZERO,
        thoughts_value,
        audio_input,
        audio_output,
    );
    let converted = Usage::new(
        input_value,
        output_value,
        details,
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| ParseResponseError::InvalidValue)?;
    let expected_total = converted
        .checked_total_tokens()
        .map_err(|_| ParseResponseError::InvalidValue)?;
    if total.is_some_and(|total| total != expected_total) {
        return Err(ParseResponseError::InvalidValue);
    }
    Ok(ConvertedUsage {
        usage: converted,
        candidates_token_count: candidates,
    })
}

fn validate_modality_details(
    details: Field<Vec<ModalityTokenCountWire>>,
    total: TokenCount,
) -> Result<TokenCount, ParseResponseError> {
    let Some(details) = field_value(details) else {
        return Ok(TokenCount::ZERO);
    };
    if details.len() > MAX_MODALITY_DETAILS {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    let mut seen = HashSet::with_capacity(details.len());
    let mut sum = 0_i64;
    let mut audio = 0_i64;
    for detail in details {
        if detail.modality == ModalityWire::Unspecified || !seen.insert(detail.modality) {
            return Err(ParseResponseError::InvalidValue);
        }
        let count = wire_token(detail.token_count)?;
        sum = sum
            .checked_add(count.get())
            .ok_or(ParseResponseError::InvalidValue)?;
        if detail.modality == ModalityWire::Audio {
            audio = count.get();
        }
    }
    if sum > total.get() {
        return Err(ParseResponseError::InvalidValue);
    }
    TokenCount::new(audio).map_err(|_| ParseResponseError::InvalidValue)
}

fn validate_candidate_token_counts(
    counts: &[Option<TokenCount>],
    usage: Option<&ConvertedUsage>,
) -> Result<(), ParseResponseError> {
    let Some(expected) = usage.and_then(|usage| usage.candidates_token_count) else {
        return Ok(());
    };
    if counts.iter().any(Option::is_none) {
        return Ok(());
    }
    let sum = counts.iter().try_fold(0_i64, |sum, count| {
        let Some(count) = count else {
            return Err(ParseResponseError::InvalidValue);
        };
        sum.checked_add(count.get())
            .ok_or(ParseResponseError::InvalidValue)
    })?;
    if sum != expected.get() {
        return Err(ParseResponseError::InvalidValue);
    }
    Ok(())
}

fn checked_token_add(
    left: TokenCount,
    right: TokenCount,
) -> Result<TokenCount, ParseResponseError> {
    left.get()
        .checked_add(right.get())
        .ok_or(ParseResponseError::InvalidValue)
        .and_then(|value| TokenCount::new(value).map_err(|_| ParseResponseError::InvalidValue))
}

fn optional_wire_token(value: Option<i64>) -> Result<Option<TokenCount>, ParseResponseError> {
    value.map(wire_token).transpose()
}

fn wire_token(value: i64) -> Result<TokenCount, ParseResponseError> {
    if value > i64::from(i32::MAX) {
        return Err(ParseResponseError::InvalidValue);
    }
    TokenCount::new(value).map_err(|_| ParseResponseError::InvalidValue)
}

fn field_value<T>(field: Field<T>) -> Option<T> {
    match field {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    }
}

#[derive(Default)]
struct ResponseConvertState {
    content_blocks: usize,
    text_bytes: usize,
    media_bytes: usize,
    argument_bytes: usize,
    json_nodes: usize,
    signature_bytes: usize,
    tool_calls: usize,
}

impl ResponseConvertState {
    fn add_block(&mut self) -> Result<(), ParseResponseError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text(&mut self, text: &str) -> Result<(), ParseResponseError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_media(&mut self, bytes: usize) -> Result<(), ParseResponseError> {
        self.media_bytes = self
            .media_bytes
            .checked_add(bytes)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        if self.media_bytes > MAX_MEDIA_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        self.add_block()
    }

    fn add_argument(&mut self, bytes: usize, nodes: usize) -> Result<(), ParseResponseError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        self.json_nodes = self
            .json_nodes
            .checked_add(nodes)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.json_nodes > MAX_TOTAL_JSON_NODES
        {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_signature(&mut self, bytes: usize) -> Result<(), ParseResponseError> {
        self.signature_bytes = self
            .signature_bytes
            .checked_add(bytes)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        if self.signature_bytes > MAX_TOTAL_SIGNATURE_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        Ok(())
    }
}

fn map_json_error(error: BoundedJsonError) -> ParseResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseResponseError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseResponseError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseResponseError::StructureLimitExceeded,
    }
}

fn map_request_validation_error(error: ParseRequestError) -> ParseResponseError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            ParseResponseError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => ParseResponseError::UnsupportedFeature,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => ParseResponseError::InvalidValue,
    }
}

/// Gemini `generateContent` 非流式响应解析错误，不保留正文或外部值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseResponseError {
    /// 响应体超过协议层防御性大小上限。
    BodyTooLarge,
    /// 响应体不是单个合法 JSON 值。
    InvalidJson,
    /// 任意层级的 JSON 对象包含重复键。
    DuplicateKey,
    /// 响应 JSON 或业务结构超过预算。
    StructureLimitExceeded,
    /// 响应字段类型、取值、角色或关联关系无效。
    InvalidValue,
    /// 响应使用了当前 Canonical 无法无损表达的能力。
    UnsupportedFeature,
}

impl fmt::Display for ParseResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BodyTooLarge => formatter.write_str("响应体超过大小限制"),
            Self::InvalidJson => formatter.write_str("响应体不是有效 JSON"),
            Self::DuplicateKey => formatter.write_str("响应体包含重复字段"),
            Self::StructureLimitExceeded => formatter.write_str("响应结构超过限制"),
            Self::InvalidValue => formatter.write_str("响应字段值无效"),
            Self::UnsupportedFeature => formatter.write_str("响应包含当前不支持的特性"),
        }
    }
}

impl Error for ParseResponseError {}
