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
    parse_response::{
        MAX_INFERENCE_GEO_BYTES, MAX_STOP_DETAILS_BYTES, ParseResponseError, validate_response_id,
    },
    response_wire::RefusalStopDetailsWire,
};
use crate::bounded_json::validate_object;
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, ResponseChoice, UnsupportedCapability, Usage,
    UsageSemantics, validate_response_capabilities,
};

/// 将 Canonical 响应构造为 Anthropic Messages 非流式 JSON。
///
/// Anthropic 不提供响应创建时间，因此 `created_at` 只校验合法性而不写入 wire；
/// 其余 Canonical 字段会在出站信任边界重新校验，无法表达的能力明确失败。
pub fn build_response(response: &CanonicalResponse) -> Result<Value, BuildResponseError> {
    if response.operation != Operation::Chat {
        return Err(BuildResponseError::UnsupportedOperation);
    }
    validate_response_capabilities(Protocol::Anthropic, response)
        .map_err(BuildResponseError::UnsupportedCapability)?;
    validate_response_id(&response.id).map_err(map_response_validation_error)?;
    validate_model(&response.model).map_err(map_request_validation_error)?;
    if response.created_at.is_some_and(|created_at| created_at < 0) {
        return Err(BuildResponseError::InvalidValue);
    }
    let [choice] = response.choices.as_slice() else {
        return Err(BuildResponseError::UnsupportedFeature);
    };
    validate_choice(choice)?;

    let content = build_content(choice)?;
    let (stop_reason, stop_sequence) = build_stop(choice)?;
    let mut usage = build_usage(
        response
            .usage
            .as_ref()
            .ok_or(BuildResponseError::InvalidValue)?,
    )?;
    let raw = validate_raw(response, choice)?;
    for (key, value) in raw.usage {
        usage.insert(key, value);
    }

    let mut root = Map::new();
    root.insert("id".to_owned(), Value::String(response.id.clone()));
    root.insert("type".to_owned(), Value::String("message".to_owned()));
    root.insert("role".to_owned(), Value::String("assistant".to_owned()));
    root.insert("content".to_owned(), Value::Array(content));
    root.insert("model".to_owned(), Value::String(response.model.clone()));
    root.insert("container".to_owned(), Value::Null);
    root.insert(
        "stop_reason".to_owned(),
        Value::String(stop_reason.to_owned()),
    );
    root.insert(
        "stop_sequence".to_owned(),
        stop_sequence.map_or(Value::Null, Value::String),
    );
    root.insert(
        "stop_details".to_owned(),
        raw.stop_details.unwrap_or(Value::Null),
    );
    root.insert("usage".to_owned(), Value::Object(usage));
    validate_object(&root, super::REQUEST_JSON_LIMITS, super::MAX_BODY_BYTES)
        .map_err(|_| BuildResponseError::StructureLimitExceeded)?;
    Ok(Value::Object(root))
}

