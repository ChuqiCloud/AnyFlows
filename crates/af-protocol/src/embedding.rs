use std::{error::Error, fmt};

use af_domain::{MAX_MODEL_NAME_BYTES, Operation};

use crate::{TokenCount, Usage, UsageSemantics};

/// 单次 Embeddings 请求允许的最大文本条数。
pub const MAX_EMBEDDING_INPUTS: usize = 2_048;
/// 单条 Embeddings 文本允许的最大 UTF-8 字节数。
pub const MAX_EMBEDDING_TEXT_BYTES: usize = 1024 * 1024;
/// 单次 Embeddings 请求中全部文本允许的最大 UTF-8 字节数。
pub const MAX_TOTAL_EMBEDDING_TEXT_BYTES: usize = 8 * 1024 * 1024;
/// 单个 Embeddings 向量允许的最大维度。
pub const MAX_EMBEDDING_DIMENSIONS: usize = 3_072;
/// 单个响应允许携带的最大向量元素总数。
pub const MAX_TOTAL_EMBEDDING_VALUES: usize = 1_000_000;

/// Embeddings 请求的文本输入，保留单文本与文本数组的来源形态。
#[derive(Clone, Eq, PartialEq)]
pub enum EmbeddingInput {
    /// 单条文本输入。
    Text(String),
    /// 按调用方顺序排列的多条文本输入。
    Texts(Vec<String>),
}

impl EmbeddingInput {
    /// 返回本次请求中的文本条数。
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Text(_) => 1,
            Self::Texts(texts) => texts.len(),
        }
    }

    /// 返回输入是否为空集合。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Texts(texts) if texts.is_empty())
    }

    fn validate(&self) -> Result<(), CanonicalEmbeddingRequestError> {
        if self.is_empty() || self.len() > MAX_EMBEDDING_INPUTS {
            return Err(CanonicalEmbeddingRequestError::InvalidInputCount);
        }

        let mut total_bytes = 0_usize;
        let mut validate_text = |text: &str| {
            if text.is_empty() || text.len() > MAX_EMBEDDING_TEXT_BYTES {
                return Err(CanonicalEmbeddingRequestError::InvalidText);
            }
            total_bytes = total_bytes
                .checked_add(text.len())
                .ok_or(CanonicalEmbeddingRequestError::TotalTextTooLarge)?;
            if total_bytes > MAX_TOTAL_EMBEDDING_TEXT_BYTES {
                return Err(CanonicalEmbeddingRequestError::TotalTextTooLarge);
            }
            Ok(())
        };

        match self {
            Self::Text(text) => validate_text(text),
            Self::Texts(texts) => {
                for text in texts {
                    validate_text(text)?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Debug for EmbeddingInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddingInput")
            .field("text_count", &self.len())
            .finish()
    }
}

/// 经边界校验的 Embeddings 向量维度。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddingDimensions(u16);

impl EmbeddingDimensions {
    /// 从外部整数构造受限向量维度。
    pub fn new(value: u32) -> Result<Self, EmbeddingDimensionsError> {
        let value = usize::try_from(value).map_err(|_| EmbeddingDimensionsError::OutOfRange)?;
        if !(1..=MAX_EMBEDDING_DIMENSIONS).contains(&value) {
            return Err(EmbeddingDimensionsError::OutOfRange);
        }
        Ok(Self(
            u16::try_from(value).map_err(|_| EmbeddingDimensionsError::OutOfRange)?,
        ))
    }

    /// 返回已校验的维度数。
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// Embeddings 请求的协议无关表示。
///
/// 它不复用 Chat 的消息、工具、采样或流式字段，避免调用方把不适用的语义带入向量化请求。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalEmbeddingRequest {
    model: String,
    input: EmbeddingInput,
    dimensions: Option<EmbeddingDimensions>,
}

impl CanonicalEmbeddingRequest {
    /// 构造经过模型名、文本输入和维度预算校验的 Embeddings 请求。
    pub fn new(
        model: String,
        input: EmbeddingInput,
        dimensions: Option<EmbeddingDimensions>,
    ) -> Result<Self, CanonicalEmbeddingRequestError> {
        validate_model(&model).map_err(|_| CanonicalEmbeddingRequestError::InvalidModel)?;
        input.validate()?;
        Ok(Self {
            model,
            input,
            dimensions,
        })
    }

    /// 返回该请求固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Embedding
    }

    /// 返回尚未经过渠道映射的客户端模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回保留来源形态的文本输入。
    #[must_use]
    pub const fn input(&self) -> &EmbeddingInput {
        &self.input
    }

    /// 返回请求的可选目标维度。
    #[must_use]
    pub const fn dimensions(&self) -> Option<EmbeddingDimensions> {
        self.dimensions
    }

    /// 返回全部文本输入的 UTF-8 字节总数，作为标准 Embeddings token 的安全预扣上界。
    #[must_use]
    pub fn total_text_bytes(&self) -> usize {
        match &self.input {
            EmbeddingInput::Text(text) => text.len(),
            EmbeddingInput::Texts(texts) => texts.iter().map(|text| text.len()).sum(),
        }
    }
}

impl fmt::Debug for CanonicalEmbeddingRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalEmbeddingRequest")
            .field("operation", &Operation::Embedding)
            .field("model", &"<已脱敏>")
            .field("input", &self.input)
            .field("dimensions", &self.dimensions)
            .finish()
    }
}

