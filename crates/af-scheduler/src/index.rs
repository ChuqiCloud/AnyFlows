use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};

use af_db::{
    DatabasePool, MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES, SchedulerAbilityRepositoryError,
    SchedulerCatalogSubject, SchedulerOutboxRepository, SchedulerOutboxRepositoryError,
    SchedulerRuntimeRecord, SchedulerRuntimeRepository, SchedulerRuntimeRepositoryError,
    SchedulerRuntimeTargetRecord,
};
use af_domain::{ChannelId, GroupId, MAX_MODEL_NAME_BYTES};
use thiserror::Error;
use tokio::time::sleep;

use crate::{
    ChannelRoutingHealth, SchedulerCandidate, StableFirstPlanError, WeightedRetryPlan,
    WeightedRetryPlanError, WeightedSelection, WeightedSelectionError, WeightedStrategy,
};

mod bound_route;
mod projection;
mod route;

pub use bound_route::{BoundRouteCandidate, BoundRouteCandidateError};
pub use projection::ChannelIndexProjectionApplyReport;
pub use route::{IndexedRouteCandidate, IndexedRoutePlan};

/// 默认渠道索引全量重建周期；后续 M3 会改为事件驱动增量并保留周期校正。
pub const DEFAULT_CHANNEL_INDEX_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const CHANNEL_INDEX_OUTBOX_TIMEOUT: Duration = Duration::from_secs(5);

/// 渠道索引数据源的一次全量读取结果。
pub type ChannelIndexSourceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<ChannelIndexSourceRecord>, ChannelIndexSourceError>>
            + Send
            + 'a,
    >,
>;

/// 带数据库目录高水位的一次全量来源读取结果。
pub struct ChannelIndexSourceSnapshot {
    records: Vec<ChannelIndexSourceRecord>,
    catalog_version: u64,
}

impl ChannelIndexSourceSnapshot {
    /// 使用完整记录和读取前取得的 outbox 高水位构造来源快照。
    #[must_use]
    pub const fn new(records: Vec<ChannelIndexSourceRecord>, catalog_version: u64) -> Self {
        Self {
            records,
            catalog_version,
        }
    }
}

impl fmt::Debug for ChannelIndexSourceSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelIndexSourceSnapshot")
            .field("record_count", &self.records.len())
            .field("catalog_version", &self.catalog_version)
            .finish()
    }
}

/// 可替换来源的一次带目录高水位读取结果。
pub type ChannelIndexVersionedSourceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ChannelIndexSourceSnapshot, ChannelIndexSourceError>>
            + Send
            + 'a,
    >,
>;

/// 可替换的渠道索引完整数据源。
///
/// 当前数据库实现每次返回完整有效能力与运行时目标目录；未来 Redis/pub-sub 增量快照落地时，
/// 只需替换数据源与失效通知，不改变请求侧快照读取方式。
pub trait ChannelIndexSource: Send + Sync + 'static {
    /// 读取一个完整且内部一致的渠道能力与运行时目标目录。
    fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a>;

    /// 先取得目录事件高水位，再读取完整目录；普通测试来源默认使用零高水位。
    fn load_versioned<'a>(&'a self) -> ChannelIndexVersionedSourceFuture<'a> {
        Box::pin(async move { Ok(ChannelIndexSourceSnapshot::new(self.load().await?, 0)) })
    }
}

/// 来源读取错误；不携带模型、渠道或底层数据库诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelIndexSourceError {
    /// 来源无法完成目录读取。
    #[error("读取渠道索引来源失败")]
    Unavailable,
    /// 来源读取超过其硬截止时间。
    #[error("读取渠道索引来源超时")]
    Timeout,
    /// 来源返回了违反索引不变量的数据。
    #[error("渠道索引来源状态损坏")]
    Invariant,
}

/// 来源提供的单条有效能力与运行时目标记录。
#[derive(Clone, Eq, PartialEq)]
pub struct ChannelIndexSourceRecord {
    group_id: GroupId,
    model: String,
    channel_id: ChannelId,
    priority: i32,
    weight: u32,
    runtime_target: Option<Arc<SchedulerRuntimeTargetRecord>>,
}

impl ChannelIndexSourceRecord {
    fn new(
        group_id: GroupId,
        model: impl Into<String>,
        channel_id: ChannelId,
        priority: i32,
        weight: u32,
    ) -> Result<Self, ChannelIndexSnapshotError> {
        let model = model.into();
        if !is_valid_model(&model) {
            return Err(ChannelIndexSnapshotError::InvalidModel);
        }
        Ok(Self {
            group_id,
            model,
            channel_id,
            priority,
            weight,
            runtime_target: None,
        })
    }

    /// 使用已校验运行时目标构造可直接进入生产快照的来源记录。
    pub fn with_runtime_target(
        group_id: GroupId,
        model: impl Into<String>,
        priority: i32,
        weight: u32,
        runtime_target: Arc<SchedulerRuntimeTargetRecord>,
    ) -> Result<Self, ChannelIndexSnapshotError> {
        let channel_id = runtime_target.channel_id();
        let mut record = Self::new(group_id, model, channel_id, priority, weight)?;
        record.runtime_target = Some(runtime_target);
        Ok(record)
    }
}

impl fmt::Debug for ChannelIndexSourceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelIndexSourceRecord")
            .field("group_id", &self.group_id)
            .field("channel_id", &self.channel_id)
            .field("priority", &self.priority)
            .field("weight", &self.weight)
            .field("has_runtime_target", &self.runtime_target.is_some())
            .finish()
    }
}

/// 构建不可变渠道索引快照时的安全错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelIndexSnapshotError {
    /// 模型名不是受限 Canonical 名称。
    #[error("渠道索引模型名无效")]
    InvalidModel,
    /// 单个分组与模型下的候选数量超过 relay 硬上限。
    #[error("渠道索引候选数量超过上限")]
    TooManyCandidates,
    /// 同一分组与模型下出现重复渠道。
    #[error("渠道索引候选渠道重复")]
    DuplicateChannel,
    /// 完整快照的能力记录总数超过安全上限。
    #[error("渠道索引记录总数超过上限")]
    TotalCapacityExceeded,
    /// 快照代数或内部记录违反不变量。
    #[error("渠道索引快照状态损坏")]
    Invariant,
}

/// 可由请求侧无锁持有的不可变渠道索引快照。
pub struct ChannelIndexSnapshot {
    generation: u64,
    catalog_version: u64,
    subject_versions: BTreeMap<SchedulerCatalogSubject, u64>,
    runtime_target_versions: BTreeMap<ChannelId, u64>,
    entries: BTreeMap<GroupId, BTreeMap<String, Arc<[SchedulerCandidate]>>>,
    runtime_targets: BTreeMap<ChannelId, Arc<SchedulerRuntimeTargetRecord>>,
    key_count: usize,
    candidate_count: usize,
}

