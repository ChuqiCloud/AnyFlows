use std::{fmt, future::Future, pin::Pin};

use af_db::{
    MAX_ADMIN_MODEL_CONTEXT_WINDOW, MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
    MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES, MAX_ADMIN_MODEL_PROVIDER_BYTES,
};
use af_domain::{MAX_MODEL_NAME_BYTES, Protocol};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AdminModelModalities, SessionAuthentication,
    model_metadata_write::{
        validate_optional_icon_url, validate_optional_text, validate_tags, validate_text,
    },
};

/// 模型目录默认每页条数。
pub const DEFAULT_MODEL_CATALOG_PAGE_SIZE: usize = 24;
/// 模型目录单页硬上限，避免一次响应占用过多内存。
pub const MAX_MODEL_CATALOG_PAGE_SIZE: usize = 100;
const MAX_MODEL_CATALOG_SEARCH_BYTES: usize = 128;

/// 模型目录公开的闭合计费模式。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCatalogBillingMode {
    /// 按五类互斥 token 分项计费。
    PerToken,
    /// 显式免费，仍记录用量。
    Free,
}

/// 模型目录可筛选的闭合输入或输出模态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCatalogModality {
    /// 文本内容。
    Text,
    /// 图像内容。
    Image,
    /// 音频内容。
    Audio,
    /// 视频内容。
    Video,
}

/// 模型目录可筛选的闭合扩展能力。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCatalogCapability {
    /// 模型声明支持推理。
    Reasoning,
    /// 模型声明支持工具调用。
    ToolCalls,
    /// 当前运行时支持 OpenAI Responses Compact。
    ResponsesCompact,
}

/// 模型目录价格所处的公开范围。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCatalogPricingScope {
    /// 游客可见的公开基础价，不承诺模型当前可调用。
    PublicBase,
    /// 当前登录用户默认分组的实时可用模型与实际价格。
    Group,
}

/// 已校验的模型目录搜索与稳定游标参数。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelCatalogQuery {
    search: Option<String>,
    billing_mode: Option<ModelCatalogBillingMode>,
    providers: Vec<String>,
    input_modalities: Vec<ModelCatalogModality>,
    output_modalities: Vec<ModelCatalogModality>,
    capabilities: Vec<ModelCatalogCapability>,
    protocols: Vec<Protocol>,
    after: Option<String>,
    limit: usize,
}

impl ModelCatalogQuery {
    /// 校验搜索文本、闭合筛选集合、模型名游标和单页容量。
    #[allow(
        clippy::too_many_arguments,
        reason = "参数与稳定模型目录查询字段一一对应"
    )]
    pub fn new(
        search: Option<String>,
        billing_mode: Option<ModelCatalogBillingMode>,
        providers: Vec<String>,
        input_modalities: Vec<ModelCatalogModality>,
        output_modalities: Vec<ModelCatalogModality>,
        capabilities: Vec<ModelCatalogCapability>,
        protocols: Vec<Protocol>,
        after: Option<String>,
        limit: usize,
    ) -> Result<Self, ModelCatalogReadError> {
        if !(1..=MAX_MODEL_CATALOG_PAGE_SIZE).contains(&limit)
            || search.as_deref().is_some_and(|value| {
                value.is_empty()
                    || value.len() > MAX_MODEL_CATALOG_SEARCH_BYTES
                    || value.trim() != value
                    || value.chars().any(char::is_control)
            })
            || after
                .as_deref()
                .is_some_and(|value| !is_valid_model_name(value))
            || !is_sorted_unique(&providers)
            || providers.iter().any(|value| !is_valid_provider(value))
            || !is_sorted_unique(&input_modalities)
            || !is_sorted_unique(&output_modalities)
            || !is_sorted_unique(&capabilities)
            || !is_sorted_unique(&protocols)
        {
            return Err(ModelCatalogReadError::InvalidQuery);
        }
        Ok(Self {
            search,
            billing_mode,
            providers,
            input_modalities,
            output_modalities,
            capabilities,
            protocols,
            after,
            limit,
        })
    }

    /// 返回可选的模型名包含搜索文本。
    #[must_use]
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    /// 返回可选的计费模式筛选。
    #[must_use]
    pub const fn billing_mode(&self) -> Option<ModelCatalogBillingMode> {
        self.billing_mode
    }

    /// 返回权威供应商标识筛选集合；集合内任一供应商匹配即可。
    #[must_use]
    pub fn providers(&self) -> &[String] {
        &self.providers
    }

    /// 返回输入模态筛选集合；集合内任一模态匹配即可。
    #[must_use]
    pub fn input_modalities(&self) -> &[ModelCatalogModality] {
        &self.input_modalities
    }

    /// 返回输出模态筛选集合；集合内任一模态匹配即可。
    #[must_use]
    pub fn output_modalities(&self) -> &[ModelCatalogModality] {
        &self.output_modalities
    }

    /// 返回扩展能力筛选集合；模型必须满足集合内全部能力。
    #[must_use]
    pub fn capabilities(&self) -> &[ModelCatalogCapability] {
        &self.capabilities
    }

    /// 返回运行时协议筛选集合；集合内任一协议匹配即可。
    #[must_use]
    pub fn protocols(&self) -> &[Protocol] {
        &self.protocols
    }

    /// 返回上一页最后一个模型名。
    #[must_use]
    pub fn after(&self) -> Option<&str> {
        self.after.as_deref()
    }

    /// 返回本页最大条数。
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for ModelCatalogQuery {
    fn default() -> Self {
        Self {
            search: None,
            billing_mode: None,
            providers: Vec::new(),
            input_modalities: Vec::new(),
            output_modalities: Vec::new(),
            capabilities: Vec::new(),
            protocols: Vec::new(),
            after: None,
            limit: DEFAULT_MODEL_CATALOG_PAGE_SIZE,
        }
    }
}

