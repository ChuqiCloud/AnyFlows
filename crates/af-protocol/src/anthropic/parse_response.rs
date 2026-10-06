use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_PARTS_PER_MESSAGE, MAX_STOP_BYTES,
        MAX_TEXT_BYTES, MAX_TOOL_CALLS, MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES,
        MAX_TOTAL_TEXT_BYTES, validate_call_id, validate_model, validate_tool_name,
        validate_value_shape,
    },
    response_wire::{
        CacheCreationWire, MessagesResponseWire, RefusalStopDetailsWire, ResponseContentBlockWire,
        ResponseThinkingBlockWire, ResponseToolUseBlockWire, ServerToolUsageWire, StopReasonWire,
        UsageWire,
    },
};
use crate::bounded_json::{BoundedJsonError, parse_value};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, Message, ResponseChoice, TokenCount, Usage,
    UsageDetails, UsageSemantics, UsageSource,
};

pub(super) const MAX_RESPONSE_ID_BYTES: usize = 256;
pub(super) const MAX_INFERENCE_GEO_BYTES: usize = 64;
pub(super) const MAX_STOP_DETAILS_BYTES: usize = 8 * 1024;

/// 解析 Anthropic Messages 非流式响应。
///
/// 响应先经过受限 JSON 边界，再按官方闭合类型解析。无法无损归一的服务端工具、
/// 容器和内容块会明确失败；可验证的同源元数据仅保存在 Anthropic raw 中。
pub fn parse_response(body: &[u8]) -> Result<CanonicalResponse, ParseResponseError> {
    if body.len() > super::MAX_BODY_BYTES {
        return Err(ParseResponseError::BodyTooLarge);
    }

    let value = parse_value(body, super::REQUEST_JSON_LIMITS).map_err(map_json_error)?;
    if uses_known_unsupported_feature(&value) {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let wire = serde_json::from_value(value).map_err(|_| ParseResponseError::InvalidValue)?;
    convert_response(wire)
}

fn uses_known_unsupported_feature(value: &Value) -> bool {
    let Some(response) = value.as_object() else {
        return false;
    };
    if response
        .get("container")
        .is_some_and(|value| !value.is_null())
    {
        return true;
    }

    response
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|blocks| blocks.iter().any(content_block_is_unsupported))
}

fn content_block_is_unsupported(block: &Value) -> bool {
    let Some(block) = block.as_object() else {
        return false;
    };
    let kind = block.get("type").and_then(Value::as_str);
    if matches!(
        kind,
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
    ) {
        return true;
    }
    if kind == Some("text")
        && block
            .get("citations")
            .is_some_and(|value| !value.is_null() && value.as_array().is_none_or(|v| !v.is_empty()))
    {
        return true;
    }
    kind == Some("tool_use")
        && block.get("caller").is_some_and(|caller| {
            !caller.is_null() && caller.get("type").and_then(Value::as_str) != Some("direct")
        })
}

fn convert_response(wire: MessagesResponseWire) -> Result<CanonicalResponse, ParseResponseError> {
    let MessagesResponseWire {
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
    } = wire;
    if container.is_some() {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    validate_response_id(&id)?;
    validate_model(&model).map_err(map_request_validation_error)?;
    if content.len() > MAX_PARTS_PER_MESSAGE {
        return Err(ParseResponseError::StructureLimitExceeded);
    }

    let mut state = ResponseConvertState::default();
    let mut seen_call_ids = HashSet::new();
    let mut converted_content = Vec::with_capacity(content.len());
    for block in content {
        converted_content.push(convert_content_block(
            block,
            &mut state,
            &mut seen_call_ids,
        )?);
    }

    let converted_stop = convert_stop_reason(stop_reason)?;
    let stop_sequence = validate_stop_sequence(stop_sequence, converted_stop)?;
    let has_tool_use = converted_content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { .. }));
    if has_tool_use != (converted_stop.finish_reason == FinishReason::ToolCalls) {
        return Err(ParseResponseError::InvalidValue);
    }

    let mut raw = Map::new();
    if let Some(details) = convert_stop_details(stop_details, converted_stop)? {
        raw.insert("stop_details".to_owned(), details);
    }
    let (usage, usage_raw) = convert_usage(usage)?;
    if !usage_raw.is_empty() {
        raw.insert("usage".to_owned(), Value::Object(usage_raw));
    }

    let choice = ResponseChoice::new(
        0,
        Message::new(Role::Assistant, converted_content),
        converted_stop.finish_reason,
    )
    .with_stop_sequence(stop_sequence);
    let raw = (!raw.is_empty()).then_some((Protocol::Anthropic, raw));
    Ok(
        CanonicalResponse::new(Operation::Chat, id, model, None, vec![choice], Some(usage))
            .with_validated_raw_passthrough(raw),
    )
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
        self.add_text_bytes(text)?;
        self.add_block()
    }

    fn add_thinking(&mut self, thinking: &str, signature: &str) -> Result<(), ParseResponseError> {
        self.add_text_bytes(thinking)?;
        self.add_text_bytes(signature)?;
        self.add_block()
    }

    fn add_text_bytes(&mut self, text: &str) -> Result<(), ParseResponseError> {
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
}

