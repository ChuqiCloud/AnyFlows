use std::{
    fmt,
    future::Future,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_config::MAX_OAUTH_REFRESH_CONCURRENCY;
use af_db::MAX_OAUTH_REFRESH_CANDIDATES;
use af_domain::CredentialId;
use futures_util::{StreamExt as _, pin_mut, stream};
use thiserror::Error;
use tokio::time::sleep;

use super::{
    OAuthExpirationProjectionBackfillBatch, OAuthRefreshCandidate, OAuthRefreshCoordinator,
    OAuthRefreshCoordinatorError, OAuthRefreshCoordinatorOutcome, OAuthRefreshPersistenceError,
    OAuthRefreshPersistenceService,
};

/// OAuth 刷新周期任务配置错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthRefreshSupervisorConfigError {
    /// 单轮候选数为零或超过数据库查询硬上限。
    #[error("OAuth 刷新批次大小无效")]
    InvalidBatchSize,
    /// 并发数为零、超过单轮批次或超过进程硬上限。
    #[error("OAuth 刷新并发数量无效")]
    InvalidConcurrency,
    /// 扫描周期为零会形成忙轮询。
    #[error("OAuth 刷新周期必须大于零")]
    ZeroInterval,
    /// 提前刷新时间为零不能形成稳定刷新窗口。
    #[error("OAuth 提前刷新时间必须大于零")]
    ZeroRefreshBeforeExpiry,
}

/// OAuth 刷新周期任务的有界运行参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OAuthRefreshSupervisorConfig {
    batch_size: usize,
    concurrency: NonZeroUsize,
    interval: Duration,
    refresh_before_expiry: Duration,
}

impl OAuthRefreshSupervisorConfig {
    /// 校验单轮批次、并发和时间边界后创建配置。
    pub fn new(
        batch_size: usize,
        concurrency: usize,
        interval: Duration,
        refresh_before_expiry: Duration,
    ) -> Result<Self, OAuthRefreshSupervisorConfigError> {
        if batch_size == 0 || batch_size > MAX_OAUTH_REFRESH_CANDIDATES {
            return Err(OAuthRefreshSupervisorConfigError::InvalidBatchSize);
        }
        let concurrency = NonZeroUsize::new(concurrency)
            .ok_or(OAuthRefreshSupervisorConfigError::InvalidConcurrency)?;
        if concurrency.get() > batch_size || concurrency.get() > MAX_OAUTH_REFRESH_CONCURRENCY {
            return Err(OAuthRefreshSupervisorConfigError::InvalidConcurrency);
        }
        if interval.is_zero() {
            return Err(OAuthRefreshSupervisorConfigError::ZeroInterval);
        }
        if refresh_before_expiry.is_zero() {
            return Err(OAuthRefreshSupervisorConfigError::ZeroRefreshBeforeExpiry);
        }
        Ok(Self {
            batch_size,
            concurrency,
            interval,
            refresh_before_expiry,
        })
    }

    /// 返回单轮最多读取的到期候选数量。
    #[must_use]
    pub const fn batch_size(self) -> usize {
        self.batch_size
    }

    /// 返回同一进程允许同时执行的刷新数量。
    #[must_use]
    pub const fn concurrency(self) -> usize {
        self.concurrency.get()
    }

    /// 返回两轮扫描之间的等待时间。
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }

    /// 返回 access token 到期前进入候选集的时间。
    #[must_use]
    pub const fn refresh_before_expiry(self) -> Duration {
        self.refresh_before_expiry
    }
}

/// 单轮 OAuth 刷新任务的聚合结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OAuthRefreshRunReport {
    backfill_scanned: usize,
    backfill_projected: usize,
    backfill_incomplete: usize,
    backfill_conflicted: usize,
    backfill_failed: usize,
    loaded: usize,
    stored: usize,
    stale: usize,
    target_not_found: usize,
    provider_mismatch: usize,
    lease_held: usize,
    failed: usize,
}

impl OAuthRefreshRunReport {
    /// 返回本轮扫描的旧凭据数量。
    #[must_use]
    pub const fn backfill_scanned(self) -> usize {
        self.backfill_scanned
    }

    /// 返回本轮成功回填到期投影的数量。
    #[must_use]
    pub const fn backfill_projected(self) -> usize {
        self.backfill_projected
    }

    /// 返回缺少完整续期材料的旧凭据数量。
    #[must_use]
    pub const fn backfill_incomplete(self) -> usize {
        self.backfill_incomplete
    }

    /// 返回回填期间已经变化的旧凭据数量。
    #[must_use]
    pub const fn backfill_conflicted(self) -> usize {
        self.backfill_conflicted
    }

    /// 返回旧凭据回填失败次数；单轮最多为一。
    #[must_use]
    pub const fn backfill_failed(self) -> usize {
        self.backfill_failed
    }