impl ChannelIndexSnapshot {
    /// 从完整来源记录构建新快照；同一键下候选会在发布前完成容量和去重校验。
    pub fn from_records(
        records: Vec<ChannelIndexSourceRecord>,
        generation: u64,
    ) -> Result<Self, ChannelIndexSnapshotError> {
        Self::from_versioned_records(records, generation, 0)
    }

    fn from_versioned_records(
        records: Vec<ChannelIndexSourceRecord>,
        generation: u64,
        catalog_version: u64,
    ) -> Result<Self, ChannelIndexSnapshotError> {
        if generation == 0 {
            return Err(ChannelIndexSnapshotError::Invariant);
        }
        validate_total_capacity(records.len())?;
        let mut grouped = BTreeMap::<GroupId, BTreeMap<String, Vec<SchedulerCandidate>>>::new();
        let mut runtime_targets: BTreeMap<ChannelId, Arc<SchedulerRuntimeTargetRecord>> =
            BTreeMap::new();
        for record in records {
            let Some(runtime_target) = record.runtime_target else {
                continue;
            };
            if runtime_target.channel_id() != record.channel_id {
                return Err(ChannelIndexSnapshotError::Invariant);
            }
            if let Some(existing) = runtime_targets.get(&record.channel_id) {
                if existing.as_ref() != runtime_target.as_ref() {
                    return Err(ChannelIndexSnapshotError::Invariant);
                }
            } else {
                runtime_targets.insert(record.channel_id, runtime_target);
            }
            let candidate = SchedulerCandidate::new(
                record.group_id,
                record.channel_id,
                record.priority,
                record.weight,
            );
            grouped
                .entry(record.group_id)
                .or_default()
                .entry(record.model)
                .or_default()
                .push(candidate);
        }

        let mut entries = BTreeMap::new();
        let mut key_count = 0_usize;
        let mut candidate_count = 0_usize;
        for (group_id, models) in grouped {
            let mut indexed_models = BTreeMap::new();
            for (model, mut candidates) in models {
                validate_candidates(&candidates)?;
                candidates.sort_unstable_by(|left, right| {
                    right
                        .priority()
                        .cmp(&left.priority())
                        .then_with(|| left.channel_id().cmp(&right.channel_id()))
                });
                key_count = key_count
                    .checked_add(1)
                    .ok_or(ChannelIndexSnapshotError::Invariant)?;
                candidate_count = candidate_count
                    .checked_add(candidates.len())
                    .ok_or(ChannelIndexSnapshotError::Invariant)?;
                indexed_models.insert(model, Arc::from(candidates));
            }
            entries.insert(group_id, indexed_models);
        }

        Ok(Self {
            generation,
            catalog_version,
            subject_versions: BTreeMap::new(),
            runtime_target_versions: BTreeMap::new(),
            entries,
            runtime_targets,
            key_count,
            candidate_count,
        })
    }

    /// 返回快照单调代数，便于观测刷新是否成功发布。
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// 返回最近一次全量数据库读取前取得的 outbox 高水位。
    #[must_use]
    pub const fn catalog_version(&self) -> u64 {
        self.catalog_version
    }

    /// 返回当前包含的 `(group_id, model)` 键数量。
    #[must_use]
    pub const fn key_count(&self) -> usize {
        self.key_count
    }

    /// 返回当前快照中的候选总数。
    #[must_use]
    pub const fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    /// 返回当前快照中去重后的渠道运行时目标数量。
    #[must_use]
    pub fn runtime_target_count(&self) -> usize {
        self.runtime_targets.len()
    }

    /// 按渠道标识返回与候选同代发布的运行时目标。
    #[must_use]
    pub fn runtime_target(&self, channel_id: ChannelId) -> Option<&SchedulerRuntimeTargetRecord> {
        self.runtime_targets.get(&channel_id).map(Arc::as_ref)
    }

    /// 按字典序遍历指定分组当前确有运行时目标的 Canonical 模型名。
    ///
    /// 返回值只暴露模型键，不暴露渠道、凭据或候选权重。
    pub fn model_names(&self, group_id: GroupId) -> impl Iterator<Item = &str> {
        self.entries
            .get(&group_id)
            .into_iter()
            .flat_map(|models| models.keys().map(String::as_str))
    }

