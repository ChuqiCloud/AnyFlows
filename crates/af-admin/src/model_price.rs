use std::{collections::BTreeSet, fmt, future::Future, pin::Pin, sync::Arc};

use af_billing::{
    BillingExpressionDefinition, BillingExpressionResult, BillingExpressionUsage,
    BillingExpressionUsageSemantics, BillingExpressionVariables, PricingRatio, PricingRatios,
};
use af_db::{
    AdminModelRepository, MAX_ADMIN_MODEL_PAGE_SIZE, MAX_ADMIN_MODEL_PROVIDER_BYTES,
    MAX_MODEL_PRICE_PAGE_SIZE, MAX_MODEL_PRICE_WRITE_BATCH, ModelPriceBillingMode,
    ModelPriceRecord, ModelPriceRepository, ModelPriceWriteError, ModelPriceWriteRecord,
};
use af_domain::{MAX_MODEL_NAME_BYTES, ModelId};
use rust_decimal::Decimal;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理模型价格列表的默认页大小。
pub const DEFAULT_ADMIN_MODEL_PRICE_PAGE_SIZE: usize = 50;
/// 单次公开参考价预览允许匹配的本地模型上限。
pub const MAX_MODEL_PRICE_SOURCE_TARGETS: usize = 10_000;
/// 管理试算 JSON 整数保持 JavaScript 安全整数精度的上限。
pub const MAX_MODEL_PRICE_EXPRESSION_PREVIEW_INTEGER: i64 = 9_007_199_254_740_991;
const MAX_SOURCE_REVISION_BYTES: usize = 128;
const MAX_SOURCE_TEXT_BYTES: usize = 128;

/// 管理端支持读取的固定公开模型价格来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelPriceSourceKind {
    /// models.dev 的提供商分组目录。
    ModelsDev,
    /// LiteLLM 官方模型价格映射。
    LiteLlm,
}

impl ModelPriceSourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelsDev => "models_dev",
            Self::LiteLlm => "litellm",
        }
    }
}

/// 管理端可写入的闭合模型计费模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelPriceBillingMode {
    /// 五类美元/百万 Token 单价。
    PerToken,
    /// 显式免费，五类单价必须全部为零。
    Free,
    /// 使用版本化 Rhai 表达式定价，五类固定单价必须全部为零。
    Expression,
}

impl From<ModelPriceBillingMode> for AdminModelPriceBillingMode {
    fn from(value: ModelPriceBillingMode) -> Self {
        match value {
            ModelPriceBillingMode::PerToken => Self::PerToken,
            ModelPriceBillingMode::Free => Self::Free,
            ModelPriceBillingMode::Expression => Self::Expression,
        }
    }
}

impl From<AdminModelPriceBillingMode> for ModelPriceBillingMode {
    fn from(value: AdminModelPriceBillingMode) -> Self {
        match value {
            AdminModelPriceBillingMode::PerToken => Self::PerToken,
            AdminModelPriceBillingMode::Free => Self::Free,
            AdminModelPriceBillingMode::Expression => Self::Expression,
        }
    }
}

/// 管理端读取的单个正式模型价格快照。
pub struct AdminModelPrice {
    model: String,
    billing_mode: AdminModelPriceBillingMode,
    prices: [Decimal; 5],
    billing_expression: Option<String>,
    version: u64,
}

impl AdminModelPrice {
    fn from_record(record: ModelPriceRecord) -> Self {
        Self {
            model: record.model().to_owned(),
            billing_mode: record.billing_mode().into(),
            prices: record.prices(),
            billing_expression: record.billing_expression().map(str::to_owned),
            version: record.version(),
        }
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn billing_mode(&self) -> AdminModelPriceBillingMode {
        self.billing_mode
    }

    #[must_use]
    pub const fn prices(&self) -> [Decimal; 5] {
        self.prices
    }

    /// 返回管理员可编辑的表达式正文；不存在时表示固定单价或免费模式。
    #[must_use]
    pub fn billing_expression(&self) -> Option<&str> {
        self.billing_expression.as_deref()
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
}

impl fmt::Debug for AdminModelPrice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelPrice")
            .field("billing_mode", &self.billing_mode)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 已校验的管理价格稳定游标查询。
#[derive(Clone, Eq, PartialEq)]
pub struct AdminModelPriceListQuery {
    after: Option<String>,
    limit: usize,
}

impl AdminModelPriceListQuery {
    pub fn new(after: Option<String>, limit: usize) -> Result<Self, AdminModelPriceError> {
        if !(1..=MAX_MODEL_PRICE_PAGE_SIZE).contains(&limit)
            || after.as_deref().is_some_and(|value| !valid_model(value))
        {
            return Err(AdminModelPriceError::InvalidInput);
        }
        Ok(Self { after, limit })
    }

    #[must_use]
    pub fn after(&self) -> Option<&str> {
        self.after.as_deref()
    }

    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for AdminModelPriceListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_MODEL_PRICE_PAGE_SIZE,
        }
    }
}