    /// 返回本轮加载的到期候选数量。
    #[must_use]
    pub const fn loaded(self) -> usize {
        self.loaded
    }

    /// 返回成功写入新 token 集合的数量。
    #[must_use]
    pub const fn stored(self) -> usize {
        self.stored
    }

    /// 返回被较新凭据事实淘汰的数量。
    #[must_use]
    pub const fn stale(self) -> usize {
        self.stale
    }

    /// 返回目标已经不存在或不再可刷新的数量。
    #[must_use]
    pub const fn target_not_found(self) -> usize {
        self.target_not_found
    }

    /// 返回候选与当前 Provider 不再一致的数量。
    #[must_use]
    pub const fn provider_mismatch(self) -> usize {
        self.provider_mismatch
    }

    /// 返回由其他实例持有同版本租约的数量。
    #[must_use]
    pub const fn lease_held(self) -> usize {
        self.lease_held
    }

    /// 返回已脱敏记录且留待后续周期重试的失败数量。
    #[must_use]
    pub const fn failed(self) -> usize {
        self.failed
    }

    fn has_activity(self) -> bool {
        self.backfill_scanned > 0 || self.backfill_failed > 0 || self.loaded > 0 || self.failed > 0
    }
}

/// OAuth 刷新周期任务错误；不携带 token、scope、端点或底层诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthRefreshSupervisorError {
    /// 系统时间不能安全转换为刷新查询使用的 Unix 秒数。
    #[error("OAuth 刷新系统时间无效")]
    InvalidClock,
    /// 到期候选查询失败。
    #[error("OAuth 刷新候选加载失败")]
    CandidateLoad(#[source] OAuthRefreshPersistenceError),
}

/// 周期扫描到期凭据并通过协调器有限并发刷新的生产执行器。
#[derive(Clone)]
pub struct OAuthRefreshSupervisor {
    core: OAuthRefreshSupervisorCore<
        Arc<OAuthRefreshPersistenceService>,
        Arc<OAuthRefreshCoordinator>,
    >,
}

impl OAuthRefreshSupervisor {
    /// 组合共享持久化服务、协调器与有界任务配置。
    #[must_use]
    pub fn new(
        persistence: Arc<OAuthRefreshPersistenceService>,
        coordinator: Arc<OAuthRefreshCoordinator>,
        config: OAuthRefreshSupervisorConfig,
    ) -> Self {
        Self {
            core: OAuthRefreshSupervisorCore::new(persistence, coordinator, config),
        }
    }

    /// 立即执行一轮旧投影回填和到期刷新。
    pub async fn run_once(&mut self) -> Result<OAuthRefreshRunReport, OAuthRefreshSupervisorError> {
        self.core.run_once_at(SystemTime::now()).await
    }

    /// 启动后立即执行首轮，此后按固定间隔运行直到关闭信号完成。
    ///
    /// 关闭会直接取消当前轮次内尚未完成的刷新 Future；分布式租约由 TTL 收敛，
    /// 不会为了等待租约释放而越过进程统一关闭截止时间。
    pub async fn run_periodic_until<F>(mut self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        self.core.run_periodic_until(shutdown).await;
    }
}

impl fmt::Debug for OAuthRefreshSupervisor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshSupervisor")
            .field("config", &self.core.config)
            .field("backfill_complete", &self.core.backfill_complete)
            .finish_non_exhaustive()
    }
}

trait OAuthRefreshCandidateSource {
    type Candidate: Send;

    fn backfill_missing_expiration_projections(
        &self,
        after_credential_id: Option<CredentialId>,
        limit: usize,
    ) -> impl Future<Output = Result<OAuthRefreshBackfillPage, OAuthRefreshPersistenceError>> + Send;

    fn due_candidates(
        &self,
        refresh_before_epoch_seconds: i64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Self::Candidate>, OAuthRefreshPersistenceError>> + Send;
}

impl OAuthRefreshCandidateSource for Arc<OAuthRefreshPersistenceService> {
    type Candidate = OAuthRefreshCandidate;

    async fn backfill_missing_expiration_projections(
        &self,
        after_credential_id: Option<CredentialId>,
        limit: usize,
    ) -> Result<OAuthRefreshBackfillPage, OAuthRefreshPersistenceError> {
        self.as_ref()
            .backfill_missing_expiration_projections(after_credential_id, limit)
            .await
            .map(OAuthRefreshBackfillPage::from)
    }

