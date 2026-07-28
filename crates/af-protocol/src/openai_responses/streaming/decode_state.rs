use std::collections::HashSet;

use serde_json::Value;

use super::{
    ParseStreamError,
    state::{MAX_OUTPUT_ITEMS, StreamBudget, StreamStateError, validate_item_id},
};
use crate::openai_responses::{
    convert::validate_tool_name,
    input::validate_call_id,
    response_wire::{
        AssistantRoleWire, FunctionCallWire, ItemStatusWire, OutputContentWire, OutputMessageWire,
        ReasoningItemWire, ResponseOutputItemWire, SummaryTextWire,
    },
};
use crate::{CanonicalStreamEvent, ContentBlock, ContentDelta};

/// 逐项校验 Responses 输出生命周期，并累计最终 Canonical 内容。
#[derive(Default)]
pub(super) struct OutputDecodeState {
    phase: OutputPhase,
    active: Option<ActiveItem>,
    next_output_index: u32,
    next_content_index: u32,
    next_tool_index: u32,
    budget: StreamBudget,
    content: Vec<ContentBlock>,
    identities: Vec<ItemIdentity>,
    seen_item_ids: HashSet<String>,
    seen_call_ids: HashSet<String>,
}

impl OutputDecodeState {
    /// 开始一个官方输出 Item，并发出对应的 Canonical 起始事件。
    pub(super) fn item_added(
        &mut self,
        output_index: u32,
        item: ResponseOutputItemWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.active.is_some()
            || output_index != self.next_output_index
            || self.identities.len() >= MAX_OUTPUT_ITEMS
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.next_output_index = self
            .next_output_index
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;

        match item {
            ResponseOutputItemWire::Reasoning(item) => {
                self.begin_reasoning(item, output_index, output)
            }
            ResponseOutputItemWire::Message(item) => self.begin_message(item, output_index),
            ResponseOutputItemWire::FunctionCall(item) => {
                self.begin_function(item, output_index, output)
            }
        }
    }

    /// 完成当前输出 Item，并发出必要的 Canonical 结束事件。
    pub(super) fn item_done(
        &mut self,
        output_index: u32,
        item: ResponseOutputItemWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        match item {
            ResponseOutputItemWire::Reasoning(item) => {
                self.finish_reasoning(output_index, item, output)
            }
            ResponseOutputItemWire::Message(item) => self.finish_message(output_index, item),
            ResponseOutputItemWire::FunctionCall(item) => {
                self.finish_function(output_index, item, output)
            }
        }
    }

    /// 开始消息中的普通输出文本 Part。
    pub(super) fn content_part_added(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        part: OutputContentWire,
    ) -> Result<(), ParseStreamError> {
        let OutputContentWire::Text(part) = part else {
            return Err(ParseStreamError::UnsupportedFeature);
        };
        let Some(ActiveItem::Message(message)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if message.output_index != output_index
            || message.id != item_id
            || message.active_part.is_some()
            || content_index != message.next_part_index
            || !part.text.is_empty()
            || has_nonempty_values(part.annotations.as_ref())
            || has_nonempty_values(part.logprobs.as_ref())
        {
            return Err(ParseStreamError::InvalidSequence);
        }

        self.budget.begin_content_block().map_err(map_state_error)?;
        let canonical_index = self.take_content_index()?;
        let message = match self.active.as_mut() {
            Some(ActiveItem::Message(message)) => message,
            _ => unreachable!("前置检查已确认当前 Item 为消息"),
        };
        message.next_part_index = message
            .next_part_index
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        message.active_part = Some(TextPartState {
            content_index,
            canonical_index,
            text: String::new(),
            text_done: false,
            emitted: false,
        });
        Ok(())
    }

    /// 累计普通输出文本增量。
    pub(super) fn output_text_delta(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        delta: String,
        logprobs: &[Value],
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Message(message)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let Some(part) = message.active_part.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if message.output_index != output_index
            || message.id != item_id
            || part.content_index != content_index
            || part.text_done
            || !logprobs.is_empty()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.budget.add_text(&delta).map_err(map_state_error)?;
        let message = match self.active.as_mut() {
            Some(ActiveItem::Message(message)) => message,
            _ => unreachable!("前置检查已确认当前 Item 为消息"),
        };
        let part = message
            .active_part
            .as_mut()
            .expect("前置检查已确认文本 Part 存在");
        part.text.push_str(&delta);
        if !delta.is_empty() {
            part.emitted = true;
            output.push(CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: part.canonical_index,
                delta: ContentDelta::Text(delta),
            });
        }
        Ok(())
    }