impl fmt::Debug for ModelCatalogQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCatalogQuery")
            .field("has_search", &self.search.is_some())
            .field("billing_mode", &self.billing_mode)
            .field("provider_count", &self.providers.len())
            .field("input_modality_count", &self.input_modalities.len())
            .field("output_modality_count", &self.output_modalities.len())
            .field("capability_count", &self.capabilities.len())
            .field("protocol_count", &self.protocols.len())
            .field("has_after", &self.after.is_some())
            .field("limit", &self.limit)
            .finish()
    }
}

/// 应用当前价格范围倍率后的五类美元/百万 token 单价。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelCatalogTokenPrices {
    values: [String; 5],
}

impl ModelCatalogTokenPrices {
    /// 将 checked Decimal 结果转换为不会经过浮点数的规范十进制文本。
    pub fn from_decimals(values: [Decimal; 5]) -> Result<Self, ModelCatalogReadError> {
        if values.into_iter().any(|value| value < Decimal::ZERO) {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self {
            values: values.map(|value| value.normalize().to_string()),
        })
    }

    /// 返回普通输入单价。
    #[must_use]
    pub fn input(&self) -> &str {
        &self.values[0]
    }

    /// 返回输出单价。
    #[must_use]
    pub fn output(&self) -> &str {
        &self.values[1]
    }

    /// 返回缓存读取单价。
    #[must_use]
    pub fn cache_read(&self) -> &str {
        &self.values[2]
    }

    /// 返回五分钟缓存创建单价。
    #[must_use]
    pub fn cache_creation_5m(&self) -> &str {
        &self.values[3]
    }

    /// 返回一小时缓存创建单价。
    #[must_use]
    pub fn cache_creation_1h(&self) -> &str {
        &self.values[4]
    }
}

impl fmt::Debug for ModelCatalogTokenPrices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelCatalogTokenPrices(<已脱敏>)")
    }
}

/// 一条目录价格实际应用的三层百万分比定点倍率。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ModelCatalogRatios {
    group_micros: i64,
    group_model_micros: i64,
    peak_micros: i64,
}

impl ModelCatalogRatios {
    /// 构造非负的分组、分组间与当前高峰倍率。
    pub const fn new(
        group_micros: i64,
        group_model_micros: i64,
        peak_micros: i64,
    ) -> Result<Self, ModelCatalogReadError> {
        if group_micros < 0 || group_model_micros < 0 || peak_micros < 0 {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self {
            group_micros,
            group_model_micros,
            peak_micros,
        })
    }

    /// 返回目标分组基础倍率。
    #[must_use]
    pub const fn group_micros(self) -> i64 {
        self.group_micros
    }

