use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use af_db::{
    DatabasePool, MAX_MODEL_PRICE_ENTRIES, ModelPriceBillingMode, ModelPriceRepository,
    ModelPriceRepositoryError,
};
use af_domain::MAX_MODEL_NAME_BYTES;
use thiserror::Error;

use crate::{
    BillingExpressionDefinition, BillingExpressionError, BillingMode, PricingRatios,
    PricingResolver, RatioPricingResolver, TokenPrices,
    expression_pricing::ExpressionPricingResolver,
};

/// 定价目录 source 的异步加载结果。
pub type ModelPriceSourceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<ModelPriceSourceRecord>, ModelPriceSourceError>> + Send + 'a,
    >,
>;

/// 可替换的完整定价目录来源；通知通道只需调用缓存的 `invalidate`。
pub trait ModelPriceSource: Send + Sync + 'static {
    /// 读取一个完整且内部一致的目录版本。
    fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a>;
}

/// source 读取错误；不携带模型名、单价或底层存储诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ModelPriceSourceError {
    /// source 无法完成目录读取。
    #[error("读取定价目录失败")]
    Unavailable,
    /// source 读取超过其硬截止时间。
    #[error("读取定价目录超时")]
    Timeout,
    /// source 返回了违反目录不变量的数据。
    #[error("定价目录来源状态损坏")]
    Invariant,
}

/// source 提供的单模型价格；构造后不再允许修改。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelPriceSourceRecord {
    model: String,
    price: ModelPrice,
}

impl ModelPriceSourceRecord {
    /// 校验模型名、模式、单价与正版本后构造 source 记录。
    pub fn new(
        model: String,
        billing_mode: BillingMode,
        prices: TokenPrices,
        version: u64,
    ) -> Result<Self, ModelPriceCacheError> {
        if !is_valid_model_name(&model)
            || version == 0
            || billing_mode == BillingMode::PerCall
            || (billing_mode == BillingMode::Free && !prices_are_zero(prices))
        {
            return Err(ModelPriceCacheError::InvalidRecord);
        }
        Ok(Self {
            model,
            price: ModelPrice {
                definition: match billing_mode {
                    BillingMode::PerToken => ModelPriceDefinition::PerToken(prices),
                    BillingMode::Free => ModelPriceDefinition::Free,
                    BillingMode::PerCall => return Err(ModelPriceCacheError::InvalidRecord),
                },
                version,
            },
        })
    }

    /// 使用已经完成版本与沙箱校验的表达式定义构造 source 记录。
    pub fn expression(
        model: String,
        definition: BillingExpressionDefinition,
        version: u64,
    ) -> Result<Self, ModelPriceCacheError> {
        if !is_valid_model_name(&model) || version == 0 {
            return Err(ModelPriceCacheError::InvalidRecord);
        }
        Ok(Self {
            model,
            price: ModelPrice {
                definition: ModelPriceDefinition::Expression(definition),
                version,
            },
        })
    }
}

impl fmt::Debug for ModelPriceSourceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelPriceSourceRecord(<redacted>)")
    }
}

/// 单个模型的不可变定价快照。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelPrice {
    definition: ModelPriceDefinition,
    version: u64,
}

#[derive(Clone, Eq, PartialEq)]
enum ModelPriceDefinition {
    PerToken(TokenPrices),
    Free,
    Expression(BillingExpressionDefinition),
}

/// 缓存中已校验的模型定价定义模式。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelPriceMode {
    /// 五类固定单价。
    PerToken,
    /// 显式免费。
    Free,
    /// 版本化计费表达式。
    Expression,
}

impl ModelPrice {
    /// 返回缓存中保存的闭合定价定义模式。
    #[must_use]
    pub const fn mode(&self) -> ModelPriceMode {
        match &self.definition {
            ModelPriceDefinition::PerToken(_) => ModelPriceMode::PerToken,
            ModelPriceDefinition::Free => ModelPriceMode::Free,
            ModelPriceDefinition::Expression(_) => ModelPriceMode::Expression,
        }
    }

    /// 返回当前 Ratio/Free 生命周期可处理的模式；Expression 尚未接线时返回空。
    #[must_use]
    pub const fn billing_mode(&self) -> Option<BillingMode> {
        match &self.definition {
            ModelPriceDefinition::PerToken(_) => Some(BillingMode::PerToken),
            ModelPriceDefinition::Free => Some(BillingMode::Free),
            ModelPriceDefinition::Expression(_) => None,
        }
    }

    /// 返回该模型的正版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 把固定单价与本次请求倍率组合成解析器；Expression 尚未接线时返回空。
    #[must_use]
    pub const fn resolver(&self, ratios: PricingRatios) -> Option<RatioPricingResolver> {
        match &self.definition {
            ModelPriceDefinition::PerToken(prices) => {
                Some(RatioPricingResolver::metered(*prices, ratios))
            }
            ModelPriceDefinition::Free => Some(RatioPricingResolver::free()),
            ModelPriceDefinition::Expression(_) => None,
        }
    }

