use std::{fmt, future::Future, time::Duration};

use af_db::{
    ChannelProbeLease, ChannelProbeRecoveryOutcome, ChannelStateRepository,
    ChannelStateRepositoryError, MAX_CHANNEL_PROBE_BATCH,
};
use af_domain::ChannelId;
use thiserror::Error;
use tokio::time::{sleep, timeout};

/// 默认单轮探活批次，低于仓储硬上限以减少后台任务尖峰占用。
pub const DEFAULT_CHANNEL_PROBE_BATCH_SIZE: usize = 16;
/// 默认探活周期；真实生产接线后可由配置层覆盖。
pub const DEFAULT_CHANNEL_PROBE_INTERVAL: Duration = Duration::from_secs(60);
/// 单个渠道探活允许占用的默认硬截止时间。
pub const DEFAULT_CHANNEL_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// 单次渠道探活的归一化结果。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelProbeStatus {
    /// 探活成功，允许尝试 CAS 恢复自动禁用状态。
    Healthy,
    /// 探活失败或上游仍不可用，不得恢复渠道。
    Unhealthy,
    /// 探活达到渠道或执行器硬截止时间，不得恢复渠道。
    TimedOut,
}

/// 周期探活执行器依赖的抽象探活端口。
///
/// 端口实现负责完成真实上游测试调用、凭据读取和错误归一化；执行器只接收脱敏后的
/// `Healthy/Unhealthy` 结论，避免把响应正文或密钥带入调度层。
pub trait ChannelProbe: Send + Sync {
    /// 对指定渠道执行一次有界探活，返回归一化健康结论。
    fn check(&self, channel_id: ChannelId) -> impl Future<Output = ChannelProbeStatus> + Send;
}

/// 探活执行器依赖的最小持久化端口。
pub trait ChannelProbeStore: Send + Sync {
    /// 按游标读取一轮自动禁用渠道租约。
    fn load_probe_candidates(
        &self,
        after_channel_id: Option<ChannelId>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ChannelProbeLease>, ChannelStateRepositoryError>> + Send;

    /// 探活成功后按租约 CAS 恢复渠道状态。
    fn recover_after_probe(
        &self,
        lease: ChannelProbeLease,
    ) -> impl Future<Output = Result<ChannelProbeRecoveryOutcome, ChannelStateRepositoryError>> + Send;
}

impl ChannelProbeStore for ChannelStateRepository {
    fn load_probe_candidates(
        &self,
        after_channel_id: Option<ChannelId>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ChannelProbeLease>, ChannelStateRepositoryError>> + Send
    {
        ChannelStateRepository::load_probe_candidates(self, after_channel_id, limit)
    }

    fn recover_after_probe(
        &self,
        lease: ChannelProbeLease,
    ) -> impl Future<Output = Result<ChannelProbeRecoveryOutcome, ChannelStateRepositoryError>> + Send
    {
        ChannelStateRepository::recover_after_probe(self, lease)
    }
}

/// 探活执行器配置错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelProbeSupervisorConfigError {
    /// 单轮批次为零或超过仓储硬上限。
    #[error("渠道探活批次大小无效")]
    InvalidBatchSize,
    /// 周期为零会造成忙轮询。
    #[error("渠道探活周期必须大于零")]
    ZeroInterval,
    /// 单次探活截止时间为零无法形成有效边界。
    #[error("渠道探活超时必须大于零")]
    ZeroProbeTimeout,
}

/// 有界探活执行器配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelProbeSupervisorConfig {
    batch_size: usize,
    interval: Duration,
    probe_timeout: Duration,
}