impl fmt::Debug for AdminModelPriceListQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelPriceListQuery")
            .field("has_after", &self.after.is_some())
            .field("limit", &self.limit)
            .finish()
    }
}

/// 一页正式模型价格。
pub struct AdminModelPricePage {
    prices: Vec<AdminModelPrice>,
    next_cursor: Option<String>,
}

impl AdminModelPricePage {
    #[must_use]
    pub fn prices(&self) -> &[AdminModelPrice] {
        &self.prices
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

impl fmt::Debug for AdminModelPricePage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelPricePage")
            .field("price_count", &self.prices.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 管理员明确确认的单个正式价格写入命令。
pub struct AdminModelPriceWriteCommand {
    record: ModelPriceWriteRecord,
}

impl AdminModelPriceWriteCommand {
    pub fn new(
        model: String,
        expected_version: Option<u64>,
        billing_mode: AdminModelPriceBillingMode,
        prices: [Decimal; 5],
    ) -> Result<Self, AdminModelPriceError> {
        Self::new_with_expression(model, expected_version, billing_mode, prices, None)
    }

    /// 校验并构造包含可选表达式正文的管理员写入命令。
    pub fn new_with_expression(
        model: String,
        expected_version: Option<u64>,
        billing_mode: AdminModelPriceBillingMode,
        prices: [Decimal; 5],
        billing_expression: Option<String>,
    ) -> Result<Self, AdminModelPriceError> {
        Self::new_with_metadata(
            model,
            expected_version,
            None,
            billing_mode,
            prices,
            billing_expression,
        )
    }

    /// 校验并构造包含可选模型上下文长度的正式价格写入命令。
    pub fn new_with_metadata(
        model: String,
        expected_version: Option<u64>,
        context_window: Option<i64>,
        billing_mode: AdminModelPriceBillingMode,
        prices: [Decimal; 5],
        billing_expression: Option<String>,
    ) -> Result<Self, AdminModelPriceError> {
        if billing_mode == AdminModelPriceBillingMode::Expression {
            let source = billing_expression
                .as_deref()
                .ok_or(AdminModelPriceError::InvalidInput)?;
            // 管理边界先编译校验，避免把无法加载的表达式写入正式目录。
            BillingExpressionDefinition::new(source.to_owned())
                .map_err(|_| AdminModelPriceError::InvalidInput)?;
        } else if billing_expression.is_some() {
            return Err(AdminModelPriceError::InvalidInput);
        }
        let record = ModelPriceWriteRecord::new_with_metadata(
            model,
            expected_version,
            context_window,
            billing_mode.into(),
            prices,
            billing_expression,
        )
        .map_err(map_write_error)?;
        Ok(Self { record })
    }

    fn into_record(self) -> ModelPriceWriteRecord {
        self.record
    }
}

impl fmt::Debug for AdminModelPriceWriteCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelPriceWriteCommand(<已脱敏>)")
    }
}

/// 一次原子应用的明确价格集合。
pub struct AdminModelPriceApplyCommand {
    items: Vec<AdminModelPriceWriteCommand>,
}

impl AdminModelPriceApplyCommand {
    pub fn new(items: Vec<AdminModelPriceWriteCommand>) -> Result<Self, AdminModelPriceError> {
        if items.is_empty() || items.len() > MAX_MODEL_PRICE_WRITE_BATCH {
            return Err(AdminModelPriceError::InvalidInput);
        }
        let unique = items
            .iter()
            .map(|item| item.record.model())
            .collect::<BTreeSet<_>>();
        if unique.len() != items.len() {
            return Err(AdminModelPriceError::InvalidInput);
        }
        Ok(Self { items })
    }
}

impl fmt::Debug for AdminModelPriceApplyCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelPriceApplyCommand")
            .field("item_count", &self.items.len())
            .finish()
    }
}