    /// 查询一个分组与模型下的候选；未知键返回空切片。
    pub fn candidates(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<&[SchedulerCandidate], ChannelIndexSnapshotError> {
        if !is_valid_model(model) {
            return Err(ChannelIndexSnapshotError::InvalidModel);
        }
        Ok(self
            .entries
            .get(&group_id)
            .and_then(|models| models.get(model))
            .map_or(&[], Arc::as_ref))
    }

    /// 判断指定模型是否在任一有效分组中存在明确具备 Compact 资格的候选。
    pub fn responses_compact_available_any_group(
        &self,
        model: &str,
    ) -> Result<bool, ChannelIndexSnapshotError> {
        if !is_valid_model(model) {
            return Err(ChannelIndexSnapshotError::InvalidModel);
        }
        for models in self.entries.values() {
            let Some(candidates) = models.get(model) else {
                continue;
            };
            for candidate in candidates.iter() {
                let target = self
                    .runtime_targets
                    .get(&candidate.channel_id())
                    .ok_or(ChannelIndexSnapshotError::Invariant)?;
                if target.schedulable_responses_compact_model(model).is_some() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// 判断指定分组与模型是否存在明确具备 Compact 资格的候选。
    pub fn responses_compact_available(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<bool, ChannelIndexSnapshotError> {
        for candidate in self.candidates(group_id, model)? {
            let target = self
                .runtime_targets
                .get(&candidate.channel_id())
                .ok_or(ChannelIndexSnapshotError::Invariant)?;
            if target.schedulable_responses_compact_model(model).is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl fmt::Debug for ChannelIndexSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelIndexSnapshot")
            .field("generation", &self.generation)
            .field("catalog_version", &self.catalog_version)
            .field("subject_version_count", &self.subject_versions.len())
            .field(
                "runtime_target_version_count",
                &self.runtime_target_versions.len(),
            )
            .field("key_count", &self.key_count)
            .field("candidate_count", &self.candidate_count)
            .field("runtime_target_count", &self.runtime_targets.len())
            .finish()
    }
}

/// 渠道索引缓存加载、刷新与状态错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelIndexCacheError {
    /// 来源读取失败。
    #[error(transparent)]
    Source(#[from] ChannelIndexSourceError),
    /// 来源记录无法构建安全快照。
    #[error(transparent)]
    Snapshot(#[from] ChannelIndexSnapshotError),
    /// 本地快照锁中毒，不能继续提供可能不一致的目录。
    #[error("渠道索引本地状态不可用")]
    StateUnavailable,
    /// 快照代数已经耗尽，不能静默回绕。
    #[error("渠道索引快照代数已耗尽")]
    GenerationExhausted,
}

struct InMemoryChannelIndexInner {
    source: Arc<dyn ChannelIndexSource>,
    snapshot: RwLock<Arc<ChannelIndexSnapshot>>,
    refresh_gate: tokio::sync::Mutex<()>,
}

/// 使用不可变全量快照的进程内渠道索引。
#[derive(Clone)]
pub struct InMemoryChannelIndex {
    inner: Arc<InMemoryChannelIndexInner>,
}

impl InMemoryChannelIndex {
    /// 从来源完整加载首个快照；失败时不得发布空索引冒充成功。
    pub async fn load(source: Arc<dyn ChannelIndexSource>) -> Result<Self, ChannelIndexCacheError> {
        let loaded = source.load_versioned().await?;
        let snapshot = Arc::new(ChannelIndexSnapshot::from_versioned_records(
            loaded.records,
            1,
            loaded.catalog_version,
        )?);
        Ok(Self {
            inner: Arc::new(InMemoryChannelIndexInner {
                source,
                snapshot: RwLock::new(snapshot),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        })
    }

    /// 使用数据库运行时渠道目录仓储作为来源加载首个快照。
    pub async fn load_from_database(
        database: DatabasePool,
    ) -> Result<Self, ChannelIndexCacheError> {
        Self::load(Arc::new(DatabaseChannelIndexSource::new(database))).await
    }

    /// 克隆当前不可变快照；刷新不会改写调用方已经取得的旧值。
    pub fn snapshot(&self) -> Result<Arc<ChannelIndexSnapshot>, ChannelIndexCacheError> {
        self.inner
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .map_err(|_| ChannelIndexCacheError::StateUnavailable)
    }

    /// 串行刷新完整索引；失败时保留此前快照。
    pub async fn refresh(&self) -> Result<Arc<ChannelIndexSnapshot>, ChannelIndexCacheError> {
        let _guard = self.inner.refresh_gate.lock().await;
        let generation = self
            .snapshot()?
            .generation()
            .checked_add(1)
            .ok_or(ChannelIndexCacheError::GenerationExhausted)?;
        let loaded = self.inner.source.load_versioned().await?;
        let next = Arc::new(ChannelIndexSnapshot::from_versioned_records(
            loaded.records,
            generation,
            loaded.catalog_version,
        )?);
        let mut current = self
            .inner
            .snapshot
            .write()
            .map_err(|_| ChannelIndexCacheError::StateUnavailable)?;
        *current = Arc::clone(&next);
        Ok(next)
    }
}

impl fmt::Debug for InMemoryChannelIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.snapshot().ok();
        formatter
            .debug_struct("InMemoryChannelIndex")
            .field(
                "generation",
                &snapshot.as_ref().map(|snapshot| snapshot.generation()),
            )
            .field(
                "key_count",
                &snapshot.as_ref().map(|snapshot| snapshot.key_count()),
            )
            .field(
                "candidate_count",
                &snapshot.as_ref().map(|snapshot| snapshot.candidate_count()),
            )
            .finish_non_exhaustive()
    }
}

/// 基于内存索引快照的 weighted 调度入口。
#[derive(Clone, Debug)]
pub struct IndexedWeightedScheduler {
    index: InMemoryChannelIndex,
    strategy: WeightedStrategy,
}

impl IndexedWeightedScheduler {
    /// 使用已加载的进程内渠道索引创建调度入口。
    #[must_use]
    pub const fn new(index: InMemoryChannelIndex) -> Self {
        Self {
            index,
            strategy: WeightedStrategy,
        }
    }

    /// 串行刷新底层不可变快照；失败时继续保留上一代可用快照。
    pub async fn refresh(&self) -> Result<(), IndexedWeightedSchedulerError> {
        self.index.refresh().await.map(|_| ()).map_err(Into::into)
    }

    /// 按渠道标识取得当前快照中的运行时目标，用于异步任务校验原目标绑定。
    pub fn runtime_target(
        &self,
        channel_id: ChannelId,
    ) -> Result<Option<Arc<SchedulerRuntimeTargetRecord>>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        Ok(snapshot.runtime_targets.get(&channel_id).cloned())
    }

    /// 查询一个分组与 Canonical 模型的可用渠道，并从最高优先级层加权选择。
    pub fn select(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Option<WeightedSelection>, IndexedWeightedSchedulerError> {
        self.select_at_priority_rank(group_id, model, 0)
    }

    /// 从指定去重优先级层选择候选，保持现有数据库调度器的 weighted 语义。
    pub fn select_at_priority_rank(
        &self,
        group_id: GroupId,
        model: &str,
        priority_rank: usize,
    ) -> Result<Option<WeightedSelection>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        self.strategy
            .select_at_priority_rank(snapshot.candidates(group_id, model)?, priority_rank)
            .map_err(Into::into)
    }

    /// 一次从稳定快照加载能力候选并创建失败重试计划。
    pub fn retry_plan(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<WeightedRetryPlan, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        WeightedRetryPlan::new(snapshot.candidates(group_id, model)?.to_vec()).map_err(Into::into)
    }

    /// 从同一代快照生成可直接交给生产转发装配层的固定路由计划。
    ///
    /// 空候选返回 `None`；快照中任一候选缺失同代运行时目标都视为目录损坏并失败关闭。
    pub fn route_plan(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
        self.route_plan_with_health(group_id, model, &BTreeMap::new())
    }

    /// 返回当前快照下最多 64 个候选渠道标识，供一次 Redis 健康批量读取使用。
    pub fn route_channel_ids(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Vec<ChannelId>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        Ok(snapshot
            .candidates(group_id, model)?
            .iter()
            .map(|candidate| candidate.channel_id())
            .collect())
    }

    /// 返回当前快照下明确具备 Responses Compact 资格的候选渠道标识。
    pub fn responses_compact_route_channel_ids(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Vec<ChannelId>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        let candidates = compact_candidates(&snapshot, group_id, model)?;
        Ok(candidates
            .iter()
            .map(|candidate| candidate.channel_id())
            .collect())
    }

    /// 从同一代快照生成仅包含明确支持渠道的 Responses Compact 路由计划。
    pub fn responses_compact_route_plan(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
        self.responses_compact_route_plan_with_health(group_id, model, &BTreeMap::new())
    }

    /// 过滤 Compact 资格后再应用渠道健康和 weighted 重试顺序。
    pub fn responses_compact_route_plan_with_health(
        &self,
        group_id: GroupId,
        model: &str,
        health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
    ) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        let candidates = compact_candidates(&snapshot, group_id, model)?;
        build_route_plan(&snapshot, candidates, health)
    }

    /// 在同一代快照中先过滤熔断渠道，再以衰减惩罚修正 weighted 抽样票数。
    pub fn route_plan_with_health(
        &self,
        group_id: GroupId,
        model: &str,
        health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
    ) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        build_route_plan(
            &snapshot,
            snapshot.candidates(group_id, model)?.to_vec(),
            health,
        )
    }
}

fn compact_candidates(
    snapshot: &ChannelIndexSnapshot,
    group_id: GroupId,
    model: &str,
) -> Result<Vec<SchedulerCandidate>, IndexedWeightedSchedulerError> {
    let mut eligible = Vec::new();
    for candidate in snapshot.candidates(group_id, model)? {
        let target = snapshot
            .runtime_targets
            .get(&candidate.channel_id())
            .ok_or(IndexedWeightedSchedulerError::Cache(
                ChannelIndexCacheError::Snapshot(ChannelIndexSnapshotError::Invariant),
            ))?;
        if target.schedulable_responses_compact_model(model).is_some() {
            eligible.push(*candidate);
        }
    }
    Ok(eligible)
}

fn build_route_plan(
    snapshot: &ChannelIndexSnapshot,
    candidates: Vec<SchedulerCandidate>,
    health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
    // 池模式把上游账号健康交给外部池管理，历史渠道惩罚也不能继续影响选路。
    let effective_health = health
        .iter()
        .filter(|(channel_id, _)| {
            !snapshot
                .runtime_targets
                .get(channel_id)
                .is_some_and(|target| target.pool_mode())
        })
        .map(|(channel_id, state)| (*channel_id, *state))
        .collect();
    let retry_order =
        WeightedRetryPlan::new_with_health(candidates, &effective_health)?.into_retry_order()?;
    let Some(first) = retry_order.first() else {
        return Ok(None);
    };
    let target_group_id = first.candidate().group_id();
    let mut candidates = Vec::with_capacity(retry_order.len());
    for selection in retry_order {
        let runtime_target = snapshot
            .runtime_targets
            .get(&selection.candidate().channel_id())
            .cloned()
            .ok_or(IndexedWeightedSchedulerError::Cache(
                ChannelIndexCacheError::Snapshot(ChannelIndexSnapshotError::Invariant),
            ))?;
        candidates.push(IndexedRouteCandidate::new(selection, runtime_target));
    }
    Ok(Some(IndexedRoutePlan::new(
        snapshot.generation(),
        target_group_id,
        candidates,
    )))
}

/// 内存索引 weighted 调度失败；模型名和持久化诊断不会进入错误文本。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum IndexedWeightedSchedulerError {
    /// 内存索引快照不可用或刷新来源损坏。
    #[error("读取渠道索引快照失败")]
    Cache(#[from] ChannelIndexCacheError),
    /// 查询模型名不是受限 Canonical 名称。
    #[error("调度查询模型名无效")]
    InvalidModel,
    /// weighted 策略无法安全完成选择。
    #[error("选择上游渠道失败")]
    Selection(#[from] WeightedSelectionError),
    /// 候选集合无法构造安全的失败重试计划。
    #[error("构造上游重试计划失败")]
    RetryPlan(#[from] WeightedRetryPlanError),
    /// StableFirst 无法生成闭合主池计划。
    #[error("构造 StableFirst 上游计划失败")]
    StableFirst(#[from] StableFirstPlanError),
}

impl From<ChannelIndexSnapshotError> for IndexedWeightedSchedulerError {
    fn from(error: ChannelIndexSnapshotError) -> Self {
        match error {
            ChannelIndexSnapshotError::InvalidModel => Self::InvalidModel,
            ChannelIndexSnapshotError::TooManyCandidates
            | ChannelIndexSnapshotError::DuplicateChannel
            | ChannelIndexSnapshotError::TotalCapacityExceeded
            | ChannelIndexSnapshotError::Invariant => {
                Self::Cache(ChannelIndexCacheError::Snapshot(error))
            }
        }
    }
}

/// 渠道索引周期刷新配置错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelIndexRefreshConfigError {
    /// 周期为零会造成忙轮询。
    #[error("渠道索引刷新周期必须大于零")]
    ZeroInterval,
}

/// 渠道索引周期刷新配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelIndexRefreshConfig {
    interval: Duration,
}

impl ChannelIndexRefreshConfig {
    /// 校验并创建周期刷新配置。
    pub fn new(interval: Duration) -> Result<Self, ChannelIndexRefreshConfigError> {
        if interval.is_zero() {
            return Err(ChannelIndexRefreshConfigError::ZeroInterval);
        }
        Ok(Self { interval })
    }

    /// 返回两次全量重建之间的等待时间。
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }
}

impl Default for ChannelIndexRefreshConfig {
    fn default() -> Self {
        Self {
            interval: DEFAULT_CHANNEL_INDEX_REFRESH_INTERVAL,
        }
    }
}

/// 单轮渠道索引刷新的聚合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelIndexRefreshReport {
    generation: u64,
    key_count: usize,
    candidate_count: usize,
}

impl ChannelIndexRefreshReport {
    /// 返回本轮成功发布后的快照代数。
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    /// 返回本轮成功发布后的索引键数量。
    #[must_use]
    pub const fn key_count(self) -> usize {
        self.key_count
    }

    /// 返回本轮成功发布后的候选总数。
    #[must_use]
    pub const fn candidate_count(self) -> usize {
        self.candidate_count
    }
}

/// 周期全量重建并原子发布渠道索引的执行器。
pub struct ChannelIndexRefreshSupervisor {
    index: InMemoryChannelIndex,
    config: ChannelIndexRefreshConfig,
}

impl ChannelIndexRefreshSupervisor {
    /// 使用已加载索引与显式配置创建执行器。
    #[must_use]
    pub const fn new(index: InMemoryChannelIndex, config: ChannelIndexRefreshConfig) -> Self {
        Self { index, config }
    }

    /// 执行一轮全量刷新；失败时底层缓存保留旧快照。
    pub async fn run_once(&self) -> Result<ChannelIndexRefreshReport, ChannelIndexCacheError> {
        let snapshot = self.index.refresh().await?;
        Ok(ChannelIndexRefreshReport {
            generation: snapshot.generation(),
            key_count: snapshot.key_count(),
            candidate_count: snapshot.candidate_count(),
        })
    }

    /// 按配置周期运行，直到外部关闭信号完成。
    pub async fn run_periodic_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                () = &mut shutdown => return,
                result = self.run_once() => {
                    match result {
                        Ok(report) => tracing::debug!(
                            generation = report.generation(),
                            key_count = report.key_count(),
                            candidate_count = report.candidate_count(),
                            "渠道索引全量刷新成功"
                        ),
                        Err(error) => tracing::warn!(
                            error_kind = channel_index_cache_error_kind(error),
                            "渠道索引全量刷新失败，保留上一份快照"
                        ),
                    }
                }
            }

            tokio::select! {
                () = &mut shutdown => return,
                () = sleep(self.config.interval) => {}
            }
        }
    }
}