    /// 构造生产请求使用的统一解析器，并在 Expression 模式固定本次请求倍率。
    pub fn request_resolver(
        &self,
        ratios: PricingRatios,
    ) -> Result<Arc<dyn PricingResolver>, BillingExpressionError> {
        match &self.definition {
            ModelPriceDefinition::PerToken(prices) => {
                Ok(Arc::new(RatioPricingResolver::metered(*prices, ratios)))
            }
            ModelPriceDefinition::Free => Ok(Arc::new(RatioPricingResolver::free())),
            ModelPriceDefinition::Expression(definition) => Ok(Arc::new(
                ExpressionPricingResolver::new(definition.compile(ratios)?),
            )),
        }
    }
}

impl fmt::Debug for ModelPrice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPrice")
            .field("mode", &self.mode())
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 一次完整加载后发布的只读定价目录。
pub struct ModelPriceSnapshot {
    generation: u64,
    entries: BTreeMap<String, ModelPrice>,
}

impl ModelPriceSnapshot {
    /// 返回本进程已应用的失效代数。
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// 返回目录模型数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 返回目录是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按区分大小写的 Canonical 模型名读取克隆安全的价格快照。
    #[must_use]
    pub fn get(&self, model: &str) -> Option<ModelPrice> {
        self.entries.get(model).cloned()
    }

    /// 按 Canonical 模型名字典序枚举克隆安全的价格记录。
    pub fn entries(&self) -> impl ExactSizeIterator<Item = (&str, ModelPrice)> {
        self.entries
            .iter()
            .map(|(model, price)| (model.as_str(), price.clone()))
    }
}

impl fmt::Debug for ModelPriceSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceSnapshot")
            .field("generation", &self.generation)
            .field("entry_count", &self.entries.len())
            .finish()
    }
}

/// 定价缓存加载、刷新与状态错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ModelPriceCacheError {
    /// source 读取失败。
    #[error(transparent)]
    Source(#[from] ModelPriceSourceError),
    /// source 记录包含无效模型名、模式、单价或版本。
    #[error("定价目录记录无效")]
    InvalidRecord,
    /// 完整目录超过容量硬上限。
    #[error("定价目录超过容量上限")]
    CapacityExceeded,
    /// 完整目录包含重复模型名。
    #[error("定价目录包含重复模型")]
    DuplicateModel,
    /// 本地快照锁中毒，不能继续提供可能不一致的目录。
    #[error("定价目录本地状态不可用")]
    StateUnavailable,
    /// 失效代数已经耗尽，不能静默回绕并错认新旧通知。
    #[error("定价目录失效代数已耗尽")]
    GenerationExhausted,
}

struct ModelPriceCacheInner {
    source: Arc<dyn ModelPriceSource>,
    snapshot: RwLock<Arc<ModelPriceSnapshot>>,
    refresh_gate: tokio::sync::Mutex<()>,
    requested_generation: AtomicU64,
    applied_generation: AtomicU64,
}

/// 使用不可变全量快照的进程内定价缓存。
#[derive(Clone)]
pub struct ModelPriceCache {
    inner: Arc<ModelPriceCacheInner>,
}

impl ModelPriceCache {
    /// 从 source 完整加载首个目录；初始加载失败时不得发布空目录冒充成功。
    pub async fn load(source: Arc<dyn ModelPriceSource>) -> Result<Self, ModelPriceCacheError> {
        let generation = 1;
        let records = source.load().await?;
        let snapshot = Arc::new(build_snapshot(records, generation)?);
        Ok(Self {
            inner: Arc::new(ModelPriceCacheInner {
                source,
                snapshot: RwLock::new(snapshot),
                refresh_gate: tokio::sync::Mutex::new(()),
                requested_generation: AtomicU64::new(generation),
                applied_generation: AtomicU64::new(generation),
            }),
        })
    }

    /// 使用数据库仓储作为 source 加载首个目录。
    pub async fn load_from_database(database: DatabasePool) -> Result<Self, ModelPriceCacheError> {
        Self::load(Arc::new(DatabaseModelPriceSource::new(database))).await
    }

    /// 克隆当前不可变快照；调用方可在后续刷新期间继续安全使用旧快照。
    pub fn snapshot(&self) -> Result<Arc<ModelPriceSnapshot>, ModelPriceCacheError> {
        self.inner
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .map_err(|_| ModelPriceCacheError::StateUnavailable)
    }