/// 管理试算使用的上游输入用量口径。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelPriceUsageSemantics {
    /// 输入总量已经包含缓存明细。
    Inclusive,
    /// 缓存明细独立于输入总量。
    CacheSeparated,
}

impl From<AdminModelPriceUsageSemantics> for BillingExpressionUsageSemantics {
    fn from(value: AdminModelPriceUsageSemantics) -> Self {
        match value {
            AdminModelPriceUsageSemantics::Inclusive => Self::Inclusive,
            AdminModelPriceUsageSemantics::CacheSeparated => Self::CacheSeparated,
        }
    }
}

/// 已校验的表达式试算用量，不保存客户端原始 JSON。
pub struct AdminModelPriceExpressionUsage(BillingExpressionUsage);

impl AdminModelPriceExpressionUsage {
    /// 构造五类 Token 用量；试算暂不接受推理或音频明细。
    pub fn new(
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_creation_5m_tokens: i64,
        cache_creation_1h_tokens: i64,
        semantics: AdminModelPriceUsageSemantics,
    ) -> Result<Self, AdminModelPriceError> {
        if [
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_5m_tokens,
            cache_creation_1h_tokens,
        ]
        .into_iter()
        .any(|value| !(0..=MAX_MODEL_PRICE_EXPRESSION_PREVIEW_INTEGER).contains(&value))
        {
            return Err(AdminModelPriceError::ExpressionPreviewInvalid);
        }
        let usage = BillingExpressionUsage::new(
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_5m_tokens,
            cache_creation_1h_tokens,
            semantics.into(),
        )
        .map_err(|_| AdminModelPriceError::ExpressionPreviewInvalid)?;
        if usage.context_length_tokens() > MAX_MODEL_PRICE_EXPRESSION_PREVIEW_INTEGER {
            return Err(AdminModelPriceError::ExpressionPreviewInvalid);
        }
        Ok(Self(usage))
    }
}

impl fmt::Debug for AdminModelPriceExpressionUsage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelPriceExpressionUsage(<已校验>)")
    }
}

/// 已校验的表达式试算倍率快照，单位均为百万分之一。
pub struct AdminModelPriceExpressionRatios(PricingRatios);

impl AdminModelPriceExpressionRatios {
    pub fn new(ratio_micros: [i64; 3]) -> Result<Self, AdminModelPriceError> {
        if ratio_micros
            .into_iter()
            .any(|value| !(0..=MAX_MODEL_PRICE_EXPRESSION_PREVIEW_INTEGER).contains(&value))
        {
            return Err(AdminModelPriceError::ExpressionPreviewInvalid);
        }
        let [group, group_model, peak] = ratio_micros
            .map(|value| PricingRatio::new(value).expect("非负安全整数必须是有效百万分倍率"));
        Ok(Self(PricingRatios::new(group, group_model, peak)))
    }
}

impl fmt::Debug for AdminModelPriceExpressionRatios {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelPriceExpressionRatios(<已校验>)")
    }
}

/// 未保存表达式的一次管理员试算命令。
pub struct AdminModelPriceExpressionPreviewCommand {
    definition: BillingExpressionDefinition,
    usage: AdminModelPriceExpressionUsage,
    ratios: AdminModelPriceExpressionRatios,
}

impl AdminModelPriceExpressionPreviewCommand {
    pub fn new(
        source: String,
        usage: AdminModelPriceExpressionUsage,
        ratios: AdminModelPriceExpressionRatios,
    ) -> Result<Self, AdminModelPriceError> {
        let definition = BillingExpressionDefinition::new(source)
            .map_err(|_| AdminModelPriceError::ExpressionInvalid)?;
        Ok(Self {
            definition,
            usage,
            ratios,
        })
    }
}

impl fmt::Debug for AdminModelPriceExpressionPreviewCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelPriceExpressionPreviewCommand(<已脱敏>)")
    }
}

/// 服务端复用生产表达式链路得到的精确试算结果。
pub struct AdminModelPriceExpressionPreview(BillingExpressionResult);

impl AdminModelPriceExpressionPreview {
    #[must_use]
    pub fn matched_tier(&self) -> &str {
        self.0.matched_tier()
    }

    #[must_use]
    pub const fn variables(&self) -> BillingExpressionVariables {
        self.0.variables()
    }

    #[must_use]
    pub const fn base_usd(&self) -> Decimal {
        self.0.base_usd()
    }

    #[must_use]
    pub const fn total_usd(&self) -> Decimal {
        self.0.total_usd()
    }

