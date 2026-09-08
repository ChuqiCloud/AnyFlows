use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
        MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES, validate_call_id,
        validate_model, validate_tool_name, validate_value_shape,
    },
    parse_response::{
        MAX_RESPONSE_CHOICES, MAX_RESPONSE_FINGERPRINT_BYTES, ParseResponseError, parse_response,
        validate_response_id,
    },
};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, ResponseChoice, UnsupportedCapability, Usage,
    validate_response_capabilities,
};

/// 将 Canonical 响应构造为 OpenAI Chat Completions 非流式 JSON。
///
/// Chat wire 必须携带真实创建时间；来源协议未提供时间时明确失败，不使用零值或
/// 当前时间补齐。构造完成后会再次经过入站解析器，确保双向边界保持一致。
pub fn build_response(response: &CanonicalResponse) -> Result<Value, BuildResponseError> {
    if response.operation != Operation::Chat {
        return Err(BuildResponseError::UnsupportedOperation);
    }
    validate_response_capabilities(Protocol::OpenAiChat, response)
        .map_err(BuildResponseError::UnsupportedCapability)?;
    validate_response_id(&response.id).map_err(map_response_validation_error)?;
    validate_model(&response.model).map_err(map_request_validation_error)?;
    let created = response
        .created_at
        .ok_or(BuildResponseError::InvalidValue)?;
    if created < 0 {
        return Err(BuildResponseError::InvalidValue);
    }
    if response.choices.is_empty() || response.choices.len() > MAX_RESPONSE_CHOICES {
        return Err(BuildResponseError::StructureLimitExceeded);
    }

    let mut state = ResponseBuildState::default();
    let mut seen_indexes = HashSet::with_capacity(response.choices.len());
    let mut choices = Vec::with_capacity(response.choices.len());
    for choice in &response.choices {
        if !seen_indexes.insert(choice.index) {
            return Err(BuildResponseError::InvalidValue);
        }
        choices.push(build_choice(choice, &mut state)?);
    }

    let mut root = Map::from_iter([
        ("id".to_owned(), Value::String(response.id.clone())),
        (
            "object".to_owned(),
            Value::String("chat.completion".to_owned()),
        ),
        ("created".to_owned(), Value::Number(created.into())),
        ("model".to_owned(), Value::String(response.model.clone())),
        ("choices".to_owned(), Value::Array(choices)),
    ]);
    if let Some(usage) = response.usage.as_ref() {
        root.insert("usage".to_owned(), build_usage(usage)?);
    }
    merge_raw(&mut root, response)?;

    let value = Value::Object(root);
    revalidate_response(&value)?;
    Ok(value)
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

fn build_choice(
    choice: &ResponseChoice,
    state: &mut ResponseBuildState,
) -> Result<Value, BuildResponseError> {
    if choice.message.role != Role::Assistant {
        return Err(BuildResponseError::InvalidValue);
    }
    if choice.stop_sequence.is_some() {
        return Err(BuildResponseError::UnsupportedFeature);
    }

    let (content, tool_calls) = build_message_content(choice, state)?;
    let has_tool_calls = !tool_calls.is_empty();
    if has_tool_calls != (choice.finish_reason == FinishReason::ToolCalls) {
        return Err(BuildResponseError::InvalidValue);
    }

    let mut message = Map::from_iter([
        ("role".to_owned(), Value::String("assistant".to_owned())),
        ("content".to_owned(), content),
    ]);
    if has_tool_calls {
        message.insert("tool_calls".to_owned(), Value::Array(tool_calls));
    }

    Ok(object_value([
        ("index", Value::Number(u64::from(choice.index).into())),
        ("message", Value::Object(message)),
        (
            "finish_reason",
            Value::String(finish_reason_name(choice.finish_reason).to_owned()),
        ),
    ]))
}

fn build_message_content(
    choice: &ResponseChoice,
    state: &mut ResponseBuildState,
) -> Result<(Value, Vec<Value>), BuildResponseError> {
    let mut text = None;
    let mut tool_calls = Vec::new();
    let mut seen_call_ids = HashSet::new();
    for block in &choice.message.content {
        match block {
            ContentBlock::Text(value) => {
                if text.is_some() || !tool_calls.is_empty() {
                    return Err(BuildResponseError::UnsupportedFeature);
                }
                state.add_text(value)?;
                text = Some(Value::String(value.clone()));
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
                tool_calls.push(build_tool_call(id, name, input, state)?);
            }
            ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildResponseError::UnsupportedFeature);
            }
        }
    }
    Ok((text.unwrap_or(Value::Null), tool_calls))
}