fn validate_choice(choice: &ResponseChoice) -> Result<(), BuildResponseError> {
    if choice.index != 0 || choice.message.role != Role::Assistant {
        return Err(BuildResponseError::InvalidValue);
    }
    if choice.message.content.len() > MAX_PARTS_PER_MESSAGE {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    Ok(())
}

#[derive(Default)]
struct ResponseBuildState {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

impl ResponseBuildState {
    fn add_text(&mut self, text: &str) -> Result<(), BuildResponseError> {
        self.add_text_bytes(text)?;
        self.add_block()
    }

    fn add_thinking(&mut self, thinking: &str, signature: &str) -> Result<(), BuildResponseError> {
        self.add_text_bytes(thinking)?;
        self.add_text_bytes(signature)?;
        self.add_block()
    }

    fn add_text_bytes(&mut self, text: &str) -> Result<(), BuildResponseError> {
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
        Ok(())
    }

    fn add_arguments(&mut self, bytes: usize, nodes: usize) -> Result<(), BuildResponseError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        self.argument_bytes = self
            .argument_bytes
            .checked_add(bytes)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(BuildResponseError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS
            || self.argument_bytes > MAX_TOTAL_ARGUMENT_BYTES
            || self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES
        {
            return Err(BuildResponseError::StructureLimitExceeded);
        }
        self.add_block()
    }

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
}

fn build_content(choice: &ResponseChoice) -> Result<Vec<Value>, BuildResponseError> {
    let mut state = ResponseBuildState::default();
    let mut seen_call_ids = HashSet::new();
    let mut output = Vec::with_capacity(choice.message.content.len());
    for block in &choice.message.content {
        output.push(match block {
            ContentBlock::Text(text) => {
                state.add_text(text)?;
                object_value([
                    ("type", Value::String("text".to_owned())),
                    ("text", Value::String(text.clone())),
                    ("citations", Value::Null),
                ])
            }
            ContentBlock::Thinking { text, signature } => {
                let signature = signature
                    .as_ref()
                    .ok_or(BuildResponseError::UnsupportedFeature)?;
                if signature.is_empty() || signature.chars().any(char::is_control) {
                    return Err(BuildResponseError::InvalidValue);
                }
                state.add_thinking(text, signature)?;
                object_value([
                    ("type", Value::String("thinking".to_owned())),
                    ("thinking", Value::String(text.clone())),
                    ("signature", Value::String(signature.clone())),
                ])
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => {
                if signature.is_some() {
                    return Err(BuildResponseError::UnsupportedFeature);
                }
                if !seen_call_ids.insert(id.as_str()) {
                    return Err(BuildResponseError::InvalidValue);
                }
                validate_call_id(id).map_err(map_request_validation_error)?;
                validate_tool_name(name).map_err(map_request_validation_error)?;
                if !input.is_object() {
                    return Err(BuildResponseError::UnsupportedFeature);
                }
                let bytes = serde_json::to_vec(input)
                    .map_err(|_| BuildResponseError::InvalidValue)?
                    .len();
                if bytes > MAX_ARGUMENT_BYTES {
                    return Err(BuildResponseError::StructureLimitExceeded);
                }
                let nodes = validate_value_shape(input, 16, 4_096, 1_024)
                    .map_err(map_request_validation_error)?;
                state.add_arguments(bytes, nodes)?;
                object_value([
                    ("type", Value::String("tool_use".to_owned())),
                    ("id", Value::String(id.clone())),
                    ("name", Value::String(name.clone())),
                    ("input", input.clone()),
                    (
                        "caller",
                        object_value([("type", Value::String("direct".to_owned()))]),
                    ),
                ])
            }
            ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildResponseError::UnsupportedFeature);
            }
        });
    }
    Ok(output)
}

fn build_stop(
    choice: &ResponseChoice,
) -> Result<(&'static str, Option<String>), BuildResponseError> {
    let has_tool_use = choice
        .message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { .. }));
    if has_tool_use != (choice.finish_reason == FinishReason::ToolCalls) {
        return Err(BuildResponseError::InvalidValue);
    }

    match (choice.finish_reason, choice.stop_sequence.as_ref()) {
        (FinishReason::Stop, Some(sequence))
            if !sequence.is_empty() && sequence.len() <= MAX_STOP_BYTES =>
        {
            Ok(("stop_sequence", Some(sequence.clone())))
        }
        (FinishReason::Stop, None) => Ok(("end_turn", None)),
        (FinishReason::Length, None) => Ok(("max_tokens", None)),
        (FinishReason::ToolCalls, None) => Ok(("tool_use", None)),
        (FinishReason::ContentFilter, None) => Ok(("refusal", None)),
        _ => Err(BuildResponseError::InvalidValue),
    }
}

pub(super) fn build_usage(usage: &Usage) -> Result<Map<String, Value>, BuildResponseError> {
    usage
        .checked_total_tokens()
        .map_err(|_| BuildResponseError::InvalidValue)?;
    let details = usage.details();
    if details.audio_input().get() != 0 || details.audio_output().get() != 0 {
        return Err(BuildResponseError::UnsupportedFeature);
    }

    let cache_read = details.cache_read().get();
    let cache_creation_5m = details.cache_creation_5m().get();
    let cache_creation_1h = details.cache_creation_1h().get();
    let cache_creation = cache_creation_5m
        .checked_add(cache_creation_1h)
        .ok_or(BuildResponseError::InvalidValue)?;
    let cache_total = cache_read
        .checked_add(cache_creation)
        .ok_or(BuildResponseError::InvalidValue)?;
    let input_tokens = match usage.semantics() {
        UsageSemantics::CacheSeparated => usage.input_tokens().get(),
        UsageSemantics::Inclusive => usage
            .input_tokens()
            .get()
            .checked_sub(cache_total)
            .ok_or(BuildResponseError::InvalidValue)?,
    };

    let cache_creation_details = if cache_creation == 0 {
        Value::Null
    } else {
        object_value([
            (
                "ephemeral_5m_input_tokens",
                Value::Number(cache_creation_5m.into()),
            ),
            (
                "ephemeral_1h_input_tokens",
                Value::Number(cache_creation_1h.into()),
            ),
        ])
    };
    let reasoning = details.reasoning().get();
    let output_details = if reasoning == 0 {
        Value::Null
    } else {
        object_value([("thinking_tokens", Value::Number(reasoning.into()))])
    };

    Ok(Map::from_iter([
        (
            "input_tokens".to_owned(),
            Value::Number(input_tokens.into()),
        ),
        (
            "cache_creation_input_tokens".to_owned(),
            Value::Number(cache_creation.into()),
        ),
        (
            "cache_read_input_tokens".to_owned(),
            Value::Number(cache_read.into()),
        ),
        ("cache_creation".to_owned(), cache_creation_details),
        (
            "output_tokens".to_owned(),
            Value::Number(usage.output_tokens().get().into()),
        ),
        ("output_tokens_details".to_owned(), output_details),
        ("inference_geo".to_owned(), Value::Null),
        ("server_tool_use".to_owned(), Value::Null),
        ("service_tier".to_owned(), Value::Null),
    ]))
}