    #[must_use]
    pub const fn quota_units(&self) -> i64 {
        self.0.quota().units()
    }
}

impl fmt::Debug for AdminModelPriceExpressionPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelPriceExpressionPreview")
            .field("matched_tier", &self.matched_tier())
            .finish_non_exhaustive()
    }
}

/// 请求公开价格源时允许用于精确匹配的权威模型身份。
pub struct ModelPriceSourceTarget {
    model: String,
    provider: String,
}

impl ModelPriceSourceTarget {
    pub fn new(model: String, provider: String) -> Result<Self, ModelPriceSourceDiscoveryError> {
        if !valid_model(&model) || !valid_text(&provider, MAX_ADMIN_MODEL_PROVIDER_BYTES) {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        Ok(Self { model, provider })
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
}

impl fmt::Debug for ModelPriceSourceTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelPriceSourceTarget(<已脱敏>)")
    }
}

/// 公开来源中与本地权威身份精确匹配的一项价格证据。
pub struct ModelPriceSourceCandidate {
    model: String,
    provider: String,
    provider_name: String,
    source_model: String,
    source_name: String,
    last_updated: Option<String>,
    context_window: Option<i64>,
    costs: [Option<Decimal>; 4],
    cache_creation_prices: [Option<Decimal>; 2],
    source_deprecation_date: Option<String>,
    has_tiered_pricing: bool,
    source_deprecated: bool,
}

impl ModelPriceSourceCandidate {
    #[allow(clippy::too_many_arguments, reason = "字段与公开价格证据一一对应")]
    pub fn new(
        model: String,
        provider: String,
        provider_name: String,
        source_model: String,
        source_name: String,
        last_updated: Option<String>,
        context_window: Option<i64>,
        costs: [Option<Decimal>; 4],
        has_tiered_pricing: bool,
        source_deprecated: bool,
    ) -> Result<Self, ModelPriceSourceDiscoveryError> {
        if !valid_model(&model)
            || !valid_text(&provider, MAX_ADMIN_MODEL_PROVIDER_BYTES)
            || !valid_text(&provider_name, MAX_SOURCE_TEXT_BYTES)
            || !valid_model(&source_model)
            || !valid_text(&source_name, MAX_SOURCE_TEXT_BYTES)
            || last_updated
                .as_deref()
                .is_some_and(|value| !valid_text(value, 32))
            || context_window
                .is_some_and(|value| !(1..=af_db::MAX_ADMIN_MODEL_CONTEXT_WINDOW).contains(&value))
            || costs.iter().flatten().any(|cost| *cost < Decimal::ZERO)
        {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        Ok(Self {
            model,
            provider,
            provider_name,
            source_model,
            source_name,
            last_updated,
            context_window,
            costs,
            cache_creation_prices: [None; 2],
            source_deprecation_date: None,
            has_tiered_pricing,
            source_deprecated,
        })
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    #[must_use]
    pub fn source_model(&self) -> &str {
        &self.source_model
    }

    #[must_use]
    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    #[must_use]
    pub fn last_updated(&self) -> Option<&str> {
        self.last_updated.as_deref()
    }

    /// 返回来源明确声明的最大上下文长度。
    #[must_use]
    pub const fn context_window(&self) -> Option<i64> {
        self.context_window
    }

    /// 返回输入、输出、缓存读取和通用缓存写公开成本。
    #[must_use]
    pub const fn costs(&self) -> [Option<Decimal>; 4] {
        self.costs
    }

    /// 返回 LiteLLM 明确提供的 5 分钟与 1 小时缓存创建单价。
    #[must_use]
    pub const fn cache_creation_prices(&self) -> [Option<Decimal>; 2] {
        self.cache_creation_prices
    }

    /// 返回来源声明的弃用日期；它只用于人工复核，不自动改变正式价格。
    #[must_use]
    pub fn source_deprecation_date(&self) -> Option<&str> {
        self.source_deprecation_date.as_deref()
    }

    /// 补充来源的两类缓存创建价格证据。
    pub fn with_cache_creation_prices(
        mut self,
        prices: [Option<Decimal>; 2],
    ) -> Result<Self, ModelPriceSourceDiscoveryError> {
        if prices.iter().flatten().any(|price| *price < Decimal::ZERO) {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        self.cache_creation_prices = prices;
        Ok(self)
    }

    /// 补充来源声明的弃用日期，并校验其为短日期文本。
    pub fn with_source_deprecation_date(
        mut self,
        date: Option<String>,
    ) -> Result<Self, ModelPriceSourceDiscoveryError> {
        if date.as_deref().is_some_and(|value| !valid_date_text(value)) {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        self.source_deprecation_date = date;
        Ok(self)
    }

    #[must_use]
    pub const fn has_tiered_pricing(&self) -> bool {
        self.has_tiered_pricing
    }

    #[must_use]
    pub const fn source_deprecated(&self) -> bool {
        self.source_deprecated
    }
}

impl fmt::Debug for ModelPriceSourceCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceSourceCandidate")
            .field("has_input", &self.costs[0].is_some())
            .field("has_context_window", &self.context_window.is_some())
            .field("has_output", &self.costs[1].is_some())
            .field("has_cache_read", &self.costs[2].is_some())
            .field("has_cache_write", &self.costs[3].is_some())
            .field(
                "has_cache_creation_5m",
                &self.cache_creation_prices[0].is_some(),
            )
            .field(
                "has_cache_creation_1h",
                &self.cache_creation_prices[1].is_some(),
            )
            .field("has_tiered_pricing", &self.has_tiered_pricing)
            .field("source_deprecated", &self.source_deprecated)
            .finish_non_exhaustive()
    }
}

/// 一次受控公开价格源读取的固定响应事实。
pub struct ModelPriceSourcePreview {
    source: &'static str,
    fetched_at: i64,
    revision: Option<String>,
    candidates: Vec<ModelPriceSourceCandidate>,
}

impl ModelPriceSourcePreview {
    pub fn new(
        source: &'static str,
        fetched_at: i64,
        revision: Option<String>,
        candidates: Vec<ModelPriceSourceCandidate>,
    ) -> Result<Self, ModelPriceSourceDiscoveryError> {
        if !matches!(source, "models_dev" | "litellm")
            || fetched_at <= 0
            || revision
                .as_deref()
                .is_some_and(|value| !valid_text(value, MAX_SOURCE_REVISION_BYTES))
            || candidates.len() > MAX_MODEL_PRICE_SOURCE_TARGETS
        {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        Ok(Self {
            source,
            fetched_at,
            revision,
            candidates,
        })
    }

    #[must_use]
    pub const fn source(&self) -> &'static str {
        self.source
    }

    #[must_use]
    pub const fn fetched_at(&self) -> i64 {
        self.fetched_at
    }

    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }

    #[must_use]
    pub fn candidates(&self) -> &[ModelPriceSourceCandidate] {
        &self.candidates
    }
}

impl fmt::Debug for ModelPriceSourcePreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceSourcePreview")
            .field("source", &self.source)
            .field("candidate_count", &self.candidates.len())
            .field("has_revision", &self.revision.is_some())
            .finish()
    }
}

