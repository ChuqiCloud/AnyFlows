use std::{error::Error, fmt};

use af_domain::{MAX_MODEL_NAME_BYTES, Operation};

use crate::{TokenCount, Usage, UsageSemantics};

/// 单次 Rerank 请求允许的最大文档数。
pub const MAX_RERANK_DOCUMENTS: usize = 1_000;
/// Rerank 查询允许的最大 UTF-8 字节数。
pub const MAX_RERANK_QUERY_BYTES: usize = 1024 * 1024;
/// 单篇 Rerank 文档允许的最大 UTF-8 字节数。
pub const MAX_RERANK_DOCUMENT_BYTES: usize = 1024 * 1024;
/// 查询和全部文档合计允许的最大 UTF-8 字节数。
pub const MAX_TOTAL_RERANK_TEXT_BYTES: usize = 8 * 1024 * 1024;
/// Rerank 响应标识允许的最大 UTF-8 字节数。
pub const MAX_RERANK_RESPONSE_ID_BYTES: usize = 256;

/// Rerank 文本文档，保留字符串与严格 `{text}` 对象的来源形态。
#[derive(Clone, Eq, PartialEq)]
pub enum RerankDocument {
    /// 直接字符串文档。
    Text(String),
    /// 仅包含 `text` 字段的对象文档。
    TextObject(String),
}

impl RerankDocument {
    /// 返回文档的文本内容。
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Text(text) | Self::TextObject(text) => text,
        }
    }

    /// 返回文档是否来自对象形态。
    #[must_use]
    pub const fn is_text_object(&self) -> bool {
        matches!(self, Self::TextObject(_))
    }

    fn validate(&self) -> Result<(), CanonicalRerankRequestError> {
        let text = self.text();
        if text.trim().is_empty() || text.len() > MAX_RERANK_DOCUMENT_BYTES {
            return Err(CanonicalRerankRequestError::InvalidDocument);
        }
        Ok(())
    }
}

impl fmt::Debug for RerankDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RerankDocument")
            .field(
                "shape",
                if self.is_text_object() {
                    &"text_object"
                } else {
                    &"text"
                },
            )
            .field("bytes", &self.text().len())
            .finish()
    }
}

/// 经校验的 Rerank 返回条数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RerankTopN(u16);

impl RerankTopN {
    /// 构造受限的返回条数。
    pub fn new(value: u32) -> Result<Self, RerankTopNError> {
        let value = usize::try_from(value).map_err(|_| RerankTopNError::OutOfRange)?;
        if !(1..=MAX_RERANK_DOCUMENTS).contains(&value) {
            return Err(RerankTopNError::OutOfRange);
        }
        Ok(Self(
            u16::try_from(value).map_err(|_| RerankTopNError::OutOfRange)?,
        ))
    }

    /// 返回已校验的返回条数。
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// Rerank 请求的协议无关表示。
///
/// 缺失 `return_documents` 时固定归一为 `false`，避免 Jina 与 Cohere 不同默认值导致
/// 同一网关请求在不同渠道返回不同正文形态。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalRerankRequest {
    model: String,
    query: String,
    documents: Vec<RerankDocument>,
    top_n: Option<RerankTopN>,
    return_documents: bool,
    total_text_bytes: usize,
}

