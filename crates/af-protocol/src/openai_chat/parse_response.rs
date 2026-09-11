use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol};
use serde_json::{Map, Value};

use super::{
    ParseRequestError,
    convert::{
        ARGUMENT_JSON_LIMITS, MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_TEXT_BYTES,
        MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES,
        validate_call_id, validate_model, validate_tool_name, validate_value_shape,
    },
    response_wire::{
        AssistantRoleWire, ChatCompletionObjectWire, ChatResponseWire, CompletionTokensDetailsWire,
        CompletionUsageWire, FinishReasonWire, PromptTokensDetailsWire, ResponseChoiceWire,
        ResponseMessageWire,
    },
};
use crate::bounded_json::{BoundedJsonError, JsonLimits, parse_value};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, Message, ResponseChoice, TokenCount, Usage,
    UsageDetails, UsageSemantics, UsageSource,
};

pub(super) const MAX_RESPONSE_CHOICES: usize = 128;
const MAX_RESPONSE_ID_BYTES: usize = 256;
pub(super) const MAX_RESPONSE_FINGERPRINT_BYTES: usize = 256;
const RESPONSE_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 32,
    max_nodes: 100_000,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: 32 * 1024 * 1024,
    max_key_bytes: 1_024,
};

/// 解析非流式 OpenAI Chat Completions 响应。
///
/// 本函数先通过受限 JSON 解析拒绝重复键和结构超限，再转换为私有 wire DTO，
/// 最后生成保留多 choice 与 usage 口径的 Canonical 响应。
pub fn parse_response(body: &[u8]) -> Result<CanonicalResponse, ParseResponseError> {
    if body.len() > super::MAX_BODY_BYTES {
        return Err(ParseResponseError::BodyTooLarge);
    }
    let value = parse_value(body, RESPONSE_JSON_LIMITS).map_err(map_json_error)?;
    if uses_known_unsupported_feature(&value) {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let wire: ChatResponseWire =
        serde_json::from_value(value).map_err(|_| ParseResponseError::InvalidValue)?;
    convert_response(wire)
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(response) = value.as_object() else {
        return false;
    };
    if response
        .get("moderation")
        .is_some_and(|value| !value.is_null())
    {
        return true;
    }
    response
        .get("choices")
        .and_then(Value::as_array)
        .is_some_and(|choices| choices.iter().any(choice_uses_unsupported_feature))
}

fn choice_uses_unsupported_feature(choice: &Value) -> bool {
    let Some(choice) = choice.as_object() else {
        return false;
    };
    if choice.get("logprobs").is_some_and(|value| !value.is_null())
        || choice.get("finish_reason").and_then(Value::as_str) == Some("function_call")
    {
        return true;
    }
    let Some(message) = choice.get("message").and_then(Value::as_object) else {
        return false;
    };
    if ["refusal", "audio", "function_call"]
        .iter()
        .any(|key| message.get(*key).is_some_and(|value| !value.is_null()))
    {
        return true;
    }
    if message
        .get("annotations")
        .and_then(Value::as_array)
        .is_some_and(|annotations| !annotations.is_empty())
    {
        return true;
    }
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| {
            calls.iter().any(|call| {
                call.as_object()
                    .and_then(|call| call.get("type"))
                    .and_then(Value::as_str)
                    == Some("custom")
            })
        })
}

