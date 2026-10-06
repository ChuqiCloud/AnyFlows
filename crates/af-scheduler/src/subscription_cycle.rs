use std::{
    fmt,
    future::Future,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    SubscriptionExpirationDueCursor, SubscriptionRepositoryError, SubscriptionResetDueCursor,
    UserSubscriptionLifecycleTransition, UserSubscriptionWindowAdvance,
};
use tokio::time::sleep;

mod store;
mod types;

pub use store::SubscriptionCycleStore;
pub use types::{
    DEFAULT_SUBSCRIPTION_CYCLE_BATCH_SIZE, DEFAULT_SUBSCRIPTION_CYCLE_INTERVAL,
    DEFAULT_SUBSCRIPTION_CYCLE_MAX_BATCHES_PER_RUN, MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN,
    SubscriptionCycleMutationOutcome, SubscriptionCyclePhaseReport, SubscriptionCycleRunReport,
    SubscriptionCycleSupervisorConfig, SubscriptionCycleSupervisorConfigError,
    SubscriptionCycleSupervisorError, SubscriptionExpirationBatch, SubscriptionWindowAdvanceBatch,
};

/// 周期推进 Active 窗口并把到期 Canceled 订阅迁移为 Expired 的执行器。
pub struct SubscriptionCycleSupervisor<S> {
    store: S,
    config: SubscriptionCycleSupervisorConfig,
}

impl<S> SubscriptionCycleSupervisor<S> {
    /// 使用显式持久化端口与有界配置创建执行器。
    #[must_use]
    pub const fn new(store: S, config: SubscriptionCycleSupervisorConfig) -> Self {
        Self { store, config }
    }
}

impl<S> SubscriptionCycleSupervisor<S>
where
    S: SubscriptionCycleStore,
{
    /// 使用单个受信系统时刻执行一轮 Active 窗口推进和 Canceled 过期迁移。
    pub async fn run_once(
        &self,
    ) -> Result<SubscriptionCycleRunReport, SubscriptionCycleSupervisorError> {
        let now = unix_now()?;
        Ok(self.run_once_at(now).await)
    }

    /// 启动后立即扫描，此后按固定间隔运行直到统一关闭信号完成。
    pub async fn run_periodic_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => return,
                result = self.run_once() => log_run_result(result),
            }

            tokio::select! {
                biased;
                () = &mut shutdown => return,
                () = sleep(self.config.interval()) => {}
            }
        }
    }

    async fn run_once_at(&self, now: u64) -> SubscriptionCycleRunReport {
        let window_advances = self.run_window_advances(now).await;
        // 两类扫描相互独立，窗口推进读取失败不能阻止取消订阅继续过期。
        let expirations = self.run_expirations(now).await;
        SubscriptionCycleRunReport {
            window_advances,
            expirations,
        }
    }

    async fn run_window_advances(&self, now: u64) -> SubscriptionCyclePhaseReport {
        let mut report = SubscriptionCyclePhaseReport::default();
        let mut cursor = None;
        for batch_index in 0..self.config.max_batches_per_run() {
            let batch = match self
                .store
                .load_window_advances(now, cursor, self.config.batch_size())
                .await
            {
                Ok(batch) => batch,
                Err(_) => {
                    report.scan_failed = true;
                    break;
                }
            };
            report.batches += 1;
            let (commands, next_cursor) = batch.into_parts();
            if invalid_reset_page(&commands, cursor, next_cursor, self.config.batch_size()) {
                report.scan_failed = true;
                break;
            }
            report.loaded += commands.len();
            for command in &commands {
                self.apply_window_advance(command, &mut report).await;
            }
            let Some(next_cursor) = next_cursor else {
                break;
            };
            if batch_index + 1 == self.config.max_batches_per_run() {
                report.truncated = true;
                break;
            }
            cursor = Some(next_cursor);
        }
        report
    }

    async fn run_expirations(&self, now: u64) -> SubscriptionCyclePhaseReport {
        let mut report = SubscriptionCyclePhaseReport::default();
        let mut cursor = None;
        for batch_index in 0..self.config.max_batches_per_run() {
            let batch = match self
                .store
                .load_expirations(now, cursor, self.config.batch_size())
                .await
            {
                Ok(batch) => batch,
                Err(_) => {
                    report.scan_failed = true;
                    break;
                }
            };
            report.batches += 1;
            let (commands, next_cursor) = batch.into_parts();
            if invalid_expiration_page(&commands, cursor, next_cursor, self.config.batch_size()) {
                report.scan_failed = true;
                break;
            }
            report.loaded += commands.len();
            for command in &commands {
                self.apply_expiration(command, &mut report).await;
            }
            let Some(next_cursor) = next_cursor else {
                break;
            };
            if batch_index + 1 == self.config.max_batches_per_run() {
                report.truncated = true;
                break;
            }
            cursor = Some(next_cursor);
        }
        report
    }

    async fn apply_window_advance(
        &self,
        command: &UserSubscriptionWindowAdvance,
        report: &mut SubscriptionCyclePhaseReport,
    ) {
        let result = self.store.advance_window(command).await;
        let result = if matches!(result, Err(SubscriptionRepositoryError::OutcomeUnknown)) {
            report.outcome_unknown_replays += 1;
            // 保留同一个命令对象，避免重放时重新取时钟或拼接窗口事实。
            self.store.advance_window(command).await
        } else {
            result
        };
        record_mutation_result(report, result);
    }

    async fn apply_expiration(
        &self,
        command: &UserSubscriptionLifecycleTransition,
        report: &mut SubscriptionCyclePhaseReport,
    ) {
        let result = self.store.expire_subscription(command).await;
        let result = if matches!(result, Err(SubscriptionRepositoryError::OutcomeUnknown)) {
            report.outcome_unknown_replays += 1;
            // 生命周期变更时间属于幂等事实，结果未知时只能重放原命令。
            self.store.expire_subscription(command).await
        } else {
            result
        };
        record_mutation_result(report, result);
    }
}