impl CanonicalRerankRequest {
    /// 构造经过模型、文本、数量与返回条数校验的 Rerank 请求。
    pub fn new(
        model: String,
        query: String,
        documents: Vec<RerankDocument>,
        top_n: Option<RerankTopN>,
        return_documents: bool,
    ) -> Result<Self, CanonicalRerankRequestError> {
        validate_model(&model).map_err(|_| CanonicalRerankRequestError::InvalidModel)?;
        if query.trim().is_empty() || query.len() > MAX_RERANK_QUERY_BYTES {
            return Err(CanonicalRerankRequestError::InvalidQuery);
        }
        if documents.is_empty() || documents.len() > MAX_RERANK_DOCUMENTS {
            return Err(CanonicalRerankRequestError::InvalidDocumentCount);
        }
        if top_n.is_some_and(|top_n| top_n.get() > documents.len()) {
            return Err(CanonicalRerankRequestError::InvalidTopN);
        }

        let mut total_text_bytes = query.len();
        for document in &documents {
            document.validate()?;
            total_text_bytes = total_text_bytes
                .checked_add(document.text().len())
                .ok_or(CanonicalRerankRequestError::TotalTextTooLarge)?;
            if total_text_bytes > MAX_TOTAL_RERANK_TEXT_BYTES {
                return Err(CanonicalRerankRequestError::TotalTextTooLarge);
            }
        }

        Ok(Self {
            model,
            query,
            documents,
            top_n,
            return_documents,
            total_text_bytes,
        })
    }

    /// 返回该请求固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Rerank
    }

    /// 返回尚未经过渠道映射的客户端模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回查询文本。
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// 返回按调用方顺序排列的文档。
    #[must_use]
    pub fn documents(&self) -> &[RerankDocument] {
        &self.documents
    }

    /// 返回可选的最大结果数；缺失时表示返回全部文档。
    #[must_use]
    pub const fn top_n(&self) -> Option<RerankTopN> {
        self.top_n
    }

    /// 返回是否要求结果携带原文档。
    #[must_use]
    pub const fn return_documents(&self) -> bool {
        self.return_documents
    }

    /// 返回查询与全部文档的 UTF-8 字节总数。
    #[must_use]
    pub const fn total_text_bytes(&self) -> usize {
        self.total_text_bytes
    }

    /// 返回协议期望的结果条数。
    #[must_use]
    pub fn expected_result_count(&self) -> usize {
        self.top_n.map_or(self.documents.len(), RerankTopN::get)
    }
}

impl fmt::Debug for CanonicalRerankRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalRerankRequest")
            .field("operation", &Operation::Rerank)
            .field("model", &"<已脱敏>")
            .field("query", &"<已脱敏>")
            .field("document_count", &self.documents.len())
            .field("top_n", &self.top_n)
            .field("return_documents", &self.return_documents)
            .field("total_text_bytes", &self.total_text_bytes)
            .finish()
    }
}

/// Canonical Rerank 请求校验错误，不保留模型、查询或文档正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalRerankRequestError {
    /// 模型名无效。
    InvalidModel,
    /// 查询为空、仅含空白或超过预算。
    InvalidQuery,
    /// 文档集合为空或超过数量上限。
    InvalidDocumentCount,
    /// 单篇文档为空、仅含空白或超过预算。
    InvalidDocument,
    /// 查询和文档合计超过文本预算。
    TotalTextTooLarge,
    /// `top_n` 超过文档数量。
    InvalidTopN,
}

impl fmt::Display for CanonicalRerankRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidModel => "Rerank 模型名无效",
            Self::InvalidQuery => "Rerank 查询无效",
            Self::InvalidDocumentCount => "Rerank 文档数量无效",
            Self::InvalidDocument => "Rerank 文档无效",
            Self::TotalTextTooLarge => "Rerank 文本超过预算",
            Self::InvalidTopN => "Rerank 返回条数无效",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalRerankRequestError {}

/// Rerank 返回条数校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RerankTopNError {
    /// 返回条数不在闭合范围内。
    OutOfRange,
}

impl fmt::Display for RerankTopNError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Rerank 返回条数超出允许范围")
    }
}

impl Error for RerankTopNError {}

/// 经校验的有限相关度分数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RerankRelevanceScore(f64);

impl RerankRelevanceScore {
    /// 构造有限相关度分数；不同供应商的分数区间不同，因此只限制为有限数。
    pub fn new(value: f64) -> Result<Self, RerankRelevanceScoreError> {
        if !value.is_finite() {
            return Err(RerankRelevanceScoreError::NotFinite);
        }
        Ok(Self(value))
    }

