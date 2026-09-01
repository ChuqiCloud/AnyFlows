use std::{
    fmt,
    future::Future,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationSsoLoginTransactionCleanupLeaseOutcome, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
};
use thiserror::Error;
use tokio::time::sleep;

/// 企业 SSO 过期登录事务清理默认单批大小。
pub const DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_BATCH_SIZE: usize = 64;
/// 企业 SSO 过期登录事务清理默认执行间隔。
pub const DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_INTERVAL: Duration =
    Duration::from_secs(60);
/// 企业 SSO 过期登录事务清理默认租约时长。
pub const DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_LEASE: Duration =
    Duration::from_secs(120);
/// 企业 SSO 过期登录事务清理单轮最多扫描批次数。
pub const DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_MAX_BATCHES_PER_RUN: usize = 8;
/// 企业 SSO 过期登录事务清理单轮批次数硬上限。
pub const MAX_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_MAX_BATCHES_PER_RUN: usize = 64;

/// 清理调度配置错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoLoginTransactionCleanupSupervisorConfigError {
    #[error("企业 SSO 登录事务清理批次大小无效")]
    InvalidBatchSize,
    #[error("企业 SSO 登录事务清理执行间隔必须大于零")]
    ZeroInterval,
    #[error("企业 SSO 登录事务清理租约时长必须大于零")]
    ZeroLease,
    #[error("企业 SSO 登录事务清理单轮批次数无效")]
    InvalidMaxBatchesPerRun,
}

/// 有界清理任务配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationSsoLoginTransactionCleanupSupervisorConfig {
    batch_size: usize,
    interval: Duration,
    lease_duration: Duration,
    max_batches_per_run: usize,
}

impl OrganizationSsoLoginTransactionCleanupSupervisorConfig {
    /// 构造批次、周期、租约和单轮工作量均受限的清理配置。
    pub fn new(
        batch_size: usize,
        interval: Duration,
        lease_duration: Duration,
        max_batches_per_run: usize,
    ) -> Result<Self, OrganizationSsoLoginTransactionCleanupSupervisorConfigError> {
        if batch_size == 0 || batch_size > 64 {
            return Err(
                OrganizationSsoLoginTransactionCleanupSupervisorConfigError::InvalidBatchSize,
            );
        }
        if interval.is_zero() {
            return Err(OrganizationSsoLoginTransactionCleanupSupervisorConfigError::ZeroInterval);
        }
        if lease_duration.is_zero() {
            return Err(OrganizationSsoLoginTransactionCleanupSupervisorConfigError::ZeroLease);
        }
        if max_batches_per_run == 0
            || max_batches_per_run
                > MAX_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_MAX_BATCHES_PER_RUN
        {
            return Err(
                OrganizationSsoLoginTransactionCleanupSupervisorConfigError::InvalidMaxBatchesPerRun,
            );
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

impl Default for OrganizationSsoLoginTransactionCleanupSupervisorConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_BATCH_SIZE,
            interval: DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_INTERVAL,
            lease_duration: DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_LEASE,
            max_batches_per_run:
                DEFAULT_ORGANIZATION_SSO_LOGIN_TRANSACTION_CLEANUP_MAX_BATCHES_PER_RUN,
        }
    }
}

/// 清理任务单轮低基数运行报告。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrganizationSsoLoginTransactionCleanupRunReport {
    batches: usize,
    deleted: usize,
    lease_held: bool,
    failed: bool,
    truncated: bool,
}

impl OrganizationSsoLoginTransactionCleanupRunReport {
    #[must_use]
    pub const fn batches(self) -> usize {
        self.batches
    }
    #[must_use]
    pub const fn deleted(self) -> usize {
        self.deleted
    }
    #[must_use]
    pub const fn lease_held(self) -> bool {
        self.lease_held
    }
    #[must_use]
    pub const fn failed(self) -> bool {
        self.failed
    }
    #[must_use]
    pub const fn truncated(self) -> bool {
        self.truncated
    }
}

/// 清理任务的最小持久化端口。
pub trait OrganizationSsoLoginTransactionCleanupStore: Send + Sync {
    fn acquire_lease(
        &self,
        now: i64,
        lease_duration: Duration,
    ) -> impl Future<
        Output = Result<
            OrganizationSsoLoginTransactionCleanupLeaseOutcome,
            OrganizationSsoRepositoryError,
        >,
    > + Send;
    fn cleanup_expired(
        &self,
        now: i64,
        limit: usize,
    ) -> impl Future<Output = Result<usize, OrganizationSsoRepositoryError>> + Send;
}