    /// 校验普通输出文本的冗余完成快照。
    pub(super) fn output_text_done(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        text: &str,
        logprobs: &[Value],
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Message(message)) = self.active.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let Some(part) = message.active_part.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if message.output_index != output_index
            || message.id != item_id
            || part.content_index != content_index
            || part.text_done
            || part.text != text
            || !logprobs.is_empty()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        part.text_done = true;
        Ok(())
    }

    /// 完成普通输出文本 Part，并保存其最终文本。
    pub(super) fn content_part_done(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        part: OutputContentWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let OutputContentWire::Text(done) = part else {
            return Err(ParseStreamError::UnsupportedFeature);
        };
        let Some(ActiveItem::Message(message)) = self.active.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let Some(active) = message.active_part.take() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if message.output_index != output_index
            || message.id != item_id
            || active.content_index != content_index
            || !active.text_done
            || active.text != done.text
            || has_nonempty_values(done.annotations.as_ref())
            || has_nonempty_values(done.logprobs.as_ref())
        {
            message.active_part = Some(active);
            return Err(ParseStreamError::InvalidSequence);
        }
        if !active.emitted {
            output.push(CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: active.canonical_index,
                delta: ContentDelta::Text(String::new()),
            });
        }
        message.parts.push(active.text);
        Ok(())
    }

    /// 开始单个推理摘要 Part。
    pub(super) fn reasoning_part_added(
        &mut self,
        item_id: &str,
        output_index: u32,
        summary_index: u32,
        part: SummaryTextWire,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Reasoning(reasoning)) = self.active.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if reasoning.output_index != output_index
            || reasoning.id != item_id
            || summary_index != 0
            || reasoning.summary.is_some()
            || !part.text.is_empty()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        reasoning.summary = Some(SummaryState::default());
        Ok(())
    }

    /// 累计推理摘要文本增量。
    pub(super) fn reasoning_text_delta(
        &mut self,
        item_id: &str,
        output_index: u32,
        summary_index: u32,
        delta: String,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Reasoning(reasoning)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let Some(summary) = reasoning.summary.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if reasoning.output_index != output_index
            || reasoning.id != item_id
            || summary_index != 0
            || summary.text_done
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.budget.add_text(&delta).map_err(map_state_error)?;
        let reasoning = match self.active.as_mut() {
            Some(ActiveItem::Reasoning(reasoning)) => reasoning,
            _ => unreachable!("前置检查已确认当前 Item 为推理"),
        };
        let summary = reasoning
            .summary
            .as_mut()
            .expect("前置检查已确认摘要 Part 存在");
        summary.text.push_str(&delta);
        if !delta.is_empty() {
            reasoning.emitted = true;
            output.push(CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: reasoning.canonical_index,
                text: delta,
                signature: None,
            });
        }
        Ok(())
    }

    /// 校验推理摘要文本的冗余完成快照。
    pub(super) fn reasoning_text_done(
        &mut self,
        item_id: &str,
        output_index: u32,
        summary_index: u32,
        text: &str,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Reasoning(reasoning)) = self.active.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let Some(summary) = reasoning.summary.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if reasoning.output_index != output_index
            || reasoning.id != item_id
            || summary_index != 0
            || summary.text_done
            || summary.text != text
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        summary.text_done = true;
        Ok(())
    }

    /// 完成推理摘要 Part。
    pub(super) fn reasoning_part_done(
        &mut self,
        item_id: &str,
        output_index: u32,
        summary_index: u32,
        part: SummaryTextWire,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Reasoning(reasoning)) = self.active.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let Some(summary) = reasoning.summary.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if reasoning.output_index != output_index
            || reasoning.id != item_id
            || summary_index != 0
            || !summary.text_done
            || summary.part_done
            || summary.text != part.text
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        summary.part_done = true;
        Ok(())
    }

    /// 累计函数调用参数增量。
    pub(super) fn function_arguments_delta(
        &mut self,
        item_id: &str,
        output_index: u32,
        delta: String,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Function(function)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if function.output_index != output_index
            || function.id != item_id
            || function.arguments_done
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        self.budget
            .add_arguments(function.arguments.len(), &delta)
            .map_err(map_state_error)?;
        let function = match self.active.as_mut() {
            Some(ActiveItem::Function(function)) => function,
            _ => unreachable!("前置检查已确认当前 Item 为函数调用"),
        };
        function.arguments.push_str(&delta);
        if !delta.is_empty() {
            output.push(CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: function.tool_index,
                partial_json: delta,
            });
        }
        Ok(())
    }

    /// 校验函数参数的冗余完成快照。
    pub(super) fn function_arguments_done(
        &mut self,
        item_id: &str,
        output_index: u32,
        arguments: &str,
        name: &str,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Function(function)) = self.active.as_mut() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if function.output_index != output_index
            || function.id != item_id
            || function.arguments_done
            || function.arguments != arguments
            || function.name != name
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        function.arguments_done = true;
        Ok(())
    }

    /// 返回是否仍有未结束的输出 Item。
    pub(super) const fn has_active_item(&self) -> bool {
        self.active.is_some()
    }

    /// 校验终态 Response 的输出 Item 标识与流内生命周期一致。
    pub(super) fn validate_terminal_item_ids(
        &self,
        response: &Value,
    ) -> Result<(), ParseStreamError> {
        let items = response
            .as_object()
            .and_then(|root| root.get("output"))
            .and_then(Value::as_array)
            .ok_or(ParseStreamError::InvalidValue)?;
        if items.len() != self.identities.len() {
            return Err(ParseStreamError::InvalidSequence);
        }
        for (item, expected) in items.iter().zip(&self.identities) {
            let item = item.as_object().ok_or(ParseStreamError::InvalidValue)?;
            if item.get("type").and_then(Value::as_str) != Some(expected.kind.as_str())
                || item.get("id").and_then(Value::as_str) != Some(expected.id.as_str())
            {
                return Err(ParseStreamError::InvalidSequence);
            }
        }
        Ok(())
    }

    /// 返回最终累计的 Canonical 内容块。
    pub(super) fn content(&self) -> &[ContentBlock] {
        &self.content
    }

    fn begin_reasoning(
        &mut self,
        item: ReasoningItemWire,
        output_index: u32,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        if self.phase != OutputPhase::Start
            || item.status != Some(ItemStatusWire::InProgress)
            || !item.summary.is_empty()
            || item
                .content
                .as_ref()
                .is_some_and(|content| !content.is_empty())
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        validate_item_id(&item.id).map_err(map_state_error)?;
        self.add_item_id(&item.id)?;
        self.budget.begin_content_block().map_err(map_state_error)?;
        let canonical_index = self.take_content_index()?;
        if let Some(signature) = item.encrypted_content.as_deref() {
            if signature.is_empty() {
                return Err(ParseStreamError::InvalidValue);
            }
            self.budget
                .add_signature(signature, 0)
                .map_err(map_state_error)?;
        }
        self.phase = OutputPhase::AfterReasoning;
        let emitted = item.encrypted_content.is_some();
        if emitted {
            output.push(CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: canonical_index,
                text: String::new(),
                signature: item.encrypted_content.clone(),
            });
        }
        self.active = Some(ActiveItem::Reasoning(ReasoningState {
            output_index,
            id: item.id,
            canonical_index,
            summary: None,
            signature: item.encrypted_content,
            emitted,
        }));
        Ok(())
    }

    fn begin_message(
        &mut self,
        item: OutputMessageWire,
        output_index: u32,
    ) -> Result<(), ParseStreamError> {
        if !matches!(self.phase, OutputPhase::Start | OutputPhase::AfterReasoning)
            || !item.content.is_empty()
            || !matches!(item.role, AssistantRoleWire::Assistant)
            || item.status != ItemStatusWire::InProgress
            || item.phase.is_some()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        validate_item_id(&item.id).map_err(map_state_error)?;
        self.add_item_id(&item.id)?;
        self.phase = OutputPhase::AfterMessage;
        self.active = Some(ActiveItem::Message(MessageState {
            output_index,
            id: item.id,
            next_part_index: 0,
            active_part: None,
            parts: Vec::new(),
        }));
        Ok(())
    }

    fn begin_function(
        &mut self,
        item: FunctionCallWire,
        output_index: u32,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let Some(id) = item.id else {
            return Err(ParseStreamError::InvalidValue);
        };
        if item.status != Some(ItemStatusWire::InProgress)
            || !item.arguments.is_empty()
            || item.caller.is_some()
            || item.namespace.is_some()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        validate_item_id(&id).map_err(map_state_error)?;
        validate_call_id(&item.call_id).map_err(|_| ParseStreamError::InvalidValue)?;
        validate_tool_name(&item.name).map_err(|_| ParseStreamError::InvalidValue)?;
        self.add_item_id(&id)?;
        if !self.seen_call_ids.insert(item.call_id.clone()) {
            return Err(ParseStreamError::InvalidValue);
        }
        self.budget.begin_tool().map_err(map_state_error)?;
        let tool_index = self.next_tool_index;
        self.next_tool_index = self
            .next_tool_index
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        self.phase = OutputPhase::Tools;
        output.push(CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index,
            id: item.call_id.clone(),
            name: item.name.clone(),
        });
        self.active = Some(ActiveItem::Function(FunctionState {
            output_index,
            id,
            call_id: item.call_id,
            name: item.name,
            tool_index,
            arguments: String::new(),
            arguments_done: false,
        }));
        Ok(())
    }

    fn finish_reasoning(
        &mut self,
        output_index: u32,
        item: ReasoningItemWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Reasoning(active)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        let summary_text = match (&active.summary, item.summary.as_slice()) {
            (None, []) => String::new(),
            (Some(summary), [done])
                if summary.text_done && summary.part_done && summary.text == done.text =>
            {
                summary.text.clone()
            }
            _ => return Err(ParseStreamError::InvalidSequence),
        };
        if active.output_index != output_index
            || active.id != item.id
            || !is_terminal_item_status(item.status)
            || item
                .content
                .as_ref()
                .is_some_and(|content| !content.is_empty())
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        match (&active.signature, &item.encrypted_content) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (None, Some(signature)) if !signature.is_empty() => {
                self.budget
                    .add_signature(signature, 0)
                    .map_err(map_state_error)?;
                output.push(CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: active.canonical_index,
                    text: String::new(),
                    signature: Some(signature.clone()),
                });
            }
            (None, None) => {}
            _ => return Err(ParseStreamError::InvalidSequence),
        }
        if !active.emitted && item.encrypted_content.is_none() {
            output.push(CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: active.canonical_index,
                text: String::new(),
                signature: None,
            });
        }
        let active = match self.active.take() {
            Some(ActiveItem::Reasoning(active)) => active,
            _ => unreachable!("前置检查已确认当前 Item 为推理"),
        };
        self.content.push(ContentBlock::Thinking {
            text: summary_text,
            signature: item.encrypted_content,
        });
        self.identities.push(ItemIdentity {
            kind: ItemKind::Reasoning,
            id: active.id,
        });
        Ok(())
    }

    fn finish_message(
        &mut self,
        output_index: u32,
        item: OutputMessageWire,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Message(active)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if active.output_index != output_index
            || active.id != item.id
            || active.active_part.is_some()
            || !matches!(item.role, AssistantRoleWire::Assistant)
            || !is_terminal_required_status(item.status)
            || item.phase.is_some()
            || item.content.len() != active.parts.len()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        for (part, expected) in item.content.iter().zip(&active.parts) {
            let OutputContentWire::Text(part) = part else {
                return Err(ParseStreamError::UnsupportedFeature);
            };
            if part.text != *expected
                || has_nonempty_values(part.annotations.as_ref())
                || has_nonempty_values(part.logprobs.as_ref())
            {
                return Err(ParseStreamError::InvalidSequence);
            }
        }
        let active = match self.active.take() {
            Some(ActiveItem::Message(active)) => active,
            _ => unreachable!("前置检查已确认当前 Item 为消息"),
        };
        self.content
            .extend(active.parts.into_iter().map(ContentBlock::Text));
        self.identities.push(ItemIdentity {
            kind: ItemKind::Message,
            id: active.id,
        });
        Ok(())
    }

    fn finish_function(
        &mut self,
        output_index: u32,
        item: FunctionCallWire,
        output: &mut Vec<CanonicalStreamEvent>,
    ) -> Result<(), ParseStreamError> {
        let Some(ActiveItem::Function(active)) = self.active.as_ref() else {
            return Err(ParseStreamError::InvalidSequence);
        };
        if active.output_index != output_index
            || item.id.as_deref() != Some(active.id.as_str())
            || item.call_id != active.call_id
            || item.name != active.name
            || item.arguments != active.arguments
            || !active.arguments_done
            || item.status != Some(ItemStatusWire::Completed)
            || item.caller.is_some()
            || item.namespace.is_some()
        {
            return Err(ParseStreamError::InvalidSequence);
        }
        let input = self
            .budget
            .finish_arguments(&active.arguments)
            .map_err(map_state_error)?;
        let active = match self.active.take() {
            Some(ActiveItem::Function(active)) => active,
            _ => unreachable!("前置检查已确认当前 Item 为函数调用"),
        };
        output.push(CanonicalStreamEvent::ToolCallEnd {
            choice_index: 0,
            tool_index: active.tool_index,
        });
        self.content.push(ContentBlock::ToolUse {
            id: active.call_id,
            name: active.name,
            input,
            signature: None,
        });
        self.identities.push(ItemIdentity {
            kind: ItemKind::Function,
            id: active.id,
        });
        Ok(())
    }

    fn take_content_index(&mut self) -> Result<u32, ParseStreamError> {
        let index = self.next_content_index;
        self.next_content_index = self
            .next_content_index
            .checked_add(1)
            .ok_or(ParseStreamError::StructureLimitExceeded)?;
        Ok(index)
    }

    fn add_item_id(&mut self, id: &str) -> Result<(), ParseStreamError> {
        if !self.seen_item_ids.insert(id.to_owned()) {
            return Err(ParseStreamError::InvalidValue);
        }
        Ok(())
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
}