    /// 返回底层有限浮点值。
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Rerank 相关度分数校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RerankRelevanceScoreError {
    /// 分数不是有限数。
    NotFinite,
}

impl fmt::Display for RerankRelevanceScoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Rerank 相关度分数必须是有限数")
    }
}

impl Error for RerankRelevanceScoreError {}

/// Cohere Rerank 的受限计费搜索单位。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RerankSearchUnits(u16);

impl RerankSearchUnits {
    /// 构造正数搜索单位；单次调用的单位数不得超过文档硬上限。
    pub fn new(value: u32) -> Result<Self, RerankSearchUnitsError> {
        let value = usize::try_from(value).map_err(|_| RerankSearchUnitsError::OutOfRange)?;
        if !(1..=MAX_RERANK_DOCUMENTS).contains(&value) {
            return Err(RerankSearchUnitsError::OutOfRange);
        }
        Ok(Self(
            u16::try_from(value).map_err(|_| RerankSearchUnitsError::OutOfRange)?,
        ))
    }

    /// 返回搜索单位数。
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// Rerank 搜索单位校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RerankSearchUnitsError {
    /// 搜索单位不是受限正数。
    OutOfRange,
}

impl fmt::Display for RerankSearchUnitsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Rerank 搜索单位超出允许范围")
    }
}

impl Error for RerankSearchUnitsError {}

/// Rerank 用量事实，可同时保留 token 与 Cohere search unit，禁止相互猜测换算。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RerankUsage {
    token_usage: Option<Usage>,
    search_units: Option<RerankSearchUnits>,
}

impl RerankUsage {
    /// 构造至少包含一种真实事实的 Rerank 用量。
    pub fn new(
        token_usage: Option<Usage>,
        search_units: Option<RerankSearchUnits>,
    ) -> Result<Self, RerankUsageError> {
        if token_usage.is_none() && search_units.is_none() {
            return Err(RerankUsageError::MissingFacts);
        }
        if let Some(usage) = token_usage {
            validate_token_usage(usage)?;
        }
        Ok(Self {
            token_usage,
            search_units,
        })
    }

    /// 返回可选的 token 用量事实。
    #[must_use]
    pub const fn token_usage(self) -> Option<Usage> {
        self.token_usage
    }

    /// 返回可选的 Cohere search unit 事实。
    #[must_use]
    pub const fn search_units(self) -> Option<RerankSearchUnits> {
        self.search_units
    }
}

/// Rerank 用量校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RerankUsageError {
    /// token 与 search unit 均缺失。
    MissingFacts,
    /// token 用量包含输出、缓存、推理、音频或非 Inclusive 语义。
    InvalidTokenUsage,
}

impl fmt::Display for RerankUsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingFacts => "Rerank 用量缺少可验证事实",
            Self::InvalidTokenUsage => "Rerank token 用量语义无效",
        };
        formatter.write_str(message)
    }
}

impl Error for RerankUsageError {}

/// 一条按相关度排序的 Rerank 结果。
#[derive(Clone, Debug, PartialEq)]
pub struct RerankResult {
    index: u32,
    relevance_score: RerankRelevanceScore,
    document: Option<RerankDocument>,
}

impl RerankResult {
    /// 构造索引、有限分数和可选文档均有效的结果。
    pub fn new(
        index: u32,
        relevance_score: RerankRelevanceScore,
        document: Option<RerankDocument>,
    ) -> Result<Self, CanonicalRerankResponseError> {
        if usize::try_from(index).map_or(true, |index| index >= MAX_RERANK_DOCUMENTS) {
            return Err(CanonicalRerankResponseError::InvalidIndex);
        }
        if let Some(document) = &document {
            document
                .validate()
                .map_err(|_| CanonicalRerankResponseError::InvalidDocument)?;
        }
        Ok(Self {
            index,
            relevance_score,
            document,
        })
    }

