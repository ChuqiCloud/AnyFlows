use std::{
    fmt,
    future::Future,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationSsoDomainReverificationBatch, OrganizationSsoDomainReverificationCursor,
    OrganizationSsoDomainReverificationLeaseOutcome, OrganizationSsoDomainReverificationOutcome,
    OrganizationSsoDomainReverificationTarget, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
};
use af_domain::{
    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS, OrganizationSsoDnsReverification,
    OrganizationSsoDnsTxtAnswers,
};
use thiserror::Error;
use tokio::time::sleep;

/// 企业 SSO 域名复验默认单批大小。
pub const DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_BATCH_SIZE: usize = 64;
/// 企业 SSO 域名复验默认执行间隔。
pub const DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_INTERVAL: Duration =
    Duration::from_secs(60);
/// 企业 SSO 域名复验默认租约时长。
pub const DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_LEASE: Duration = Duration::from_secs(120);
/// 企业 SSO 域名复验单轮最多扫描批次数。
pub const DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_MAX_BATCHES_PER_RUN: usize = 8;
/// 企业 SSO 域名复验单轮批次数硬上限。
pub const MAX_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_MAX_BATCHES_PER_RUN: usize = 64;

/// 复验调度配置错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoDomainReverificationSupervisorConfigError {
    #[error("企业 SSO 域名复验批次大小无效")]
    InvalidBatchSize,
    #[error("企业 SSO 域名复验执行间隔必须大于零")]
    ZeroInterval,
    #[error("企业 SSO 域名复验租约时长必须大于零")]
    ZeroLease,
    #[error("企业 SSO 域名复验单轮批次数无效")]
    InvalidMaxBatchesPerRun,
}

/// 有界复验任务配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationSsoDomainReverificationSupervisorConfig {
    batch_size: usize,
    interval: Duration,
    lease_duration: Duration,
    max_batches_per_run: usize,
}

impl OrganizationSsoDomainReverificationSupervisorConfig {
    /// 构造批次、周期、租约和单轮工作量均受限的调度配置。
    pub fn new(
        batch_size: usize,
        interval: Duration,
        lease_duration: Duration,
        max_batches_per_run: usize,
    ) -> Result<Self, OrganizationSsoDomainReverificationSupervisorConfigError> {
        if batch_size == 0 || batch_size > 100 {
            return Err(OrganizationSsoDomainReverificationSupervisorConfigError::InvalidBatchSize);
        }
        if interval.is_zero() {
            return Err(OrganizationSsoDomainReverificationSupervisorConfigError::ZeroInterval);
        }
        if lease_duration.is_zero() {
            return Err(OrganizationSsoDomainReverificationSupervisorConfigError::ZeroLease);
        }
        if max_batches_per_run == 0
            || max_batches_per_run > MAX_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_MAX_BATCHES_PER_RUN
        {
            return Err(
                OrganizationSsoDomainReverificationSupervisorConfigError::InvalidMaxBatchesPerRun,
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

impl Default for OrganizationSsoDomainReverificationSupervisorConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_BATCH_SIZE,
            interval: DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_INTERVAL,
            lease_duration: DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_LEASE,
            max_batches_per_run: DEFAULT_ORGANIZATION_SSO_DOMAIN_REVERIFICATION_MAX_BATCHES_PER_RUN,
        }
    }
}

/// DNS 结果端口；错误不携带域名、TXT 原文或令牌。
pub trait OrganizationSsoDomainReverificationResolver: Send + Sync {
    fn lookup_txt(
        &self,
        target: &OrganizationSsoDomainReverificationTarget,
    ) -> impl Future<Output = Result<OrganizationSsoDnsTxtAnswers, ()>> + Send;
}

/// 复验任务最小持久化端口。
pub trait OrganizationSsoDomainReverificationStore: Send + Sync {
    fn acquire_lease(
        &self,
        now: i64,
        lease_duration: Duration,
    ) -> impl Future<
        Output = Result<
            OrganizationSsoDomainReverificationLeaseOutcome,
            OrganizationSsoRepositoryError,
        >,
    > + Send;
    fn load_due(
        &self,
        now: i64,
        after: Option<OrganizationSsoDomainReverificationCursor>,
        limit: usize,
    ) -> impl Future<
        Output = Result<OrganizationSsoDomainReverificationBatch, OrganizationSsoRepositoryError>,
    > + Send;
    fn refresh(
        &self,
        target: &OrganizationSsoDomainReverificationTarget,
        now: i64,
        recheck_at: i64,
    ) -> impl Future<
        Output = Result<OrganizationSsoDomainReverificationOutcome, OrganizationSsoRepositoryError>,
    > + Send;
    fn revoke(
        &self,
        target: &OrganizationSsoDomainReverificationTarget,
        now: i64,
    ) -> impl Future<
        Output = Result<OrganizationSsoDomainReverificationOutcome, OrganizationSsoRepositoryError>,
    > + Send;
}

impl OrganizationSsoDomainReverificationStore for OrganizationSsoProviderRepository {
    async fn acquire_lease(
        &self,
        now: i64,
        lease_duration: Duration,
    ) -> Result<OrganizationSsoDomainReverificationLeaseOutcome, OrganizationSsoRepositoryError>
    {
        self.acquire_domain_reverification_lease(now, lease_duration)
            .await
    }
    async fn load_due(
        &self,
        now: i64,
        after: Option<OrganizationSsoDomainReverificationCursor>,
        limit: usize,
    ) -> Result<OrganizationSsoDomainReverificationBatch, OrganizationSsoRepositoryError> {
        self.list_domain_reverification_due(now, after, limit).await
    }
    async fn refresh(
        &self,
        target: &OrganizationSsoDomainReverificationTarget,
        now: i64,
        recheck_at: i64,
    ) -> Result<OrganizationSsoDomainReverificationOutcome, OrganizationSsoRepositoryError> {
        self.refresh_domain_reverification(target, now, recheck_at)
            .await
    }
    async fn revoke(
        &self,
        target: &OrganizationSsoDomainReverificationTarget,
        now: i64,
    ) -> Result<OrganizationSsoDomainReverificationOutcome, OrganizationSsoRepositoryError> {
        self.revoke_domain_after_reverification(target, now).await
    }
}

/// 单轮复验低基数报告。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrganizationSsoDomainReverificationRunReport {
    batches: usize,
    scanned: usize,
    refreshed: usize,
    revoked: usize,
    existing: usize,
    skipped: usize,
    failed: usize,
    lease_held: bool,
    scan_failed: bool,
    truncated: bool,
}