fn build_tool_call(
    id: &str,
    name: &str,
    input: &Value,
    state: &mut ResponseBuildState,
) -> Result<Value, BuildResponseError> {
    validate_call_id(id).map_err(map_request_validation_error)?;
    validate_tool_name(name).map_err(map_request_validation_error)?;
    if !input.is_object() {
        return Err(BuildResponseError::UnsupportedFeature);
    }
    let arguments = serde_json::to_string(input).map_err(|_| BuildResponseError::InvalidValue)?;
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    let nodes =
        validate_value_shape(input, 16, 4_096, 1_024).map_err(map_request_validation_error)?;
    state.add_arguments(arguments.len(), nodes)?;

    Ok(object_value([
        ("id", Value::String(id.to_owned())),
        ("type", Value::String("function".to_owned())),
        (
            "function",
            object_value([
                ("name", Value::String(name.to_owned())),
                ("arguments", Value::String(arguments)),
            ]),
        ),
    ]))
}

fn finish_reason_name(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::ContentFilter => "content_filter",
    }
}

fn build_usage(usage: &Usage) -> Result<Value, BuildResponseError> {
    let details = usage.details();
    if details.cache_creation_5m().get() != 0 || details.cache_creation_1h().get() != 0 {
        return Err(BuildResponseError::UnsupportedFeature);
    }
    let prompt_tokens = usage
        .checked_input_tokens()
        .map_err(|_| BuildResponseError::InvalidValue)?
        .get();
    let completion_tokens = usage.output_tokens().get();
    let total_tokens = usage
        .checked_total_tokens()
        .map_err(|_| BuildResponseError::InvalidValue)?
        .get();

    let mut value = Map::from_iter([
        (
            "prompt_tokens".to_owned(),
            Value::Number(prompt_tokens.into()),
        ),
        (
            "completion_tokens".to_owned(),
            Value::Number(completion_tokens.into()),
        ),
        (
            "total_tokens".to_owned(),
            Value::Number(total_tokens.into()),
        ),
    ]);
    let cache_read = details.cache_read().get();
    let audio_input = details.audio_input().get();
    if cache_read != 0 || audio_input != 0 {
        value.insert(
            "prompt_tokens_details".to_owned(),
            object_value([
                ("cached_tokens", Value::Number(cache_read.into())),
                ("audio_tokens", Value::Number(audio_input.into())),
            ]),
        );
    }
    let reasoning = details.reasoning().get();
    let audio_output = details.audio_output().get();
    if reasoning != 0 || audio_output != 0 {
        value.insert(
            "completion_tokens_details".to_owned(),
            object_value([
                ("reasoning_tokens", Value::Number(reasoning.into())),
                ("audio_tokens", Value::Number(audio_output.into())),
            ]),
        );
    }
    Ok(Value::Object(value))
}

fn merge_raw(
    root: &mut Map<String, Value>,
    response: &CanonicalResponse,
) -> Result<(), BuildResponseError> {
    let Some(raw) = response.raw_passthrough() else {
        return Ok(());
    };
    let fields = raw
        .fields_for_protocol(Protocol::OpenAiChat)
        .map_err(|_| BuildResponseError::RawProtocolMismatch)?;
    for (key, value) in fields {
        match key.as_str() {
            "service_tier"
                if matches!(
                    value.as_str(),
                    Some("auto" | "default" | "flex" | "scale" | "priority")
                ) => {}
            "system_fingerprint" => {
                let fingerprint = value.as_str().ok_or(BuildResponseError::InvalidValue)?;
                if fingerprint.is_empty()
                    || fingerprint.len() > MAX_RESPONSE_FINGERPRINT_BYTES
                    || fingerprint.chars().any(char::is_control)
                {
                    return Err(BuildResponseError::InvalidValue);
                }
            }
            "service_tier" => return Err(BuildResponseError::InvalidValue),
            _ => return Err(BuildResponseError::UnsupportedFeature),
        }
        if root.insert(key.clone(), value.clone()).is_some() {
            return Err(BuildResponseError::InvalidValue);
        }
    }
    Ok(())
}

fn revalidate_response(value: &Value) -> Result<(), BuildResponseError> {
    let bytes = serde_json::to_vec(value).map_err(|_| BuildResponseError::InvalidValue)?;
    parse_response(&bytes)
        .map(|_| ())
        .map_err(map_response_validation_error)
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
        | ParseRequestError::InvalidValue
        | ParseRequestError::ConflictingParameters => BuildResponseError::InvalidValue,
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

/// OpenAI Chat 非流式响应构造错误，不保留 Canonical 敏感值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResponseError {
    /// 当前构造器仅支持 Chat 操作。
    UnsupportedOperation,
    /// Canonical 字段或关联关系无效。
    InvalidValue,
    /// 目标协议缺少一项已声明的响应能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 使用了 Chat Completions 无法表达的能力。
    UnsupportedFeature,
    /// 响应结构或序列化正文超过预算。
    StructureLimitExceeded,
    /// raw 字段来源不是 OpenAI Chat，禁止跨协议透传。
    RawProtocolMismatch,
}

impl fmt::Display for BuildResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation => formatter.write_str("响应操作类型不受支持"),
            Self::InvalidValue => formatter.write_str("响应字段值无效"),
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedFeature => {
                formatter.write_str("响应包含 Chat Completions 无法表达的特性")
            }
            Self::StructureLimitExceeded => formatter.write_str("响应结构超过限制"),
            Self::RawProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
        }
    }
}

impl Error for BuildResponseError {}
