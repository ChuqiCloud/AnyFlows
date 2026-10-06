use serde_json::Value;

use super::super::{
    ParseRequestError,
    convert::{
        MAX_ARGUMENT_BYTES, MAX_CONTENT_BLOCKS, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
        MAX_TOTAL_ARGUMENT_BYTES, MAX_TOTAL_ARGUMENT_NODES, MAX_TOTAL_TEXT_BYTES,
        validate_value_shape,
    },
};
use crate::bounded_json::{BoundedJsonError, JsonLimits, parse_value};

pub(super) const MAX_OUTPUT_ITEMS: usize = 1_024;
pub(super) const MAX_OUTPUT_ITEM_ID_BYTES: usize = 256;
pub(super) const MAX_REASONING_SIGNATURE_BYTES: usize = 1024 * 1024;

const ARGUMENT_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 16,
    max_nodes: 4_096,
    max_object_entries: 1_024,
    max_array_items: 4_096,
    max_string_bytes: MAX_ARGUMENT_BYTES,
    max_key_bytes: 1_024,
};

/// decoder 与 encoder 共用的流累计预算。
#[derive(Default)]
pub(super) struct StreamBudget {
    content_blocks: usize,
    text_bytes: usize,
    tool_calls: usize,
    argument_bytes: usize,
    argument_nodes: usize,
}

impl StreamBudget {
    /// 登记一个文本或推理内容块。
    pub(super) fn begin_content_block(&mut self) -> Result<(), StreamStateError> {
        self.add_content_block()
    }

    /// 登记一个工具调用及其对应的 Canonical 内容块。
    pub(super) fn begin_tool(&mut self) -> Result<(), StreamStateError> {
        self.tool_calls = self
            .tool_calls
            .checked_add(1)
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        if self.tool_calls > MAX_TOOL_CALLS {
            return Err(StreamStateError::StructureLimitExceeded);
        }
        self.add_content_block()
    }

    /// 累计普通文本或推理摘要增量。
    pub(super) fn add_text(&mut self, text: &str) -> Result<(), StreamStateError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(StreamStateError::StructureLimitExceeded);
        }
        self.add_text_bytes(text.len())
    }

    /// 累计不透明推理签名，并应用独立的单签名上限。
    pub(super) fn add_signature(
        &mut self,
        signature: &str,
        current_bytes: usize,
    ) -> Result<(), StreamStateError> {
        let next = current_bytes
            .checked_add(signature.len())
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        if next > MAX_REASONING_SIGNATURE_BYTES || signature.chars().any(char::is_control) {
            return Err(StreamStateError::InvalidValue);
        }
        self.add_text_bytes(signature.len())
    }

    /// 累计一个函数参数 JSON 增量。
    pub(super) fn add_arguments(
        &mut self,
        current_bytes: usize,
        delta: &str,
    ) -> Result<(), StreamStateError> {
        let next_item = current_bytes
            .checked_add(delta.len())
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        let next_total = self
            .argument_bytes
            .checked_add(delta.len())
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        if next_item > MAX_ARGUMENT_BYTES || next_total > MAX_TOTAL_ARGUMENT_BYTES {
            return Err(StreamStateError::StructureLimitExceeded);
        }
        self.argument_bytes = next_total;
        Ok(())
    }

    /// 校验完整函数参数对象并登记结构节点预算。
    pub(super) fn finish_arguments(&mut self, arguments: &str) -> Result<Value, StreamStateError> {
        let value =
            parse_value(arguments.as_bytes(), ARGUMENT_JSON_LIMITS).map_err(map_argument_error)?;
        if !value.is_object() {
            return Err(StreamStateError::UnsupportedFeature);
        }
        let nodes = validate_value_shape(&value, 16, 4_096, 1_024).map_err(map_request_error)?;
        self.argument_nodes = self
            .argument_nodes
            .checked_add(nodes)
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        if self.argument_nodes > MAX_TOTAL_ARGUMENT_NODES {
            return Err(StreamStateError::StructureLimitExceeded);
        }
        Ok(value)
    }

    fn add_content_block(&mut self) -> Result<(), StreamStateError> {
        self.content_blocks = self
            .content_blocks
            .checked_add(1)
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        if self.content_blocks > MAX_CONTENT_BLOCKS {
            return Err(StreamStateError::StructureLimitExceeded);
        }
        Ok(())
    }

    fn add_text_bytes(&mut self, bytes: usize) -> Result<(), StreamStateError> {
        self.text_bytes = self
            .text_bytes
            .checked_add(bytes)
            .ok_or(StreamStateError::StructureLimitExceeded)?;
        if self.text_bytes > MAX_TOTAL_TEXT_BYTES {
            return Err(StreamStateError::StructureLimitExceeded);
        }
        Ok(())
    }
}

/// 校验 Responses 输出 Item 的不透明标识。
pub(super) fn validate_item_id(id: &str) -> Result<(), StreamStateError> {
    if id.is_empty()
        || id.len() > MAX_OUTPUT_ITEM_ID_BYTES
        || id.trim() != id
        || id.chars().any(char::is_control)
    {
        return Err(StreamStateError::InvalidValue);
    }
    Ok(())
}

fn map_argument_error(error: BoundedJsonError) -> StreamStateError {
    match error {
        BoundedJsonError::InvalidJson => StreamStateError::UnsupportedFeature,
        BoundedJsonError::DuplicateKey => StreamStateError::InvalidValue,
        BoundedJsonError::LimitExceeded => StreamStateError::StructureLimitExceeded,
    }
}

fn map_request_error(error: ParseRequestError) -> StreamStateError {
    match error {
        ParseRequestError::BodyTooLarge | ParseRequestError::StructureLimitExceeded => {
            StreamStateError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => StreamStateError::UnsupportedFeature,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::ConflictingParameters
        | ParseRequestError::InvalidValue => StreamStateError::InvalidValue,
    }
}

/// 共享状态校验只返回固定分类，不携带外部值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StreamStateError {
    InvalidValue,
    UnsupportedFeature,
    StructureLimitExceeded,
}
