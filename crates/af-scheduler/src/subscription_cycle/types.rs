use std::{fmt, time::Duration};

use af_db::{
    MAX_SUBSCRIPTION_PAGE_SIZE, SubscriptionExpirationDueCursor, SubscriptionResetDueCursor,
    UserSubscriptionLifecycleTransition, UserSubscriptionWindowAdvance,
};
use thiserror::Error;

/// 订阅周期任务默认单批记录数。
pub const DEFAULT_SUBSCRIPTION_CYCLE_BATCH_SIZE: usize = 64;
/// 订阅周期任务默认执行间隔。
pub const DEFAULT_SUBSCRIPTION_CYCLE_INTERVAL: Duration = Duration::from_secs(60);
/// 每类订阅状态单轮默认最多扫描的批次数。
pub const DEFAULT_SUBSCRIPTION_CYCLE_MAX_BATCHES_PER_RUN: usize = 8;
/// 每类订阅状态单轮扫描批次数硬上限。
pub const MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN: usize = 64;

/// 到期 Active 订阅窗口推进命令的稳定分页结果。
pub struct SubscriptionWindowAdvanceBatch {
    commands: Vec<UserSubscriptionWindowAdvance>,
    next_cursor: Option<SubscriptionResetDueCursor>,
}

impl SubscriptionWindowAdvanceBatch {
    /// 使用已固化命令和下一页游标创建批次。
    #[must_use]
    pub fn new(
        commands: Vec<UserSubscriptionWindowAdvance>,
        next_cursor: Option<SubscriptionResetDueCursor>,
    ) -> Self {
        Self {
            commands,
            next_cursor,
        }
    }

    /// 返回当前批次包含的命令数。
    #[must_use]
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// 返回继续扫描到期 Active 订阅的游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<SubscriptionResetDueCursor> {
        self.next_cursor
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Vec<UserSubscriptionWindowAdvance>,
        Option<SubscriptionResetDueCursor>,
    ) {
        (self.commands, self.next_cursor)
    }
}

impl fmt::Debug for SubscriptionWindowAdvanceBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionWindowAdvanceBatch")
            .field("command_count", &self.commands.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 到期 Canceled 订阅过期命令的稳定分页结果。
pub struct SubscriptionExpirationBatch {
    commands: Vec<UserSubscriptionLifecycleTransition>,
    next_cursor: Option<SubscriptionExpirationDueCursor>,
}

impl SubscriptionExpirationBatch {
    /// 使用已固化命令和下一页游标创建批次。
    #[must_use]
    pub fn new(
        commands: Vec<UserSubscriptionLifecycleTransition>,
        next_cursor: Option<SubscriptionExpirationDueCursor>,
    ) -> Self {
        Self {
            commands,
            next_cursor,
        }
    }

    /// 返回当前批次包含的命令数。
    #[must_use]
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// 返回继续扫描到期 Canceled 订阅的游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<SubscriptionExpirationDueCursor> {
        self.next_cursor
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Vec<UserSubscriptionLifecycleTransition>,
        Option<SubscriptionExpirationDueCursor>,
    ) {
        (self.commands, self.next_cursor)
    }
}

impl fmt::Debug for SubscriptionExpirationBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionExpirationBatch")
            .field("command_count", &self.commands.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 订阅周期任务写入的归一化闭合结果。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscriptionCycleMutationOutcome {
    /// 本次调用提交了窗口推进或状态迁移。
    Applied,
    /// 相同命令已经提交，本次确认了幂等重放。
    Existing,
    /// 目标已删除、不再到期或状态已不属于当前扫描类别。
    Skipped,
}

/// 订阅周期执行器配置错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SubscriptionCycleSupervisorConfigError {
    /// 单批记录数为零或超过数据库分页硬上限。
    #[error("订阅周期任务批次大小无效")]
    InvalidBatchSize,
    /// 周期为零会造成忙轮询。
    #[error("订阅周期任务执行间隔必须大于零")]
    ZeroInterval,
    /// 单轮批次数为零或超过调度层硬上限。
    #[error("订阅周期任务单轮批次数无效")]
    InvalidMaxBatchesPerRun,
}

/// 有界订阅生命周期周期任务配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionCycleSupervisorConfig {
    batch_size: usize,
    interval: Duration,
    max_batches_per_run: usize,
}