/// 公开价格来源失败分类，不携带 URL、正文或模型成本。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ModelPriceSourceDiscoveryError {
    #[error("公开价格源请求超时")]
    Timeout,
    #[error("公开价格源当前不可用")]
    Unavailable,
    #[error("公开价格源响应超过容量上限")]
    ResponseTooLarge,
    #[error("公开价格源响应无效")]
    InvalidResponse,
    #[error("公开价格源候选超过容量上限")]
    CandidateLimitExceeded,
}

pub type ModelPriceSourceDiscoveryFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ModelPriceSourcePreview, ModelPriceSourceDiscoveryError>>
            + Send
            + 'a,
    >,
>;

/// 只负责固定公开来源读取和精确身份匹配的端口。
pub trait ModelPriceSourceDiscoverer: Send + Sync {
    fn discover<'a>(
        &'a self,
        source: ModelPriceSourceKind,
        targets: Vec<ModelPriceSourceTarget>,
    ) -> ModelPriceSourceDiscoveryFuture<'a>;
}

/// 正式价格写入后刷新运行时不可变快照的稳定错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("运行时模型价格快照刷新失败")]
pub struct ModelPriceRuntimeRefreshError;

pub type ModelPriceRuntimeRefreshFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), ModelPriceRuntimeRefreshError>> + Send + 'a>>;

/// 隔离管理应用域和具体计费缓存实现的刷新端口。
pub trait ModelPriceRuntimeRefresher: Send + Sync {
    fn refresh<'a>(&'a self) -> ModelPriceRuntimeRefreshFuture<'a>;
}