impl fmt::Debug for ChannelIndexRefreshSupervisor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelIndexRefreshSupervisor")
            .field("interval", &self.config.interval)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct DatabaseChannelIndexSource {
    repository: SchedulerRuntimeRepository,
    outbox: SchedulerOutboxRepository,
}

impl DatabaseChannelIndexSource {
    fn new(database: DatabasePool) -> Self {
        Self {
            repository: SchedulerRuntimeRepository::new(database.clone()),
            outbox: SchedulerOutboxRepository::new(database, CHANNEL_INDEX_OUTBOX_TIMEOUT)
                .expect("固定调度 outbox 查询超时必须有效"),
        }
    }
}

impl ChannelIndexSource for DatabaseChannelIndexSource {
    fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
        Box::pin(async move {
            self.repository
                .load_all()
                .await
                .map_err(map_repository_error)?
                .into_iter()
                .map(|record| {
                    runtime_record_to_source(record).map_err(|_| ChannelIndexSourceError::Invariant)
                })
                .collect()
        })
    }

    fn load_versioned<'a>(&'a self) -> ChannelIndexVersionedSourceFuture<'a> {
        Box::pin(async move {
            // 高水位必须先于目录读取，避免把读取后的并发事件错误标记为已包含。
            let catalog_version = self
                .outbox
                .latest_event_id()
                .await
                .map_err(map_outbox_repository_error)?;
            Ok(ChannelIndexSourceSnapshot::new(
                self.load().await?,
                catalog_version,
            ))
        })
    }
}