impl OrganizationSsoLoginTransactionCleanupStore for OrganizationSsoProviderRepository {
    async fn acquire_lease(
        &self,
        now: i64,
        lease_duration: Duration,
    ) -> Result<OrganizationSsoLoginTransactionCleanupLeaseOutcome, OrganizationSsoRepositoryError>
    {
        self.acquire_login_transaction_cleanup_lease(now, lease_duration)
            .await
    }

    async fn cleanup_expired(
        &self,
        now: i64,
        limit: usize,
    ) -> Result<usize, OrganizationSsoRepositoryError> {
        self.cleanup_expired_unclaimed_login_transactions(now, limit)
            .await
    }
}

/// 带数据库租约的过期登录事务清理执行器。
pub struct OrganizationSsoLoginTransactionCleanupSupervisor<S> {
    store: S,
    config: OrganizationSsoLoginTransactionCleanupSupervisorConfig,
}

impl<S> OrganizationSsoLoginTransactionCleanupSupervisor<S> {
    #[must_use]
    pub const fn new(
        store: S,
        config: OrganizationSsoLoginTransactionCleanupSupervisorConfig,
    ) -> Self {
        Self { store, config }
    }
}

impl<S> OrganizationSsoLoginTransactionCleanupSupervisor<S>
where
    S: OrganizationSsoLoginTransactionCleanupStore,
{
    /// 执行一轮有界清理；任何依赖错误都会失败关闭本轮。
    pub async fn run_once_at(&self, now: i64) -> OrganizationSsoLoginTransactionCleanupRunReport {
        let started_at = Instant::now();
        let mut report = OrganizationSsoLoginTransactionCleanupRunReport::default();
        let mut lease = match self
            .store
            .acquire_lease(now, self.config.lease_duration())
            .await
        {
            Ok(value) => value,
            Err(_) => {
                report.failed = true;
                return report;
            }
        };
        if matches!(
            lease,
            OrganizationSsoLoginTransactionCleanupLeaseOutcome::Held
        ) {
            report.lease_held = true;
            return report;
        }
        for batch_index in 0..self.config.max_batches_per_run() {
            let Some(lease_now) = elapsed_scan_time(now, started_at.elapsed()) else {
                report.failed = true;
                return report;
            };
            lease = match self
                .store
                .acquire_lease(lease_now, self.config.lease_duration())
                .await
            {
                Ok(value) => value,
                Err(_) => {
                    report.failed = true;
                    return report;
                }
            };
            if matches!(
                lease,
                OrganizationSsoLoginTransactionCleanupLeaseOutcome::Held
            ) {
                report.lease_held = true;
                report.truncated = true;
                return report;
            }
            let deleted = match self
                .store
                .cleanup_expired(now, self.config.batch_size())
                .await
            {
                Ok(value) => value,
                Err(_) => {
                    report.failed = true;
                    return report;
                }
            };
            if deleted > self.config.batch_size() {
                report.failed = true;
                return report;
            }
            report.batches += 1;
            report.deleted = report.deleted.saturating_add(deleted);
            if deleted < self.config.batch_size() {
                break;
            }
            if batch_index + 1 == self.config.max_batches_per_run() {
                report.truncated = true;
            }
        }
        report
    }

    /// 启动即扫描并按固定间隔运行，直到统一关闭信号完成。
    pub async fn run_periodic_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => return,
                report = self.run_once_at(unix_now()) => log_report(report),
            }
            tokio::select! { biased; () = &mut shutdown => return, () = sleep(self.config.interval()) => {} }
        }
    }
}

impl<S> fmt::Debug for OrganizationSsoLoginTransactionCleanupSupervisor<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoLoginTransactionCleanupSupervisor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0)
}

fn elapsed_scan_time(started_at: i64, elapsed: Duration) -> Option<i64> {
    i64::try_from(elapsed.as_secs())
        .ok()
        .and_then(|seconds| started_at.checked_add(seconds))
}

