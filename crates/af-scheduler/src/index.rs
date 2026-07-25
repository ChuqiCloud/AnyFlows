use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};

use af_db::{
    DatabasePool, MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES, SchedulerAbilityRepository,
    SchedulerAbilityRepositoryError,
};
use af_domain::{ChannelId, GroupId, MAX_MODEL_NAME_BYTES};
use thiserror::Error;
use tokio::time::sleep;

use crate::{
    SchedulerCandidate, WeightedRetryPlan, WeightedRetryPlanError, WeightedSelection,
    WeightedSelectionError, WeightedStrategy,
};

/// 默认渠道索引全量重建周期；后续 M3 会改为事件驱动增量并保留周期校正。
pub const DEFAULT_CHANNEL_INDEX_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

/// 渠道索引数据源的一次全量读取结果。
pub type ChannelIndexSourceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<ChannelIndexSourceRecord>, ChannelIndexSourceError>>
            + Send
            + 'a,
    >,
>;

/// 可替换的渠道索引完整数据源。
///
/// 当前数据库实现每次返回完整有效能力目录；未来 Redis/pub-sub 增量快照落地时，
/// 只需替换数据源与失效通知，不改变请求侧快照读取方式。
pub trait ChannelIndexSource: Send + Sync + 'static {
    /// 读取一个完整且内部一致的渠道能力目录。
    fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a>;
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

/// 来源提供的单条有效能力记录。
#[derive(Clone, Eq, PartialEq)]
pub struct ChannelIndexSourceRecord {
    group_id: GroupId,
    model: String,
    channel_id: ChannelId,
    priority: i32,
    weight: u32,
}

impl ChannelIndexSourceRecord {
    /// 使用已校验标识与调度元数据构造索引来源记录。
    pub fn new(
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
        })
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
    entries: BTreeMap<GroupId, BTreeMap<String, Arc<[SchedulerCandidate]>>>,
    key_count: usize,
    candidate_count: usize,
}

impl ChannelIndexSnapshot {
    /// 从完整来源记录构建新快照；同一键下候选会在发布前完成容量和去重校验。
    pub fn from_records(
        records: Vec<ChannelIndexSourceRecord>,
        generation: u64,
    ) -> Result<Self, ChannelIndexSnapshotError> {
        if generation == 0 {
            return Err(ChannelIndexSnapshotError::Invariant);
        }
        validate_total_capacity(records.len())?;
        let mut grouped = BTreeMap::<GroupId, BTreeMap<String, Vec<SchedulerCandidate>>>::new();
        for record in records {
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
            entries,
            key_count,
            candidate_count,
        })
    }

    /// 返回快照单调代数，便于观测刷新是否成功发布。
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
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
}

impl fmt::Debug for ChannelIndexSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelIndexSnapshot")
            .field("generation", &self.generation)
            .field("key_count", &self.key_count)
            .field("candidate_count", &self.candidate_count)
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
        let snapshot = Arc::new(ChannelIndexSnapshot::from_records(source.load().await?, 1)?);
        Ok(Self {
            inner: Arc::new(InMemoryChannelIndexInner {
                source,
                snapshot: RwLock::new(snapshot),
                refresh_gate: tokio::sync::Mutex::new(()),
            }),
        })
    }

    /// 使用数据库能力仓储作为来源加载首个快照。
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
        let next = Arc::new(ChannelIndexSnapshot::from_records(
            self.inner.source.load().await?,
            generation,
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
    repository: SchedulerAbilityRepository,
}

impl DatabaseChannelIndexSource {
    fn new(database: DatabasePool) -> Self {
        Self {
            repository: SchedulerAbilityRepository::new(database),
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
                    ChannelIndexSourceRecord::new(
                        record.group_id(),
                        record.model().to_owned(),
                        record.channel_id(),
                        record.priority(),
                        record.weight(),
                    )
                    .map_err(|_| ChannelIndexSourceError::Invariant)
                })
                .collect()
        })
    }
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

fn map_repository_error(error: SchedulerAbilityRepositoryError) -> ChannelIndexSourceError {
    match error {
        SchedulerAbilityRepositoryError::Query => ChannelIndexSourceError::Unavailable,
        SchedulerAbilityRepositoryError::Timeout => ChannelIndexSourceError::Timeout,
        SchedulerAbilityRepositoryError::InvalidConfiguration
        | SchedulerAbilityRepositoryError::InvalidModel
        | SchedulerAbilityRepositoryError::Invariant => ChannelIndexSourceError::Invariant,
        _ => ChannelIndexSourceError::Invariant,
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
        assert_eq!(candidates[0].channel_id(), ChannelId::new(1).unwrap());
        assert_eq!(candidates[0].priority(), 9);
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

    fn record(
        group_id: i64,
        model: &str,
        channel_id: i64,
        priority: i32,
        weight: u32,
    ) -> ChannelIndexSourceRecord {
        ChannelIndexSourceRecord::new(
            GroupId::new(group_id).unwrap(),
            model,
            ChannelId::new(channel_id).unwrap(),
            priority,
            weight,
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
