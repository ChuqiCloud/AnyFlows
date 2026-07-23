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
    DatabasePool, GroupPricingRepository, GroupPricingRepositoryError,
    MAX_GROUP_MODEL_RATIO_ENTRIES, MAX_GROUP_PRICING_ENTRIES,
};
use af_domain::GroupId;
use thiserror::Error;

use crate::{PricingError, PricingRatio, PricingRatios};

const SECONDS_PER_DAY: u32 = 24 * 60 * 60;

/// 分组计费来源的异步加载结果。
pub type GroupPricingSourceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<GroupPricingSourceCatalog, GroupPricingSourceError>> + Send + 'a,
    >,
>;

/// 可替换的完整分组计费目录来源；通知通道只需调用缓存的 `invalidate`。
pub trait GroupPricingSource: Send + Sync + 'static {
    /// 读取一个完整且内部一致的有效分组与附加倍率目录。
    fn load<'a>(&'a self) -> GroupPricingSourceFuture<'a>;
}

/// 来源读取错误；不携带分组标识、倍率或底层存储诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GroupPricingSourceError {
    /// 来源无法完成目录读取。
    #[error("读取分组计费目录失败")]
    Unavailable,
    /// 来源读取超过其硬截止时间。
    #[error("读取分组计费目录超时")]
    Timeout,
    /// 来源返回了违反目录不变量的数据。
    #[error("分组计费目录来源状态损坏")]
    Invariant,
}

/// 来源提供的高峰倍率窗口。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GroupPeakPricing {
    ratio: PricingRatio,
    start_second: u32,
    end_second: u32,
}

impl GroupPeakPricing {
    /// 校验倍率与时间边界后构造窗口；起止相等会产生全天歧义，因此拒绝。
    pub fn new(
        ratio: PricingRatio,
        start_second: u32,
        end_second: u32,
    ) -> Result<Self, GroupPricingCacheError> {
        if start_second >= SECONDS_PER_DAY
            || end_second >= SECONDS_PER_DAY
            || start_second == end_second
        {
            return Err(GroupPricingCacheError::InvalidRecord);
        }
        Ok(Self {
            ratio,
            start_second,
            end_second,
        })
    }

    fn is_active(self, current_second: u32) -> bool {
        if self.start_second < self.end_second {
            (self.start_second..self.end_second).contains(&current_second)
        } else {
            // 跨午夜窗口拆成 `[start, 24h)` 与 `[0, end)` 两段。
            current_second >= self.start_second || current_second < self.end_second
        }
    }
}

impl fmt::Debug for GroupPeakPricing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GroupPeakPricing(<redacted>)")
    }
}

/// 来源提供的单个有效分组计费配置。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GroupPricingSourceRecord {
    group_id: GroupId,
    ratio: PricingRatio,
    peak: Option<GroupPeakPricing>,
}

impl GroupPricingSourceRecord {
    /// 使用已校验的分组标识、基础倍率和可选高峰窗口构造记录。
    #[must_use]
    pub const fn new(
        group_id: GroupId,
        ratio: PricingRatio,
        peak: Option<GroupPeakPricing>,
    ) -> Self {
        Self {
            group_id,
            ratio,
            peak,
        }
    }
}

impl fmt::Debug for GroupPricingSourceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GroupPricingSourceRecord(<redacted>)")
    }
}

/// 来源提供的用户分组到实际计费分组附加倍率。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GroupModelRatioSourceRecord {
    source_group_id: GroupId,
    target_group_id: GroupId,
    ratio: PricingRatio,
}

impl GroupModelRatioSourceRecord {
    /// 使用两个已校验分组标识和非负定点倍率构造覆盖记录。
    #[must_use]
    pub const fn new(
        source_group_id: GroupId,
        target_group_id: GroupId,
        ratio: PricingRatio,
    ) -> Self {
        Self {
            source_group_id,
            target_group_id,
            ratio,
        }
    }
}

impl fmt::Debug for GroupModelRatioSourceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GroupModelRatioSourceRecord(<redacted>)")
    }
}

/// 来源一次完整读取返回的目录输入。
#[derive(Clone, Eq, PartialEq)]
pub struct GroupPricingSourceCatalog {
    groups: Vec<GroupPricingSourceRecord>,
    group_model_ratios: Vec<GroupModelRatioSourceRecord>,
}