fn convert_content_block(
    block: ResponseContentBlockWire,
    state: &mut ResponseConvertState,
    seen_call_ids: &mut HashSet<String>,
) -> Result<ContentBlock, ParseResponseError> {
    match block {
        ResponseContentBlockWire::Text(block) => {
            if block
                .citations
                .is_some_and(|citations| !citations.is_empty())
            {
                return Err(ParseResponseError::UnsupportedFeature);
            }
            state.add_text(&block.text)?;
            Ok(ContentBlock::Text(block.text))
        }
        ResponseContentBlockWire::Thinking(ResponseThinkingBlockWire {
            thinking,
            signature,
        }) => {
            if signature.is_empty() || signature.chars().any(char::is_control) {
                return Err(ParseResponseError::InvalidValue);
            }
            state.add_thinking(&thinking, &signature)?;
            Ok(ContentBlock::Thinking {
                text: thinking,
                signature: Some(signature),
            })
        }
        ResponseContentBlockWire::ToolUse(ResponseToolUseBlockWire {
            id,
            name,
            input,
            _caller: _,
        }) => {
            if !seen_call_ids.insert(id.clone()) {
                return Err(ParseResponseError::InvalidValue);
            }
            validate_call_id(&id).map_err(map_request_validation_error)?;
            validate_tool_name(&name).map_err(map_request_validation_error)?;
            if !input.is_object() {
                return Err(ParseResponseError::UnsupportedFeature);
            }
            let bytes = serde_json::to_vec(&input)
                .map_err(|_| ParseResponseError::InvalidValue)?
                .len();
            if bytes > MAX_ARGUMENT_BYTES {
                return Err(ParseResponseError::StructureLimitExceeded);
            }
            let nodes = validate_value_shape(&input, 16, 4_096, 1_024)
                .map_err(map_request_validation_error)?;
            state.add_arguments(bytes, nodes)?;
            Ok(ContentBlock::ToolUse {
                id,
                name,
                input,
                signature: None,
            })
        }
    }
}

#[derive(Clone, Copy)]
struct ConvertedStopReason {
    finish_reason: FinishReason,
    expects_stop_sequence: bool,
    is_refusal: bool,
}

fn convert_stop_reason(reason: StopReasonWire) -> Result<ConvertedStopReason, ParseResponseError> {
    let converted = match reason {
        StopReasonWire::EndTurn => ConvertedStopReason {
            finish_reason: FinishReason::Stop,
            expects_stop_sequence: false,
            is_refusal: false,
        },
        StopReasonWire::MaxTokens | StopReasonWire::ModelContextWindowExceeded => {
            ConvertedStopReason {
                finish_reason: FinishReason::Length,
                expects_stop_sequence: false,
                is_refusal: false,
            }
        }
        StopReasonWire::StopSequence => ConvertedStopReason {
            finish_reason: FinishReason::Stop,
            expects_stop_sequence: true,
            is_refusal: false,
        },
        StopReasonWire::ToolUse => ConvertedStopReason {
            finish_reason: FinishReason::ToolCalls,
            expects_stop_sequence: false,
            is_refusal: false,
        },
        StopReasonWire::Refusal => ConvertedStopReason {
            finish_reason: FinishReason::ContentFilter,
            expects_stop_sequence: false,
            is_refusal: true,
        },
        StopReasonWire::PauseTurn => return Err(ParseResponseError::UnsupportedFeature),
    };
    Ok(converted)
}

fn validate_stop_sequence(
    stop_sequence: Option<String>,
    reason: ConvertedStopReason,
) -> Result<Option<String>, ParseResponseError> {
    match (reason.expects_stop_sequence, stop_sequence) {
        (true, Some(value)) if !value.is_empty() && value.len() <= MAX_STOP_BYTES => {
            Ok(Some(value))
        }
        (false, None) => Ok(None),
        _ => Err(ParseResponseError::InvalidValue),
    }
}

