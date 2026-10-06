use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value};

use super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
        MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES, validate_model,
        validate_tool_name, validate_value_shape,
    },
    input::validate_call_id,
    parse_response::{ParseResponseError, validate_response_id},
    response_raw::{ResponseRawError, validate_raw_fields},
    response_wire::ResponseStatusWire,
};
use crate::bounded_json::validate_object;
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, ResponseChoice, UnsupportedCapability, Usage,
    validate_response_capabilities,
};

const MAX_REASONING_SIGNATURE_BYTES: usize = 1024 * 1024;

/// 将 Canonical 响应构造为 OpenAI Responses 非流式 JSON。
///
/// 构造器只接受单个索引为零的 Assistant 候选，并为 Responses 必需的输出
/// Item 标识按响应 ID 确定性派生；函数关联始终保留真实 `call_id`。
pub fn build_response(response: &CanonicalResponse) -> Result<Value, BuildResponseError> {
    if response.operation != Operation::Responses {
        return Err(BuildResponseError::UnsupportedOperation);
    }
    validate_response_capabilities(Protocol::OpenAiResponses, response)
        .map_err(BuildResponseError::UnsupportedCapability)?;
    validate_response_id(&response.id).map_err(map_response_validation_error)?;
    validate_model(&response.model).map_err(map_request_validation_error)?;
    let created_at = response
        .created_at
        .ok_or(BuildResponseError::InvalidValue)?;
    if created_at < 0 {
        return Err(BuildResponseError::InvalidValue);
    }
    let [choice] = response.choices.as_slice() else {
        return Err(BuildResponseError::UnsupportedFeature);
    };
    validate_choice(choice)?;

    let (status, incomplete_details) = build_status(choice.finish_reason);
    let output = build_output(&response.id, choice, status)?;
    let usage = build_usage(response.usage.as_ref())?;
    let mut root = Map::from_iter([
        ("id".to_owned(), Value::String(response.id.clone())),
        ("object".to_owned(), Value::String("response".to_owned())),
        ("created_at".to_owned(), Value::Number(created_at.into())),
        (
            "status".to_owned(),
            Value::String(status.as_str().to_owned()),
        ),
        ("error".to_owned(), Value::Null),
        ("incomplete_details".to_owned(), incomplete_details),
        ("model".to_owned(), Value::String(response.model.clone())),
        ("output".to_owned(), Value::Array(output)),
        ("usage".to_owned(), usage),
    ]);
    merge_raw(&mut root, response, status, created_at)?;
    validate_object(&root, super::REQUEST_JSON_LIMITS, super::MAX_BODY_BYTES)
        .map_err(|_| BuildResponseError::StructureLimitExceeded)?;
    Ok(Value::Object(root))
}

fn validate_choice(choice: &ResponseChoice) -> Result<(), BuildResponseError> {
    if choice.index != 0 || choice.message.role != Role::Assistant || choice.stop_sequence.is_some()
    {
        return Err(BuildResponseError::InvalidValue);
    }
    if choice.message.content.len() > MAX_CONTENT_BLOCKS {
        return Err(BuildResponseError::StructureLimitExceeded);
    }
    let has_tools = choice
        .message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { .. }));
    if has_tools != (choice.finish_reason == FinishReason::ToolCalls) {
        return Err(BuildResponseError::InvalidValue);
    }
    Ok(())
}

