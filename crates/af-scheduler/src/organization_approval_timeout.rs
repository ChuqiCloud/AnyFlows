use std::{
    fmt,
    future::Future,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationApprovalRepository, OrganizationApprovalRepositoryError,
    OrganizationApprovalTimeoutBatch, OrganizationApprovalTimeoutCommand,
    OrganizationApprovalTimeoutCursor, OrganizationApprovalTimeoutLeaseOutcome,
    OrganizationApprovalTimeoutOutcome,
};
use thiserror::Error;
use tokio::time::sleep;

/// 企业审批超时任务默认单批申请数。
pub const DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_BATCH_SIZE: usize = 64;
/// 企业审批超时任务默认执行间隔。
pub const DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_INTERVAL: Duration = Duration::from_secs(60);
/// 企业审批超时任务默认租约时长。
pub const DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_LEASE: Duration = Duration::from_secs(120);
/// 企业审批超时任务单轮默认最多扫描批次数。
pub const DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_MAX_BATCHES_PER_RUN: usize = 8;
/// 企业审批超时任务单轮扫描批次数硬上限。
pub const MAX_ORGANIZATION_APPROVAL_TIMEOUT_MAX_BATCHES_PER_RUN: usize = 64;

/// 超时扫描执行器配置错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationApprovalTimeoutSupervisorConfigError {
    /// 批次大小不能超过数据库分页边界。
    #[error("企业审批超时任务批次大小无效")]
    InvalidBatchSize,
    /// 周期为零会造成忙轮询。
    #[error("企业审批超时任务执行间隔必须大于零")]
    ZeroInterval,
    /// 租约时长为零无法提供分布式互斥。
    #[error("企业审批超时任务租约时长必须大于零")]
    ZeroLease,
    /// 单轮批次数必须有界。
    #[error("企业审批超时任务单轮批次数无效")]
    InvalidMaxBatchesPerRun,
}

/// 有界企业审批超时任务配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalTimeoutSupervisorConfig {
    batch_size: usize,
    interval: Duration,
    lease_duration: Duration,
    max_batches_per_run: usize,
}

impl OrganizationApprovalTimeoutSupervisorConfig {
    /// 校验并创建超时任务配置。
    pub fn new(
        batch_size: usize,
        interval: Duration,
        lease_duration: Duration,
        max_batches_per_run: usize,
    ) -> Result<Self, OrganizationApprovalTimeoutSupervisorConfigError> {
        if batch_size == 0 || batch_size > 100 {
            return Err(OrganizationApprovalTimeoutSupervisorConfigError::InvalidBatchSize);
        }
        if interval.is_zero() {
            return Err(OrganizationApprovalTimeoutSupervisorConfigError::ZeroInterval);
        }
        if lease_duration.is_zero() {
            return Err(OrganizationApprovalTimeoutSupervisorConfigError::ZeroLease);
        }
        if max_batches_per_run == 0
            || max_batches_per_run > MAX_ORGANIZATION_APPROVAL_TIMEOUT_MAX_BATCHES_PER_RUN
        {
            return Err(OrganizationApprovalTimeoutSupervisorConfigError::InvalidMaxBatchesPerRun);
        }
        Ok(Self {
            batch_size,
            interval,
            lease_duration,
            max_batches_per_run,
        })
    }

    #[must_use]
    pub const fn batch_size(self) -> usize {
        self.batch_size
    }
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }
    #[must_use]
    pub const fn lease_duration(self) -> Duration {
        self.lease_duration
    }
    #[must_use]
    pub const fn max_batches_per_run(self) -> usize {
        self.max_batches_per_run
    }
}

impl Default for OrganizationApprovalTimeoutSupervisorConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_BATCH_SIZE,
            interval: DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_INTERVAL,
            lease_duration: DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_LEASE,
            max_batches_per_run: DEFAULT_ORGANIZATION_APPROVAL_TIMEOUT_MAX_BATCHES_PER_RUN,
        }
    }
}