impl ChannelProbeSupervisorConfig {
    /// 校验并创建探活执行器配置。
    pub fn new(
        batch_size: usize,
        interval: Duration,
        probe_timeout: Duration,
    ) -> Result<Self, ChannelProbeSupervisorConfigError> {
        if batch_size == 0 || batch_size > MAX_CHANNEL_PROBE_BATCH {
            return Err(ChannelProbeSupervisorConfigError::InvalidBatchSize);
        }
        if interval.is_zero() {
            return Err(ChannelProbeSupervisorConfigError::ZeroInterval);
        }
        if probe_timeout.is_zero() {
            return Err(ChannelProbeSupervisorConfigError::ZeroProbeTimeout);
        }
        Ok(Self {
            batch_size,
            interval,
            probe_timeout,
        })
    }

    /// 返回单轮读取的最大租约数量。
    #[must_use]
    pub const fn batch_size(self) -> usize {
        self.batch_size
    }

    /// 返回周期执行间隔。
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }

    /// 返回单渠道探活硬截止时间。
    #[must_use]
    pub const fn probe_timeout(self) -> Duration {
        self.probe_timeout
    }
}

impl Default for ChannelProbeSupervisorConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_CHANNEL_PROBE_BATCH_SIZE,
            interval: DEFAULT_CHANNEL_PROBE_INTERVAL,
            probe_timeout: DEFAULT_CHANNEL_PROBE_TIMEOUT,
        }
    }
}

/// 单轮探活执行的聚合结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelProbeRunReport {
    loaded: usize,
    healthy: usize,
    unhealthy: usize,
    timed_out: usize,
    recovered: usize,
    recovery_noop: usize,
    recovery_failed: usize,
}

impl ChannelProbeRunReport {
    /// 返回本轮加载到的租约数量。
    #[must_use]
    pub const fn loaded(self) -> usize {
        self.loaded
    }

    /// 返回探活成功的渠道数量。
    #[must_use]
    pub const fn healthy(self) -> usize {
        self.healthy
    }

    /// 返回探活明确失败的渠道数量。
    #[must_use]
    pub const fn unhealthy(self) -> usize {
        self.unhealthy
    }

    /// 返回达到单渠道硬截止时间的探活数量。
    #[must_use]
    pub const fn timed_out(self) -> usize {
        self.timed_out
    }

    /// 返回成功恢复为启用状态的渠道数量。
    #[must_use]
    pub const fn recovered(self) -> usize {
        self.recovered
    }

    /// 返回无需或不能恢复的幂等 CAS 结果数量。
    #[must_use]
    pub const fn recovery_noop(self) -> usize {
        self.recovery_noop
    }

    /// 返回探活成功但恢复写入失败的数量。
    #[must_use]
    pub const fn recovery_failed(self) -> usize {
        self.recovery_failed
    }
}

/// 探活执行器运行错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelProbeSupervisorError {
    /// 读取探活租约失败。
    #[error("读取渠道探活租约失败")]
    Store(#[from] ChannelStateRepositoryError),
}

/// 周期消费自动禁用租约并在成功探活后 CAS 恢复渠道的执行器。
pub struct ChannelProbeSupervisor<S, P> {
    store: S,
    probe: P,
    config: ChannelProbeSupervisorConfig,
    after_channel_id: Option<ChannelId>,
}

impl<S, P> ChannelProbeSupervisor<S, P> {
    /// 使用显式仓储、探活端口与配置创建执行器。
    #[must_use]
    pub const fn new(store: S, probe: P, config: ChannelProbeSupervisorConfig) -> Self {
        Self {
            store,
            probe,
            config,
            after_channel_id: None,
        }
    }

    /// 返回下一轮读取会使用的游标，主要用于测试和运行态观测。
    #[must_use]
    pub const fn after_channel_id(&self) -> Option<ChannelId> {
        self.after_channel_id
    }
}