fn runtime_record_to_source(
    record: SchedulerRuntimeRecord,
) -> Result<ChannelIndexSourceRecord, ChannelIndexSnapshotError> {
    let (ability, target) = record.into_parts();
    ChannelIndexSourceRecord::with_runtime_target(
        ability.group_id(),
        ability.model().to_owned(),
        ability.priority(),
        ability.weight(),
        target,
    )
}

fn validate_candidates(candidates: &[SchedulerCandidate]) -> Result<(), ChannelIndexSnapshotError> {
    WeightedRetryPlan::new(candidates.to_vec())
        .map(|_| ())
        .map_err(map_retry_plan_error)
}

fn validate_total_capacity(count: usize) -> Result<(), ChannelIndexSnapshotError> {
    if count > MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES {
        return Err(ChannelIndexSnapshotError::TotalCapacityExceeded);
    }
    Ok(())
}

fn map_retry_plan_error(error: WeightedRetryPlanError) -> ChannelIndexSnapshotError {
    match error {
        WeightedRetryPlanError::TooManyCandidates => ChannelIndexSnapshotError::TooManyCandidates,
        WeightedRetryPlanError::DuplicateChannel => ChannelIndexSnapshotError::DuplicateChannel,
        WeightedRetryPlanError::MixedGroups
        | WeightedRetryPlanError::Selection(_)
        | WeightedRetryPlanError::Invariant => ChannelIndexSnapshotError::Invariant,
    }
}

fn map_repository_error(error: SchedulerRuntimeRepositoryError) -> ChannelIndexSourceError {
    match error {
        SchedulerRuntimeRepositoryError::Query
        | SchedulerRuntimeRepositoryError::Ability(SchedulerAbilityRepositoryError::Query) => {
            ChannelIndexSourceError::Unavailable
        }
        SchedulerRuntimeRepositoryError::Timeout
        | SchedulerRuntimeRepositoryError::Ability(SchedulerAbilityRepositoryError::Timeout) => {
            ChannelIndexSourceError::Timeout
        }
        SchedulerRuntimeRepositoryError::InvalidConfiguration
        | SchedulerRuntimeRepositoryError::Invariant
        | SchedulerRuntimeRepositoryError::Ability(_) => ChannelIndexSourceError::Invariant,
        _ => ChannelIndexSourceError::Invariant,
    }
}

fn map_outbox_repository_error(error: SchedulerOutboxRepositoryError) -> ChannelIndexSourceError {
    match error {
        SchedulerOutboxRepositoryError::Query => ChannelIndexSourceError::Unavailable,
        SchedulerOutboxRepositoryError::Timeout => ChannelIndexSourceError::Timeout,
        SchedulerOutboxRepositoryError::Invariant => ChannelIndexSourceError::Invariant,
    }
}