/// 超时任务执行结果；字段只提供低基数计数，不携带资源摘要。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrganizationApprovalTimeoutRunReport {
    batches: usize,
    scanned: usize,
    loaded: usize,
    applied: usize,
    existing: usize,
    skipped: usize,
    failed: usize,
    outcome_unknown_replays: usize,
    lease_held: bool,
    scan_failed: bool,
    truncated: bool,
}

impl OrganizationApprovalTimeoutRunReport {
    #[must_use]
    pub const fn batches(self) -> usize {
        self.batches
    }
    #[must_use]
    pub const fn scanned(self) -> usize {
        self.scanned
    }
    #[must_use]
    pub const fn loaded(self) -> usize {
        self.loaded
    }
    #[must_use]
    pub const fn applied(self) -> usize {
        self.applied
    }
    #[must_use]
    pub const fn existing(self) -> usize {
        self.existing
    }
    #[must_use]
    pub const fn skipped(self) -> usize {
        self.skipped
    }
    #[must_use]
    pub const fn failed(self) -> usize {
        self.failed
    }
    #[must_use]
    pub const fn outcome_unknown_replays(self) -> usize {
        self.outcome_unknown_replays
    }
    #[must_use]
    pub const fn lease_held(self) -> bool {
        self.lease_held
    }
    #[must_use]
    pub const fn scan_failed(self) -> bool {
        self.scan_failed
    }
    #[must_use]
    pub const fn truncated(self) -> bool {
        self.truncated
    }
}

/// 超时任务依赖的最小持久化端口。
pub trait OrganizationApprovalTimeoutStore: Send + Sync {
    /// 竞争全局扫描租约。
    fn acquire_timeout_lease(
        &self,
        now: i64,
        lease_duration: Duration,
    ) -> impl Future<
        Output = Result<
            OrganizationApprovalTimeoutLeaseOutcome,
            OrganizationApprovalRepositoryError,
        >,
    > + Send;

    /// 按申请主键加载一批已过期且尚未决定的申请命令。
    fn load_timeout_due(
        &self,
        now: i64,
        after: Option<OrganizationApprovalTimeoutCursor>,
        limit: usize,
    ) -> impl Future<
        Output = Result<OrganizationApprovalTimeoutBatch, OrganizationApprovalRepositoryError>,
    > + Send;

    /// 写入或重放一条 TimedOut 决定。
    fn record_timeout(
        &self,
        command: &OrganizationApprovalTimeoutCommand,
        now: i64,
    ) -> impl Future<
        Output = Result<OrganizationApprovalTimeoutOutcome, OrganizationApprovalRepositoryError>,
    > + Send;
}

impl OrganizationApprovalTimeoutStore for OrganizationApprovalRepository {
    async fn acquire_timeout_lease(
        &self,
        now: i64,
        lease_duration: Duration,
    ) -> Result<OrganizationApprovalTimeoutLeaseOutcome, OrganizationApprovalRepositoryError> {
        OrganizationApprovalRepository::acquire_timeout_lease(self, now, lease_duration).await
    }

    async fn load_timeout_due(
        &self,
        now: i64,
        after: Option<OrganizationApprovalTimeoutCursor>,
        limit: usize,
    ) -> Result<OrganizationApprovalTimeoutBatch, OrganizationApprovalRepositoryError> {
        OrganizationApprovalRepository::list_timeout_due(self, now, after, limit).await
    }

    async fn record_timeout(
        &self,
        command: &OrganizationApprovalTimeoutCommand,
        now: i64,
    ) -> Result<OrganizationApprovalTimeoutOutcome, OrganizationApprovalRepositoryError> {
        OrganizationApprovalRepository::record_timeout(self, command, now).await
    }
}

/// 具备数据库租约和幂等决定写入的企业审批超时执行器。
pub struct OrganizationApprovalTimeoutSupervisor<S> {
    store: S,
    config: OrganizationApprovalTimeoutSupervisorConfig,
}