impl GroupPricingSourceCatalog {
    /// 构造完整目录输入；重复和悬空引用由快照构建边界统一拒绝。
    #[must_use]
    pub fn new(
        groups: Vec<GroupPricingSourceRecord>,
        group_model_ratios: Vec<GroupModelRatioSourceRecord>,
    ) -> Self {
        Self {
            groups,
            group_model_ratios,
        }
    }
}

impl fmt::Debug for GroupPricingSourceCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupPricingSourceCatalog")
            .field("group_count", &self.groups.len())
            .field("group_model_ratio_count", &self.group_model_ratios.len())
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct GroupPricing {
    ratio: PricingRatio,
    peak: Option<GroupPeakPricing>,
}

/// 一次完整加载后发布的只读分组计费目录。
pub struct GroupPricingSnapshot {
    generation: u64,
    groups: BTreeMap<GroupId, GroupPricing>,
    group_model_ratios: BTreeMap<(GroupId, GroupId), PricingRatio>,
}

impl GroupPricingSnapshot {
    /// 返回本进程已应用的失效代数。
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// 返回有效分组数。
    #[must_use]
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// 返回分组间附加倍率数。
    #[must_use]
    pub fn group_model_ratio_count(&self) -> usize {
        self.group_model_ratios.len()
    }

    /// 为一次请求固定基础、跨分组附加与当前高峰三层倍率。
    pub fn ratios_for_request(
        &self,
        source_group_id: GroupId,
        target_group_id: GroupId,
        current_second: u32,
    ) -> Result<PricingRatios, GroupPricingLookupError> {
        if current_second >= SECONDS_PER_DAY {
            return Err(GroupPricingLookupError::InvalidCurrentTime);
        }
        if !self.groups.contains_key(&source_group_id) {
            return Err(GroupPricingLookupError::SourceGroupNotFound);
        }
        let target = self
            .groups
            .get(&target_group_id)
            .copied()
            .ok_or(GroupPricingLookupError::TargetGroupNotFound)?;
        let group_model = self
            .group_model_ratios
            .get(&(source_group_id, target_group_id))
            .copied()
            .unwrap_or(PricingRatio::ONE);
        let applied_peak = target.peak.map_or(PricingRatio::ONE, |peak| {
            if peak.is_active(current_second) {
                peak.ratio
            } else {
                PricingRatio::ONE
            }
        });
        Ok(PricingRatios::new(target.ratio, group_model, applied_peak))
    }
}

impl fmt::Debug for GroupPricingSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupPricingSnapshot")
            .field("generation", &self.generation)
            .field("group_count", &self.groups.len())
            .field("group_model_ratio_count", &self.group_model_ratios.len())
            .finish()
    }
}

/// 请求级倍率查找错误；不回显分组标识或倍率。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GroupPricingLookupError {
    /// 用户来源分组不在当前有效目录中。
    #[error("来源分组计费配置不存在")]
    SourceGroupNotFound,
    /// 实际计费分组不在当前有效目录中。
    #[error("目标分组计费配置不存在")]
    TargetGroupNotFound,
    /// 当前时间不是一天内有效的整秒数。
    #[error("请求计费时间无效")]
    InvalidCurrentTime,
}

/// 分组计费缓存加载、刷新与状态错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GroupPricingCacheError {
    /// 来源读取失败。
    #[error(transparent)]
    Source(#[from] GroupPricingSourceError),
    /// 来源记录包含无效倍率或高峰窗口。
    #[error("分组计费目录记录无效")]
    InvalidRecord,
    /// 有效分组目录超过容量硬上限。
    #[error("分组计费目录超过容量上限")]
    GroupCapacityExceeded,
    /// 分组间附加倍率目录超过容量硬上限。
    #[error("分组附加倍率目录超过容量上限")]
    GroupModelRatioCapacityExceeded,
    /// 完整目录包含重复有效分组。
    #[error("分组计费目录包含重复分组")]
    DuplicateGroup,
    /// 完整目录包含重复的来源到目标附加倍率。
    #[error("分组计费目录包含重复附加倍率")]
    DuplicateGroupModelRatio,
    /// 附加倍率引用了不在当前有效目录中的分组。
    #[error("分组附加倍率引用无效分组")]
    OrphanGroupModelRatio,
    /// 本地快照锁中毒，不能继续提供可能不一致的目录。
    #[error("分组计费目录本地状态不可用")]
    StateUnavailable,
    /// 失效代数已经耗尽，不能静默回绕并错认新旧通知。
    #[error("分组计费目录失效代数已耗尽")]
    GenerationExhausted,
}