    /// 返回来源分组到目标分组的附加倍率。
    #[must_use]
    pub const fn group_model_micros(self) -> i64 {
        self.group_model_micros
    }

    /// 返回当前时刻已应用的高峰倍率。
    #[must_use]
    pub const fn peak_micros(self) -> i64 {
        self.peak_micros
    }
}

impl fmt::Debug for ModelCatalogRatios {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelCatalogRatios(<已脱敏>)")
    }
}

/// 模型广场允许公开展示的闭合商品生命周期。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCatalogLifecycle {
    /// 正常提供的活动模型。
    Active,
    /// 仍可调用但应引导迁移的弃用模型。
    Deprecated,
}

/// 模型广场公开的闭合运行时状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCatalogRuntimeStatus {
    /// 游客目录不探测或承诺当前运行时候选。
    NotEvaluated,
    /// 当前登录用户默认分组存在运行时候选且可以完成定价。
    Available,
}

/// 一条已经通过可见性与生命周期筛选的权威商品元数据。
pub struct ModelCatalogMetadata {
    model: String,
    display_name: String,
    provider: String,
    description: Option<String>,
    icon_url: Option<String>,
    tags: Vec<String>,
    context_window: Option<i64>,
    input_modalities: AdminModelModalities,
    output_modalities: AdminModelModalities,
    supports_reasoning: bool,
    supports_tool_calls: bool,
    lifecycle: ModelCatalogLifecycle,
}

impl ModelCatalogMetadata {
    /// 组合数据库已校验字段，并再次守卫公开目录所需的闭合边界。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与权威模型商品元数据一一对应"
    )]
    pub fn new(
        model: String,
        display_name: String,
        provider: String,
        description: Option<String>,
        icon_url: Option<String>,
        tags: Vec<String>,
        context_window: Option<i64>,
        input_modalities: AdminModelModalities,
        output_modalities: AdminModelModalities,
        supports_reasoning: bool,
        supports_tool_calls: bool,
        lifecycle: ModelCatalogLifecycle,
    ) -> Result<Self, ModelCatalogReadError> {
        if !is_valid_model_name(&model)
            || validate_text(&display_name, MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES).is_err()
            || validate_text(&provider, MAX_ADMIN_MODEL_PROVIDER_BYTES).is_err()
            || validate_optional_text(description.as_deref(), MAX_ADMIN_MODEL_DESCRIPTION_BYTES)
                .is_err()
            || validate_optional_icon_url(icon_url.as_deref()).is_err()
            || validate_tags(&tags).is_err()
            || context_window
                .is_some_and(|value| !(1..=MAX_ADMIN_MODEL_CONTEXT_WINDOW).contains(&value))
            || input_modalities.is_empty()
            || output_modalities.is_empty()
        {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self {
            model,
            display_name,
            provider,
            description,
            icon_url,
            tags,
            context_window,
            input_modalities,
            output_modalities,
            supports_reasoning,
            supports_tool_calls,
            lifecycle,
        })
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    #[must_use]
    pub fn icon_url(&self) -> Option<&str> {
        self.icon_url.as_deref()
    }

    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    #[must_use]
    pub const fn context_window(&self) -> Option<i64> {
        self.context_window
    }

    #[must_use]
    pub const fn input_modalities(&self) -> AdminModelModalities {
        self.input_modalities
    }

    #[must_use]
    pub const fn output_modalities(&self) -> AdminModelModalities {
        self.output_modalities
    }

    #[must_use]
    pub const fn supports_reasoning(&self) -> bool {
        self.supports_reasoning
    }

    #[must_use]
    pub const fn supports_tool_calls(&self) -> bool {
        self.supports_tool_calls
    }

    #[must_use]
    pub const fn lifecycle(&self) -> ModelCatalogLifecycle {
        self.lifecycle
    }
}

impl fmt::Debug for ModelCatalogMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCatalogMetadata")
            .field("tag_count", &self.tags.len())
            .field("has_context_window", &self.context_window.is_some())
            .field("lifecycle", &self.lifecycle)
            .finish_non_exhaustive()
    }
}

/// 一条已经完成可见性和价格边界校验的模型目录记录。
pub struct ModelCatalogItem {
    metadata: ModelCatalogMetadata,
    billing_mode: ModelCatalogBillingMode,
    prices: Option<ModelCatalogTokenPrices>,
    ratios: ModelCatalogRatios,
    price_version: u64,
    runtime_status: ModelCatalogRuntimeStatus,
    available_protocols: Vec<Protocol>,
    supports_responses_compact: bool,
}