impl<S> OrganizationApprovalTimeoutSupervisor<S> {
    /// 使用显式持久化端口与有界配置创建执行器。
    #[must_use]
    pub const fn new(store: S, config: OrganizationApprovalTimeoutSupervisorConfig) -> Self {
        Self { store, config }
    }
}

impl<S> OrganizationApprovalTimeoutSupervisor<S>
where
    S: OrganizationApprovalTimeoutStore,
{
    /// 使用单个受信系统时刻执行一轮扫描。
    pub async fn run_once(&self) -> OrganizationApprovalTimeoutRunReport {
        match unix_now() {
            Ok(now) => self.run_once_at(now).await,
            Err(_) => OrganizationApprovalTimeoutRunReport {
                scan_failed: true,
                ..Default::default()
            },
        }
    }

    /// 使用传入的 Unix 秒执行一轮扫描，供回归测试固定时钟。
    pub async fn run_once_at(&self, now: i64) -> OrganizationApprovalTimeoutRunReport {
        let lease = match self
            .store
            .acquire_timeout_lease(now, self.config.lease_duration())
            .await
        {
            Ok(lease) => lease,
            Err(_) => {
                return OrganizationApprovalTimeoutRunReport {
                    scan_failed: true,
                    ..Default::default()
                };
            }
        };
        if matches!(lease, OrganizationApprovalTimeoutLeaseOutcome::Held) {
            return OrganizationApprovalTimeoutRunReport {
                lease_held: true,
                ..Default::default()
            };
        }

        let mut report = OrganizationApprovalTimeoutRunReport::default();
        let mut cursor = None;
        for batch_index in 0..self.config.max_batches_per_run() {
            let batch = match self
                .store
                .load_timeout_due(now, cursor, self.config.batch_size())
                .await
            {
                Ok(batch) => batch,
                Err(_) => {
                    report.scan_failed = true;
                    break;
                }
            };
            report.batches += 1;
            let (commands, scanned_count, next_cursor) = batch.into_parts();
            if invalid_page(
                scanned_count,
                commands.len(),
                cursor,
                next_cursor,
                self.config.batch_size(),
            ) {
                report.scan_failed = true;
                break;
            }
            report.scanned += scanned_count;
            report.loaded += commands.len();
            for command in &commands {
                self.apply(command, now, &mut report).await;
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

    /// 启动即扫描，此后按固定间隔运行直到统一关闭信号完成。
    pub async fn run_periodic_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        self.run_periodic_until_with_report(shutdown, |_| {}).await;
    }

    /// 启动即扫描，并将每轮低基数报告交给组合根；调度 crate 不依赖 telemetry。
    pub async fn run_periodic_until_with_report<F, R>(&self, shutdown: F, mut on_report: R)
    where
        F: Future<Output = ()> + Send,
        R: FnMut(OrganizationApprovalTimeoutRunReport) + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => return,
                report = self.run_once() => {
                    on_report(report);
                    log_report(report);
                },
            }
            tokio::select! {
                biased;
                () = &mut shutdown => return,
                () = sleep(self.config.interval()) => {}
            }
        }
    }

    async fn apply(
        &self,
        command: &OrganizationApprovalTimeoutCommand,
        now: i64,
        report: &mut OrganizationApprovalTimeoutRunReport,
    ) {
        let result = self.store.record_timeout(command, now).await;
        let result = if matches!(
            result,
            Err(OrganizationApprovalRepositoryError::OutcomeUnknown)
        ) {
            report.outcome_unknown_replays += 1;
            // 结果未知时只能重放同一个命令，不能重新计算决定键或过期时间。
            self.store.record_timeout(command, now).await
        } else {
            result
        };
        match result {
            Ok(OrganizationApprovalTimeoutOutcome::Applied(_)) => report.applied += 1,
            Ok(OrganizationApprovalTimeoutOutcome::Existing(_)) => report.existing += 1,
            Ok(OrganizationApprovalTimeoutOutcome::Skipped) => report.skipped += 1,
            Err(_) => report.failed += 1,
        }
    }
}

impl<S> fmt::Debug for OrganizationApprovalTimeoutSupervisor<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalTimeoutSupervisor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