impl<S, P> ChannelProbeSupervisor<S, P>
where
    S: ChannelProbeStore,
    P: ChannelProbe,
{
    /// 执行一轮有界探活。
    ///
    /// 只有 `Healthy` 探活结果会触发恢复写入；探活失败、超时或恢复写入失败都不会
    /// 修改渠道状态，也不会阻止本轮继续处理后续租约。
    pub async fn run_once(&mut self) -> Result<ChannelProbeRunReport, ChannelProbeSupervisorError> {
        let leases = self
            .store
            .load_probe_candidates(self.after_channel_id, self.config.batch_size)
            .await?;
        if leases.is_empty() {
            self.after_channel_id = None;
            return Ok(ChannelProbeRunReport::default());
        }
        self.after_channel_id = leases.last().map(ChannelProbeLease::channel_id);

        let mut report = ChannelProbeRunReport {
            loaded: leases.len(),
            ..ChannelProbeRunReport::default()
        };
        for lease in leases {
            match timeout(
                self.config.probe_timeout,
                self.probe.check(lease.channel_id()),
            )
            .await
            {
                Ok(ChannelProbeStatus::Healthy) => {
                    report.healthy += 1;
                    match self.store.recover_after_probe(lease).await {
                        Ok(ChannelProbeRecoveryOutcome::Recovered) => report.recovered += 1,
                        Ok(
                            ChannelProbeRecoveryOutcome::AlreadyEnabled
                            | ChannelProbeRecoveryOutcome::StaleProbe
                            | ChannelProbeRecoveryOutcome::NotEligible,
                        ) => report.recovery_noop += 1,
                        Err(_) => report.recovery_failed += 1,
                    }
                }
                Ok(ChannelProbeStatus::Unhealthy) => report.unhealthy += 1,
                Ok(ChannelProbeStatus::TimedOut) => report.timed_out += 1,
                Err(_) => report.timed_out += 1,
            }
        }
        Ok(report)
    }

    /// 按配置周期运行，直到外部关闭信号完成。
    ///
    /// 租约加载失败属于后台任务瞬时故障，执行器记录结构化日志后进入下一周期；真正的
    /// 任务重启和关闭截止由上层 supervisor 负责。
    pub async fn run_periodic_until<F>(&mut self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                () = &mut shutdown => return,
                result = self.run_once() => {
                    if result.is_err() {
                        tracing::warn!(
                            error_kind = "channel_probe_load",
                            "渠道探活租约加载失败，本周期跳过"
                        );
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

impl<S, P> fmt::Debug for ChannelProbeSupervisor<S, P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelProbeSupervisor")
            .field("batch_size", &self.config.batch_size)
            .field("interval", &self.config.interval)
            .field("probe_timeout", &self.config.probe_timeout)
            .field("after_channel_id", &self.after_channel_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        future,
        sync::{Arc, Mutex},
    };

    use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
    use tokio::sync::{Notify, oneshot};

    use super::*;

    #[test]
    fn config_rejects_unbounded_or_busy_loop_values() {
        assert_eq!(
            ChannelProbeSupervisorConfig::new(0, Duration::from_secs(1), Duration::from_secs(1)),
            Err(ChannelProbeSupervisorConfigError::InvalidBatchSize)
        );
        assert_eq!(
            ChannelProbeSupervisorConfig::new(
                MAX_CHANNEL_PROBE_BATCH + 1,
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(ChannelProbeSupervisorConfigError::InvalidBatchSize)
        );
        assert_eq!(
            ChannelProbeSupervisorConfig::new(1, Duration::ZERO, Duration::from_secs(1)),
            Err(ChannelProbeSupervisorConfigError::ZeroInterval)
        );
        assert_eq!(
            ChannelProbeSupervisorConfig::new(1, Duration::from_secs(1), Duration::ZERO),
            Err(ChannelProbeSupervisorConfigError::ZeroProbeTimeout)
        );
    }

    #[tokio::test]
    async fn healthy_probe_is_the_only_path_to_recovery() {
        let healthy = lease(1);
        let unhealthy = lease(2);
        let store = FakeStore::new(vec![vec![healthy.clone(), unhealthy.clone()]]);
        let probe = FakeProbe::new(vec![
            (healthy.channel_id(), ProbeBehavior::Healthy),
            (unhealthy.channel_id(), ProbeBehavior::Unhealthy),
        ]);
        let mut supervisor = ChannelProbeSupervisor::new(store.clone(), probe, fast_config());

        let report = supervisor.run_once().await.unwrap();

        assert_eq!(report.loaded(), 2);
        assert_eq!(report.healthy(), 1);
        assert_eq!(report.unhealthy(), 1);
        assert_eq!(report.recovered(), 1);
        assert_eq!(store.recoveries(), vec![healthy.channel_id()]);
        assert_eq!(supervisor.after_channel_id(), Some(unhealthy.channel_id()));
    }

    #[tokio::test]
    async fn probe_timeout_does_not_recover_channel() {
        let pending = lease(3);
        let store = FakeStore::new(vec![vec![pending.clone()]]);
        let probe = FakeProbe::new(vec![(pending.channel_id(), ProbeBehavior::Pending)]);
        let mut supervisor = ChannelProbeSupervisor::new(store.clone(), probe, fast_config());

        let report = supervisor.run_once().await.unwrap();

        assert_eq!(report.loaded(), 1);
        assert_eq!(report.timed_out(), 1);
        assert_eq!(report.recovered(), 0);
        assert!(store.recoveries().is_empty());
    }

    #[tokio::test]
    async fn recovery_failure_is_counted_without_aborting_remaining_leases() {
        let failed = lease(4);
        let recovered = lease(5);
        let store = FakeStore::new(vec![vec![failed.clone(), recovered.clone()]])
            .with_recovery_results(vec![
                Err(ChannelStateRepositoryError::Query),
                Ok(ChannelProbeRecoveryOutcome::AlreadyEnabled),
            ]);
        let probe = FakeProbe::new(vec![
            (failed.channel_id(), ProbeBehavior::Healthy),
            (recovered.channel_id(), ProbeBehavior::Healthy),
        ]);
        let mut supervisor = ChannelProbeSupervisor::new(store.clone(), probe, fast_config());

        let report = supervisor.run_once().await.unwrap();

        assert_eq!(report.healthy(), 2);
        assert_eq!(report.recovery_failed(), 1);
        assert_eq!(report.recovery_noop(), 1);
        assert_eq!(
            store.recoveries(),
            vec![failed.channel_id(), recovered.channel_id()]
        );
    }

    #[tokio::test]
    async fn empty_tail_resets_cursor_for_next_scan() {
        let first = lease(6);
        let store = FakeStore::new(vec![vec![first.clone()], Vec::new()]);
        let probe = FakeProbe::new(vec![(first.channel_id(), ProbeBehavior::Unhealthy)]);
        let mut supervisor = ChannelProbeSupervisor::new(store.clone(), probe, fast_config());

        assert_eq!(supervisor.run_once().await.unwrap().loaded(), 1);
        assert_eq!(supervisor.after_channel_id(), Some(first.channel_id()));
        assert_eq!(supervisor.run_once().await.unwrap().loaded(), 0);
        assert_eq!(supervisor.after_channel_id(), None);
        assert_eq!(
            store.loads(),
            vec![(None, 2), (Some(first.channel_id()), 2)]
        );
    }

    #[tokio::test]
    async fn periodic_runner_cancels_inflight_probe_on_shutdown() {
        let pending = lease(7);
        let store = FakeStore::new(vec![vec![pending]]);
        let started = Arc::new(Notify::new());
        let probe = BlockingProbe {
            started: Arc::clone(&started),
        };
        let mut supervisor = ChannelProbeSupervisor::new(store.clone(), probe, fast_config());
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            supervisor
                .run_periodic_until(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        started.notified().await;
        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(store.recoveries().is_empty());
    }

    fn fast_config() -> ChannelProbeSupervisorConfig {
        ChannelProbeSupervisorConfig::new(2, Duration::from_millis(1), Duration::from_millis(1))
            .unwrap()
    }

    fn lease(channel_id: i64) -> ChannelProbeLease {
        ChannelProbeLease::new(
            ChannelId::new(channel_id).unwrap(),
            TimeDateTimeWithTimeZone::now_utc(),
        )
    }

    #[derive(Clone)]
    struct FakeStore {
        batches: Arc<Mutex<VecDeque<Vec<ChannelProbeLease>>>>,
        loads: Arc<Mutex<Vec<(Option<ChannelId>, usize)>>>,
        recoveries: Arc<Mutex<Vec<ChannelId>>>,
        recovery_results:
            Arc<Mutex<VecDeque<Result<ChannelProbeRecoveryOutcome, ChannelStateRepositoryError>>>>,
    }

    impl FakeStore {
        fn new(batches: Vec<Vec<ChannelProbeLease>>) -> Self {
            Self {
                batches: Arc::new(Mutex::new(batches.into())),
                loads: Arc::new(Mutex::new(Vec::new())),
                recoveries: Arc::new(Mutex::new(Vec::new())),
                recovery_results: Arc::new(Mutex::new(VecDeque::new())),
            }
        }

        fn with_recovery_results(
            self,
            results: Vec<Result<ChannelProbeRecoveryOutcome, ChannelStateRepositoryError>>,
        ) -> Self {
            *self.recovery_results.lock().unwrap() = results.into();
            self
        }

        fn loads(&self) -> Vec<(Option<ChannelId>, usize)> {
            self.loads.lock().unwrap().clone()
        }

        fn recoveries(&self) -> Vec<ChannelId> {
            self.recoveries.lock().unwrap().clone()
        }
    }

    impl ChannelProbeStore for FakeStore {
        fn load_probe_candidates(
            &self,
            after_channel_id: Option<ChannelId>,
            limit: usize,
        ) -> impl Future<Output = Result<Vec<ChannelProbeLease>, ChannelStateRepositoryError>> + Send
        {
            self.loads.lock().unwrap().push((after_channel_id, limit));
            let batch = self.batches.lock().unwrap().pop_front().unwrap_or_default();
            async move { Ok(batch) }
        }

        fn recover_after_probe(
            &self,
            lease: ChannelProbeLease,
        ) -> impl Future<Output = Result<ChannelProbeRecoveryOutcome, ChannelStateRepositoryError>> + Send
        {
            self.recoveries.lock().unwrap().push(lease.channel_id());
            let result = self
                .recovery_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(ChannelProbeRecoveryOutcome::Recovered));
            async move { result }
        }
    }

    #[derive(Clone, Copy)]
    enum ProbeBehavior {
        Healthy,
        Unhealthy,
        Pending,
    }

    #[derive(Clone)]
    struct FakeProbe {
        behaviors: Arc<Mutex<VecDeque<(ChannelId, ProbeBehavior)>>>,
    }

    impl FakeProbe {
        fn new(behaviors: Vec<(ChannelId, ProbeBehavior)>) -> Self {
            Self {
                behaviors: Arc::new(Mutex::new(behaviors.into())),
            }
        }
    }

    impl ChannelProbe for FakeProbe {
        fn check(&self, channel_id: ChannelId) -> impl Future<Output = ChannelProbeStatus> + Send {
            let (expected_channel_id, behavior) = self
                .behaviors
                .lock()
                .unwrap()
                .pop_front()
                .expect("测试探活行为必须覆盖每个租约");
            assert_eq!(expected_channel_id, channel_id);
            async move {
                match behavior {
                    ProbeBehavior::Healthy => ChannelProbeStatus::Healthy,
                    ProbeBehavior::Unhealthy => ChannelProbeStatus::Unhealthy,
                    ProbeBehavior::Pending => future::pending().await,
                }
            }
        }
    }

    struct BlockingProbe {
        started: Arc<Notify>,
    }

    impl ChannelProbe for BlockingProbe {
        fn check(&self, _channel_id: ChannelId) -> impl Future<Output = ChannelProbeStatus> + Send {
            let started = Arc::clone(&self.started);
            async move {
                started.notify_one();
                future::pending().await
            }
        }
    }
}