impl ModelCatalogItem {
    /// 组合已经通过可见范围与定价快照校验的目录记录。
    pub fn new(
        metadata: ModelCatalogMetadata,
        billing_mode: ModelCatalogBillingMode,
        prices: Option<ModelCatalogTokenPrices>,
        ratios: ModelCatalogRatios,
        price_version: u64,
        runtime_status: ModelCatalogRuntimeStatus,
        available_protocols: Vec<Protocol>,
        supports_responses_compact: bool,
    ) -> Result<Self, ModelCatalogReadError> {
        let prices_match_mode = matches!(
            (billing_mode, prices.is_some()),
            (ModelCatalogBillingMode::PerToken, true) | (ModelCatalogBillingMode::Free, false)
        );
        let protocols_are_stable = available_protocols
            .windows(2)
            .all(|window| window[0] < window[1]);
        if !prices_match_mode || price_version == 0 || !protocols_are_stable {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self {
            metadata,
            billing_mode,
            prices,
            ratios,
            price_version,
            runtime_status,
            available_protocols,
            supports_responses_compact,
        })
    }

    /// 返回客户端请求使用的 Canonical 模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        self.metadata.model()
    }

    /// 返回经过可见性与生命周期筛选的权威商品元数据。
    #[must_use]
    pub const fn metadata(&self) -> &ModelCatalogMetadata {
        &self.metadata
    }

    /// 返回显式计费模式。
    #[must_use]
    pub const fn billing_mode(&self) -> ModelCatalogBillingMode {
        self.billing_mode
    }

    /// 返回按 token 模式的精确展示单价；免费模式固定为空。
    #[must_use]
    pub const fn prices(&self) -> Option<&ModelCatalogTokenPrices> {
        self.prices.as_ref()
    }

    /// 返回本条目实际应用的倍率快照。
    #[must_use]
    pub const fn ratios(&self) -> ModelCatalogRatios {
        self.ratios
    }

    /// 返回模型价格的持久化正版本。
    #[must_use]
    pub const fn price_version(&self) -> u64 {
        self.price_version
    }

    /// 返回当前访问主体对应的运行时状态。
    #[must_use]
    pub const fn runtime_status(&self) -> ModelCatalogRuntimeStatus {
        self.runtime_status
    }

    /// 返回当前目录访问主体可实际调度的协议集合；游客目录固定为空，表示不承诺可调用。
    #[must_use]
    pub fn available_protocols(&self) -> &[Protocol] {
        &self.available_protocols
    }

    /// 返回当前目录范围内是否存在明确具备 Responses Compact 资格的渠道。
    #[must_use]
    pub const fn supports_responses_compact(&self) -> bool {
        self.supports_responses_compact
    }
}

impl fmt::Debug for ModelCatalogItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCatalogItem")
            .field("billing_mode", &self.billing_mode)
            .field("has_prices", &self.prices.is_some())
            .field("price_version", &self.price_version)
            .field("runtime_status", &self.runtime_status)
            .field("available_protocol_count", &self.available_protocols.len())
            .field(
                "supports_responses_compact",
                &self.supports_responses_compact,
            )
            .finish_non_exhaustive()
    }
}

/// 一页按模型名稳定排序的目录结果。
pub struct ModelCatalogPage {
    pricing_scope: ModelCatalogPricingScope,
    items: Vec<ModelCatalogItem>,
    next_cursor: Option<String>,
}

/// 当前目录访问范围内的一项供应商聚合。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelCatalogProvider {
    name: String,
    model_count: u64,
}

impl ModelCatalogProvider {
    /// 构造经过权威目录扫描得到的非空供应商与正模型数量。
    pub fn new(name: String, model_count: u64) -> Result<Self, ModelCatalogReadError> {
        if !is_valid_provider(&name) || model_count == 0 {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self { name, model_count })
    }

    /// 返回模型元数据中的权威供应商标识。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回当前目录访问范围内属于该供应商的模型数量。
    #[must_use]
    pub const fn model_count(&self) -> u64 {
        self.model_count
    }
}