struct GroupPricingCacheInner {
    source: Arc<dyn GroupPricingSource>,
    snapshot: RwLock<Arc<GroupPricingSnapshot>>,
    refresh_gate: tokio::sync::Mutex<()>,
    requested_generation: AtomicU64,
    applied_generation: AtomicU64,
}

/// 使用不可变全量快照的进程内分组计费缓存。
#[derive(Clone)]
pub struct GroupPricingCache {
    inner: Arc<GroupPricingCacheInner>,
}

impl GroupPricingCache {
    /// 从来源完整加载首个目录；失败时不得发布空目录冒充成功。
    pub async fn load(source: Arc<dyn GroupPricingSource>) -> Result<Self, GroupPricingCacheError> {
        let generation = 1;
        let catalog = source.load().await?;
        let snapshot = Arc::new(build_snapshot(catalog, generation)?);
        Ok(Self {
            inner: Arc::new(GroupPricingCacheInner {
                source,
                snapshot: RwLock::new(snapshot),
                refresh_gate: tokio::sync::Mutex::new(()),
                requested_generation: AtomicU64::new(generation),
                applied_generation: AtomicU64::new(generation),
            }),
        })
    }

    /// 使用数据库仓储作为来源加载首个目录。
    pub async fn load_from_database(
        database: DatabasePool,
    ) -> Result<Self, GroupPricingCacheError> {
        Self::load(Arc::new(DatabaseGroupPricingSource::new(database))).await
    }

    /// 克隆当前不可变快照；刷新不会改写调用方已经取得的旧值。
    pub fn snapshot(&self) -> Result<Arc<GroupPricingSnapshot>, GroupPricingCacheError> {
        self.inner
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .map_err(|_| GroupPricingCacheError::StateUnavailable)
    }

    /// 标记目录失效并返回新的单调代数；本方法不执行 IO。
    pub fn invalidate(&self) -> Result<u64, GroupPricingCacheError> {
        self.inner
            .requested_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                generation.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| GroupPricingCacheError::GenerationExhausted)
    }

    /// 返回是否仍有失效通知尚未被成功刷新的快照覆盖。
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.inner.applied_generation.load(Ordering::Acquire)
            < self.inner.requested_generation.load(Ordering::Acquire)
    }

    /// 无条件串行刷新完整目录；失败时保留此前快照和待处理失效状态。
    pub async fn refresh(&self) -> Result<Arc<GroupPricingSnapshot>, GroupPricingCacheError> {
        let _guard = self.inner.refresh_gate.lock().await;
        self.refresh_locked().await
    }

    /// 仅在失效代数领先时刷新；并发调用取得 gate 后会重新判断。
    pub async fn refresh_if_stale(
        &self,
    ) -> Result<Arc<GroupPricingSnapshot>, GroupPricingCacheError> {
        if !self.is_stale() {
            return self.snapshot();
        }
        let _guard = self.inner.refresh_gate.lock().await;
        if !self.is_stale() {
            return self.snapshot();
        }
        self.refresh_locked().await
    }

    async fn refresh_locked(&self) -> Result<Arc<GroupPricingSnapshot>, GroupPricingCacheError> {
        // 加载期间到达的新通知必须继续保持 stale，不能被本轮旧目录覆盖。
        let target_generation = self.inner.requested_generation.load(Ordering::Acquire);
        let catalog = self.inner.source.load().await?;
        let next = Arc::new(build_snapshot(catalog, target_generation)?);
        let mut current = self
            .inner
            .snapshot
            .write()
            .map_err(|_| GroupPricingCacheError::StateUnavailable)?;
        *current = Arc::clone(&next);
        self.inner
            .applied_generation
            .store(target_generation, Ordering::Release);
        Ok(next)
    }
}