/// Canonical Embeddings 请求的校验错误，不保留模型名或输入内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalEmbeddingRequestError {
    /// 模型名为空、超长、含控制字符或带首尾空白。
    InvalidModel,
    /// 输入条数为空或超过上限。
    InvalidInputCount,
    /// 单条输入为空或超过单条文本预算。
    InvalidText,
    /// 全部输入文本超过单次请求预算。
    TotalTextTooLarge,
}

impl fmt::Display for CanonicalEmbeddingRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidModel => "Embeddings 模型名无效",
            Self::InvalidInputCount => "Embeddings 输入条数无效",
            Self::InvalidText => "Embeddings 文本输入无效",
            Self::TotalTextTooLarge => "Embeddings 输入文本超过预算",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalEmbeddingRequestError {}

/// Embeddings 向量维度的校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingDimensionsError {
    /// 维度不在协议允许的闭合范围内。
    OutOfRange,
}

impl fmt::Display for EmbeddingDimensionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Embeddings 向量维度超出允许范围")
    }
}

impl Error for EmbeddingDimensionsError {}

/// 一条带稳定索引的 Embeddings 向量。
#[derive(Clone, PartialEq)]
pub struct EmbeddingVector {
    index: u32,
    values: Vec<f64>,
}

impl EmbeddingVector {
    /// 构造并校验一条有限浮点向量。
    pub fn new(index: u32, values: Vec<f64>) -> Result<Self, CanonicalEmbeddingResponseError> {
        if values.is_empty()
            || values.len() > MAX_EMBEDDING_DIMENSIONS
            || values.iter().any(|value| !value.is_finite())
        {
            return Err(CanonicalEmbeddingResponseError::InvalidVector);
        }
        Ok(Self { index, values })
    }

    /// 返回该向量对应的输入索引。
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// 返回已校验的浮点向量。
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }
}

impl fmt::Debug for EmbeddingVector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddingVector")
            .field("index", &self.index)
            .field("dimension", &self.values.len())
            .finish()
    }
}

/// Embeddings 非流式响应的协议无关表示。
#[derive(Clone, PartialEq)]
pub struct CanonicalEmbeddingResponse {
    model: String,
    vectors: Vec<EmbeddingVector>,
    usage: Usage,
}

impl CanonicalEmbeddingResponse {
    /// 构造并规范化一个已完成的 Embeddings 响应。
    pub fn new(
        model: String,
        mut vectors: Vec<EmbeddingVector>,
        usage: Usage,
    ) -> Result<Self, CanonicalEmbeddingResponseError> {
        validate_model(&model).map_err(|_| CanonicalEmbeddingResponseError::InvalidModel)?;
        if vectors.is_empty() || vectors.len() > MAX_EMBEDDING_INPUTS {
            return Err(CanonicalEmbeddingResponseError::InvalidVectorCount);
        }
        vectors.sort_by_key(EmbeddingVector::index);

        let mut value_count = 0_usize;
        for (position, vector) in vectors.iter().enumerate() {
            let expected_index = u32::try_from(position)
                .map_err(|_| CanonicalEmbeddingResponseError::InvalidIndexes)?;
            if vector.index != expected_index {
                return Err(CanonicalEmbeddingResponseError::InvalidIndexes);
            }
            value_count = value_count
                .checked_add(vector.values.len())
                .ok_or(CanonicalEmbeddingResponseError::VectorBudgetExceeded)?;
            if value_count > MAX_TOTAL_EMBEDDING_VALUES {
                return Err(CanonicalEmbeddingResponseError::VectorBudgetExceeded);
            }
        }
        validate_embedding_usage(usage)?;
        Ok(Self {
            model,
            vectors,
            usage,
        })
    }