fn convert_response(wire: ChatResponseWire) -> Result<CanonicalResponse, ParseResponseError> {
    let ChatResponseWire {
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
    } = wire;
    let _ = request_id;
    if moderation.is_some() {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    if !matches!(object, ChatCompletionObjectWire::ChatCompletion) {
        return Err(ParseResponseError::InvalidValue);
    }
    validate_response_id(&id)?;
    validate_model(&model).map_err(map_request_validation_error)?;
    if created < 0 {
        return Err(ParseResponseError::InvalidValue);
    }
    if choices.is_empty() || choices.len() > MAX_RESPONSE_CHOICES {
        return Err(ParseResponseError::StructureLimitExceeded);
    }

    let mut state = ResponseConvertState::default();
    let mut seen_indexes = HashSet::with_capacity(choices.len());
    let mut converted_choices = Vec::with_capacity(choices.len());
    for choice in choices {
        if !seen_indexes.insert(choice.index) {
            return Err(ParseResponseError::InvalidValue);
        }
        converted_choices.push(convert_choice(choice, &mut state)?);
    }

    let usage = convert_usage(usage)?;
    let mut raw = Map::new();
    if let Some(service_tier) = service_tier {
        raw.insert(
            "service_tier".to_owned(),
            Value::String(service_tier.as_str().to_owned()),
        );
    }
    if let Some(system_fingerprint) = system_fingerprint {
        if system_fingerprint.is_empty()
            || system_fingerprint.len() > MAX_RESPONSE_FINGERPRINT_BYTES
            || system_fingerprint.chars().any(char::is_control)
        {
            return Err(ParseResponseError::InvalidValue);
        }
        raw.insert(
            "system_fingerprint".to_owned(),
            Value::String(system_fingerprint),
        );
    }
    let raw = (!raw.is_empty()).then_some(raw);

    Ok(CanonicalResponse::new(
        Operation::Chat,
        id,
        model,
        Some(created),
        converted_choices,
        usage,
    )
    .with_validated_raw_passthrough(raw.map(|fields| (Protocol::OpenAiChat, fields))))
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

#[derive(Default)]
struct ResponseConvertState {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

impl ResponseConvertState {
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

    fn add_arguments(&mut self, bytes: usize, nodes: usize) -> Result<(), ParseResponseError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(ParseResponseError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES
        {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        self.add_block()
    }
}

fn convert_choice(
    choice: ResponseChoiceWire,
    state: &mut ResponseConvertState,
) -> Result<ResponseChoice, ParseResponseError> {
    let ResponseChoiceWire {
        index,
        message,
        finish_reason,
        logprobs,
    } = choice;
    if logprobs.is_some() {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let reason = convert_finish_reason(finish_reason)?;
    let message = convert_message(message, state)?;
    if reason == FinishReason::ToolCalls
        && !message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
    {
        return Err(ParseResponseError::InvalidValue);
    }
    Ok(ResponseChoice::new(index, message, reason))
}

pub(super) fn convert_finish_reason(
    reason: FinishReasonWire,
) -> Result<FinishReason, ParseResponseError> {
    match reason {
        FinishReasonWire::Stop => Ok(FinishReason::Stop),
        FinishReasonWire::Length => Ok(FinishReason::Length),
        FinishReasonWire::ToolCalls => Ok(FinishReason::ToolCalls),
        FinishReasonWire::ContentFilter => Ok(FinishReason::ContentFilter),
        FinishReasonWire::FunctionCall => Err(ParseResponseError::UnsupportedFeature),
    }
}

fn convert_message(
    message: ResponseMessageWire,
    state: &mut ResponseConvertState,
) -> Result<Message, ParseResponseError> {
    let _ = (&message.reasoning_content, &message.reasoning);
    if !matches!(message.role, AssistantRoleWire::Assistant) {
        return Err(ParseResponseError::InvalidValue);
    }
    if message
        .annotations
        .as_ref()
        .is_some_and(|annotations| !annotations.is_empty())
    {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    if message.refusal.is_some() || message.audio.is_some() || message.function_call.is_some() {
        return Err(ParseResponseError::UnsupportedFeature);
    }

    let mut content = Vec::new();
    if let Some(text) = message.content {
        state.add_text(&text)?;
        content.push(ContentBlock::Text(text));
    }

    let tool_calls = match message.tool_calls {
        None => Vec::new(),
        Some(calls) => {
            if calls.len() > MAX_TOOL_CALLS {
                return Err(ParseResponseError::StructureLimitExceeded);
            }
            calls
        }
    };
    let mut seen_ids = HashSet::with_capacity(tool_calls.len());
    for tool_call in tool_calls {
        let super::wire::ToolCallWire::Function(payload) = tool_call;
        if !seen_ids.insert(payload.id.clone()) {
            return Err(ParseResponseError::InvalidValue);
        }
        validate_call_id(&payload.id).map_err(map_request_validation_error)?;
        validate_tool_name(&payload.function.name).map_err(map_request_validation_error)?;
        let arguments = payload.function.arguments;
        if arguments.len() > MAX_ARGUMENT_BYTES {
            return Err(ParseResponseError::StructureLimitExceeded);
        }
        let input =
            parse_value(arguments.as_bytes(), ARGUMENT_JSON_LIMITS).map_err(map_argument_error)?;
        if !input.is_object() {
            return Err(ParseResponseError::UnsupportedFeature);
        }
        let nodes =
            validate_value_shape(&input, 16, 4_096, 1_024).map_err(map_request_validation_error)?;
        state.add_arguments(arguments.len(), nodes)?;
        content.push(ContentBlock::ToolUse {
            id: payload.id,
            name: payload.function.name,
            input,
            signature: None,
        });
    }
    Ok(Message::new(af_domain::Role::Assistant, content))
}

pub(super) fn convert_usage(
    usage: Option<CompletionUsageWire>,
) -> Result<Option<Usage>, ParseResponseError> {
    let Some(usage) = usage else {
        return Ok(None);
    };
    let prompt_tokens = token_count(usage.prompt_tokens)?;
    let completion_tokens = token_count(usage.completion_tokens)?;
    let expected_total = prompt_tokens
        .get()
        .checked_add(completion_tokens.get())
        .ok_or(ParseResponseError::InvalidValue)?;
    if let Some(total_tokens) = usage.total_tokens
        && total_tokens != expected_total
    {
        return Err(ParseResponseError::InvalidValue);
    }

    let (mut cache_read, audio_input) = convert_prompt_details(usage.prompt_tokens_details)?;
    match (
        usage.prompt_cache_hit_tokens,
        usage.prompt_cache_miss_tokens,
    ) {
        (None, None) => {}
        (Some(hit), Some(miss)) => {
            let hit = token_count(hit)?;
            let miss = token_count(miss)?;
            let prompt_breakdown = hit
                .get()
                .checked_add(miss.get())
                .ok_or(ParseResponseError::InvalidValue)?;
            if prompt_breakdown != prompt_tokens.get()
                || (cache_read != TokenCount::ZERO && cache_read != hit)
            {
                return Err(ParseResponseError::InvalidValue);
            }
            // DeepSeek 的专用字段比兼容字段更精确，允许其覆盖缺省的零值。
            cache_read = hit;
        }
        _ => return Err(ParseResponseError::InvalidValue),
    }
    let (reasoning, audio_output) = convert_completion_details(usage.completion_tokens_details)?;
    let details = UsageDetails::new(
        cache_read,
        TokenCount::ZERO,
        TokenCount::ZERO,
        reasoning,
        audio_input,
        audio_output,
    );
    Usage::new(
        prompt_tokens,
        completion_tokens,
        details,
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map(Some)
    .map_err(|_| ParseResponseError::InvalidValue)
}

fn convert_prompt_details(
    details: Option<PromptTokensDetailsWire>,
) -> Result<(TokenCount, TokenCount), ParseResponseError> {
    let Some(details) = details else {
        return Ok((TokenCount::ZERO, TokenCount::ZERO));
    };
    let cache_read = optional_token(details.cached_tokens)?;
    let audio_input = optional_token(details.audio_tokens)?;
    let cache_write = optional_token(details.cache_write_tokens)?;
    if cache_write != TokenCount::ZERO {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    Ok((cache_read, audio_input))
}

fn convert_completion_details(
    details: Option<CompletionTokensDetailsWire>,
) -> Result<(TokenCount, TokenCount), ParseResponseError> {
    let Some(details) = details else {
        return Ok((TokenCount::ZERO, TokenCount::ZERO));
    };
    let reasoning = optional_token(details.reasoning_tokens)?;
    let audio_output = optional_token(details.audio_tokens)?;
    let accepted = optional_token(details.accepted_prediction_tokens)?;
    let rejected = optional_token(details.rejected_prediction_tokens)?;
    if accepted != TokenCount::ZERO || rejected != TokenCount::ZERO {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    Ok((reasoning, audio_output))
}

fn token_count(value: i64) -> Result<TokenCount, ParseResponseError> {
    TokenCount::new(value).map_err(|_| ParseResponseError::InvalidValue)
}

fn optional_token(value: Option<i64>) -> Result<TokenCount, ParseResponseError> {
    value.map_or(Ok(TokenCount::ZERO), token_count)
}

fn map_argument_error(error: BoundedJsonError) -> ParseResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseResponseError::UnsupportedFeature,
        BoundedJsonError::DuplicateKey => ParseResponseError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseResponseError::StructureLimitExceeded,
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
        ParseRequestError::InvalidJson | ParseRequestError::DuplicateKey => {
            ParseResponseError::InvalidValue
        }
        ParseRequestError::ConflictingParameters | ParseRequestError::InvalidValue => {
            ParseResponseError::InvalidValue
        }
    }
}

/// OpenAI Chat 非流式响应解析错误，不保留响应正文、字段名或底层错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseResponseError {
    /// 响应体超过协议层防御性大小上限。
    BodyTooLarge,
    /// 响应体不是单个合法 JSON 值。
    InvalidJson,
    /// 任意层级的 JSON 对象包含重复键。
    DuplicateKey,
    /// 响应结构超过预算。
    StructureLimitExceeded,
    /// 响应字段的类型、取值或关联关系无效。
    InvalidValue,
    /// 响应包含当前 Canonical 无法无损表达的特性。
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