#[derive(Default)]
struct ValidatedRawResponse {
    stop_details: Option<Value>,
    usage: Map<String, Value>,
}

fn validate_raw(
    response: &CanonicalResponse,
    choice: &ResponseChoice,
) -> Result<ValidatedRawResponse, BuildResponseError> {
    let Some(raw) = response.raw_passthrough() else {
        return Ok(ValidatedRawResponse::default());
    };
    let fields = raw
        .fields_for_protocol(Protocol::Anthropic)
        .map_err(|_| BuildResponseError::RawProtocolMismatch)?;
    let mut validated = ValidatedRawResponse::default();
    for (key, value) in fields {
        match key.as_str() {
            "stop_details" => {
                if choice.finish_reason != FinishReason::ContentFilter {
                    return Err(BuildResponseError::InvalidValue);
                }
                validate_stop_details(value)?;
                validated.stop_details = Some(value.clone());
            }
            "usage" => validate_raw_usage(value, &mut validated.usage)?,
            _ => return Err(BuildResponseError::UnsupportedFeature),
        }
    }
    Ok(validated)
}

fn validate_stop_details(value: &Value) -> Result<(), BuildResponseError> {
    let details: RefusalStopDetailsWire =
        serde_json::from_value(value.clone()).map_err(|_| BuildResponseError::InvalidValue)?;
    let RefusalStopDetailsWire::Refusal(details) = details;
    if details.explanation.as_ref().is_some_and(|value| {
        value.len() > MAX_STOP_DETAILS_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    Ok(())
}

fn validate_raw_usage(
    value: &Value,
    output: &mut Map<String, Value>,
) -> Result<(), BuildResponseError> {
    let usage = value.as_object().ok_or(BuildResponseError::InvalidValue)?;
    for (key, value) in usage {
        match key.as_str() {
            "inference_geo" => {
                let value = value.as_str().ok_or(BuildResponseError::InvalidValue)?;
                if value.is_empty()
                    || value.len() > MAX_INFERENCE_GEO_BYTES
                    || value.trim() != value
                    || value.chars().any(char::is_control)
                {
                    return Err(BuildResponseError::InvalidValue);
                }
            }
            "service_tier" => {
                if !matches!(value.as_str(), Some("standard" | "priority" | "batch")) {
                    return Err(BuildResponseError::InvalidValue);
                }
            }
            _ => return Err(BuildResponseError::UnsupportedFeature),
        }
        output.insert(key.clone(), value.clone());
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

/// Anthropic Messages 非流式响应构造错误，不保留 Canonical 敏感值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResponseError {
    /// 当前构造器仅支持 Chat 操作。
    UnsupportedOperation,
    /// Canonical 字段或关联关系无效。
    InvalidValue,
    /// 目标协议缺少一项已声明的响应能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 使用了 Anthropic 响应无法表达的能力。
    UnsupportedFeature,
    /// 响应结构或序列化正文超过预算。
    StructureLimitExceeded,
    /// raw 字段来源不是 Anthropic，禁止跨协议透传。
    RawProtocolMismatch,
}

impl fmt::Display for BuildResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation => formatter.write_str("响应操作类型不受支持"),
            Self::InvalidValue => formatter.write_str("响应字段值无效"),
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedFeature => formatter.write_str("响应包含 Anthropic 无法表达的特性"),
            Self::StructureLimitExceeded => formatter.write_str("响应结构超过限制"),
            Self::RawProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
        }
    }
}

impl Error for BuildResponseError {}