fn invalid_page(
    scanned_count: usize,
    command_count: usize,
    previous: Option<OrganizationApprovalTimeoutCursor>,
    next: Option<OrganizationApprovalTimeoutCursor>,
    batch_size: usize,
) -> bool {
    scanned_count > batch_size
        || command_count > scanned_count
        || (scanned_count == 0 && next.is_some())
        || previous
            .zip(next)
            .is_some_and(|(previous, next)| next.id() <= previous.id())
}

fn unix_now() -> Result<i64, ()> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or(())
}

fn log_report(report: OrganizationApprovalTimeoutRunReport) {
    if report.scan_failed || report.failed > 0 {
        tracing::error!(
            batches = report.batches,
            scanned = report.scanned,
            applied = report.applied,
            existing = report.existing,
            skipped = report.skipped,
            failed = report.failed,
            lease_held = report.lease_held,
            truncated = report.truncated,
            "企业审批超时扫描失败"
        );
    } else if report.scanned > 0 || report.lease_held {
        tracing::info!(
            batches = report.batches,
            scanned = report.scanned,
            applied = report.applied,
            existing = report.existing,
            skipped = report.skipped,
            lease_held = report.lease_held,
            truncated = report.truncated,
            "企业审批超时扫描完成"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use super::*;
    use af_db::OrganizationApprovalTimeoutLease;

    #[derive(Clone)]
    struct FakeStore {
        lease: OrganizationApprovalTimeoutLeaseOutcome,
        batches: Arc<
            Mutex<
                VecDeque<
                    Result<OrganizationApprovalTimeoutBatch, OrganizationApprovalRepositoryError>,
                >,
            >,
        >,
        outcomes: Arc<
            Mutex<
                VecDeque<
                    Result<OrganizationApprovalTimeoutOutcome, OrganizationApprovalRepositoryError>,
                >,
            >,
        >,
    }

    impl OrganizationApprovalTimeoutStore for FakeStore {
        async fn acquire_timeout_lease(
            &self,
            _now: i64,
            _lease_duration: Duration,
        ) -> Result<OrganizationApprovalTimeoutLeaseOutcome, OrganizationApprovalRepositoryError>
        {
            Ok(self.lease)
        }

        async fn load_timeout_due(
            &self,
            _now: i64,
            _after: Option<OrganizationApprovalTimeoutCursor>,
            _limit: usize,
        ) -> Result<OrganizationApprovalTimeoutBatch, OrganizationApprovalRepositoryError> {
            self.batches
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(OrganizationApprovalTimeoutBatch::new(Vec::new(), 0, None)))
        }

        async fn record_timeout(
            &self,
            _command: &OrganizationApprovalTimeoutCommand,
            _now: i64,
        ) -> Result<OrganizationApprovalTimeoutOutcome, OrganizationApprovalRepositoryError>
        {
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(OrganizationApprovalTimeoutOutcome::Skipped))
        }
    }

    fn store(
        lease: OrganizationApprovalTimeoutLeaseOutcome,
        batches: Vec<Result<OrganizationApprovalTimeoutBatch, OrganizationApprovalRepositoryError>>,
        outcomes: Vec<
            Result<OrganizationApprovalTimeoutOutcome, OrganizationApprovalRepositoryError>,
        >,
    ) -> FakeStore {
        FakeStore {
            lease,
            batches: Arc::new(Mutex::new(batches.into_iter().collect())),
            outcomes: Arc::new(Mutex::new(outcomes.into_iter().collect())),
        }
    }

    #[tokio::test]
    async fn held_lease_does_not_scan() {
        let store = store(
            OrganizationApprovalTimeoutLeaseOutcome::Held,
            Vec::new(),
            Vec::new(),
        );
        let report = OrganizationApprovalTimeoutSupervisor::new(
            store,
            OrganizationApprovalTimeoutSupervisorConfig::new(
                2,
                Duration::from_secs(1),
                Duration::from_secs(2),
                1,
            )
            .unwrap(),
        )
        .run_once_at(100)
        .await;
        assert!(report.lease_held());
        assert_eq!(report.batches(), 0);
    }

    #[tokio::test]
    async fn bounded_scan_replays_unknown_with_the_same_command() {
        let command = OrganizationApprovalTimeoutCommand::new(
            af_domain::OrganizationId::new(1).unwrap(),
            af_domain::OrganizationApprovalRequestId::new(2).unwrap(),
            99,
            af_domain::OrganizationApprovalDecisionKey::new([7; 16]).unwrap(),
        )
        .unwrap();
        let store = store(
            OrganizationApprovalTimeoutLeaseOutcome::Acquired(
                OrganizationApprovalTimeoutLease::new(
                    1,
                    af_db::DatabaseTimestamp::from_unix_timestamp(200).unwrap(),
                )
                .unwrap(),
            ),
            vec![Ok(OrganizationApprovalTimeoutBatch::new(
                vec![command],
                1,
                None,
            ))],
            vec![
                Err(OrganizationApprovalRepositoryError::OutcomeUnknown),
                Ok(OrganizationApprovalTimeoutOutcome::Applied(
                    af_domain::OrganizationApprovalDecisionId::new(3).unwrap(),
                )),
            ],
        );
        let report = OrganizationApprovalTimeoutSupervisor::new(
            store,
            OrganizationApprovalTimeoutSupervisorConfig::default(),
        )
        .run_once_at(100)
        .await;
        assert_eq!(report.scanned(), 1);
        assert_eq!(report.applied(), 1);
        assert_eq!(report.outcome_unknown_replays(), 1);
    }

    #[tokio::test]
    async fn max_batches_report_truncation_without_failing() {
        let command = OrganizationApprovalTimeoutCommand::new(
            af_domain::OrganizationId::new(1).unwrap(),
            af_domain::OrganizationApprovalRequestId::new(2).unwrap(),
            99,
            af_domain::OrganizationApprovalDecisionKey::new([8; 16]).unwrap(),
        )
        .unwrap();
        let store = store(
            OrganizationApprovalTimeoutLeaseOutcome::Acquired(
                OrganizationApprovalTimeoutLease::new(
                    1,
                    af_db::DatabaseTimestamp::from_unix_timestamp(200).unwrap(),
                )
                .unwrap(),
            ),
            vec![Ok(OrganizationApprovalTimeoutBatch::new(
                vec![command],
                1,
                Some(OrganizationApprovalTimeoutCursor::new(2).unwrap()),
            ))],
            vec![Ok(OrganizationApprovalTimeoutOutcome::Applied(
                af_domain::OrganizationApprovalDecisionId::new(4).unwrap(),
            ))],
        );
        let config = OrganizationApprovalTimeoutSupervisorConfig::new(
            1,
            Duration::from_secs(1),
            Duration::from_secs(2),
            1,
        )
        .unwrap();
        let report = OrganizationApprovalTimeoutSupervisor::new(store, config)
            .run_once_at(100)
            .await;
        assert!(report.truncated());
        assert!(!report.scan_failed());
        assert_eq!(report.batches(), 1);
        assert_eq!(report.applied(), 1);
    }

    #[tokio::test]
    async fn malformed_empty_page_with_cursor_fails_closed() {
        let store = store(
            OrganizationApprovalTimeoutLeaseOutcome::Acquired(
                OrganizationApprovalTimeoutLease::new(
                    1,
                    af_db::DatabaseTimestamp::from_unix_timestamp(200).unwrap(),
                )
                .unwrap(),
            ),
            vec![Ok(OrganizationApprovalTimeoutBatch::new(
                Vec::new(),
                0,
                Some(OrganizationApprovalTimeoutCursor::new(3).unwrap()),
            ))],
            Vec::new(),
        );
        let report = OrganizationApprovalTimeoutSupervisor::new(
            store,
            OrganizationApprovalTimeoutSupervisorConfig::default(),
        )
        .run_once_at(100)
        .await;
        assert!(report.scan_failed());
    }
}