/// 模型价格管理用例的稳定失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminModelPriceError {
    #[error("模型价格管理请求参数无效")]
    InvalidInput,
    #[error("模型价格管理权限不足")]
    Forbidden,
    #[error("模型价格对应的模型不存在")]
    ModelNotFound,
    #[error("模型价格发生并发冲突")]
    Conflict,
    #[error("模型价格表达式无效")]
    ExpressionInvalid,
    #[error("模型价格表达式试算输入无效")]
    ExpressionPreviewInvalid,
    #[error("模型价格表达式试算执行失败")]
    ExpressionEvaluationFailed,
    #[error("公开价格源请求超时")]
    SourceTimeout,
    #[error("公开价格源不可用")]
    SourceUnavailable,
    #[error("公开价格源响应超过容量上限")]
    SourceResponseTooLarge,
    #[error("公开价格源响应无效")]
    SourceInvalidResponse,
    #[error("公开价格源候选超过容量上限")]
    SourceCandidateLimitExceeded,
    #[error("运行时模型价格刷新失败")]
    RuntimeRefreshFailed,
    #[error("模型价格管理内部失败")]
    Internal,
}

pub type AdminModelPriceListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModelPricePage, AdminModelPriceError>> + Send + 'a>>;
pub type AdminModelPricePreviewFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ModelPriceSourcePreview, AdminModelPriceError>> + Send + 'a>,
>;
pub type AdminModelPriceWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<AdminModelPrice>, AdminModelPriceError>> + Send + 'a>>;
pub type AdminModelPriceExpressionPreviewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminModelPriceExpressionPreview, AdminModelPriceError>>
            + Send
            + 'a,
    >,
>;

/// 管理员价格读取、公开参考价预览和原子应用端口。
pub trait AdminModelPriceService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminModelPriceListQuery,
    ) -> AdminModelPriceListFuture<'a>;

    fn preview<'a>(
        &'a self,
        principal: SessionPrincipal,
        source: ModelPriceSourceKind,
    ) -> AdminModelPricePreviewFuture<'a>;

    fn preview_expression<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelPriceExpressionPreviewCommand,
    ) -> AdminModelPriceExpressionPreviewFuture<'a>;

    fn apply<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelPriceApplyCommand,
    ) -> AdminModelPriceWriteFuture<'a>;
}

/// 使用数据库仓储、受控公开来源与运行时刷新端口实现价格管理。
pub struct DatabaseAdminModelPriceService {
    prices: ModelPriceRepository,
    models: AdminModelRepository,
    source: Arc<dyn ModelPriceSourceDiscoverer>,
    refresher: Arc<dyn ModelPriceRuntimeRefresher>,
}

impl DatabaseAdminModelPriceService {
    #[must_use]
    pub fn new(
        prices: ModelPriceRepository,
        models: AdminModelRepository,
        source: Arc<dyn ModelPriceSourceDiscoverer>,
        refresher: Arc<dyn ModelPriceRuntimeRefresher>,
    ) -> Self {
        Self {
            prices,
            models,
            source,
            refresher,
        }
    }
}

impl AdminModelPriceService for DatabaseAdminModelPriceService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminModelPriceListQuery,
    ) -> AdminModelPriceListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .prices
                .list_page(query.after(), query.limit())
                .await
                .map_err(map_write_error)?;
            let (prices, next_cursor) = page.into_parts();
            Ok(AdminModelPricePage {
                prices: prices
                    .into_iter()
                    .map(AdminModelPrice::from_record)
                    .collect(),
                next_cursor,
            })
        })
    }

    fn preview<'a>(
        &'a self,
        principal: SessionPrincipal,
        source: ModelPriceSourceKind,
    ) -> AdminModelPricePreviewFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let targets = self.load_source_targets().await?;
            self.source
                .discover(source, targets)
                .await
                .map_err(map_source_error)
        })
    }

    fn preview_expression<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelPriceExpressionPreviewCommand,
    ) -> AdminModelPriceExpressionPreviewFuture<'a> {
        Box::pin(async move { evaluate_expression_preview(principal, command) })
    }

    fn apply<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelPriceApplyCommand,
    ) -> AdminModelPriceWriteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let records = self
                .prices
                .apply_batch(
                    command
                        .items
                        .into_iter()
                        .map(AdminModelPriceWriteCommand::into_record)
                        .collect(),
                )
                .await
                .map_err(map_write_error)?;
            self.refresher
                .refresh()
                .await
                .map_err(|_| AdminModelPriceError::RuntimeRefreshFailed)?;
            Ok(records
                .into_iter()
                .map(AdminModelPrice::from_record)
                .collect())
        })
    }
}

