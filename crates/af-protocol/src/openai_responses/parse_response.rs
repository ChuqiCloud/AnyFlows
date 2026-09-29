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
    response_raw::{ResponseRawError, validate_raw_fields},
    response_wire::{
        AssistantRoleWire, CompactionItemWire, FunctionCallWire, IncompleteReasonWire,
        InputTokensDetailsWire, ItemStatusWire, MessagePhaseWire, OutputContentWire,
        OutputMessageWire, ReasoningItemWire, ResponseObjectWire, ResponseOutputItemWire,
        ResponseStatusWire, ResponseUsageWire, ResponsesResponseWire,
    },
};
use crate::bounded_json::{BoundedJsonError, JsonLimits, parse_value};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, Message, ResponseChoice, TokenCount, Usage,
    UsageDetails, UsageSemantics, UsageSource,
};

const MAX_RESPONSE_ID_BYTES: usize = 220;
const MAX_OUTPUT_ITEM_ID_BYTES: usize = 256;
const MAX_OUTPUT_ITEMS: usize = 1_024;
const MAX_REASONING_SIGNATURE_BYTES: usize = 1024 * 1024;
const ARGUMENT_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

/// 解析 OpenAI Responses 非流式响应。
///
/// 解析过程拒绝重复 JSON 键、非终态响应和当前 Canonical 无法无损保存的
/// 输出 Item；官方请求回显字段只在完成独立白名单校验后进入同源 raw。
pub fn parse_response(body: &[u8]) -> Result<CanonicalResponse, ParseResponseError> {
    if body.len() > super::MAX_BODY_BYTES {
        return Err(ParseResponseError::BodyTooLarge);
    }
    let value = parse_value(body, super::REQUEST_JSON_LIMITS).map_err(map_json_error)?;
    if uses_known_unsupported_output_item(&value) {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let wire: ResponsesResponseWire =
        serde_json::from_value(value).map_err(|_| ParseResponseError::InvalidValue)?;
    convert_response(wire)
}

fn uses_known_unsupported_output_item(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|response| response.get("output"))
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                matches!(
                    item.as_object()
                        .and_then(|item| item.get("type"))
                        .and_then(Value::as_str),
                    Some(kind)
                        if !matches!(kind, "message" | "function_call" | "reasoning" | "compaction")
                )
            })
        })
}