impl fmt::Debug for ModelCatalogProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCatalogProvider")
            .field("model_count", &self.model_count)
            .finish_non_exhaustive()
    }
}

/// 当前访问主体完整可见范围内的供应商聚合。
pub struct ModelCatalogProviderSummary {
    pricing_scope: ModelCatalogPricingScope,
    providers: Vec<ModelCatalogProvider>,
}

impl ModelCatalogProviderSummary {
    /// 组合权威价格范围和按供应商标识稳定排序的聚合项。
    pub fn new(
        pricing_scope: ModelCatalogPricingScope,
        providers: Vec<ModelCatalogProvider>,
    ) -> Result<Self, ModelCatalogReadError> {
        if providers
            .windows(2)
            .any(|window| window[0].name() >= window[1].name())
        {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self {
            pricing_scope,
            providers,
        })
    }

    /// 返回供应商聚合对应的目录价格范围。
    #[must_use]
    pub const fn pricing_scope(&self) -> ModelCatalogPricingScope {
        self.pricing_scope
    }

    /// 返回完整、稳定排序的供应商聚合项。
    #[must_use]
    pub fn providers(&self) -> &[ModelCatalogProvider] {
        &self.providers
    }
}

impl fmt::Debug for ModelCatalogProviderSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCatalogProviderSummary")
            .field("pricing_scope", &self.pricing_scope)
            .field("provider_count", &self.providers.len())
            .finish()
    }
}

impl ModelCatalogPage {
    /// 组合价格范围、本页条目与可选的下一页模型名游标。
    #[must_use]
    pub fn from_parts(
        pricing_scope: ModelCatalogPricingScope,
        items: Vec<ModelCatalogItem>,
        next_cursor: Option<String>,
    ) -> Self {
        Self {
            pricing_scope,
            items,
            next_cursor,
        }
    }

    /// 返回本页价格是公开基础价还是当前分组实际价。
    #[must_use]
    pub const fn pricing_scope(&self) -> ModelCatalogPricingScope {
        self.pricing_scope
    }

    /// 返回本页目录条目。
    #[must_use]
    pub fn items(&self) -> &[ModelCatalogItem] {
        &self.items
    }