impl fmt::Debug for GroupPricingCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupPricingCache")
            .field("stale", &self.is_stale())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct DatabaseGroupPricingSource {
    repository: GroupPricingRepository,
}

impl DatabaseGroupPricingSource {
    fn new(database: DatabasePool) -> Self {
        Self {
            repository: GroupPricingRepository::new(database),
        }
    }
}

impl GroupPricingSource for DatabaseGroupPricingSource {
    fn load<'a>(&'a self) -> GroupPricingSourceFuture<'a> {
        Box::pin(async move {
            let catalog = self
                .repository
                .load_snapshot()
                .await
                .map_err(map_repository_error)?;
            let groups = catalog
                .groups()
                .iter()
                .copied()
                .map(|record| {
                    let ratio = pricing_ratio(record.ratio_micros())?;
                    let peak = record
                        .peak()
                        .map(|peak| {
                            GroupPeakPricing::new(
                                pricing_ratio(peak.ratio_micros())?,
                                peak.start_second(),
                                peak.end_second(),
                            )
                        })
                        .transpose()
                        .map_err(|_| GroupPricingSourceError::Invariant)?;
                    Ok(GroupPricingSourceRecord::new(
                        record.group_id(),
                        ratio,
                        peak,
                    ))
                })
                .collect::<Result<Vec<_>, GroupPricingSourceError>>()?;
            let group_model_ratios = catalog
                .group_model_ratios()
                .iter()
                .copied()
                .map(|record| {
                    Ok(GroupModelRatioSourceRecord::new(
                        record.source_group_id(),
                        record.target_group_id(),
                        pricing_ratio(record.ratio_micros())?,
                    ))
                })
                .collect::<Result<Vec<_>, GroupPricingSourceError>>()?;
            Ok(GroupPricingSourceCatalog::new(groups, group_model_ratios))
        })
    }
}

fn build_snapshot(
    catalog: GroupPricingSourceCatalog,
    generation: u64,
) -> Result<GroupPricingSnapshot, GroupPricingCacheError> {
    if catalog.groups.len() > MAX_GROUP_PRICING_ENTRIES {
        return Err(GroupPricingCacheError::GroupCapacityExceeded);
    }
    if catalog.group_model_ratios.len() > MAX_GROUP_MODEL_RATIO_ENTRIES {
        return Err(GroupPricingCacheError::GroupModelRatioCapacityExceeded);
    }
    let mut groups = BTreeMap::new();
    for record in catalog.groups {
        if groups
            .insert(
                record.group_id,
                GroupPricing {
                    ratio: record.ratio,
                    peak: record.peak,
                },
            )
            .is_some()
        {
            return Err(GroupPricingCacheError::DuplicateGroup);
        }
    }
    let mut group_model_ratios = BTreeMap::new();
    for record in catalog.group_model_ratios {
        if !groups.contains_key(&record.source_group_id)
            || !groups.contains_key(&record.target_group_id)
        {
            return Err(GroupPricingCacheError::OrphanGroupModelRatio);
        }
        if group_model_ratios
            .insert(
                (record.source_group_id, record.target_group_id),
                record.ratio,
            )
            .is_some()
        {
            return Err(GroupPricingCacheError::DuplicateGroupModelRatio);
        }
    }
    Ok(GroupPricingSnapshot {
        generation,
        groups,
        group_model_ratios,
    })
}

fn pricing_ratio(micros: i64) -> Result<PricingRatio, GroupPricingSourceError> {
    PricingRatio::new(micros).map_err(map_pricing_error)
}

const fn map_pricing_error(_error: PricingError) -> GroupPricingSourceError {
    GroupPricingSourceError::Invariant
}

const fn map_repository_error(error: GroupPricingRepositoryError) -> GroupPricingSourceError {
    match error {
        GroupPricingRepositoryError::Query => GroupPricingSourceError::Unavailable,
        GroupPricingRepositoryError::Timeout => GroupPricingSourceError::Timeout,
        GroupPricingRepositoryError::InvalidConfiguration
        | GroupPricingRepositoryError::Invariant => GroupPricingSourceError::Invariant,
        _ => GroupPricingSourceError::Invariant,
    }
}
