use std::{collections::BTreeSet, fmt, future::Future, pin::Pin, sync::Arc};

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
const MAX_SOURCE_REVISION_BYTES: usize = 128;
const MAX_SOURCE_TEXT_BYTES: usize = 128;

/// 管理端可写入的闭合模型计费模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelPriceBillingMode {
    /// 五类美元/百万 Token 单价。
    PerToken,
    /// 显式免费，五类单价必须全部为零。
    Free,
}

impl From<ModelPriceBillingMode> for AdminModelPriceBillingMode {
    fn from(value: ModelPriceBillingMode) -> Self {
        match value {
            ModelPriceBillingMode::PerToken => Self::PerToken,
            ModelPriceBillingMode::Free => Self::Free,
        }
    }
}

impl From<AdminModelPriceBillingMode> for ModelPriceBillingMode {
    fn from(value: AdminModelPriceBillingMode) -> Self {
        match value {
            AdminModelPriceBillingMode::PerToken => Self::PerToken,
            AdminModelPriceBillingMode::Free => Self::Free,
        }
    }
}

/// 管理端读取的单个正式模型价格快照。
pub struct AdminModelPrice {
    model: String,
    billing_mode: AdminModelPriceBillingMode,
    prices: [Decimal; 5],
    version: u64,
}

impl AdminModelPrice {
    fn from_record(record: ModelPriceRecord) -> Self {
        Self {
            model: record.model().to_owned(),
            billing_mode: record.billing_mode().into(),
            prices: record.prices(),
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
        let record =
            ModelPriceWriteRecord::new(model, expected_version, billing_mode.into(), prices)
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
    costs: [Option<Decimal>; 4],
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
            costs,
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

    /// 返回输入、输出、缓存读取和通用缓存写公开成本。
    #[must_use]
    pub const fn costs(&self) -> [Option<Decimal>; 4] {
        self.costs
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
            .field("has_output", &self.costs[1].is_some())
            .field("has_cache_read", &self.costs[2].is_some())
            .field("has_cache_write", &self.costs[3].is_some())
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
        if source != "models_dev"
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

/// 管理员价格读取、公开参考价预览和原子应用端口。
pub trait AdminModelPriceService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminModelPriceListQuery,
    ) -> AdminModelPriceListFuture<'a>;

    fn preview<'a>(&'a self, principal: SessionPrincipal) -> AdminModelPricePreviewFuture<'a>;

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

    fn preview<'a>(&'a self, principal: SessionPrincipal) -> AdminModelPricePreviewFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let targets = self.load_source_targets().await?;
            self.source
                .discover(targets)
                .await
                .map_err(map_source_error)
        })
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
    fn normal_user_cannot_enter_price_management() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminModelPriceError::Forbidden)
        );
    }
}