fn evaluate_expression_preview(
    principal: SessionPrincipal,
    command: AdminModelPriceExpressionPreviewCommand,
) -> Result<AdminModelPriceExpressionPreview, AdminModelPriceError> {
    require_admin(principal)?;
    let expression = command
        .definition
        .compile(command.ratios.0)
        .map_err(|_| AdminModelPriceError::ExpressionEvaluationFailed)?;
    expression
        .evaluate_prepared_usage(&command.usage.0)
        .map(AdminModelPriceExpressionPreview)
        .map_err(|_| AdminModelPriceError::ExpressionEvaluationFailed)
}

impl DatabaseAdminModelPriceService {
    async fn load_source_targets(
        &self,
    ) -> Result<Vec<ModelPriceSourceTarget>, AdminModelPriceError> {
        let mut targets = Vec::new();
        let mut after: Option<ModelId> = None;
        loop {
            let page = self
                .models
                .list(after, MAX_ADMIN_MODEL_PAGE_SIZE)
                .await
                .map_err(|_| AdminModelPriceError::Internal)?;
            let (models, next_cursor) = page.into_parts();
            for model in models {
                targets.push(
                    ModelPriceSourceTarget::new(
                        model.model().to_owned(),
                        model.provider().to_owned(),
                    )
                    .map_err(map_source_error)?,
                );
                if targets.len() > MAX_MODEL_PRICE_SOURCE_TARGETS {
                    return Err(AdminModelPriceError::SourceCandidateLimitExceeded);
                }
            }
            let Some(next_cursor) = next_cursor else {
                return Ok(targets);
            };
            if after.is_some_and(|current| next_cursor.get() <= current.get()) {
                return Err(AdminModelPriceError::Internal);
            }
            after = Some(next_cursor);
        }
    }
}

impl fmt::Debug for DatabaseAdminModelPriceService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminModelPriceService(<受控>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminModelPriceError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminModelPriceError::Forbidden)
    }
}

fn map_write_error(error: ModelPriceWriteError) -> AdminModelPriceError {
    match error {
        ModelPriceWriteError::InvalidInput => AdminModelPriceError::InvalidInput,
        ModelPriceWriteError::ModelNotFound => AdminModelPriceError::ModelNotFound,
        ModelPriceWriteError::Conflict => AdminModelPriceError::Conflict,
        ModelPriceWriteError::Query
        | ModelPriceWriteError::Timeout
        | ModelPriceWriteError::Invariant => AdminModelPriceError::Internal,
    }
}

fn map_source_error(error: ModelPriceSourceDiscoveryError) -> AdminModelPriceError {
    match error {
        ModelPriceSourceDiscoveryError::Timeout => AdminModelPriceError::SourceTimeout,
        ModelPriceSourceDiscoveryError::Unavailable => AdminModelPriceError::SourceUnavailable,
        ModelPriceSourceDiscoveryError::ResponseTooLarge => {
            AdminModelPriceError::SourceResponseTooLarge
        }
        ModelPriceSourceDiscoveryError::InvalidResponse => {
            AdminModelPriceError::SourceInvalidResponse
        }
        ModelPriceSourceDiscoveryError::CandidateLimitExceeded => {
            AdminModelPriceError::SourceCandidateLimitExceeded
        }
    }
}