    /// 返回下一页从哪个模型名之后继续。
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

impl fmt::Debug for ModelCatalogPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCatalogPage")
            .field("pricing_scope", &self.pricing_scope)
            .field("item_count", &self.items.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 模型目录读取失败分类，不携带模型、分组、价格或内部快照诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ModelCatalogReadError {
    /// 查询字段、游标或页大小不满足闭合边界。
    #[error("模型目录查询无效")]
    InvalidQuery,
    /// 运行时目录、定价快照或时钟不可用。
    #[error("模型目录读取失败")]
    Internal,
}

/// 模型目录异步读取结果。
pub type ModelCatalogListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelCatalogPage, ModelCatalogReadError>> + Send + 'a>>;

/// API Key 可见的模型标识与公开元数据，不携带渠道或定价信息。
pub struct GatewayModel {
    id: String,
    created: i64,
    owned_by: String,
}

impl GatewayModel {
    /// 组合通过目录校验的公开标识、创建时间及供应商名称。
    pub fn new(id: String, created: i64, owned_by: String) -> Result<Self, ModelCatalogReadError> {
        if !is_valid_model_name(&id) || !is_valid_provider(&owned_by) {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(Self {
            id,
            created,
            owned_by,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn created(&self) -> i64 {
        self.created
    }

    #[must_use]
    pub fn owned_by(&self) -> &str {
        &self.owned_by
    }
}

impl fmt::Debug for GatewayModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GatewayModel(<redacted>)")
    }
}

/// API Key 完整模型列表异步读取结果。
pub type GatewayModelListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<GatewayModel>, ModelCatalogReadError>> + Send + 'a>>;

/// 模型目录供应商聚合异步读取结果。
pub type ModelCatalogProvidersFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ModelCatalogProviderSummary, ModelCatalogReadError>> + Send + 'a,
    >,
>;

/// 游客、登录用户与 API Key 共用的模型目录异步只读端口。
pub trait ModelCatalogReader: Send + Sync {
    /// 按令牌有效分组、模型白名单和目录可见性返回完整模型列表。
    fn list_for_token(
        &self,
        authentication: crate::TokenAuthentication,
    ) -> GatewayModelListFuture<'_>;

    /// 无会话时读取公开基础目录，有会话时按数据库回查分组读取可用模型。
    fn list<'a>(
        &'a self,
        authentication: Option<SessionAuthentication>,
        query: &'a ModelCatalogQuery,
    ) -> ModelCatalogListFuture<'a>;

    /// 返回与模型目录相同权限范围内的完整供应商及模型数量。
    fn providers(
        &self,
        authentication: Option<SessionAuthentication>,
    ) -> ModelCatalogProvidersFuture<'_>;
}

fn is_valid_model_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn is_valid_provider(value: &str) -> bool {
    validate_text(value, MAX_ADMIN_MODEL_PROVIDER_BYTES).is_ok()
}

fn is_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|window| window[0] < window[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_item_boundaries_fail_closed() {
        assert_eq!(ModelCatalogQuery::default().limit(), 24);
        for search in [Some(String::new()), Some(" padded ".to_owned())] {
            assert_eq!(
                ModelCatalogQuery::new(
                    search,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    None,
                    24,
                ),
                Err(ModelCatalogReadError::InvalidQuery)
            );
        }
        assert_eq!(
            ModelCatalogQuery::new(
                None,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some("bad\nmodel".to_owned()),
                24,
            ),
            Err(ModelCatalogReadError::InvalidQuery)
        );
        assert_eq!(
            ModelCatalogQuery::new(
                None,
                None,
                vec!["OpenAI".to_owned(), "OpenAI".to_owned()],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                24,
            ),
            Err(ModelCatalogReadError::InvalidQuery)
        );
        assert_eq!(
            ModelCatalogQuery::new(
                None,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                0,
            ),
            Err(ModelCatalogReadError::InvalidQuery)
        );
        assert_eq!(
            ModelCatalogQuery::new(
                None,
                None,
                Vec::new(),
                vec![ModelCatalogModality::Text, ModelCatalogModality::Text],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                24,
            ),
            Err(ModelCatalogReadError::InvalidQuery)
        );

        let ratios = ModelCatalogRatios::new(1_000_000, 1_000_000, 1_000_000).unwrap();
        assert!(
            ModelCatalogItem::new(
                metadata("gpt-5"),
                ModelCatalogBillingMode::Free,
                Some(ModelCatalogTokenPrices::from_decimals([Decimal::ZERO; 5]).unwrap()),
                ratios,
                1,
                ModelCatalogRuntimeStatus::NotEvaluated,
                Vec::new(),
                false,
            )
            .is_err()
        );

        assert!(
            ModelCatalogItem::new(
                metadata("gpt-5"),
                ModelCatalogBillingMode::Free,
                None,
                ratios,
                1,
                ModelCatalogRuntimeStatus::NotEvaluated,
                vec![Protocol::OpenAiResponses, Protocol::OpenAiChat],
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn decimal_prices_keep_exact_text_without_float_conversion() {
        let prices = ModelCatalogTokenPrices::from_decimals([
            Decimal::new(125, 2),
            Decimal::new(10, 0),
            Decimal::new(125, 3),
            Decimal::ZERO,
            Decimal::new(2_500_000_001, 9),
        ])
        .unwrap();
        assert_eq!(prices.input(), "1.25");
        assert_eq!(prices.output(), "10");
        assert_eq!(prices.cache_read(), "0.125");
        assert_eq!(prices.cache_creation_5m(), "0");
        assert_eq!(prices.cache_creation_1h(), "2.500000001");
    }

    #[test]
    fn page_keeps_the_authoritative_pricing_scope() {
        let page =
            ModelCatalogPage::from_parts(ModelCatalogPricingScope::PublicBase, Vec::new(), None);
        assert_eq!(page.pricing_scope(), ModelCatalogPricingScope::PublicBase);
    }

    fn metadata(model: &str) -> ModelCatalogMetadata {
        ModelCatalogMetadata::new(
            model.to_owned(),
            "Model".to_owned(),
            "provider".to_owned(),
            None,
            None,
            Vec::new(),
            None,
            AdminModelModalities::new(true, false, false, false),
            AdminModelModalities::new(true, false, false, false),
            false,
            false,
            ModelCatalogLifecycle::Active,
        )
        .unwrap()
    }
}