struct ReasoningState {
    output_index: u32,
    id: String,
    canonical_index: u32,
    summary: Option<SummaryState>,
    signature: Option<String>,
    emitted: bool,
}

#[derive(Default)]
struct SummaryState {
    text: String,
    text_done: bool,
    part_done: bool,
}

struct MessageState {
    output_index: u32,
    id: String,
    next_part_index: u32,
    active_part: Option<TextPartState>,
    parts: Vec<String>,
}

struct TextPartState {
    content_index: u32,
    canonical_index: u32,
    text: String,
    text_done: bool,
    emitted: bool,
}

struct FunctionState {
    output_index: u32,
    id: String,
    call_id: String,
    name: String,
    tool_index: u32,
    arguments: String,
    arguments_done: bool,
}

struct ItemIdentity {
    kind: ItemKind,
    id: String,
}

#[derive(Clone, Copy)]
enum ItemKind {
    Reasoning,
    Message,
    Function,
}

impl ItemKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Reasoning => "reasoning",
            Self::Message => "message",
            Self::Function => "function_call",
        }
    }
}

fn is_terminal_item_status(status: Option<ItemStatusWire>) -> bool {
    matches!(
        status,
        Some(ItemStatusWire::Completed | ItemStatusWire::Incomplete)
    )
}

fn is_terminal_required_status(status: ItemStatusWire) -> bool {
    matches!(
        status,
        ItemStatusWire::Completed | ItemStatusWire::Incomplete
    )
}

fn has_nonempty_values(values: Option<&Vec<Value>>) -> bool {
    values.is_some_and(|values| !values.is_empty())
}

fn map_state_error(error: StreamStateError) -> ParseStreamError {
    match error {
        StreamStateError::InvalidValue => ParseStreamError::InvalidValue,
        StreamStateError::UnsupportedFeature => ParseStreamError::UnsupportedFeature,
        StreamStateError::StructureLimitExceeded => ParseStreamError::StructureLimitExceeded,
    }
}