impl SubscriptionCycleSupervisorConfig {
    /// 校验并创建订阅周期任务配置。
    pub fn new(
        batch_size: usize,
        interval: Duration,
        max_batches_per_run: usize,
    ) -> Result<Self, SubscriptionCycleSupervisorConfigError> {
        if batch_size == 0 || batch_size > MAX_SUBSCRIPTION_PAGE_SIZE {
            return Err(SubscriptionCycleSupervisorConfigError::InvalidBatchSize);
        }
        if interval.is_zero() {
            return Err(SubscriptionCycleSupervisorConfigError::ZeroInterval);
        }
        if max_batches_per_run == 0 || max_batches_per_run > MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN
        {
            return Err(SubscriptionCycleSupervisorConfigError::InvalidMaxBatchesPerRun);
        }
        Ok(Self {
            batch_size,
            interval,
            max_batches_per_run,
        })
    }

    /// 返回单次数据库分页的记录上限。
    #[must_use]
    pub const fn batch_size(self) -> usize {
        self.batch_size
    }

    /// 返回两轮扫描之间的等待时间。
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }

    /// 返回每类订阅状态单轮最多扫描的批次数。
    #[must_use]
    pub const fn max_batches_per_run(self) -> usize {
        self.max_batches_per_run
    }
}

impl Default for SubscriptionCycleSupervisorConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_SUBSCRIPTION_CYCLE_BATCH_SIZE,
            interval: DEFAULT_SUBSCRIPTION_CYCLE_INTERVAL,
            max_batches_per_run: DEFAULT_SUBSCRIPTION_CYCLE_MAX_BATCHES_PER_RUN,
        }
    }
}

/// 单类订阅状态在一轮任务中的聚合结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionCyclePhaseReport {
    pub(super) batches: usize,
    pub(super) loaded: usize,
    pub(super) applied: usize,
    pub(super) existing: usize,
    pub(super) skipped: usize,
    pub(super) conflicted: usize,
    pub(super) failed: usize,
    pub(super) outcome_unknown_replays: usize,
    pub(super) scan_failed: bool,
    pub(super) truncated: bool,
}

impl SubscriptionCyclePhaseReport {
    /// 返回本轮成功加载的分页数量，空页也计入一次扫描。
    #[must_use]
    pub const fn batches(self) -> usize {
        self.batches
    }

    /// 返回本轮加载到的命令数量。
    #[must_use]
    pub const fn loaded(self) -> usize {
        self.loaded
    }

    /// 返回本轮首次成功提交的命令数量。
    #[must_use]
    pub const fn applied(self) -> usize {
        self.applied
    }

    /// 返回本轮确认已经提交的幂等命令数量。
    #[must_use]
    pub const fn existing(self) -> usize {
        self.existing
    }

    /// 返回目标已不属于当前扫描类别而跳过的命令数量。
    #[must_use]
    pub const fn skipped(self) -> usize {
        self.skipped
    }

    /// 返回被其他实例或生命周期操作抢先提交的 CAS 冲突数量。
    #[must_use]
    pub const fn conflicted(self) -> usize {
        self.conflicted
    }

    /// 返回重放后仍未能确认或遇到持久化故障的命令数量。
    #[must_use]
    pub const fn failed(self) -> usize {
        self.failed
    }

    /// 返回因首次结果未知而使用原命令重放的次数。
    #[must_use]
    pub const fn outcome_unknown_replays(self) -> usize {
        self.outcome_unknown_replays
    }

    /// 返回本轮是否遇到分页读取或游标契约故障。
    #[must_use]
    pub const fn scan_failed(self) -> bool {
        self.scan_failed
    }

    /// 返回本轮是否因批次数硬上限而保留了下一页。
    #[must_use]
    pub const fn truncated(self) -> bool {
        self.truncated
    }

    pub(super) const fn has_activity(self) -> bool {
        self.loaded > 0
    }

    pub(super) const fn has_failures(self) -> bool {
        self.scan_failed || self.failed > 0
    }
}

/// 一轮订阅周期任务的聚合结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionCycleRunReport {
    pub(super) window_advances: SubscriptionCyclePhaseReport,
    pub(super) expirations: SubscriptionCyclePhaseReport,
}

impl SubscriptionCycleRunReport {
    /// 返回 Active 窗口推进阶段的报告。
    #[must_use]
    pub const fn window_advances(self) -> SubscriptionCyclePhaseReport {
        self.window_advances
    }

    /// 返回 Canceled 过期迁移阶段的报告。
    #[must_use]
    pub const fn expirations(self) -> SubscriptionCyclePhaseReport {
        self.expirations
    }

    pub(super) const fn has_activity(self) -> bool {
        self.window_advances.has_activity() || self.expirations.has_activity()
    }

    pub(super) const fn has_failures(self) -> bool {
        self.window_advances.has_failures() || self.expirations.has_failures()
    }
}

/// 订阅周期任务单轮执行错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SubscriptionCycleSupervisorError {
    /// 系统时间早于 Unix Epoch 或超过数据库可表示范围。
    #[error("订阅周期任务系统时间无效")]
    InvalidClock,
}
