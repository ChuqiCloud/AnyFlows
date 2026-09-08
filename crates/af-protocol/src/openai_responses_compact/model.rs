use std::fmt;

use af_domain::Operation;
use serde_json::Value;

use super::error::{CanonicalResponsesCompactionRequestError, ResponsesCompactionUsageError};
use super::validation::{
    item_kind, token, validate_input_model, validate_model, validate_opaque_id, validate_text,
};
use crate::TokenCount;

/// 一条已经完成字段和大小校验的 Responses 上下文 Item。
#[derive(Clone, Eq, PartialEq)]
pub struct ResponsesCompactionItem(pub(super) Value);

impl ResponsesCompactionItem {
    /// 从协议边界已完成校验的 JSON 创建不透明 Item。
    pub(crate) fn from_validated_value(value: Value) -> Self {
        Self(value)
    }

    /// 返回只读的已校验 JSON；调用方不得将其转成其他协议。
    #[must_use]
    pub fn as_value(&self) -> &Value {
        &self.0
    }
}

impl fmt::Debug for ResponsesCompactionItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponsesCompactionItem")
            .field("type", &item_kind(&self.0).unwrap_or("<无效>"))
            .finish()
    }
}

/// 压缩请求携带的完整上下文窗口输入。
#[derive(Clone, PartialEq)]
pub enum ResponsesCompactionInput {
    /// OpenAI 允许的单字符串简写。
    Text(String),
    /// 按客户端顺序保存的完整 Item 列表。
    Items(Vec<ResponsesCompactionItem>),
}

impl ResponsesCompactionInput {
    /// 返回有序 Item 列表；字符串简写输入返回 `None`。
    #[must_use]
    pub fn items(&self) -> Option<&[ResponsesCompactionItem]> {
        match self {
            Self::Text(_) => None,
            Self::Items(items) => Some(items),
        }
    }
}

impl fmt::Debug for ResponsesCompactionInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(_) => formatter.write_str("ResponsesCompactionInput::Text(<已脱敏>)"),
            Self::Items(items) => formatter
                .debug_struct("ResponsesCompactionInput::Items")
                .field("item_count", &items.len())
                .finish(),
        }
    }
}

/// `/responses/compact` 的 Canonical 请求。
#[derive(Clone, PartialEq)]
pub struct CanonicalResponsesCompactionRequest {
    pub(super) model: String,
    pub(super) input: Option<ResponsesCompactionInput>,
    pub(super) instructions: Option<String>,
    pub(super) previous_response_id: Option<String>,
}

impl CanonicalResponsesCompactionRequest {
    /// 从已校验字段构造压缩请求；至少需要输入窗口或上一响应引用。
    pub fn new(
        model: String,
        input: Option<ResponsesCompactionInput>,
        instructions: Option<String>,
        previous_response_id: Option<String>,
    ) -> Result<Self, CanonicalResponsesCompactionRequestError> {
        validate_model(&model)
            .map_err(|_| CanonicalResponsesCompactionRequestError::InvalidValue)?;
        if input.is_none() && previous_response_id.is_none() {
            return Err(CanonicalResponsesCompactionRequestError::MissingContext);
        }
        if let Some(input) = input.as_ref() {
            validate_input_model(input)
                .map_err(|_| CanonicalResponsesCompactionRequestError::InvalidValue)?;
        }
        if let Some(instructions) = instructions.as_deref() {
            validate_text(instructions)
                .map_err(|_| CanonicalResponsesCompactionRequestError::InvalidValue)?;
        }
        if let Some(id) = previous_response_id.as_deref() {
            validate_opaque_id(id)
                .map_err(|_| CanonicalResponsesCompactionRequestError::InvalidValue)?;
        }
        Ok(Self {
            model,
            input,
            instructions,
            previous_response_id,
        })
    }

    /// 返回独立压缩执行意图。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::ResponsesCompact
    }

    /// 返回客户端请求的模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回完整上下文窗口输入。
    #[must_use]
    pub const fn input(&self) -> Option<&ResponsesCompactionInput> {
        self.input.as_ref()
    }

    /// 返回本轮顶层指令。
    #[must_use]
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// 返回上一响应引用。
    #[must_use]
    pub fn previous_response_id(&self) -> Option<&str> {
        self.previous_response_id.as_deref()
    }
}