fn valid_model(value: &str) -> bool {
    valid_text(value, MAX_MODEL_NAME_BYTES)
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_date_text(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .as_bytes()
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use af_domain::UserId;

    use super::*;

    #[test]
    fn write_commands_reject_duplicate_models_and_free_nonzero_prices() {
        assert!(matches!(
            AdminModelPriceWriteCommand::new(
                "gpt-test".to_owned(),
                None,
                AdminModelPriceBillingMode::Free,
                [Decimal::ONE; 5],
            ),
            Err(AdminModelPriceError::InvalidInput)
        ));
        let command = || {
            AdminModelPriceWriteCommand::new(
                "gpt-test".to_owned(),
                None,
                AdminModelPriceBillingMode::PerToken,
                [Decimal::ONE; 5],
            )
            .unwrap()
        };
        assert!(matches!(
            AdminModelPriceApplyCommand::new(vec![command(), command()]),
            Err(AdminModelPriceError::InvalidInput)
        ));
    }

    #[test]
    fn expression_write_commands_require_valid_source_and_zero_prices() {
        let valid = AdminModelPriceWriteCommand::new_with_expression(
            "gpt-test".to_owned(),
            None,
            AdminModelPriceBillingMode::Expression,
            [Decimal::ZERO; 5],
            Some("tier(\"base\", p)".to_owned()),
        );
        assert!(valid.is_ok());

        for (prices, source) in [
            ([Decimal::ONE; 5], Some("tier(\"base\", p)".to_owned())),
            ([Decimal::ZERO; 5], None),
            ([Decimal::ZERO; 5], Some("not valid".to_owned())),
        ] {
            assert!(matches!(
                AdminModelPriceWriteCommand::new_with_expression(
                    "gpt-test".to_owned(),
                    None,
                    AdminModelPriceBillingMode::Expression,
                    prices,
                    source,
                ),
                Err(AdminModelPriceError::InvalidInput)
            ));
        }

        for billing_mode in [
            AdminModelPriceBillingMode::PerToken,
            AdminModelPriceBillingMode::Free,
        ] {
            assert!(matches!(
                AdminModelPriceWriteCommand::new_with_expression(
                    "gpt-test".to_owned(),
                    None,
                    billing_mode,
                    [Decimal::ZERO; 5],
                    Some("tier(\"base\", p)".to_owned()),
                ),
                Err(AdminModelPriceError::InvalidInput)
            ));
        }
    }

    #[test]
    fn normal_user_cannot_enter_price_management() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminModelPriceError::Forbidden)
        );
    }

    #[test]
    fn expression_preview_reuses_normalized_usage_and_exact_ratio_math() {
        let usage = AdminModelPriceExpressionUsage::new(
            1_000,
            500,
            200,
            100,
            50,
            AdminModelPriceUsageSemantics::Inclusive,
        )
        .unwrap();
        let ratios = AdminModelPriceExpressionRatios::new([1_500_000, 800_000, 2_000_000]).unwrap();
        let command = AdminModelPriceExpressionPreviewCommand::new(
            r#"v1:tier("base", p * 2 + c * 10 + cr * 0.5)"#.to_owned(),
            usage,
            ratios,
        )
        .unwrap();
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        let preview = evaluate_expression_preview(principal, command).unwrap();

        assert_eq!(preview.matched_tier(), "base");
        assert_eq!(preview.variables().input_tokens(), 800);
        assert_eq!(preview.variables().context_length_tokens(), 1_000);
        assert_eq!(preview.base_usd(), Decimal::new(67, 4));
        assert_eq!(preview.total_usd(), Decimal::new(1608, 5));
        assert_eq!(preview.quota_units(), 8_040);
    }

    #[test]
    fn expression_preview_rejects_invalid_source_usage_and_runtime_result() {
        assert_eq!(
            AdminModelPriceExpressionUsage::new(
                10,
                0,
                11,
                0,
                0,
                AdminModelPriceUsageSemantics::Inclusive,
            )
            .unwrap_err(),
            AdminModelPriceError::ExpressionPreviewInvalid
        );
        assert_eq!(
            AdminModelPriceExpressionUsage::new(
                MAX_MODEL_PRICE_EXPRESSION_PREVIEW_INTEGER,
                0,
                1,
                0,
                0,
                AdminModelPriceUsageSemantics::CacheSeparated,
            )
            .unwrap_err(),
            AdminModelPriceError::ExpressionPreviewInvalid
        );
        let usage = || {
            AdminModelPriceExpressionUsage::new(
                10,
                0,
                0,
                0,
                0,
                AdminModelPriceUsageSemantics::Inclusive,
            )
            .unwrap()
        };
        let ratios = || AdminModelPriceExpressionRatios::new([1_000_000; 3]).unwrap();
        assert_eq!(
            AdminModelPriceExpressionPreviewCommand::new(
                "not valid".to_owned(),
                usage(),
                ratios(),
            )
            .unwrap_err(),
            AdminModelPriceError::ExpressionInvalid
        );
        let command = AdminModelPriceExpressionPreviewCommand::new(
            r#"tier("negative", -p)"#.to_owned(),
            usage(),
            ratios(),
        )
        .unwrap();
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        assert_eq!(
            evaluate_expression_preview(principal, command).unwrap_err(),
            AdminModelPriceError::ExpressionEvaluationFailed
        );
    }
}