impl<S> fmt::Debug for SubscriptionCycleSupervisor<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionCycleSupervisor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

fn record_mutation_result(
    report: &mut SubscriptionCyclePhaseReport,
    result: Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError>,
) {
    match result {
        Ok(SubscriptionCycleMutationOutcome::Applied) => report.applied += 1,
        Ok(SubscriptionCycleMutationOutcome::Existing) => report.existing += 1,
        Ok(SubscriptionCycleMutationOutcome::Skipped) => report.skipped += 1,
        Err(
            SubscriptionRepositoryError::Conflict | SubscriptionRepositoryError::BindingConflict,
        ) => report.conflicted += 1,
        Err(
            SubscriptionRepositoryError::Query
            | SubscriptionRepositoryError::Timeout
            | SubscriptionRepositoryError::OutcomeUnknown
            | SubscriptionRepositoryError::Invariant,
        ) => report.failed += 1,
    }
}

fn invalid_reset_page(
    commands: &[UserSubscriptionWindowAdvance],
    previous: Option<SubscriptionResetDueCursor>,
    next: Option<SubscriptionResetDueCursor>,
    batch_size: usize,
) -> bool {
    commands.len() > batch_size
        || (commands.is_empty() && next.is_some())
        || previous
            .zip(next)
            .is_some_and(|(previous, next)| !reset_cursor_advances(previous, next))
}

fn invalid_expiration_page(
    commands: &[UserSubscriptionLifecycleTransition],
    previous: Option<SubscriptionExpirationDueCursor>,
    next: Option<SubscriptionExpirationDueCursor>,
    batch_size: usize,
) -> bool {
    commands.len() > batch_size
        || (commands.is_empty() && next.is_some())
        || previous
            .zip(next)
            .is_some_and(|(previous, next)| !expiration_cursor_advances(previous, next))
}

const fn reset_cursor_advances(
    previous: SubscriptionResetDueCursor,
    next: SubscriptionResetDueCursor,
) -> bool {
    next.window_ends_at() > previous.window_ends_at()
        || (next.window_ends_at() == previous.window_ends_at()
            && next.database_id() > previous.database_id())
}

const fn expiration_cursor_advances(
    previous: SubscriptionExpirationDueCursor,
    next: SubscriptionExpirationDueCursor,
) -> bool {
    next.window_ends_at() > previous.window_ends_at()
        || (next.window_ends_at() == previous.window_ends_at()
            && next.database_id() > previous.database_id())
}

fn unix_now() -> Result<u64, SubscriptionCycleSupervisorError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SubscriptionCycleSupervisorError::InvalidClock)?
        .as_secs();
    if now > i64::MAX as u64 {
        return Err(SubscriptionCycleSupervisorError::InvalidClock);
    }
    Ok(now)
}

fn log_run_result(result: Result<SubscriptionCycleRunReport, SubscriptionCycleSupervisorError>) {
    match result {
        Ok(report) if report.has_failures() => {
            let windows = report.window_advances();
            let expirations = report.expirations();
            tracing::warn!(
                window_scan_failed = windows.scan_failed(),
                window_failed = windows.failed(),
                expiration_scan_failed = expirations.scan_failed(),
                expiration_failed = expirations.failed(),
                "订阅周期任务完成扫描，但存在未确认操作"
            );
        }
        Ok(report) if report.has_activity() => {
            let windows = report.window_advances();
            let expirations = report.expirations();
            tracing::info!(
                windows_loaded = windows.loaded(),
                windows_applied = windows.applied(),
                windows_existing = windows.existing(),
                windows_skipped = windows.skipped(),
                windows_conflicted = windows.conflicted(),
                windows_replayed = windows.outcome_unknown_replays(),
                windows_truncated = windows.truncated(),
                expirations_loaded = expirations.loaded(),
                expirations_applied = expirations.applied(),
                expirations_existing = expirations.existing(),
                expirations_skipped = expirations.skipped(),
                expirations_conflicted = expirations.conflicted(),
                expirations_replayed = expirations.outcome_unknown_replays(),
                expirations_truncated = expirations.truncated(),
                "订阅周期任务完成一轮扫描"
            );
        }
        Ok(_) => tracing::debug!("订阅周期任务完成空闲扫描"),
        Err(_) => tracing::error!(
            error_kind = "subscription_cycle_clock",
            "订阅周期任务无法读取有效系统时间"
        ),
    }
}

#[cfg(test)]
mod tests;