impl fmt::Debug for CanonicalResponsesCompactionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalResponsesCompactionRequest")
            .field("operation", &self.operation())
            .field("model", &"<已脱敏>")
            .field("input", &self.input)
            .field("has_instructions", &self.instructions.is_some())
            .field(
                "has_previous_response_id",
                &self.previous_response_id.is_some(),
            )
            .finish()
    }
}

/// 独立压缩响应的完整 usage。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponsesCompactionUsage {
    input_tokens: TokenCount,
    cached_tokens: TokenCount,
    cache_write_tokens: TokenCount,
    output_tokens: TokenCount,
    reasoning_tokens: TokenCount,
    total_tokens: TokenCount,
}

impl ResponsesCompactionUsage {
    /// 构造并校验 Responses Inclusive usage；总量必须等于输入与输出之和。
    pub fn new(
        input_tokens: i64,
        cached_tokens: i64,
        cache_write_tokens: i64,
        output_tokens: i64,
        reasoning_tokens: i64,
        total_tokens: i64,
    ) -> Result<Self, ResponsesCompactionUsageError> {
        let input_tokens = token(input_tokens)?;
        let cached_tokens = token(cached_tokens)?;
        let cache_write_tokens = token(cache_write_tokens)?;
        let output_tokens = token(output_tokens)?;
        let reasoning_tokens = token(reasoning_tokens)?;
        let total_tokens = token(total_tokens)?;
        let input_sum = cached_tokens
            .get()
            .checked_add(cache_write_tokens.get())
            .ok_or(ResponsesCompactionUsageError::Overflow)?;
        if input_sum > input_tokens.get() {
            return Err(ResponsesCompactionUsageError::InputDetailsExceedTotal);
        }
        if reasoning_tokens.get() > output_tokens.get() {
            return Err(ResponsesCompactionUsageError::OutputDetailsExceedTotal);
        }
        let expected_total = input_tokens
            .get()
            .checked_add(output_tokens.get())
            .ok_or(ResponsesCompactionUsageError::Overflow)?;
        if total_tokens.get() != expected_total {
            return Err(ResponsesCompactionUsageError::TotalMismatch);
        }
        Ok(Self {
            input_tokens,
            cached_tokens,
            cache_write_tokens,
            output_tokens,
            reasoning_tokens,
            total_tokens,
        })
    }

    /// 返回输入令牌总数。
    #[must_use]
    pub const fn input_tokens(self) -> TokenCount {
        self.input_tokens
    }

    /// 返回缓存命中令牌数。
    #[must_use]
    pub const fn cached_tokens(self) -> TokenCount {
        self.cached_tokens
    }

    /// 返回缓存写入令牌数。
    #[must_use]
    pub const fn cache_write_tokens(self) -> TokenCount {
        self.cache_write_tokens
    }

    /// 返回压缩摘要输出令牌数。
    #[must_use]
    pub const fn output_tokens(self) -> TokenCount {
        self.output_tokens
    }

    /// 返回输出中的推理令牌数。
    #[must_use]
    pub const fn reasoning_tokens(self) -> TokenCount {
        self.reasoning_tokens
    }

    /// 按 Inclusive 口径返回总令牌数。
    #[must_use]
    pub const fn total_tokens(self) -> TokenCount {
        self.total_tokens
    }
}

/// `/responses/compact` 的 Canonical 响应。
#[derive(Clone, PartialEq)]
pub struct CanonicalResponsesCompactionResponse {
    pub(super) id: String,
    pub(super) created_at: i64,
    pub(super) output: Vec<ResponsesCompactionItem>,
    pub(super) usage: ResponsesCompactionUsage,
}

impl CanonicalResponsesCompactionResponse {
    /// 返回独立压缩执行意图。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::ResponsesCompact
    }

    /// 返回上游响应标识。
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 返回上游创建时间。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回按原顺序保留的压缩窗口 Item。
    #[must_use]
    pub fn output(&self) -> &[ResponsesCompactionItem] {
        &self.output
    }

    /// 返回独立压缩 usage。
    #[must_use]
    pub const fn usage(&self) -> ResponsesCompactionUsage {
        self.usage
    }
}

impl fmt::Debug for CanonicalResponsesCompactionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalResponsesCompactionResponse")
            .field("operation", &self.operation())
            .field("id", &"<已脱敏>")
            .field("created_at", &self.created_at)
            .field("output_count", &self.output.len())
            .field("usage", &self.usage)
            .finish()
    }
}