    /// 返回该响应固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Embedding
    }

    /// 返回响应模型名；网关重编码前应改写为客户端请求的模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回按索引升序规范化的向量集合。
    #[must_use]
    pub fn vectors(&self) -> &[EmbeddingVector] {
        &self.vectors
    }

    /// 将已验证的上游响应改写为客户端可见模型名。
    ///
    /// 网关必须在下游编码前调用本方法，避免把渠道私有模型标识泄露给客户端；重建过程会
    /// 再次校验模型名、向量索引和用量不变量。
    pub fn rebind_model(&self, model: String) -> Result<Self, CanonicalEmbeddingResponseError> {
        Self::new(model, self.vectors.clone(), self.usage)
    }

    /// 返回已校验的输入 token 用量。
    #[must_use]
    pub const fn usage(&self) -> Usage {
        self.usage
    }

    /// 验证该响应与原请求的输入条数和指定维度一致。
    pub fn validate_for_request(
        &self,
        request: &CanonicalEmbeddingRequest,
    ) -> Result<(), CanonicalEmbeddingResponseError> {
        if self.vectors.len() != request.input.len() {
            return Err(CanonicalEmbeddingResponseError::InputCountMismatch);
        }
        // 同一模型的一批向量必须使用相同维度，避免未指定 dimensions 时接受畸形响应。
        let expected_dimensions = self
            .vectors
            .first()
            .map(|vector| vector.values.len())
            .ok_or(CanonicalEmbeddingResponseError::InvalidVectorCount)?;
        if self
            .vectors
            .iter()
            .any(|vector| vector.values.len() != expected_dimensions)
        {
            return Err(CanonicalEmbeddingResponseError::DimensionsMismatch);
        }
        if usize::try_from(self.usage.input_tokens().get())
            .map_or(true, |tokens| tokens > request.total_text_bytes())
        {
            return Err(CanonicalEmbeddingResponseError::UsageExceedsInputBudget);
        }
        if let Some(dimensions) = request.dimensions
            && self
                .vectors
                .iter()
                .any(|vector| vector.values.len() != dimensions.get())
        {
            return Err(CanonicalEmbeddingResponseError::DimensionsMismatch);
        }
        Ok(())
    }
}

impl fmt::Debug for CanonicalEmbeddingResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalEmbeddingResponse")
            .field("operation", &Operation::Embedding)
            .field("model", &"<已脱敏>")
            .field("vector_count", &self.vectors.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// Canonical Embeddings 响应的校验错误，不保留模型名、向量或上游用量数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalEmbeddingResponseError {
    /// 响应模型名无效。
    InvalidModel,
    /// 向量集合为空或超过输入上限。
    InvalidVectorCount,
    /// 向量为空、超维或包含非有限数值。
    InvalidVector,
    /// 向量索引没有恰好覆盖从零开始的连续范围。
    InvalidIndexes,
    /// 向量元素总数超过受控内存预算。
    VectorBudgetExceeded,
    /// 输入 token 与总 token 的 Embeddings 用量语义无效。
    InvalidUsage,
    /// 响应向量条数与请求输入条数不一致。
    InputCountMismatch,
    /// 响应向量维度彼此不一致，或与请求指定维度不一致。
    DimensionsMismatch,
    /// 上游输入 token 超过请求文本的安全字节上界。
    UsageExceedsInputBudget,
}

impl fmt::Display for CanonicalEmbeddingResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidModel => "Embeddings 响应模型名无效",
            Self::InvalidVectorCount => "Embeddings 响应向量条数无效",
            Self::InvalidVector => "Embeddings 响应向量无效",
            Self::InvalidIndexes => "Embeddings 响应向量索引无效",
            Self::VectorBudgetExceeded => "Embeddings 响应向量超过预算",
            Self::InvalidUsage => "Embeddings 响应用量无效",
            Self::InputCountMismatch => "Embeddings 响应与请求输入条数不一致",
            Self::DimensionsMismatch => "Embeddings 响应与请求向量维度不一致",
            Self::UsageExceedsInputBudget => "Embeddings 响应用量超过请求输入预算",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalEmbeddingResponseError {}

fn validate_model(model: &str) -> Result<(), ()> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.chars().any(char::is_control)
    {
        return Err(());
    }
    Ok(())
}

fn validate_embedding_usage(usage: Usage) -> Result<(), CanonicalEmbeddingResponseError> {
    let details = usage.details();
    if usage.semantics() != UsageSemantics::Inclusive
        || usage.output_tokens() != TokenCount::ZERO
        || details.cache_read() != TokenCount::ZERO
        || details.cache_creation_5m() != TokenCount::ZERO
        || details.cache_creation_1h() != TokenCount::ZERO
        || details.reasoning() != TokenCount::ZERO
        || details.audio_input() != TokenCount::ZERO
        || details.audio_output() != TokenCount::ZERO
        || usage
            .checked_total_tokens()
            .map_err(|_| CanonicalEmbeddingResponseError::InvalidUsage)?
            != usage.input_tokens()
    {
        return Err(CanonicalEmbeddingResponseError::InvalidUsage);
    }
    Ok(())
}