fn convert_stop_details(
    details: Option<RefusalStopDetailsWire>,
    reason: ConvertedStopReason,
) -> Result<Option<Value>, ParseResponseError> {
    let Some(RefusalStopDetailsWire::Refusal(details)) = details else {
        return Ok(None);
    };
    if !reason.is_refusal {
        return Err(ParseResponseError::InvalidValue);
    }
    if details.explanation.as_ref().is_some_and(|value| {
        value.len() > MAX_STOP_DETAILS_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(ParseResponseError::StructureLimitExceeded);
    }

    let mut value = Map::new();
    value.insert("type".to_owned(), Value::String("refusal".to_owned()));
    value.insert(
        "category".to_owned(),
        details.category.map_or(Value::Null, |category| {
            Value::String(category.as_str().to_owned())
        }),
    );
    value.insert(
        "explanation".to_owned(),
        details.explanation.map_or(Value::Null, Value::String),
    );
    Ok(Some(Value::Object(value)))
}

pub(super) fn convert_usage(
    usage: UsageWire,
) -> Result<(Usage, Map<String, Value>), ParseResponseError> {
    let input_tokens = token_count(usage.input_tokens)?;
    let output_tokens = token_count(usage.output_tokens)?;
    let cache_read = optional_token(usage.cache_read_input_tokens)?;
    let (cache_creation_5m, cache_creation_1h, has_breakdown) =
        convert_cache_creation(usage.cache_creation)?;
    let split_creation = cache_creation_5m
        .get()
        .checked_add(cache_creation_1h.get())
        .ok_or(ParseResponseError::InvalidValue)?;
    match usage.cache_creation_input_tokens {
        Some(total) if total < 0 => return Err(ParseResponseError::InvalidValue),
        Some(total) if has_breakdown && total != split_creation => {
            return Err(ParseResponseError::InvalidValue);
        }
        Some(total) if !has_breakdown && total != 0 => {
            return Err(ParseResponseError::UnsupportedFeature);
        }
        _ => {}
    }

    let reasoning = usage
        .output_tokens_details
        .map_or(Ok(TokenCount::ZERO), |details| {
            token_count(details.thinking_tokens)
        })?;
    reject_server_tool_usage(usage.server_tool_use)?;

    let details = UsageDetails::new(
        cache_read,
        cache_creation_5m,
        cache_creation_1h,
        reasoning,
        TokenCount::ZERO,
        TokenCount::ZERO,
    );
    let converted = Usage::new(
        input_tokens,
        output_tokens,
        details,
        UsageSource::Upstream,
        UsageSemantics::CacheSeparated,
    )
    .map_err(|_| ParseResponseError::InvalidValue)?;
    converted
        .checked_total_tokens()
        .map_err(|_| ParseResponseError::InvalidValue)?;

    let mut raw = Map::new();
    if let Some(inference_geo) = usage.inference_geo {
        if inference_geo.is_empty()
            || inference_geo.len() > MAX_INFERENCE_GEO_BYTES
            || inference_geo.trim() != inference_geo
            || inference_geo.chars().any(char::is_control)
        {
            return Err(ParseResponseError::InvalidValue);
        }
        raw.insert("inference_geo".to_owned(), Value::String(inference_geo));
    }
    if let Some(service_tier) = usage.service_tier {
        raw.insert(
            "service_tier".to_owned(),
            Value::String(service_tier.as_str().to_owned()),
        );
    }
    Ok((converted, raw))
}

fn convert_cache_creation(
    cache_creation: Option<CacheCreationWire>,
) -> Result<(TokenCount, TokenCount, bool), ParseResponseError> {
    let Some(cache_creation) = cache_creation else {
        return Ok((TokenCount::ZERO, TokenCount::ZERO, false));
    };
    Ok((
        token_count(cache_creation.ephemeral_5m_input_tokens)?,
        token_count(cache_creation.ephemeral_1h_input_tokens)?,
        true,
    ))
}

fn reject_server_tool_usage(usage: Option<ServerToolUsageWire>) -> Result<(), ParseResponseError> {
    let Some(usage) = usage else {
        return Ok(());
    };
    let web_fetch = optional_token(usage.web_fetch_requests)?;
    let web_search = optional_token(usage.web_search_requests)?;
    if web_fetch != TokenCount::ZERO || web_search != TokenCount::ZERO {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    Ok(())
}

fn token_count(value: i64) -> Result<TokenCount, ParseResponseError> {
    TokenCount::new(value).map_err(|_| ParseResponseError::InvalidValue)
}

fn optional_token(value: Option<i64>) -> Result<TokenCount, ParseResponseError> {
    value.map_or(Ok(TokenCount::ZERO), token_count)
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

/// Anthropic Messages 非流式响应解析错误，不保留正文、字段名或外部值。
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
    /// 响应字段类型、取值或关联关系无效。
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