fn log_report(report: OrganizationSsoLoginTransactionCleanupRunReport) {
    if report.failed {
        tracing::error!(
            batches = report.batches,
            deleted = report.deleted,
            lease_held = report.lease_held,
            truncated = report.truncated,
            "企业 SSO 过期登录事务清理失败"
        );
    } else if report.deleted > 0 || report.lease_held {
        tracing::info!(
            batches = report.batches,
            deleted = report.deleted,
            lease_held = report.lease_held,
            truncated = report.truncated,
            "企业 SSO 过期登录事务清理完成"
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

    #[derive(Clone)]
    struct FakeStore {
        leases: Arc<
            Mutex<
                VecDeque<
                    Result<
                        OrganizationSsoLoginTransactionCleanupLeaseOutcome,
                        OrganizationSsoRepositoryError,
                    >,
                >,
            >,
        >,
        cleanups: Arc<Mutex<VecDeque<Result<usize, OrganizationSsoRepositoryError>>>>,
    }

    impl OrganizationSsoLoginTransactionCleanupStore for FakeStore {
        async fn acquire_lease(
            &self,
            _now: i64,
            _lease_duration: Duration,
        ) -> Result<
            OrganizationSsoLoginTransactionCleanupLeaseOutcome,
            OrganizationSsoRepositoryError,
        > {
            self.leases.lock().unwrap().pop_front().unwrap_or(Ok(
                OrganizationSsoLoginTransactionCleanupLeaseOutcome::Acquired,
            ))
        }

        async fn cleanup_expired(
            &self,
            _now: i64,
            _limit: usize,
        ) -> Result<usize, OrganizationSsoRepositoryError> {
            self.cleanups.lock().unwrap().pop_front().unwrap_or(Ok(0))
        }
    }

    fn store(
        leases: Vec<
            Result<
                OrganizationSsoLoginTransactionCleanupLeaseOutcome,
                OrganizationSsoRepositoryError,
            >,
        >,
        cleanups: Vec<Result<usize, OrganizationSsoRepositoryError>>,
    ) -> FakeStore {
        FakeStore {
            leases: Arc::new(Mutex::new(leases.into_iter().collect())),
            cleanups: Arc::new(Mutex::new(cleanups.into_iter().collect())),
        }
    }

    fn config(
        batch_size: usize,
        max_batches: usize,
    ) -> OrganizationSsoLoginTransactionCleanupSupervisorConfig {
        OrganizationSsoLoginTransactionCleanupSupervisorConfig::new(
            batch_size,
            Duration::from_secs(1),
            Duration::from_secs(2),
            max_batches,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn held_lease_does_not_cleanup() {
        let report = OrganizationSsoLoginTransactionCleanupSupervisor::new(
            store(
                vec![Ok(OrganizationSsoLoginTransactionCleanupLeaseOutcome::Held)],
                Vec::new(),
            ),
            config(1, 1),
        )
        .run_once_at(100)
        .await;
        assert!(report.lease_held());
        assert_eq!(report.batches(), 0);
        assert_eq!(report.deleted(), 0);
    }

    #[tokio::test]
    async fn max_batches_are_bounded_and_report_truncation() {
        let report = OrganizationSsoLoginTransactionCleanupSupervisor::new(
            store(
                vec![
                    Ok(OrganizationSsoLoginTransactionCleanupLeaseOutcome::Acquired),
                    Ok(OrganizationSsoLoginTransactionCleanupLeaseOutcome::Acquired),
                    Ok(OrganizationSsoLoginTransactionCleanupLeaseOutcome::Acquired),
                ],
                vec![Ok(1), Ok(1)],
            ),
            config(1, 2),
        )
        .run_once_at(100)
        .await;
        assert!(report.truncated());
        assert!(!report.failed());
        assert_eq!(report.batches(), 2);
        assert_eq!(report.deleted(), 2);
    }

    #[tokio::test]
    async fn deletion_over_batch_limit_fails_closed() {
        let report = OrganizationSsoLoginTransactionCleanupSupervisor::new(
            store(
                vec![
                    Ok(OrganizationSsoLoginTransactionCleanupLeaseOutcome::Acquired),
                    Ok(OrganizationSsoLoginTransactionCleanupLeaseOutcome::Acquired),
                ],
                vec![Ok(2)],
            ),
            config(1, 1),
        )
        .run_once_at(100)
        .await;
        assert!(report.failed());
        assert_eq!(report.batches(), 0);
        assert_eq!(report.deleted(), 0);
    }

    #[test]
    fn elapsed_scan_time_is_checked() {
        assert_eq!(elapsed_scan_time(10, Duration::from_secs(3)), Some(13));
        assert_eq!(elapsed_scan_time(i64::MAX, Duration::from_secs(1)), None);
    }
}