    /// 返回原始请求文档索引。
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// 返回有限相关度分数。
    #[must_use]
    pub const fn relevance_score(&self) -> RerankRelevanceScore {
        self.relevance_score
    }

    /// 返回上游按请求要求携带的可选文档。
    #[must_use]
    pub const fn document(&self) -> Option<&RerankDocument> {
        self.document.as_ref()
    }
}

/// Rerank 非流式响应的协议无关表示。
#[derive(Clone, PartialEq)]
pub struct CanonicalRerankResponse {
    response_id: Option<String>,
    model: Option<String>,
    results: Vec<RerankResult>,
    usage: Option<RerankUsage>,
}

impl CanonicalRerankResponse {
    /// 构造并校验一个按相关度降序排列的 Rerank 响应。
    pub fn new(
        response_id: Option<String>,
        model: Option<String>,
        results: Vec<RerankResult>,
        usage: Option<RerankUsage>,
    ) -> Result<Self, CanonicalRerankResponseError> {
        if response_id.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > MAX_RERANK_RESPONSE_ID_BYTES
                || value.trim() != value
                || value.chars().any(char::is_control)
        }) {
            return Err(CanonicalRerankResponseError::InvalidResponseId);
        }
        if model
            .as_deref()
            .is_some_and(|model| validate_model(model).is_err())
        {
            return Err(CanonicalRerankResponseError::InvalidModel);
        }
        if results.is_empty() || results.len() > MAX_RERANK_DOCUMENTS {
            return Err(CanonicalRerankResponseError::InvalidResultCount);
        }

        let mut seen_indexes = [false; MAX_RERANK_DOCUMENTS];
        let mut returned_text_bytes = 0_usize;
        let mut previous_score = None;
        for result in &results {
            let index = usize::try_from(result.index)
                .map_err(|_| CanonicalRerankResponseError::InvalidIndex)?;
            if index >= MAX_RERANK_DOCUMENTS || seen_indexes[index] {
                return Err(CanonicalRerankResponseError::InvalidIndex);
            }
            seen_indexes[index] = true;
            if previous_score.is_some_and(|score: f64| score < result.relevance_score.get()) {
                return Err(CanonicalRerankResponseError::InvalidOrder);
            }
            previous_score = Some(result.relevance_score.get());
            if let Some(document) = &result.document {
                returned_text_bytes = returned_text_bytes
                    .checked_add(document.text().len())
                    .ok_or(CanonicalRerankResponseError::DocumentBudgetExceeded)?;
                if returned_text_bytes > MAX_TOTAL_RERANK_TEXT_BYTES {
                    return Err(CanonicalRerankResponseError::DocumentBudgetExceeded);
                }
            }
        }

        Ok(Self {
            response_id,
            model,
            results,
            usage,
        })
    }

    /// 返回该响应固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Rerank
    }

    /// 返回可选的供应商响应标识。
    #[must_use]
    pub fn response_id(&self) -> Option<&str> {
        self.response_id.as_deref()
    }

    /// 返回可选的上游模型名；网关公开编码前应改写为客户端模型名。
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// 返回保持相关度顺序的结果集合。
    #[must_use]
    pub fn results(&self) -> &[RerankResult] {
        &self.results
    }

    /// 返回可选的真实用量事实。
    #[must_use]
    pub const fn usage(&self) -> Option<RerankUsage> {
        self.usage
    }

    /// 将已验证响应改写为客户端可见模型名。
    pub fn rebind_model(&self, model: String) -> Result<Self, CanonicalRerankResponseError> {
        Self::new(
            self.response_id.clone(),
            Some(model),
            self.results.clone(),
            self.usage,
        )
    }

    /// 验证结果数量、索引、返回文档与用量均和原请求一致。
    pub fn validate_for_request(
        &self,
        request: &CanonicalRerankRequest,
    ) -> Result<(), CanonicalRerankResponseError> {
        if self.results.len() != request.expected_result_count() {
            return Err(CanonicalRerankResponseError::ResultCountMismatch);
        }
        for result in &self.results {
            let index = usize::try_from(result.index)
                .map_err(|_| CanonicalRerankResponseError::InvalidIndex)?;
            let expected_document = request
                .documents
                .get(index)
                .ok_or(CanonicalRerankResponseError::InvalidIndex)?;
            match (request.return_documents, result.document.as_ref()) {
                (false, None) => {}
                (true, Some(document)) if document.text() == expected_document.text() => {}
                (false, Some(_)) | (true, None) | (true, Some(_)) => {
                    return Err(CanonicalRerankResponseError::DocumentMismatch);
                }
            }
        }

        if let Some(usage) = self.usage {
            if usage.token_usage().is_some_and(|token_usage| {
                usize::try_from(token_usage.input_tokens().get())
                    .map_or(true, |tokens| tokens > request.total_text_bytes)
            }) {
                return Err(CanonicalRerankResponseError::UsageExceedsInputBudget);
            }
            if usage
                .search_units()
                .is_some_and(|units| units.get() > request.documents.len())
            {
                return Err(CanonicalRerankResponseError::SearchUnitsExceedDocumentCount);
            }
        }
        Ok(())
    }
}