fn convert_response(wire: ResponsesResponseWire) -> Result<CanonicalResponse, ParseResponseError> {
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
        _access_programs,
        extra,
    } = wire;
    if !matches!(object, ResponseObjectWire::Response) {
        return Err(ParseResponseError::InvalidValue);
    }
    if !matches!(
        status,
        ResponseStatusWire::Completed | ResponseStatusWire::Incomplete
    ) || error.is_some()
        || output_text.is_some()
        || moderation.is_some()
    {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    validate_response_id(&id)?;
    validate_model(&model).map_err(map_request_validation_error)?;
    if created_at < 0 {
        return Err(ParseResponseError::InvalidValue);
    }
    if output.len() > MAX_OUTPUT_ITEMS {
        return Err(ParseResponseError::StructureLimitExceeded);
    }

    let incomplete_reason = validate_root_status(status, incomplete_details)?;
    let mut state = ResponseConvertState::default();
    let mut content = Vec::new();
    for item in output {
        convert_output_item(item, status, &mut state, &mut content)?;
    }
    if state.tool_calls != 0 && state.message_phase == Some(MessagePhaseWire::FinalAnswer) {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    if status == ResponseStatusWire::Incomplete && state.tool_calls != 0 {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let finish_reason = match incomplete_reason {
        Some(IncompleteReasonWire::MaxOutputTokens) => FinishReason::Length,
        Some(IncompleteReasonWire::ContentFilter) => FinishReason::ContentFilter,
        None if state.tool_calls == 0 => FinishReason::Stop,
        None => FinishReason::ToolCalls,
    };
    let choice = ResponseChoice::new(0, Message::new(Role::Assistant, content), finish_reason);
    let usage = convert_usage(usage)?;
    let raw = validate_raw(extra, status, created_at)?;

    Ok(CanonicalResponse::new(
        Operation::Responses,
        id,
        model,
        Some(created_at),
        vec![choice],
        usage,
    )
    .with_validated_raw_passthrough(raw.map(|fields| (Protocol::OpenAiResponses, fields))))
}

fn validate_root_status(
    status: ResponseStatusWire,
    incomplete_details: Option<super::response_wire::IncompleteDetailsWire>,
) -> Result<Option<IncompleteReasonWire>, ParseResponseError> {
    match (status, incomplete_details) {
        (ResponseStatusWire::Completed, None) => Ok(None),
        (ResponseStatusWire::Incomplete, Some(details)) => details
            .reason
            .map(Some)
            .ok_or(ParseResponseError::InvalidValue),
        (ResponseStatusWire::Completed | ResponseStatusWire::Incomplete, _) => {
            Err(ParseResponseError::InvalidValue)
        }
        _ => Err(ParseResponseError::UnsupportedFeature),
    }
}

pub(super) fn validate_response_id(id: &str) -> Result<(), ParseResponseError> {
    validate_opaque_id(id, MAX_RESPONSE_ID_BYTES)
}

fn validate_output_item_id(id: &str) -> Result<(), ParseResponseError> {
    validate_opaque_id(id, MAX_OUTPUT_ITEM_ID_BYTES)
}

fn validate_opaque_id(id: &str, max_bytes: usize) -> Result<(), ParseResponseError> {
    if id.is_empty() || id.len() > max_bytes || id.trim() != id || id.chars().any(char::is_control)
    {
        return Err(ParseResponseError::InvalidValue);
    }
    Ok(())
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum OutputPhase {
    #[default]
    Start,
    AfterReasoning,
    AfterMessage,
    Tools,
}

#[derive(Default)]
struct ResponseConvertState {
    phase: OutputPhase,
    message_phase: Option<MessagePhaseWire>,
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
    seen_item_ids: HashSet<String>,
    seen_call_ids: HashSet<String>,
}

impl ResponseConvertState {
    fn begin_reasoning(&mut self) -> Result<(), ParseResponseError> {
        if self.phase != OutputPhase::Start {
            return Err(ParseResponseError::UnsupportedFeature);
        }
        self.phase = OutputPhase::AfterReasoning;
        Ok(())
    }

    fn begin_compaction(&mut self) -> Result<(), ParseResponseError> {
        if self.phase != OutputPhase::Start {
            return Err(ParseResponseError::UnsupportedFeature);
        }
        Ok(())
    }

    fn begin_message(&mut self) -> Result<(), ParseResponseError> {
        if !matches!(self.phase, OutputPhase::Start | OutputPhase::AfterReasoning) {
            return Err(ParseResponseError::UnsupportedFeature);
        }
        self.phase = OutputPhase::AfterMessage;
        Ok(())
    }

    fn begin_tool(&mut self) {
        self.phase = OutputPhase::Tools;
    }

    fn add_item_id(&mut self, id: String) -> Result<(), ParseResponseError> {
        validate_output_item_id(&id)?;
        if !self.seen_item_ids.insert(id) {
            return Err(ParseResponseError::InvalidValue);
        }
        Ok(())
    }

    fn add_text(&mut self, text: &str) -> Result<(), ParseResponseError> {
        self.add_text_bytes(text)?;
        self.add_block()
    }

    fn add_thinking(
        &mut self,
        text: &str,
        signature: Option<&str>,
    ) -> Result<(), ParseResponseError> {
        self.add_text_bytes(text)?;
        if let Some(signature) = signature {
            if signature.is_empty()
                || signature.len() > MAX_REASONING_SIGNATURE_BYTES
                || signature.chars().any(char::is_control)
            {
                return Err(ParseResponseError::InvalidValue);
            }
            self.add_text_bytes(signature)?;
        }
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

    fn add_arguments(
        &mut self,
        call_id: String,
        bytes: usize,
        nodes: usize,
    ) -> Result<(), ParseResponseError> {
        if !self.seen_call_ids.insert(call_id) {
            return Err(ParseResponseError::InvalidValue);
        }
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

fn convert_output_item(
    item: ResponseOutputItemWire,
    root_status: ResponseStatusWire,
    state: &mut ResponseConvertState,
    content: &mut Vec<ContentBlock>,
) -> Result<(), ParseResponseError> {
    match item {
        ResponseOutputItemWire::Message(message) => {
            state.begin_message()?;
            convert_message(message, root_status, state, content)
        }
        ResponseOutputItemWire::FunctionCall(call) => {
            state.begin_tool();
            convert_function_call(call, root_status, state, content)
        }
        ResponseOutputItemWire::Reasoning(reasoning) => {
            state.begin_reasoning()?;
            convert_reasoning(reasoning, root_status, state, content)
        }
        ResponseOutputItemWire::Compaction(compaction) => {
            state.begin_compaction()?;
            convert_compaction(compaction, root_status, state, content)
        }
    }
}

fn convert_compaction(
    item: CompactionItemWire,
    root_status: ResponseStatusWire,
    state: &mut ResponseConvertState,
    output: &mut Vec<ContentBlock>,
) -> Result<(), ParseResponseError> {
    let CompactionItemWire {
        id,
        encrypted_content,
    } = item;
    if let Some(id) = id.as_deref() {
        state.add_item_id(id.to_owned())?;
    }
    validate_item_status(Some(ItemStatusWire::Completed), root_status)?;
    if encrypted_content.is_empty()
        || encrypted_content.len() > super::convert::MAX_ENCRYPTED_REASONING_BYTES
        || encrypted_content.chars().any(char::is_control)
    {
        return Err(ParseResponseError::InvalidValue);
    }
    let mut object = Map::from_iter([
        ("type".to_owned(), Value::String("compaction".to_owned())),
        (
            "encrypted_content".to_owned(),
            Value::String(encrypted_content),
        ),
    ]);
    if let Some(id) = id {
        object.insert("id".to_owned(), Value::String(id));
    }
    state.add_block()?;
    output.push(ContentBlock::Compaction(
        crate::ResponsesCompactionItem::from_validated_value(Value::Object(object)),
    ));
    Ok(())
}

fn convert_message(
    message: OutputMessageWire,
    root_status: ResponseStatusWire,
    state: &mut ResponseConvertState,
    output: &mut Vec<ContentBlock>,
) -> Result<(), ParseResponseError> {
    state.add_item_id(message.id)?;
    validate_item_status(Some(message.status), root_status)?;
    if !matches!(message.role, AssistantRoleWire::Assistant) {
        return Err(ParseResponseError::InvalidValue);
    }
    match message.phase {
        None | Some(MessagePhaseWire::FinalAnswer) => {}
        Some(MessagePhaseWire::Commentary) => {
            return Err(ParseResponseError::UnsupportedFeature);
        }
    }
    state.message_phase = message.phase;
    for content in message.content {
        match content {
            OutputContentWire::Text(text) => {
                if text
                    .annotations
                    .as_ref()
                    .is_some_and(|annotations| !annotations.is_empty())
                    || text
                        .logprobs
                        .as_ref()
                        .is_some_and(|logprobs| !logprobs.is_empty())
                {
                    return Err(ParseResponseError::UnsupportedFeature);
                }
                state.add_text(&text.text)?;
                output.push(ContentBlock::Text(text.text));
            }
            OutputContentWire::Refusal(refusal) => {
                let _ = refusal.refusal;
                return Err(ParseResponseError::UnsupportedFeature);
            }
        }
    }
    Ok(())
}

fn convert_function_call(
    call: FunctionCallWire,
    root_status: ResponseStatusWire,
    state: &mut ResponseConvertState,
    output: &mut Vec<ContentBlock>,
) -> Result<(), ParseResponseError> {
    if let Some(id) = call.id {
        state.add_item_id(id)?;
    }
    validate_item_status(call.status, root_status)?;
    if call.caller.is_some() || call.namespace.is_some() {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    validate_call_id(&call.call_id).map_err(map_request_validation_error)?;
    validate_tool_name(&call.name).map_err(map_request_validation_error)?;
    if call.arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(ParseResponseError::StructureLimitExceeded);
    }
    let input =
        parse_value(call.arguments.as_bytes(), ARGUMENT_JSON_LIMITS).map_err(map_argument_error)?;
    if !input.is_object() {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let nodes =
        validate_value_shape(&input, 16, 4_096, 1_024).map_err(map_request_validation_error)?;
    state.add_arguments(call.call_id.clone(), call.arguments.len(), nodes)?;
    output.push(ContentBlock::ToolUse {
        id: call.call_id,
        name: call.name,
        input,
        signature: None,
    });
    Ok(())
}

fn convert_reasoning(
    reasoning: ReasoningItemWire,
    root_status: ResponseStatusWire,
    state: &mut ResponseConvertState,
    output: &mut Vec<ContentBlock>,
) -> Result<(), ParseResponseError> {
    state.add_item_id(reasoning.id)?;
    validate_item_status(reasoning.status, root_status)?;
    if reasoning
        .content
        .as_ref()
        .is_some_and(|content| !content.is_empty())
        || reasoning.summary.len() > 1
    {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let text = reasoning
        .summary
        .into_iter()
        .next()
        .map_or_else(String::new, |summary| summary.text);
    state.add_thinking(&text, reasoning.encrypted_content.as_deref())?;
    output.push(ContentBlock::Thinking {
        text,
        signature: reasoning.encrypted_content,
    });
    Ok(())
}

fn validate_item_status(
    status: Option<ItemStatusWire>,
    root_status: ResponseStatusWire,
) -> Result<(), ParseResponseError> {
    match (root_status, status) {
        (ResponseStatusWire::Completed, None | Some(ItemStatusWire::Completed))
        | (
            ResponseStatusWire::Incomplete,
            None | Some(ItemStatusWire::Completed | ItemStatusWire::Incomplete),
        ) => Ok(()),
        (ResponseStatusWire::Completed | ResponseStatusWire::Incomplete, _) => {
            Err(ParseResponseError::InvalidValue)
        }
        _ => Err(ParseResponseError::UnsupportedFeature),
    }
}

fn convert_usage(usage: Option<ResponseUsageWire>) -> Result<Option<Usage>, ParseResponseError> {
    let Some(usage) = usage else {
        return Ok(None);
    };
    let input = token_count(usage.input_tokens)?;
    let output = token_count(usage.output_tokens)?;
    let expected_total = input
        .get()
        .checked_add(output.get())
        .ok_or(ParseResponseError::InvalidValue)?;
    if usage.total_tokens != expected_total {
        return Err(ParseResponseError::InvalidValue);
    }

    let input_details =
        merge_input_details(usage.input_tokens_details, usage.prompt_tokens_details)?;
    let cache_read = optional_token(input_details.cached_tokens)?;
    let cache_write = optional_token(input_details.cache_write_tokens)?;
    if cache_write != TokenCount::ZERO {
        return Err(ParseResponseError::UnsupportedFeature);
    }
    let output_details = usage.output_tokens_details.unwrap_or_default();
    let reasoning = optional_token(output_details.reasoning_tokens)?;
    Usage::new(
        input,
        output,
        UsageDetails::new(
            cache_read,
            TokenCount::ZERO,
            TokenCount::ZERO,
            reasoning,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map(Some)
    .map_err(|_| ParseResponseError::InvalidValue)
}

/// 合并 Responses 正式字段与第三方 Chat 风格别名，禁止冲突事实进入计费。
fn merge_input_details(
    input: Option<InputTokensDetailsWire>,
    prompt: Option<InputTokensDetailsWire>,
) -> Result<InputTokensDetailsWire, ParseResponseError> {
    let input = input.unwrap_or_default();
    let prompt = prompt.unwrap_or_default();
    Ok(InputTokensDetailsWire {
        cache_write_tokens: merge_optional_detail(
            input.cache_write_tokens,
            prompt.cache_write_tokens,
        )?,
        cached_tokens: merge_optional_detail(input.cached_tokens, prompt.cached_tokens)?,
    })
}

fn merge_optional_detail(
    primary: Option<i64>,
    alias: Option<i64>,
) -> Result<Option<i64>, ParseResponseError> {
    match (primary, alias) {
        (Some(primary), Some(alias)) if primary != alias => Err(ParseResponseError::InvalidValue),
        (Some(value), _) | (_, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

fn validate_raw(
    extra: Map<String, Value>,
    status: ResponseStatusWire,
    created_at: i64,
) -> Result<Option<Map<String, Value>>, ParseResponseError> {
    if extra.is_empty() {
        return Ok(None);
    }
    validate_raw_fields(&extra, status, created_at).map_err(map_raw_error)?;
    Ok(Some(extra))
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
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::ConflictingParameters
        | ParseRequestError::InvalidValue => ParseResponseError::InvalidValue,
    }
}

fn map_raw_error(error: ResponseRawError) -> ParseResponseError {
    match error {
        ResponseRawError::InvalidValue => ParseResponseError::InvalidValue,
        ResponseRawError::UnsupportedFeature => ParseResponseError::UnsupportedFeature,
        ResponseRawError::StructureLimitExceeded => ParseResponseError::StructureLimitExceeded,
    }
}

/// OpenAI Responses 非流式响应解析错误，不保留响应正文或字段值。
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
    /// 响应字段的类型、取值或关联关系无效。
    InvalidValue,
    /// 响应使用了当前 Canonical 无法无损表达的特性。
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