    async fn due_candidates(
        &self,
        refresh_before_epoch_seconds: i64,
        limit: usize,
    ) -> Result<Vec<Self::Candidate>, OAuthRefreshPersistenceError> {
        self.as_ref()
            .due_candidates(refresh_before_epoch_seconds, limit)
            .await
    }
}

trait OAuthRefreshCandidateRunner<C> {
    fn refresh(
        &self,
        candidate: C,
    ) -> impl Future<Output = Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError>> + Send;
}

impl OAuthRefreshCandidateRunner<OAuthRefreshCandidate> for Arc<OAuthRefreshCoordinator> {
    async fn refresh(
        &self,
        candidate: OAuthRefreshCandidate,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        self.as_ref().refresh(candidate).await
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OAuthRefreshBackfillPage {
    scanned: usize,
    projected: usize,
    incomplete: usize,
    conflicted: usize,
    last_credential_id: Option<CredentialId>,
}

impl From<OAuthExpirationProjectionBackfillBatch> for OAuthRefreshBackfillPage {
    fn from(batch: OAuthExpirationProjectionBackfillBatch) -> Self {
        Self {
            scanned: batch.scanned(),
            projected: batch.projected(),
            incomplete: batch.incomplete(),
            conflicted: batch.conflicted(),
            last_credential_id: batch.last_credential_id(),
        }
    }
}

#[derive(Clone)]
struct OAuthRefreshSupervisorCore<S, R> {
    source: S,
    runner: R,
    config: OAuthRefreshSupervisorConfig,
    backfill_after_credential_id: Option<CredentialId>,
    backfill_complete: bool,
}

impl<S, R> OAuthRefreshSupervisorCore<S, R> {
    const fn new(source: S, runner: R, config: OAuthRefreshSupervisorConfig) -> Self {
        Self {
            source,
            runner,
            config,
            backfill_after_credential_id: None,
            backfill_complete: false,
        }
    }
}

impl<S, R> OAuthRefreshSupervisorCore<S, R>
where
    S: OAuthRefreshCandidateSource,
    R: OAuthRefreshCandidateRunner<S::Candidate>,
{
    async fn run_once_at(
        &mut self,
        now: SystemTime,
    ) -> Result<OAuthRefreshRunReport, OAuthRefreshSupervisorError> {
        let mut report = OAuthRefreshRunReport::default();
        self.run_backfill(&mut report).await;
        let refresh_before_epoch_seconds =
            refresh_before_epoch_seconds(now, self.config.refresh_before_expiry)?;
        let candidates = self
            .source
            .due_candidates(refresh_before_epoch_seconds, self.config.batch_size)
            .await
            .map_err(OAuthRefreshSupervisorError::CandidateLoad)?;
        report.loaded = candidates.len();

        // 所有刷新 Future 都归属于当前轮次；关闭或 panic 会整体释放，禁止遗留后台任务。
        let refreshes = stream::iter(candidates)
            .map(|candidate| self.runner.refresh(candidate))
            .buffer_unordered(self.config.concurrency.get());
        pin_mut!(refreshes);
        while let Some(result) = refreshes.next().await {
            match result {
                Ok(OAuthRefreshCoordinatorOutcome::Stored) => report.stored += 1,
                Ok(OAuthRefreshCoordinatorOutcome::Stale) => report.stale += 1,
                Ok(OAuthRefreshCoordinatorOutcome::TargetNotFound) => {
                    report.target_not_found += 1;
                }
                Ok(OAuthRefreshCoordinatorOutcome::ProviderMismatch) => {
                    report.provider_mismatch += 1;
                }
                Ok(OAuthRefreshCoordinatorOutcome::LeaseHeld) => report.lease_held += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(
                        error_kind = coordinator_error_kind(error),
                        "OAuth 凭据刷新失败，留待后续周期处理"
                    );
                }
            }
        }
        Ok(report)
    }

    async fn run_backfill(&mut self, report: &mut OAuthRefreshRunReport) {
        if self.backfill_complete {
            return;
        }
        let page = self
            .source
            .backfill_missing_expiration_projections(
                self.backfill_after_credential_id,
                self.config.batch_size,
            )
            .await;
        match page {
            Ok(page) => {
                report.backfill_scanned = page.scanned;
                report.backfill_projected = page.projected;
                report.backfill_incomplete = page.incomplete;
                report.backfill_conflicted = page.conflicted;
                if page.scanned < self.config.batch_size {
                    self.backfill_complete = true;
                } else if let Some(last_credential_id) = page.last_credential_id {
                    self.backfill_after_credential_id = Some(last_credential_id);
                } else {
                    // 生产仓储不会返回“有记录但无游标”；失败关闭进度以避免忙循环。
                    self.backfill_complete = true;
                    report.backfill_failed = 1;
                    tracing::warn!(
                        error_kind = "oauth_refresh_backfill_cursor",
                        "OAuth 到期投影回填未返回有效游标，停止本进程后续回填"
                    );
                }
            }
            Err(error) => {
                report.backfill_failed = 1;
                tracing::warn!(
                    error_kind = persistence_error_kind(error),
                    "OAuth 到期投影回填失败，本周期继续处理已有投影"
                );
            }
        }
    }

    async fn run_periodic_until<F>(&mut self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => return,
                result = self.run_once_at(SystemTime::now()) => {
                    match result {
                        Ok(report) if report.has_activity() => tracing::info!(
                            backfill_scanned = report.backfill_scanned(),
                            backfill_projected = report.backfill_projected(),
                            backfill_incomplete = report.backfill_incomplete(),
                            backfill_conflicted = report.backfill_conflicted(),
                            backfill_failed = report.backfill_failed(),
                            loaded = report.loaded(),
                            stored = report.stored(),
                            stale = report.stale(),
                            target_not_found = report.target_not_found(),
                            provider_mismatch = report.provider_mismatch(),
                            lease_held = report.lease_held(),
                            failed = report.failed(),
                            "OAuth 刷新任务完成一轮扫描"
                        ),
                        Ok(_) => tracing::debug!("OAuth 刷新任务完成空闲扫描"),
                        Err(error) => tracing::warn!(
                            error_kind = supervisor_error_kind(error),
                            "OAuth 刷新候选加载失败，本周期跳过"
                        ),
                    }
                }
            }

            tokio::select! {
                biased;
                () = &mut shutdown => return,
                () = sleep(self.config.interval) => {}
            }
        }
    }
}