impl fmt::Debug for CanonicalRerankResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalRerankResponse")
            .field("operation", &Operation::Rerank)
            .field(
                "response_id",
                &self.response_id.as_ref().map(|_| "<已脱敏>"),
            )
            .field("model", &self.model.as_ref().map(|_| "<已脱敏>"))
            .field("result_count", &self.results.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// Canonical Rerank 响应校验错误，不保留上游标识、模型、文档或用量数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalRerankResponseError {
    /// 响应标识无效。
    InvalidResponseId,
    /// 响应模型名无效。
    InvalidModel,
    /// 结果集合为空或超过数量上限。
    InvalidResultCount,
    /// 结果索引越界或重复。
    InvalidIndex,
    /// 结果没有按相关度非递增排列。
    InvalidOrder,
    /// 返回文档为空、仅含空白或超过单篇预算。
    InvalidDocument,
    /// 返回文档正文合计超过预算。
    DocumentBudgetExceeded,
    /// 响应结果数量与请求的 `top_n` 不一致。
    ResultCountMismatch,
    /// 返回文档状态或正文与原请求不一致。
    DocumentMismatch,
    /// token 用量超过请求文本的 UTF-8 字节安全上界。
    UsageExceedsInputBudget,
    /// search unit 超过请求文档数。
    SearchUnitsExceedDocumentCount,
}

impl fmt::Display for CanonicalRerankResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidResponseId => "Rerank 响应标识无效",
            Self::InvalidModel => "Rerank 响应模型名无效",
            Self::InvalidResultCount => "Rerank 响应结果数量无效",
            Self::InvalidIndex => "Rerank 响应结果索引无效",
            Self::InvalidOrder => "Rerank 响应结果顺序无效",
            Self::InvalidDocument => "Rerank 响应文档无效",
            Self::DocumentBudgetExceeded => "Rerank 响应文档超过预算",
            Self::ResultCountMismatch => "Rerank 响应结果数量与请求不一致",
            Self::DocumentMismatch => "Rerank 响应文档与请求不一致",
            Self::UsageExceedsInputBudget => "Rerank 响应用量超过请求输入预算",
            Self::SearchUnitsExceedDocumentCount => "Rerank 搜索单位超过请求文档数",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalRerankResponseError {}

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

fn validate_token_usage(usage: Usage) -> Result<(), RerankUsageError> {
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
            .map_err(|_| RerankUsageError::InvalidTokenUsage)?
            != usage.input_tokens()
    {
        return Err(RerankUsageError::InvalidTokenUsage);
    }
    Ok(())
}