fn build_status(reason: FinishReason) -> (ResponseStatusWire, Value) {
    match reason {
        FinishReason::Stop | FinishReason::ToolCalls => {
            (ResponseStatusWire::Completed, Value::Null)
        }
        FinishReason::Length => (
            ResponseStatusWire::Incomplete,
            object_value([("reason", Value::String("max_output_tokens".to_owned()))]),
        ),
        FinishReason::ContentFilter => (
            ResponseStatusWire::Incomplete,
            object_value([("reason", Value::String("content_filter".to_owned()))]),
        ),
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum OutputPhase {
    #[default]
    Start,
    Text,
    Tools,
}

#[derive(Default)]
struct ResponseBuildState {
    phase: OutputPhase,
    has_reasoning: bool,
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    seen_call_ids: HashSet<String>,
}

impl ResponseBuildState {
    fn add_thinking(
        &mut self,
        text: &str,
        signature: Option<&str>,
    ) -> Result<(), BuildResponseError> {
        if self.phase != OutputPhase::Start || self.has_reasoning {
            return Err(BuildResponseError::UnsupportedFeature);
        }
        self.has_reasoning = true;
        self.add_text_bytes(text)?;
        if let Some(signature) = signature {
            if signature.is_empty()
                || signature.len() > MAX_REASONING_SIGNATURE_BYTES
                || signature.chars().any(char::is_control)
            {
                return Err(BuildResponseError::InvalidValue);
            }
            self.add_text_bytes(signature)?;
        }
        self.add_block()
    }

    fn add_text(&mut self, text: &str) -> Result<(), BuildResponseError> {
        if self.phase == OutputPhase::Tools {
            return Err(BuildResponseError::UnsupportedFeature);
        }
        self.phase = OutputPhase::Text;
        self.add_text_bytes(text)?;
        self.add_block()
    }

    fn add_tool(
        &mut self,
        call_id: &str,
        bytes: usize,
        nodes: usize,
    ) -> Result<(), BuildResponseError> {
        self.phase = OutputPhase::Tools;
        if !self.seen_call_ids.insert(call_id.to_owned()) {
            return Err(BuildResponseError::InvalidValue);
        }
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

fn build_output(
    response_id: &str,
    choice: &ResponseChoice,
    status: ResponseStatusWire,
) -> Result<Vec<Value>, BuildResponseError> {
    let mut state = ResponseBuildState::default();
    let mut reasoning = None;
    let mut compactions = Vec::new();
    let mut text_parts = Vec::new();
    let mut tools = Vec::new();
    let mut saw_non_compaction = false;
    for block in &choice.message.content {
        match block {
            ContentBlock::Compaction(item) if !saw_non_compaction => {
                compactions.push(item.as_value().clone());
            }
            ContentBlock::Thinking { text, signature } => {
                saw_non_compaction = true;
                state.add_thinking(text, signature.as_deref())?;
                reasoning = Some((text, signature.as_deref()));
            }
            ContentBlock::Text(text) => {
                saw_non_compaction = true;
                state.add_text(text)?;
                text_parts.push(object_value([
                    ("type", Value::String("output_text".to_owned())),
                    ("text", Value::String(text.clone())),
                    ("annotations", Value::Array(Vec::new())),
                    ("logprobs", Value::Array(Vec::new())),
                ]));
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => {
                saw_non_compaction = true;
                if signature.is_some() {
                    return Err(BuildResponseError::UnsupportedFeature);
                }
                validate_call_id(id).map_err(map_request_validation_error)?;
                validate_tool_name(name).map_err(map_request_validation_error)?;
                if !input.is_object() {
                    return Err(BuildResponseError::UnsupportedFeature);
                }
                let arguments =
                    serde_json::to_string(input).map_err(|_| BuildResponseError::InvalidValue)?;
                if arguments.len() > MAX_ARGUMENT_BYTES {
                    return Err(BuildResponseError::StructureLimitExceeded);
                }
                let nodes = validate_value_shape(input, 16, 4_096, 1_024)
                    .map_err(map_request_validation_error)?;
                state.add_tool(id, arguments.len(), nodes)?;
                tools.push((id, name, arguments));
            }
            ContentBlock::Image { .. }
            | ContentBlock::Audio { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::CacheControl(_)
            | ContentBlock::Compaction(_) => {
                return Err(BuildResponseError::UnsupportedFeature);
            }
        }
    }

    let item_status = status.as_str();
    let mut output = compactions;
    if let Some((text, signature)) = reasoning {
        let summary = if text.is_empty() {
            Vec::new()
        } else {
            vec![object_value([
                ("type", Value::String("summary_text".to_owned())),
                ("text", Value::String(text.clone())),
            ])]
        };
        let mut item = Map::from_iter([
            (
                "id".to_owned(),
                Value::String(derive_item_id("rs", response_id, output.len())),
            ),
            ("type".to_owned(), Value::String("reasoning".to_owned())),
            ("status".to_owned(), Value::String(item_status.to_owned())),
            ("summary".to_owned(), Value::Array(summary)),
        ]);
        if let Some(signature) = signature {
            item.insert(
                "encrypted_content".to_owned(),
                Value::String(signature.to_owned()),
            );
        }
        output.push(Value::Object(item));
    }
    if !text_parts.is_empty() {
        let mut message = Map::from_iter([
            (
                "id".to_owned(),
                Value::String(derive_item_id("msg", response_id, output.len())),
            ),
            ("type".to_owned(), Value::String("message".to_owned())),
            ("status".to_owned(), Value::String(item_status.to_owned())),
            ("role".to_owned(), Value::String("assistant".to_owned())),
            ("content".to_owned(), Value::Array(text_parts)),
        ]);
        // Canonical 的非工具终态消息等价于 Responses 最终回答；工具前说明仍不猜测阶段。
        if choice.finish_reason != FinishReason::ToolCalls {
            message.insert("phase".to_owned(), Value::String("final_answer".to_owned()));
        }
        output.push(Value::Object(message));
    }
    for (call_id, name, arguments) in tools {
        output.push(object_value([
            (
                "id",
                Value::String(derive_item_id("fc", response_id, output.len())),
            ),
            ("type", Value::String("function_call".to_owned())),
            ("status", Value::String(item_status.to_owned())),
            ("call_id", Value::String(call_id.clone())),
            ("name", Value::String(name.clone())),
            ("arguments", Value::String(arguments)),
        ]));
    }
    Ok(output)
}

pub(super) fn derive_item_id(prefix: &str, response_id: &str, index: usize) -> String {
    let suffix = response_id.strip_prefix("resp_").unwrap_or(response_id);
    format!("{prefix}_{suffix}_{index}")
}

fn build_usage(usage: Option<&Usage>) -> Result<Value, BuildResponseError> {
    let Some(usage) = usage else {
        return Ok(Value::Null);
    };
    let details = usage.details();
    if details.cache_creation_5m().get() != 0
        || details.cache_creation_1h().get() != 0
        || details.audio_input().get() != 0
        || details.audio_output().get() != 0
    {
        return Err(BuildResponseError::UnsupportedFeature);
    }
    let input = usage
        .checked_input_tokens()
        .map_err(|_| BuildResponseError::InvalidValue)?
        .get();
    let total = usage
        .checked_total_tokens()
        .map_err(|_| BuildResponseError::InvalidValue)?
        .get();
    Ok(object_value([
        ("input_tokens", Value::Number(input.into())),
        (
            "input_tokens_details",
            object_value([
                ("cache_write_tokens", Value::Number(0.into())),
                (
                    "cached_tokens",
                    Value::Number(details.cache_read().get().into()),
                ),
            ]),
        ),
        (
            "output_tokens",
            Value::Number(usage.output_tokens().get().into()),
        ),
        (
            "output_tokens_details",
            object_value([(
                "reasoning_tokens",
                Value::Number(details.reasoning().get().into()),
            )]),
        ),
        ("total_tokens", Value::Number(total.into())),
    ]))
}

fn merge_raw(
    root: &mut Map<String, Value>,
    response: &CanonicalResponse,
    status: ResponseStatusWire,
    created_at: i64,
) -> Result<(), BuildResponseError> {
    let Some(raw) = response.raw_passthrough() else {
        return Ok(());
    };
    let fields = raw
        .fields_for_protocol(Protocol::OpenAiResponses)
        .map_err(|_| BuildResponseError::RawProtocolMismatch)?;
    if fields.keys().any(|key| root.contains_key(key)) {
        return Err(BuildResponseError::FieldConflict);
    }
    validate_raw_fields(fields, status, created_at).map_err(map_raw_error)?;
    for (key, value) in fields {
        root.insert(key.clone(), value.clone());
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
        | ParseRequestError::ConflictingParameters
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

fn map_raw_error(error: ResponseRawError) -> BuildResponseError {
    match error {
        ResponseRawError::InvalidValue => BuildResponseError::InvalidValue,
        ResponseRawError::UnsupportedFeature => BuildResponseError::UnsupportedFeature,
        ResponseRawError::StructureLimitExceeded => BuildResponseError::StructureLimitExceeded,
    }
}

/// OpenAI Responses 非流式响应构造错误，不携带 Canonical 敏感值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResponseError {
    /// Canonical 操作不是 Responses。
    UnsupportedOperation,
    /// Canonical 字段的取值或关联关系无效。
    InvalidValue,
    /// 目标协议缺少一项已声明的响应能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 使用了 Responses 无法无损表达的能力。
    UnsupportedFeature,
    /// Canonical 响应超过结构或序列化预算。
    StructureLimitExceeded,
    /// raw 字段来源协议不是 Responses。
    RawProtocolMismatch,
    /// raw 字段与已编码字段发生碰撞。
    FieldConflict,
}

impl fmt::Display for BuildResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation => formatter.write_str("响应操作不是 Responses"),
            Self::InvalidValue => formatter.write_str("Canonical 响应字段值无效"),
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedFeature => formatter.write_str("响应包含 Responses 不支持的特性"),
            Self::StructureLimitExceeded => formatter.write_str("Canonical 响应结构超过限制"),
            Self::RawProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
            Self::FieldConflict => formatter.write_str("响应字段相互冲突"),
        }
    }
}

impl Error for BuildResponseError {}