fn channel_index_cache_error_kind(error: ChannelIndexCacheError) -> &'static str {
    match error {
        ChannelIndexCacheError::Source(ChannelIndexSourceError::Unavailable) => {
            "channel_index_source_unavailable"
        }
        ChannelIndexCacheError::Source(ChannelIndexSourceError::Timeout) => "channel_index_timeout",
        ChannelIndexCacheError::Source(ChannelIndexSourceError::Invariant)
        | ChannelIndexCacheError::Snapshot(_)
        | ChannelIndexCacheError::GenerationExhausted => "channel_index_invariant",
        ChannelIndexCacheError::StateUnavailable => "channel_index_state_unavailable",
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use af_db::{
        EncryptedCredentialEnvelope, SchedulerRuntimeCredentialRecord, SchedulerRuntimeProjection,
        SchedulerRuntimeTargetRecord,
    };
    use af_domain::{
        ChannelType, CredentialId, CredentialKind, Protocol, ResponsesCompactMode,
        ResponsesCompactProbeResult, RouteChannelId, RouteStrategy,
    };

    use crate::{RouteWaitKind, StickyRouteOutcome, StickyWaitPolicy};

    use super::*;

    #[test]
    fn snapshot_groups_candidates_without_leaking_models_in_debug() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                record(1, "gpt-secret", 2, 5, 0),
                record(1, "gpt-secret", 1, 9, 7),
            ],
            3,
        )
        .unwrap();

        let candidates = snapshot
            .candidates(GroupId::new(1).unwrap(), "gpt-secret")
            .unwrap();

        assert_eq!(snapshot.generation(), 3);
        assert_eq!(snapshot.key_count(), 1);
        assert_eq!(snapshot.candidate_count(), 2);
        assert_eq!(snapshot.runtime_target_count(), 2);
        assert_eq!(
            snapshot
                .model_names(GroupId::new(1).unwrap())
                .collect::<Vec<_>>(),
            vec!["gpt-secret"]
        );
        assert_eq!(candidates[0].channel_id(), ChannelId::new(1).unwrap());
        assert_eq!(candidates[0].priority(), 9);
        assert!(
            snapshot
                .runtime_target(ChannelId::new(1).unwrap())
                .is_some()
        );
        assert!(!format!("{snapshot:?}").contains("gpt-secret"));
    }

    #[test]
    fn snapshot_rejects_invalid_models_and_duplicate_channels() {
        assert_eq!(
            ChannelIndexSourceRecord::new(
                GroupId::new(1).unwrap(),
                " bad ",
                ChannelId::new(1).unwrap(),
                0,
                0,
            ),
            Err(ChannelIndexSnapshotError::InvalidModel)
        );
        assert_eq!(
            ChannelIndexSnapshot::from_records(
                vec![record(1, "gpt-5", 1, 0, 0), record(1, "gpt-5", 1, 1, 0)],
                1,
            )
            .unwrap_err(),
            ChannelIndexSnapshotError::DuplicateChannel
        );
        assert_eq!(
            validate_total_capacity(MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES + 1),
            Err(ChannelIndexSnapshotError::TotalCapacityExceeded)
        );
    }

    #[test]
    fn snapshot_excludes_records_without_runtime_targets() {
        let missing_target = ChannelIndexSourceRecord::new(
            GroupId::new(1).unwrap(),
            "gpt-5",
            ChannelId::new(1).unwrap(),
            9,
            0,
        )
        .unwrap();
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![missing_target, record(1, "gpt-5", 2, 1, 0)],
            1,
        )
        .unwrap();

        assert_eq!(snapshot.candidate_count(), 1);
        assert_eq!(snapshot.runtime_target_count(), 1);
        assert_eq!(
            snapshot
                .candidates(GroupId::new(1).unwrap(), "gpt-5")
                .unwrap()[0]
                .channel_id(),
            ChannelId::new(2).unwrap()
        );
    }

    #[test]
    fn compact_route_filters_by_native_target_mode_and_probe_fact() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                compact_record(
                    7,
                    "gpt-compact",
                    1,
                    4,
                    ResponsesCompactMode::Auto,
                    ResponsesCompactProbeResult::Supported,
                ),
                compact_record(
                    7,
                    "gpt-compact",
                    2,
                    3,
                    ResponsesCompactMode::Auto,
                    ResponsesCompactProbeResult::Unknown,
                ),
                compact_record(
                    7,
                    "gpt-compact",
                    3,
                    2,
                    ResponsesCompactMode::ForceOff,
                    ResponsesCompactProbeResult::Supported,
                ),
                compact_record(
                    7,
                    "gpt-compact",
                    4,
                    1,
                    ResponsesCompactMode::ForceOn,
                    ResponsesCompactProbeResult::Unsupported,
                ),
                record(7, "gpt-compact", 5, 0, 1),
            ],
            9,
        )
        .unwrap();
        assert!(
            snapshot
                .responses_compact_available(GroupId::new(7).unwrap(), "gpt-compact")
                .unwrap()
        );
        assert!(
            snapshot
                .responses_compact_available_any_group("gpt-compact")
                .unwrap()
        );
        assert!(
            !snapshot
                .responses_compact_available(GroupId::new(8).unwrap(), "gpt-compact")
                .unwrap()
        );
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let scheduler = IndexedWeightedScheduler::new(index);

        assert_eq!(
            scheduler
                .responses_compact_route_channel_ids(GroupId::new(7).unwrap(), "gpt-compact",)
                .unwrap(),
            vec![ChannelId::new(1).unwrap(), ChannelId::new(4).unwrap()]
        );
        let route = scheduler
            .responses_compact_route_plan(GroupId::new(7).unwrap(), "gpt-compact")
            .unwrap()
            .unwrap();
        assert_eq!(route.candidates().len(), 2);
        assert!(
            route
                .candidates()
                .iter()
                .all(|candidate| { matches!(candidate.channel_id().get(), 1 | 4) })
        );
    }

    #[tokio::test]
    async fn refresh_replaces_snapshot_and_keeps_old_snapshot_on_failure() {
        let source = Arc::new(SequenceSource::new(vec![
            Ok(vec![record(1, "gpt-5", 1, 1, 0)]),
            Ok(vec![record(1, "gpt-5", 2, 2, 0)]),
            Err(ChannelIndexSourceError::Unavailable),
        ]));
        let index = InMemoryChannelIndex::load(source).await.unwrap();

        assert_eq!(index.snapshot().unwrap().generation(), 1);
        let refreshed = index.refresh().await.unwrap();
        assert_eq!(refreshed.generation(), 2);
        assert_eq!(
            refreshed
                .candidates(GroupId::new(1).unwrap(), "gpt-5")
                .unwrap()[0]
                .channel_id(),
            ChannelId::new(2).unwrap()
        );

        assert_eq!(
            index.refresh().await.unwrap_err(),
            ChannelIndexCacheError::Source(ChannelIndexSourceError::Unavailable)
        );
        assert_eq!(index.snapshot().unwrap().generation(), 2);
    }

    #[tokio::test]
    async fn subject_projection_replaces_only_its_scope_and_ignores_stale_versions() {
        let index = InMemoryChannelIndex::load(Arc::new(SequenceSource::new(vec![Ok(vec![
            record(1, "gpt-5", 1, 9, 0),
            record(1, "gpt-5", 2, 8, 0),
            record(2, "claude", 1, 7, 0),
        ])])))
        .await
        .unwrap();
        let channel_subject = SchedulerCatalogSubject::Channel(ChannelId::new(1).unwrap());

        let report = index
            .apply_projections(vec![
                SchedulerRuntimeProjection::new(5, channel_subject, Vec::new()).unwrap(),
            ])
            .await
            .unwrap();
        assert_eq!(report.applied_subject_count(), 1);
        assert_eq!(report.generation(), 2);
        let snapshot = index.snapshot().unwrap();
        assert_eq!(snapshot.candidate_count(), 1);
        assert_eq!(
            snapshot
                .candidates(GroupId::new(1).unwrap(), "gpt-5")
                .unwrap()[0]
                .channel_id(),
            ChannelId::new(2).unwrap()
        );
        assert!(
            snapshot
                .candidates(GroupId::new(2).unwrap(), "claude")
                .unwrap()
                .is_empty()
        );

        let stale = index
            .apply_projections(vec![
                SchedulerRuntimeProjection::new(4, channel_subject, Vec::new()).unwrap(),
            ])
            .await
            .unwrap();
        assert_eq!(stale.applied_subject_count(), 0);
        assert_eq!(stale.generation(), 2);

        let group_subject = SchedulerCatalogSubject::Group(GroupId::new(1).unwrap());
        let deduplicated = index
            .apply_projections(vec![
                SchedulerRuntimeProjection::new(6, group_subject, Vec::new()).unwrap(),
                SchedulerRuntimeProjection::new(7, group_subject, Vec::new()).unwrap(),
            ])
            .await
            .unwrap();
        assert_eq!(deduplicated.applied_subject_count(), 1);
        assert_eq!(deduplicated.generation(), 3);
        assert_eq!(index.snapshot().unwrap().candidate_count(), 0);
    }

    #[tokio::test]
    async fn full_snapshot_high_watermark_blocks_late_projection() {
        let snapshot =
            ChannelIndexSnapshot::from_versioned_records(vec![record(1, "gpt-5", 1, 1, 0)], 3, 10)
                .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let report = index
            .apply_projections(vec![
                SchedulerRuntimeProjection::new(
                    10,
                    SchedulerCatalogSubject::Channel(ChannelId::new(1).unwrap()),
                    Vec::new(),
                )
                .unwrap(),
            ])
            .await
            .unwrap();

        assert_eq!(report.applied_subject_count(), 0);
        assert_eq!(report.generation(), 3);
        assert_eq!(index.snapshot().unwrap().candidate_count(), 1);
    }

    #[test]
    fn indexed_scheduler_reuses_weighted_retry_semantics() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![record(1, "gpt-5", 1, 9, 0), record(1, "gpt-5", 2, 1, 0)],
            1,
        )
        .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let mut plan = IndexedWeightedScheduler::new(index)
            .retry_plan(GroupId::new(1).unwrap(), "gpt-5")
            .unwrap();

        assert_eq!(plan.select_next().unwrap().unwrap().priority_rank(), 0);
        assert_eq!(plan.select_next().unwrap().unwrap().priority_rank(), 1);
        assert_eq!(plan.select_next().unwrap(), None);
    }

    #[test]
    fn route_plan_pins_targets_and_billing_group_to_one_snapshot() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                record(7, "gpt-route", 1, 9, 0),
                record(7, "gpt-route", 2, 1, 0),
            ],
            11,
        )
        .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };

        let route = IndexedWeightedScheduler::new(index)
            .route_plan(GroupId::new(7).unwrap(), "gpt-route")
            .unwrap()
            .unwrap();

        assert_eq!(route.generation(), 11);
        assert_eq!(route.target_group_id(), GroupId::new(7).unwrap());
        assert_eq!(route.candidates().len(), 2);
        for candidate in route.candidates() {
            assert_eq!(
                candidate.selection().candidate().channel_id(),
                candidate.runtime_target().channel_id()
            );
        }
        assert!(!format!("{route:?}").contains("gpt-route"));
    }

    #[test]
    fn bound_route_plan_intersects_abilities_and_keeps_only_bound_credentials() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                ChannelIndexSourceRecord::with_runtime_target(
                    GroupId::new(7).unwrap(),
                    "gpt-routed",
                    1,
                    1,
                    Arc::new(runtime_target_with_credentials(1, &[101, 102])),
                )
                .unwrap(),
            ],
            12,
        )
        .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let scheduler = IndexedWeightedScheduler::new(index);
        let bindings = [BoundRouteCandidate::new(
            RouteChannelId::new(41).unwrap(),
            ChannelId::new(1).unwrap(),
            CredentialId::new(102).unwrap(),
            20,
            5,
            None,
        )
        .unwrap()];

        assert!(
            scheduler
                .bound_route_plan(
                    GroupId::new(8).unwrap(),
                    "gpt-routed",
                    RouteStrategy::Weighted,
                    &bindings,
                    &BTreeMap::new(),
                )
                .unwrap()
                .is_none()
        );

        let plan = scheduler
            .bound_route_plan(
                GroupId::new(7).unwrap(),
                "gpt-routed",
                RouteStrategy::Weighted,
                &bindings,
                &BTreeMap::new(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(plan.target_group_id(), GroupId::new(7).unwrap());
        assert_eq!(plan.candidates().len(), 1);
        assert_eq!(
            plan.candidates()[0]
                .runtime_target()
                .credentials()
                .iter()
                .map(SchedulerRuntimeCredentialRecord::credential_id)
                .collect::<Vec<_>>(),
            [102]
        );
        assert_eq!(
            plan.candidates()[0].route_channel_id(CredentialId::new(102).unwrap()),
            Some(RouteChannelId::new(41).unwrap())
        );
    }

    #[test]
    fn pool_mode_ignores_channel_cooling_but_keeps_other_channels_filtered() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                record(7, "gpt-pool", 1, 0, 1),
                pool_record(7, "gpt-pool", 2, 0, 1),
            ],
            11,
        )
        .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let health = BTreeMap::from([
            (
                ChannelId::new(1).unwrap(),
                ChannelRoutingHealth::new(0, true),
            ),
            (
                ChannelId::new(2).unwrap(),
                ChannelRoutingHealth::new(0, true),
            ),
        ]);

        let route = IndexedWeightedScheduler::new(index)
            .route_plan_with_health(GroupId::new(7).unwrap(), "gpt-pool", &health)
            .unwrap()
            .unwrap();
        assert_eq!(route.candidates().len(), 1);
        assert_eq!(
            route.candidates()[0].channel_id(),
            ChannelId::new(2).unwrap()
        );
    }

    #[test]
    fn sticky_channel_moves_first_and_exposes_short_then_fallback_waits() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                record(7, "gpt-sticky", 1, 10, 1),
                record(7, "gpt-sticky", 2, 0, 1),
            ],
            12,
        )
        .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let mut route = IndexedWeightedScheduler::new(index)
            .route_plan(GroupId::new(7).unwrap(), "gpt-sticky")
            .unwrap()
            .unwrap();
        let policy = StickyWaitPolicy::new(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(10),
        )
        .unwrap();

        assert_eq!(
            route.prefer_sticky_channel(ChannelId::new(2).unwrap(), policy),
            StickyRouteOutcome::Applied
        );
        assert_eq!(
            route.candidates()[0].channel_id(),
            ChannelId::new(2).unwrap()
        );
        assert_eq!(
            route.candidates()[0].wait_plan().kind(),
            RouteWaitKind::Sticky
        );
        assert_eq!(
            route.candidates()[0].wait_plan().timeout(),
            std::time::Duration::from_secs(1)
        );
        assert_eq!(route.candidates()[0].selection().attempt().get(), 1);
        assert_eq!(route.candidates()[0].selection().priority_rank(), 1);
        assert_eq!(
            route.candidates()[1].wait_plan().kind(),
            RouteWaitKind::Fallback
        );
        assert_eq!(
            route.candidates()[1].wait_plan().timeout(),
            std::time::Duration::from_secs(10)
        );
        assert_eq!(route.candidates()[1].selection().attempt().get(), 2);
        assert_eq!(route.candidates()[1].selection().priority_rank(), 0);
        assert_eq!(
            route.prefer_sticky_channel(ChannelId::new(99).unwrap(), policy),
            StickyRouteOutcome::Unavailable
        );
    }

    #[test]
    fn unavailable_sticky_channel_resets_all_candidates_to_fallback_waits() {
        let snapshot = ChannelIndexSnapshot::from_records(
            vec![
                record(7, "gpt-sticky-unavailable", 1, 10, 1),
                record(7, "gpt-sticky-unavailable", 2, 0, 1),
            ],
            13,
        )
        .unwrap();
        let index = InMemoryChannelIndex {
            inner: Arc::new(InMemoryChannelIndexInner {
                source: Arc::new(SequenceSource::new(Vec::new())),
                snapshot: RwLock::new(Arc::new(snapshot)),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        };
        let mut route = IndexedWeightedScheduler::new(index)
            .route_plan(GroupId::new(7).unwrap(), "gpt-sticky-unavailable")
            .unwrap()
            .unwrap();
        let policy = StickyWaitPolicy::new(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(10),
        )
        .unwrap();

        assert_eq!(
            route.prefer_sticky_channel(ChannelId::new(99).unwrap(), policy),
            StickyRouteOutcome::Unavailable
        );
        assert_eq!(
            route
                .candidates()
                .iter()
                .map(|candidate| candidate.channel_id())
                .collect::<Vec<_>>(),
            vec![ChannelId::new(1).unwrap(), ChannelId::new(2).unwrap()]
        );
        assert!(
            route
                .candidates()
                .iter()
                .all(|candidate| candidate.wait_plan().kind() == RouteWaitKind::Fallback)
        );
        assert!(
            route
                .candidates()
                .iter()
                .all(|candidate| candidate.wait_plan().timeout()
                    == std::time::Duration::from_secs(10))
        );
    }

    fn record(
        group_id: i64,
        model: &str,
        channel_id: i64,
        priority: i32,
        weight: u32,
    ) -> ChannelIndexSourceRecord {
        ChannelIndexSourceRecord::with_runtime_target(
            GroupId::new(group_id).unwrap(),
            model,
            priority,
            weight,
            Arc::new(runtime_target(channel_id)),
        )
        .unwrap()
    }

    fn pool_record(
        group_id: i64,
        model: &str,
        channel_id: i64,
        priority: i32,
        weight: u32,
    ) -> ChannelIndexSourceRecord {
        ChannelIndexSourceRecord::with_runtime_target(
            GroupId::new(group_id).unwrap(),
            model,
            priority,
            weight,
            Arc::new(runtime_target(channel_id).with_pool_mode(true)),
        )
        .unwrap()
    }

    fn compact_record(
        group_id: i64,
        model: &str,
        channel_id: i64,
        weight: u32,
        mode: ResponsesCompactMode,
        result: ResponsesCompactProbeResult,
    ) -> ChannelIndexSourceRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            100 + channel_id,
            CredentialKind::ApiKey,
            EncryptedCredentialEnvelope::new("test-key", [0x42; 24], vec![0x24; 16]).unwrap(),
            false,
        )
        .unwrap();
        let target = SchedulerRuntimeTargetRecord::new(
            ChannelId::new(channel_id).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiResponses,
            Some(format!("https://channel-{channel_id}.example.com")),
            credential,
            Vec::new(),
        )
        .unwrap()
        .with_responses_compact_mode(mode)
        .and_then(|target| target.with_responses_compact_probe_result(result))
        .unwrap();
        ChannelIndexSourceRecord::with_runtime_target(
            GroupId::new(group_id).unwrap(),
            model,
            0,
            weight,
            Arc::new(target),
        )
        .unwrap()
    }

    fn runtime_target(channel_id: i64) -> SchedulerRuntimeTargetRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            100 + channel_id,
            CredentialKind::ApiKey,
            EncryptedCredentialEnvelope::new("test-key", [0x42; 24], vec![0x24; 16]).unwrap(),
            false,
        )
        .unwrap();
        SchedulerRuntimeTargetRecord::new(
            ChannelId::new(channel_id).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some(format!("https://channel-{channel_id}.example.com")),
            credential,
            Vec::new(),
        )
        .unwrap()
    }

    fn runtime_target_with_credentials(
        channel_id: i64,
        credential_ids: &[i64],
    ) -> SchedulerRuntimeTargetRecord {
        let credentials = credential_ids
            .iter()
            .map(|credential_id| {
                SchedulerRuntimeCredentialRecord::new(
                    *credential_id,
                    CredentialKind::ApiKey,
                    EncryptedCredentialEnvelope::new(
                        "test-key",
                        [u8::try_from(*credential_id).unwrap_or(0x42); 24],
                        vec![0x24; 16],
                    )
                    .unwrap(),
                    false,
                )
                .unwrap()
            })
            .collect();
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(channel_id).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some(format!("https://channel-{channel_id}.example.com")),
            credentials,
            Vec::new(),
        )
        .unwrap()
    }

    #[derive(Clone)]
    struct SequenceSource {
        results:
            Arc<Mutex<VecDeque<Result<Vec<ChannelIndexSourceRecord>, ChannelIndexSourceError>>>>,
    }

    impl SequenceSource {
        fn new(
            results: Vec<Result<Vec<ChannelIndexSourceRecord>, ChannelIndexSourceError>>,
        ) -> Self {
            Self {
                results: Arc::new(Mutex::new(results.into())),
            }
        }
    }

    impl ChannelIndexSource for SequenceSource {
        fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
            Box::pin(async move {
                self.results
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| Ok(Vec::new()))
            })
        }
    }
}
