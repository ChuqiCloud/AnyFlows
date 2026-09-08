use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{
    EncodeStreamError,
    state::{StreamBudget, StreamStateError, validate_item_id},
    wire::PendingEvent,
};
use crate::openai_responses::{
    build_response::derive_item_id, convert::validate_tool_name, input::validate_call_id,
};
use crate::{ContentBlock, ContentDelta, FinishReason, ResponsesCompactionItem};

/// 将单候选 Canonical 生命周期重组为 Responses 输出 Item 事件。
#[derive(Default)]
pub(super) struct OutputEncodeState {
    phase: OutputPhase,
    active: Option<ActiveItem>,
    next_output_index: u32,
    next_content_index: u32,
    next_tool_index: u32,
    budget: StreamBudget,
    content: Vec<ContentBlock>,
    item_statuses: Vec<&'static str>,
    seen_call_ids: HashSet<String>,
    tool_count: usize,
    finish_reason: Option<FinishReason>,
}

impl OutputEncodeState {
    /// 编码一个完整的不透明压缩 Item 生命周期。
    pub(super) fn compaction(
        &mut self,
        item: ResponsesCompactionItem,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        if self.finish_reason.is_some() || self.active.is_some() || self.phase != OutputPhase::Start
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let object = item
            .as_value()
            .as_object()
            .ok_or(EncodeStreamError::InvalidSequence)?;
        if object.get("type").and_then(Value::as_str) != Some("compaction") {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or(EncodeStreamError::InvalidSequence)?;
        let encrypted = object
            .get("encrypted_content")
            .and_then(Value::as_str)
            .ok_or(EncodeStreamError::InvalidSequence)?;
        validate_item_id(id).map_err(map_state_error)?;
        if encrypted.is_empty() || encrypted.chars().any(char::is_control) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.budget.begin_content_block().map_err(map_state_error)?;
        let output_index = self.take_output_index()?;
        events.push(PendingEvent::new(
            "response.output_item.added",
            [
                ("output_index", number(output_index)),
                ("item", item.as_value().clone()),
            ],
        ));
        self.active = Some(ActiveItem::Compaction(CompactionState {
            output_index,
            id: id.to_owned(),
            item,
        }));
        Ok(())
    }

    /// 编码压缩 Item 的完成事件并纳入终态快照。
    pub(super) fn compaction_end(
        &mut self,
        item: ResponsesCompactionItem,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        let Some(ActiveItem::Compaction(active)) = self.active.take() else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        if active.item != item
            || active.id
                != item
                    .as_value()
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
        {
            self.active = Some(ActiveItem::Compaction(active));
            return Err(EncodeStreamError::InvalidSequence);
        }
        events.push(PendingEvent::new(
            "response.output_item.done",
            [
                ("output_index", number(active.output_index)),
                ("item", item.as_value().clone()),
            ],
        ));
        self.content.push(ContentBlock::Compaction(item));
        self.item_statuses.push("completed");
        Ok(())
    }

    /// 编码普通文本增量，按需打开消息 Item 与文本 Part。
    pub(super) fn content_delta(
        &mut self,
        response_id: &str,
        content_index: u32,
        delta: ContentDelta,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        let ContentDelta::Text(text) = delta else {
            return Err(EncodeStreamError::UnsupportedEvent);
        };
        if self.finish_reason.is_some() || self.phase == OutputPhase::Tools {
            return Err(EncodeStreamError::InvalidSequence);
        }
        if matches!(self.active, Some(ActiveItem::Reasoning(_))) {
            self.close_active("completed", None, events)?;
        }
        if self.active.is_none() {
            self.open_message(response_id, events)?;
        }
        if !matches!(self.active, Some(ActiveItem::Message(_))) {
            return Err(EncodeStreamError::InvalidSequence);
        }

        let needs_new_part = match self.active.as_ref() {
            Some(ActiveItem::Message(message)) => message
                .active_part
                .as_ref()
                .is_none_or(|part| part.canonical_index != content_index),
            _ => unreachable!("前置检查已确认当前 Item 为消息"),
        };
        if needs_new_part {
            self.close_text_part(events)?;
            self.open_text_part(content_index, events)?;
        }
        self.budget.add_text(&text).map_err(map_state_error)?;
        let message = match self.active.as_mut() {
            Some(ActiveItem::Message(message)) => message,
            _ => unreachable!("前置检查已确认当前 Item 为消息"),
        };
        let part = message
            .active_part
            .as_mut()
            .expect("文本 Part 必须已经打开");
        part.text.push_str(&text);
        if !text.is_empty() {
            events.push(PendingEvent::new(
                "response.output_text.delta",
                [
                    ("item_id", Value::String(message.id.clone())),
                    ("output_index", number(message.output_index)),
                    ("content_index", number(part.part_index)),
                    ("delta", Value::String(text)),
                    ("logprobs", Value::Array(Vec::new())),
                ],
            ));
        }
        Ok(())
    }

    /// 编码推理摘要及可选不透明连续上下文。
    pub(super) fn reasoning_delta(
        &mut self,
        response_id: &str,
        content_index: u32,
        text: String,
        signature: Option<String>,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        if self.finish_reason.is_some() {
            return Err(EncodeStreamError::InvalidSequence);
        }
        if matches!(self.active, Some(ActiveItem::Message(_)))
            && self.phase == OutputPhase::AfterMessage
            && text.is_empty()
            && let Some(signature) = signature.as_deref()
        {
            self.close_active("completed", None, events)?;
            return self.replace_closed_reasoning_signature(content_index, signature);
        }
        if !matches!(self.phase, OutputPhase::Start | OutputPhase::AfterReasoning) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        if self.active.is_none() {
            if content_index != self.next_content_index {
                return Err(EncodeStreamError::InvalidSequence);
            }
            self.budget.begin_content_block().map_err(map_state_error)?;
            self.next_content_index = self
                .next_content_index
                .checked_add(1)
                .ok_or(EncodeStreamError::StructureLimitExceeded)?;
            let output_index = self.take_output_index()?;
            let id = derive_item_id("rs", response_id, output_index as usize);
            validate_item_id(&id).map_err(map_state_error)?;
            events.push(PendingEvent::new(
                "response.output_item.added",
                [
                    ("output_index", number(output_index)),
                    ("item", reasoning_item(&id, "in_progress", "", None, false)),
                ],
            ));
            self.phase = OutputPhase::AfterReasoning;
            self.active = Some(ActiveItem::Reasoning(ReasoningState {
                output_index,
                id,
                canonical_index: content_index,
                text: String::new(),
                signature: String::new(),
                summary_started: false,
            }));
        }
        let Some(ActiveItem::Reasoning(reasoning)) = self.active.as_ref() else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        if reasoning.canonical_index != content_index {
            return Err(EncodeStreamError::InvalidSequence);
        }

        if !text.is_empty() && !reasoning.summary_started {
            events.push(PendingEvent::new(
                "response.reasoning_summary_part.added",
                [
                    ("item_id", Value::String(reasoning.id.clone())),
                    ("output_index", number(reasoning.output_index)),
                    ("summary_index", number(0)),
                    ("part", summary_part("")),
                ],
            ));
        }
        self.budget.add_text(&text).map_err(map_state_error)?;
        if let Some(signature) = signature.as_deref() {
            if signature.is_empty() {
                return Err(EncodeStreamError::InvalidSequence);
            }
            self.budget
                .add_signature(signature, 0)
                .map_err(map_state_error)?;
        }
        let reasoning = match self.active.as_mut() {
            Some(ActiveItem::Reasoning(reasoning)) => reasoning,
            _ => unreachable!("前置检查已确认当前 Item 为推理"),
        };
        if !text.is_empty() {
            reasoning.summary_started = true;
            events.push(PendingEvent::new(
                "response.reasoning_summary_text.delta",
                [
                    ("item_id", Value::String(reasoning.id.clone())),
                    ("output_index", number(reasoning.output_index)),
                    ("summary_index", number(0)),
                    ("delta", Value::String(text.clone())),
                ],
            ));
            reasoning.text.push_str(&text);
        }
        if let Some(signature) = signature {
            reasoning.signature = signature;
        }
        Ok(())
    }

    fn replace_closed_reasoning_signature(
        &mut self,
        content_index: u32,
        signature: &str,
    ) -> Result<(), EncodeStreamError> {
        if signature.is_empty() {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let content = self
            .content
            .get_mut(content_index as usize)
            .ok_or(EncodeStreamError::InvalidSequence)?;
        let ContentBlock::Thinking {
            signature: current, ..
        } = content
        else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        self.budget
            .add_signature(signature, 0)
            .map_err(map_state_error)?;
        *current = Some(signature.to_owned());
        Ok(())
    }

    /// 开始一个函数调用输出 Item。
    pub(super) fn tool_start(
        &mut self,
        response_id: &str,
        tool_index: u32,
        call_id: String,
        name: String,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        if self.finish_reason.is_some() || tool_index != self.next_tool_index {
            return Err(EncodeStreamError::InvalidSequence);
        }
        if matches!(self.active, Some(ActiveItem::Function(_))) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.close_active("completed", None, events)?;
        validate_call_id(&call_id).map_err(|_| EncodeStreamError::InvalidSequence)?;
        validate_tool_name(&name).map_err(|_| EncodeStreamError::InvalidSequence)?;
        if !self.seen_call_ids.insert(call_id.clone()) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.budget.begin_tool().map_err(map_state_error)?;
        self.next_tool_index = self
            .next_tool_index
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        self.tool_count = self
            .tool_count
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        let output_index = self.take_output_index()?;
        let id = derive_item_id("fc", response_id, output_index as usize);
        validate_item_id(&id).map_err(map_state_error)?;
        events.push(PendingEvent::new(
            "response.output_item.added",
            [
                ("output_index", number(output_index)),
                (
                    "item",
                    function_item(&id, "in_progress", &call_id, &name, ""),
                ),
            ],
        ));
        self.phase = OutputPhase::Tools;
        self.active = Some(ActiveItem::Function(FunctionState {
            output_index,
            id,
            call_id,
            name,
            tool_index,
            arguments: String::new(),
        }));
        Ok(())
    }

    /// 编码函数参数 JSON 增量。
    pub(super) fn tool_arguments_delta(
        &mut self,
        tool_index: u32,
        partial_json: String,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        let Some(ActiveItem::Function(function)) = self.active.as_ref() else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        if function.tool_index != tool_index {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.budget
            .add_arguments(function.arguments.len(), &partial_json)
            .map_err(map_state_error)?;
        let function = match self.active.as_mut() {
            Some(ActiveItem::Function(function)) => function,
            _ => unreachable!("前置检查已确认当前 Item 为函数调用"),
        };
        function.arguments.push_str(&partial_json);
        if !partial_json.is_empty() {
            events.push(PendingEvent::new(
                "response.function_call_arguments.delta",
                [
                    ("item_id", Value::String(function.id.clone())),
                    ("output_index", number(function.output_index)),
                    ("delta", Value::String(partial_json)),
                ],
            ));
        }
        Ok(())
    }

    /// 完成函数调用并校验完整 JSON 参数对象。
    pub(super) fn tool_end(
        &mut self,
        tool_index: u32,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        let Some(ActiveItem::Function(function)) = self.active.as_ref() else {
            return Err(EncodeStreamError::InvalidSequence);
        };
        if function.tool_index != tool_index {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let input = self
            .budget
            .finish_arguments(&function.arguments)
            .map_err(map_state_error)?;
        let function = match self.active.take() {
            Some(ActiveItem::Function(function)) => function,
            _ => unreachable!("前置检查已确认当前 Item 为函数调用"),
        };
        events.push(PendingEvent::new(
            "response.function_call_arguments.done",
            [
                ("item_id", Value::String(function.id.clone())),
                ("output_index", number(function.output_index)),
                ("arguments", Value::String(function.arguments.clone())),
                ("name", Value::String(function.name.clone())),
            ],
        ));
        events.push(PendingEvent::new(
            "response.output_item.done",
            [
                ("output_index", number(function.output_index)),
                (
                    "item",
                    function_item(
                        &function.id,
                        "completed",
                        &function.call_id,
                        &function.name,
                        &function.arguments,
                    ),
                ),
            ],
        ));
        self.content.push(ContentBlock::ToolUse {
            id: function.call_id,
            name: function.name,
            input,
            signature: None,
        });
        self.item_statuses.push("completed");
        Ok(())
    }

    /// 结束唯一候选，并闭合仍打开的文本或推理 Item。
    pub(super) fn finish(
        &mut self,
        reason: FinishReason,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        if self.finish_reason.is_some() || matches!(self.active, Some(ActiveItem::Function(_))) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let has_tools = self.tool_count != 0;
        if (reason == FinishReason::ToolCalls) != has_tools
            || (matches!(reason, FinishReason::Length | FinishReason::ContentFilter) && has_tools)
        {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let item_status = if matches!(reason, FinishReason::Length | FinishReason::ContentFilter) {
            "incomplete"
        } else {
            "completed"
        };
        let message_phase = (!has_tools).then_some("final_answer");
        self.close_active(item_status, message_phase, events)?;
        self.finish_reason = Some(reason);
        Ok(())
    }

    /// 返回已闭合的最终内容块。
    pub(super) fn content(&self) -> &[ContentBlock] {
        &self.content
    }

    /// 返回候选结束原因。
    pub(super) const fn finish_reason(&self) -> Option<FinishReason> {
        self.finish_reason
    }

    /// 返回每个输出 Item 在终态快照中的实际状态。
    pub(super) fn item_statuses(&self) -> &[&'static str] {
        &self.item_statuses
    }

    fn open_message(
        &mut self,
        response_id: &str,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        if !matches!(self.phase, OutputPhase::Start | OutputPhase::AfterReasoning) {
            return Err(EncodeStreamError::InvalidSequence);
        }
        let output_index = self.take_output_index()?;
        let id = derive_item_id("msg", response_id, output_index as usize);
        validate_item_id(&id).map_err(map_state_error)?;
        events.push(PendingEvent::new(
            "response.output_item.added",
            [
                ("output_index", number(output_index)),
                ("item", message_item(&id, "in_progress", &[], None)),
            ],
        ));
        self.phase = OutputPhase::AfterMessage;
        self.active = Some(ActiveItem::Message(MessageState {
            output_index,
            id,
            next_part_index: 0,
            active_part: None,
            parts: Vec::new(),
        }));
        Ok(())
    }

    fn open_text_part(
        &mut self,
        canonical_index: u32,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        if canonical_index != self.next_content_index {
            return Err(EncodeStreamError::InvalidSequence);
        }
        self.budget.begin_content_block().map_err(map_state_error)?;
        self.next_content_index = self
            .next_content_index
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        let message = match self.active.as_mut() {
            Some(ActiveItem::Message(message)) => message,
            _ => return Err(EncodeStreamError::InvalidSequence),
        };
        let part_index = message.next_part_index;
        message.next_part_index = message
            .next_part_index
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        events.push(PendingEvent::new(
            "response.content_part.added",
            [
                ("item_id", Value::String(message.id.clone())),
                ("output_index", number(message.output_index)),
                ("content_index", number(part_index)),
                ("part", output_text_part("")),
            ],
        ));
        message.active_part = Some(TextPartState {
            canonical_index,
            part_index,
            text: String::new(),
        });
        Ok(())
    }

    fn close_text_part(&mut self, events: &mut Vec<PendingEvent>) -> Result<(), EncodeStreamError> {
        let Some(ActiveItem::Message(message)) = self.active.as_mut() else {
            return Ok(());
        };
        let Some(part) = message.active_part.take() else {
            return Ok(());
        };
        events.push(PendingEvent::new(
            "response.output_text.done",
            [
                ("item_id", Value::String(message.id.clone())),
                ("output_index", number(message.output_index)),
                ("content_index", number(part.part_index)),
                ("text", Value::String(part.text.clone())),
                ("logprobs", Value::Array(Vec::new())),
            ],
        ));
        events.push(PendingEvent::new(
            "response.content_part.done",
            [
                ("item_id", Value::String(message.id.clone())),
                ("output_index", number(message.output_index)),
                ("content_index", number(part.part_index)),
                ("part", output_text_part(&part.text)),
            ],
        ));
        message.parts.push(part.text);
        Ok(())
    }

    fn close_active(
        &mut self,
        status: &'static str,
        message_phase: Option<&'static str>,
        events: &mut Vec<PendingEvent>,
    ) -> Result<(), EncodeStreamError> {
        match self.active.take() {
            None => Ok(()),
            Some(ActiveItem::Function(function)) => {
                self.active = Some(ActiveItem::Function(function));
                Err(EncodeStreamError::InvalidSequence)
            }
            Some(ActiveItem::Compaction(compaction)) => {
                self.active = Some(ActiveItem::Compaction(compaction));
                Err(EncodeStreamError::InvalidSequence)
            }
            Some(ActiveItem::Reasoning(reasoning)) => {
                if reasoning.summary_started {
                    events.push(PendingEvent::new(
                        "response.reasoning_summary_text.done",
                        [
                            ("item_id", Value::String(reasoning.id.clone())),
                            ("output_index", number(reasoning.output_index)),
                            ("summary_index", number(0)),
                            ("text", Value::String(reasoning.text.clone())),
                        ],
                    ));
                    events.push(PendingEvent::new(
                        "response.reasoning_summary_part.done",
                        [
                            ("item_id", Value::String(reasoning.id.clone())),
                            ("output_index", number(reasoning.output_index)),
                            ("summary_index", number(0)),
                            ("part", summary_part(&reasoning.text)),
                        ],
                    ));
                }
                events.push(PendingEvent::new(
                    "response.output_item.done",
                    [
                        ("output_index", number(reasoning.output_index)),
                        (
                            "item",
                            reasoning_item(
                                &reasoning.id,
                                status,
                                &reasoning.text,
                                (!reasoning.signature.is_empty())
                                    .then_some(reasoning.signature.as_str()),
                                reasoning.summary_started,
                            ),
                        ),
                    ],
                ));
                self.content.push(ContentBlock::Thinking {
                    text: reasoning.text,
                    signature: (!reasoning.signature.is_empty()).then_some(reasoning.signature),
                });
                self.item_statuses.push(status);
                Ok(())
            }
            Some(ActiveItem::Message(mut message)) => {
                self.active = Some(ActiveItem::Message(message));
                self.close_text_part(events)?;
                message = match self.active.take() {
                    Some(ActiveItem::Message(message)) => message,
                    _ => unreachable!("消息关闭期间 Item 类型不会变化"),
                };
                events.push(PendingEvent::new(
                    "response.output_item.done",
                    [
                        ("output_index", number(message.output_index)),
                        (
                            "item",
                            message_item(&message.id, status, &message.parts, message_phase),
                        ),
                    ],
                ));
                self.content
                    .extend(message.parts.into_iter().map(ContentBlock::Text));
                self.item_statuses.push(status);
                Ok(())
            }
        }
    }

    fn take_output_index(&mut self) -> Result<u32, EncodeStreamError> {
        let index = self.next_output_index;
        self.next_output_index = self
            .next_output_index
            .checked_add(1)
            .ok_or(EncodeStreamError::StructureLimitExceeded)?;
        Ok(index)
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum OutputPhase {
    #[default]
    Start,
    AfterReasoning,
    AfterMessage,
    Tools,
}

enum ActiveItem {
    Reasoning(ReasoningState),
    Message(MessageState),
    Function(FunctionState),
    Compaction(CompactionState),
}

struct CompactionState {
    output_index: u32,
    id: String,
    item: ResponsesCompactionItem,
}

struct ReasoningState {
    output_index: u32,
    id: String,
    canonical_index: u32,
    text: String,
    signature: String,
    summary_started: bool,
}

struct MessageState {
    output_index: u32,
    id: String,
    next_part_index: u32,
    active_part: Option<TextPartState>,
    parts: Vec<String>,
}

struct TextPartState {
    canonical_index: u32,
    part_index: u32,
    text: String,
}

struct FunctionState {
    output_index: u32,
    id: String,
    call_id: String,
    name: String,
    tool_index: u32,
    arguments: String,
}

fn reasoning_item(
    id: &str,
    status: &str,
    text: &str,
    signature: Option<&str>,
    has_summary: bool,
) -> Value {
    let summary = if has_summary {
        vec![summary_part(text)]
    } else {
        Vec::new()
    };
    let mut item = Map::from_iter([
        ("id".to_owned(), Value::String(id.to_owned())),
        ("type".to_owned(), Value::String("reasoning".to_owned())),
        ("status".to_owned(), Value::String(status.to_owned())),
        ("summary".to_owned(), Value::Array(summary)),
    ]);
    if let Some(signature) = signature {
        item.insert(
            "encrypted_content".to_owned(),
            Value::String(signature.to_owned()),
        );
    }
    Value::Object(item)
}

fn message_item(id: &str, status: &str, parts: &[String], phase: Option<&str>) -> Value {
    let mut item = Map::from_iter([
        ("id".to_owned(), Value::String(id.to_owned())),
        ("type".to_owned(), Value::String("message".to_owned())),
        ("status".to_owned(), Value::String(status.to_owned())),
        ("role".to_owned(), Value::String("assistant".to_owned())),
        (
            "content".to_owned(),
            Value::Array(parts.iter().map(|text| output_text_part(text)).collect()),
        ),
    ]);
    if let Some(phase) = phase {
        item.insert("phase".to_owned(), Value::String(phase.to_owned()));
    }
    Value::Object(item)
}

fn function_item(id: &str, status: &str, call_id: &str, name: &str, arguments: &str) -> Value {
    object_value([
        ("id", Value::String(id.to_owned())),
        ("type", Value::String("function_call".to_owned())),
        ("status", Value::String(status.to_owned())),
        ("call_id", Value::String(call_id.to_owned())),
        ("name", Value::String(name.to_owned())),
        ("arguments", Value::String(arguments.to_owned())),
    ])
}

fn output_text_part(text: &str) -> Value {
    object_value([
        ("type", Value::String("output_text".to_owned())),
        ("text", Value::String(text.to_owned())),
        ("annotations", Value::Array(Vec::new())),
        ("logprobs", Value::Array(Vec::new())),
    ])
}

fn summary_part(text: &str) -> Value {
    object_value([
        ("type", Value::String("summary_text".to_owned())),
        ("text", Value::String(text.to_owned())),
    ])
}

fn object_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(Map::from_iter(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    ))
}

fn number(value: u32) -> Value {
    Value::Number(value.into())
}

fn map_state_error(error: StreamStateError) -> EncodeStreamError {
    match error {
        StreamStateError::InvalidValue => EncodeStreamError::InvalidSequence,
        StreamStateError::UnsupportedFeature => EncodeStreamError::UnsupportedEvent,
        StreamStateError::StructureLimitExceeded => EncodeStreamError::StructureLimitExceeded,
    }
}