impl OrganizationSsoDomainReverificationRunReport {
    #[must_use]
    pub const fn batches(self) -> usize {
        self.batches
    }
    #[must_use]
    pub const fn scanned(self) -> usize {
        self.scanned
    }
    #[must_use]
    pub const fn refreshed(self) -> usize {
        self.refreshed
    }
    #[must_use]
    pub const fn revoked(self) -> usize {
        self.revoked
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

/// 带数据库租约、DNS 复验和 CAS 撤销的执行器。
pub struct OrganizationSsoDomainReverificationSupervisor<S, R> {
    store: S,
    resolver: R,
    config: OrganizationSsoDomainReverificationSupervisorConfig,
}

impl<S, R> OrganizationSsoDomainReverificationSupervisor<S, R> {
    #[must_use]
    pub const fn new(
        store: S,
        resolver: R,
        config: OrganizationSsoDomainReverificationSupervisorConfig,
    ) -> Self {
        Self {
            store,
            resolver,
            config,
        }
    }
}

impl<S, R> OrganizationSsoDomainReverificationSupervisor<S, R>
where
    S: OrganizationSsoDomainReverificationStore,
    R: OrganizationSsoDomainReverificationResolver,
{
    /// 使用单个受信 Unix 秒执行一轮扫描。
    pub async fn run_once_at(&self, now: i64) -> OrganizationSsoDomainReverificationRunReport {
        let started_at = Instant::now();
        let lease = match self
            .store
            .acquire_lease(now, self.config.lease_duration())
            .await
        {
            Ok(lease) => lease,
            Err(_) => {
                return OrganizationSsoDomainReverificationRunReport {
                    scan_failed: true,
                    ..Default::default()
                };
            }
        };
        if matches!(lease, OrganizationSsoDomainReverificationLeaseOutcome::Held) {
            return OrganizationSsoDomainReverificationRunReport {
                lease_held: true,
                ..Default::default()
            };
        }
        let mut report = OrganizationSsoDomainReverificationRunReport::default();
        let mut cursor = None;
        'batches: for batch_index in 0..self.config.max_batches_per_run() {
            let batch = match self
                .store
                .load_due(now, cursor, self.config.batch_size())
                .await
            {
                Ok(batch) => batch,
                Err(_) => {
                    report.scan_failed = true;
                    break;
                }
            };
            report.batches += 1;
            let (targets, scanned_count, next_cursor) = batch.into_parts();
            if invalid_page(
                scanned_count,
                targets.len(),
                cursor,
                next_cursor,
                self.config.batch_size(),
            ) {
                report.scan_failed = true;
                break;
            }
            report.scanned += scanned_count;
            for target in &targets {
                let Some(lease_now) = elapsed_scan_time(now, started_at.elapsed()) else {
                    report.scan_failed = true;
                    break 'batches;
                };
                match self
                    .store
                    .acquire_lease(lease_now, self.config.lease_duration())
                    .await
                {
                    Ok(OrganizationSsoDomainReverificationLeaseOutcome::Acquired(_)) => {}
                    Ok(OrganizationSsoDomainReverificationLeaseOutcome::Held) => {
                        report.lease_held = true;
                        report.truncated = true;
                        break 'batches;
                    }
                    Err(_) => {
                        report.scan_failed = true;
                        break 'batches;
                    }
                }
                self.apply(target, now, &mut report).await;
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

    async fn apply(
        &self,
        target: &OrganizationSsoDomainReverificationTarget,
        now: i64,
        report: &mut OrganizationSsoDomainReverificationRunReport,
    ) {
        let result = match self.resolver.lookup_txt(target).await {
            Ok(answers) => OrganizationSsoDnsReverification::verify(
                target.domain(),
                af_domain::OrganizationSsoDomainStatus::Verified,
                target.proof_digest(),
                &answers,
                now,
                DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
            )
            .ok()
            .map(|verification| self.store.refresh(target, now, verification.recheck_at())),
            Err(_) => None,
        };
        let outcome = match result {
            Some(future) => match future.await {
                Ok(outcome) => outcome,
                Err(_) => {
                    report.failed += 1;
                    return;
                }
            },
            None => match self.store.revoke(target, now).await {
                Ok(outcome) => outcome,
                Err(_) => {
                    report.failed += 1;
                    return;
                }
            },
        };
        match outcome {
            OrganizationSsoDomainReverificationOutcome::Refreshed => report.refreshed += 1,
            OrganizationSsoDomainReverificationOutcome::Revoked => report.revoked += 1,
            OrganizationSsoDomainReverificationOutcome::Existing => report.existing += 1,
            OrganizationSsoDomainReverificationOutcome::Skipped => report.skipped += 1,
        }
    }
}

fn elapsed_scan_time(started_at: i64, elapsed: Duration) -> Option<i64> {
    i64::try_from(elapsed.as_secs())
        .ok()
        .and_then(|seconds| started_at.checked_add(seconds))
}

impl<S, R> fmt::Debug for OrganizationSsoDomainReverificationSupervisor<S, R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoDomainReverificationSupervisor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

fn invalid_page(
    scanned: usize,
    targets: usize,
    previous: Option<OrganizationSsoDomainReverificationCursor>,
    next: Option<OrganizationSsoDomainReverificationCursor>,
    limit: usize,
) -> bool {
    scanned > limit
        || targets > scanned
        || (scanned == 0 && next.is_some())
        || previous
            .zip(next)
            .is_some_and(|(previous, next)| next.id() <= previous.id())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0)
}

fn log_report(report: OrganizationSsoDomainReverificationRunReport) {
    if report.scan_failed || report.failed > 0 {
        tracing::error!(
            batches = report.batches,
            scanned = report.scanned,
            refreshed = report.refreshed,
            revoked = report.revoked,
            existing = report.existing,
            skipped = report.skipped,
            failed = report.failed,
            lease_held = report.lease_held,
            truncated = report.truncated,
            "企业 SSO 域名复验扫描失败"
        );
    } else if report.scanned > 0 || report.lease_held {
        tracing::info!(
            batches = report.batches,
            scanned = report.scanned,
            refreshed = report.refreshed,
            revoked = report.revoked,
            existing = report.existing,
            skipped = report.skipped,
            lease_held = report.lease_held,
            truncated = report.truncated,
            "企业 SSO 域名复验扫描完成"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use af_db::OrganizationSsoDomainReverificationLease;
    use af_domain::{
        OrganizationId, OrganizationSsoDomain, OrganizationSsoDomainProof,
        OrganizationSsoDomainProofToken,
    };
    use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;

    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde1";

    #[derive(Clone)]
    struct FakeStore {
        lease: OrganizationSsoDomainReverificationLeaseOutcome,
        lease_calls: Arc<AtomicUsize>,
        batches: Arc<Mutex<VecDeque<OrganizationSsoDomainReverificationBatch>>>,
        writes: Arc<Mutex<VecDeque<OrganizationSsoDomainReverificationOutcome>>>,
    }

    impl OrganizationSsoDomainReverificationStore for FakeStore {
        async fn acquire_lease(
            &self,
            _: i64,
            _: Duration,
        ) -> Result<OrganizationSsoDomainReverificationLeaseOutcome, OrganizationSsoRepositoryError>
        {
            self.lease_calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.lease)
        }
        async fn load_due(
            &self,
            _: i64,
            _: Option<OrganizationSsoDomainReverificationCursor>,
            _: usize,
        ) -> Result<OrganizationSsoDomainReverificationBatch, OrganizationSsoRepositoryError>
        {
            Ok(self.batches.lock().unwrap().pop_front().unwrap_or_else(|| {
                OrganizationSsoDomainReverificationBatch::new(Vec::new(), 0, None)
            }))
        }
        async fn refresh(
            &self,
            _: &OrganizationSsoDomainReverificationTarget,
            _: i64,
            _: i64,
        ) -> Result<OrganizationSsoDomainReverificationOutcome, OrganizationSsoRepositoryError>
        {
            Ok(self
                .writes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(OrganizationSsoDomainReverificationOutcome::Refreshed))
        }
        async fn revoke(
            &self,
            _: &OrganizationSsoDomainReverificationTarget,
            _: i64,
        ) -> Result<OrganizationSsoDomainReverificationOutcome, OrganizationSsoRepositoryError>
        {
            Ok(self
                .writes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(OrganizationSsoDomainReverificationOutcome::Revoked))
        }
    }

    #[derive(Clone)]
    struct FakeResolver(Result<OrganizationSsoDnsTxtAnswers, ()>);

    impl OrganizationSsoDomainReverificationResolver for FakeResolver {
        async fn lookup_txt(
            &self,
            _: &OrganizationSsoDomainReverificationTarget,
        ) -> Result<OrganizationSsoDnsTxtAnswers, ()> {
            self.0.clone()
        }
    }

    fn target() -> OrganizationSsoDomainReverificationTarget {
        let domain = OrganizationSsoDomain::new("example.com").unwrap();
        let proof = OrganizationSsoDomainProof::from_token(
            domain.clone(),
            OrganizationSsoDomainProofToken::new(TOKEN).unwrap(),
            1_000,
            3_600,
        )
        .unwrap();
        OrganizationSsoDomainReverificationTarget::new(
            OrganizationId::new(1).unwrap(),
            1,
            domain,
            proof.digest(),
            2,
        )
        .unwrap()
    }

    fn store(
        lease: OrganizationSsoDomainReverificationLeaseOutcome,
        outcome: OrganizationSsoDomainReverificationOutcome,
    ) -> FakeStore {
        FakeStore {
            lease,
            lease_calls: Arc::new(AtomicUsize::new(0)),
            batches: Arc::new(Mutex::new(VecDeque::from([
                OrganizationSsoDomainReverificationBatch::new(vec![target()], 1, None),
            ]))),
            writes: Arc::new(Mutex::new(VecDeque::from([outcome]))),
        }
    }

    fn acquired() -> OrganizationSsoDomainReverificationLeaseOutcome {
        OrganizationSsoDomainReverificationLeaseOutcome::Acquired(
            OrganizationSsoDomainReverificationLease::new(
                1,
                TimeDateTimeWithTimeZone::from_unix_timestamp(20_000).unwrap(),
            )
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn matching_txt_refreshes_verified_domain() {
        let store = store(
            acquired(),
            OrganizationSsoDomainReverificationOutcome::Refreshed,
        );
        let lease_calls = Arc::clone(&store.lease_calls);
        let report = OrganizationSsoDomainReverificationSupervisor::new(
            store,
            FakeResolver(Ok(OrganizationSsoDnsTxtAnswers::new(vec![
                TOKEN.to_owned(),
            ]))),
            OrganizationSsoDomainReverificationSupervisorConfig::default(),
        )
        .run_once_at(10_000)
        .await;
        assert_eq!(report.refreshed(), 1);
        assert_eq!(report.revoked(), 0);
        assert_eq!(lease_calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn dns_failure_revokes_and_existing_replay_is_observable() {
        let report = OrganizationSsoDomainReverificationSupervisor::new(
            store(
                acquired(),
                OrganizationSsoDomainReverificationOutcome::Revoked,
            ),
            FakeResolver(Err(())),
            OrganizationSsoDomainReverificationSupervisorConfig::default(),
        )
        .run_once_at(10_000)
        .await;
        assert_eq!(report.revoked(), 1);

        let replay = OrganizationSsoDomainReverificationSupervisor::new(
            store(
                acquired(),
                OrganizationSsoDomainReverificationOutcome::Existing,
            ),
            FakeResolver(Err(())),
            OrganizationSsoDomainReverificationSupervisorConfig::default(),
        )
        .run_once_at(10_000)
        .await;
        assert_eq!(replay.existing(), 1);
    }

    #[tokio::test]
    async fn held_lease_skips_scan() {
        let report = OrganizationSsoDomainReverificationSupervisor::new(
            store(
                OrganizationSsoDomainReverificationLeaseOutcome::Held,
                OrganizationSsoDomainReverificationOutcome::Skipped,
            ),
            FakeResolver(Err(())),
            OrganizationSsoDomainReverificationSupervisorConfig::default(),
        )
        .run_once_at(10_000)
        .await;
        assert!(report.lease_held());
        assert_eq!(report.scanned(), 0);
    }
}