fn refresh_before_epoch_seconds(
    now: SystemTime,
    refresh_before_expiry: Duration,
) -> Result<i64, OAuthRefreshSupervisorError> {
    let now = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OAuthRefreshSupervisorError::InvalidClock)?
        .as_secs();
    let threshold = now
        .checked_add(refresh_before_expiry.as_secs())
        .ok_or(OAuthRefreshSupervisorError::InvalidClock)?;
    i64::try_from(threshold).map_err(|_| OAuthRefreshSupervisorError::InvalidClock)
}

const fn supervisor_error_kind(error: OAuthRefreshSupervisorError) -> &'static str {
    match error {
        OAuthRefreshSupervisorError::InvalidClock => "oauth_refresh_clock",
        OAuthRefreshSupervisorError::CandidateLoad(_) => "oauth_refresh_candidate_load",
    }
}

const fn coordinator_error_kind(error: OAuthRefreshCoordinatorError) -> &'static str {
    match error {
        OAuthRefreshCoordinatorError::DuplicateProviderProfile { .. } => {
            "oauth_refresh_duplicate_profile"
        }
        OAuthRefreshCoordinatorError::ProviderProfileNotConfigured { .. } => {
            "oauth_refresh_profile_missing"
        }
        OAuthRefreshCoordinatorError::HttpClientUnavailable => "oauth_refresh_http_client",
        OAuthRefreshCoordinatorError::SingleflightCapacityExceeded => {
            "oauth_refresh_singleflight_capacity"
        }
        OAuthRefreshCoordinatorError::SingleflightUnavailable => {
            "oauth_refresh_singleflight_unavailable"
        }
        OAuthRefreshCoordinatorError::LeaderAborted => "oauth_refresh_leader_aborted",
        OAuthRefreshCoordinatorError::DistributedLease(_) => "oauth_refresh_lease",
        OAuthRefreshCoordinatorError::TokenExchange(_) => "oauth_refresh_token_exchange",
        OAuthRefreshCoordinatorError::Persistence(_) => "oauth_refresh_persistence",
        OAuthRefreshCoordinatorError::FailureStatePersistence { .. } => {
            "oauth_refresh_failure_state"
        }
    }
}

const fn persistence_error_kind(error: OAuthRefreshPersistenceError) -> &'static str {
    match error {
        OAuthRefreshPersistenceError::InvalidCandidateQuery => "oauth_refresh_candidate_query",
        OAuthRefreshPersistenceError::InvalidCandidate => "oauth_refresh_candidate_invalid",
        OAuthRefreshPersistenceError::MissingRefreshToken => "oauth_refresh_token_missing",
        OAuthRefreshPersistenceError::InvalidResultProvider => "oauth_refresh_result_provider",
        OAuthRefreshPersistenceError::InvalidExpiration => "oauth_refresh_expiration",
        OAuthRefreshPersistenceError::InvalidTokenSet => "oauth_refresh_token_set",
        OAuthRefreshPersistenceError::Encryption => "oauth_refresh_encryption",
        OAuthRefreshPersistenceError::RepositoryUnavailable => "oauth_refresh_repository",
        OAuthRefreshPersistenceError::RepositoryTimeout => "oauth_refresh_repository_timeout",
        OAuthRefreshPersistenceError::Invariant => "oauth_refresh_invariant",
    }
}

#[cfg(test)]
mod tests;