    /// 标记目录失效并返回新的单调代数；本方法不执行 IO。
    pub fn invalidate(&self) -> Result<u64, ModelPriceCacheError> {
        self.inner
            .requested_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                generation.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| ModelPriceCacheError::GenerationExhausted)
    }

    /// 返回是否仍有失效通知尚未被成功刷新的快照覆盖。
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.inner.applied_generation.load(Ordering::Acquire)
            < self.inner.requested_generation.load(Ordering::Acquire)
    }

    /// 无条件串行刷新完整目录；失败时保留此前快照和待处理失效状态。
    pub async fn refresh(&self) -> Result<Arc<ModelPriceSnapshot>, ModelPriceCacheError> {
        let _guard = self.inner.refresh_gate.lock().await;
        self.refresh_locked().await
    }

    /// 仅在失效代数领先时刷新；并发调用会在取得 gate 后重新判断。
    pub async fn refresh_if_stale(&self) -> Result<Arc<ModelPriceSnapshot>, ModelPriceCacheError> {
        if !self.is_stale() {
            return self.snapshot();
        }
        let _guard = self.inner.refresh_gate.lock().await;
        if !self.is_stale() {
            return self.snapshot();
        }
        self.refresh_locked().await
    }

    async fn refresh_locked(&self) -> Result<Arc<ModelPriceSnapshot>, ModelPriceCacheError> {
        // 先固定本轮目标代数；加载期间到达的新通知会继续保持 stale，不能被覆盖。
        let target_generation = self.inner.requested_generation.load(Ordering::Acquire);
        let records = self.inner.source.load().await?;
        let next = Arc::new(build_snapshot(records, target_generation)?);
        let mut current = self
            .inner
            .snapshot
            .write()
            .map_err(|_| ModelPriceCacheError::StateUnavailable)?;
        *current = Arc::clone(&next);
        self.inner
            .applied_generation
            .store(target_generation, Ordering::Release);
        Ok(next)
    }
}

impl fmt::Debug for ModelPriceCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceCache")
            .field("stale", &self.is_stale())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct DatabaseModelPriceSource {
    repository: ModelPriceRepository,
}

impl DatabaseModelPriceSource {
    fn new(database: DatabasePool) -> Self {
        Self {
            repository: ModelPriceRepository::new(database),
        }
    }
}

impl ModelPriceSource for DatabaseModelPriceSource {
    fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a> {
        Box::pin(async move {
            self.repository
                .load_all()
                .await
                .map_err(map_repository_error)?
                .into_iter()
                .map(|record| match record.billing_mode() {
                    ModelPriceBillingMode::PerToken | ModelPriceBillingMode::Free => {
                        let prices = record.prices();
                        let prices =
                            TokenPrices::new(prices[0], prices[1], prices[2], prices[3], prices[4])
                                .map_err(|_| ModelPriceSourceError::Invariant)?;
                        let billing_mode =
                            if record.billing_mode() == ModelPriceBillingMode::PerToken {
                                BillingMode::PerToken
                            } else {
                                BillingMode::Free
                            };
                        ModelPriceSourceRecord::new(
                            record.model().to_owned(),
                            billing_mode,
                            prices,
                            record.version(),
                        )
                        .map_err(|_| ModelPriceSourceError::Invariant)
                    }
                    ModelPriceBillingMode::Expression => {
                        let source = record
                            .billing_expression()
                            .ok_or(ModelPriceSourceError::Invariant)?;
                        let definition = BillingExpressionDefinition::new(source.to_owned())
                            .map_err(|_| ModelPriceSourceError::Invariant)?;
                        ModelPriceSourceRecord::expression(
                            record.model().to_owned(),
                            definition,
                            record.version(),
                        )
                        .map_err(|_| ModelPriceSourceError::Invariant)
                    }
                })
                .collect()
        })
    }
}

fn build_snapshot(
    records: Vec<ModelPriceSourceRecord>,
    generation: u64,
) -> Result<ModelPriceSnapshot, ModelPriceCacheError> {
    if records.len() > MAX_MODEL_PRICE_ENTRIES {
        return Err(ModelPriceCacheError::CapacityExceeded);
    }
    let mut entries = BTreeMap::new();
    for record in records {
        if entries.insert(record.model, record.price).is_some() {
            return Err(ModelPriceCacheError::DuplicateModel);
        }
    }
    Ok(ModelPriceSnapshot {
        generation,
        entries,
    })
}

fn is_valid_model_name(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn prices_are_zero(prices: TokenPrices) -> bool {
    [
        prices.input(),
        prices.output(),
        prices.cache_read(),
        prices.cache_creation_5m(),
        prices.cache_creation_1h(),
    ]
    .into_iter()
    .all(|price| price.is_zero())
}

const fn map_repository_error(error: ModelPriceRepositoryError) -> ModelPriceSourceError {
    match error {
        ModelPriceRepositoryError::Query => ModelPriceSourceError::Unavailable,
        ModelPriceRepositoryError::Timeout => ModelPriceSourceError::Timeout,
        ModelPriceRepositoryError::InvalidConfiguration | ModelPriceRepositoryError::Invariant => {
            ModelPriceSourceError::Invariant
        }
        _ => ModelPriceSourceError::Invariant,
    }
}
