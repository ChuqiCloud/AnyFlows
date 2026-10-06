use std::{collections::BTreeMap, fmt, future::Future, sync::Arc, time::Duration};

use af_cache::{
    CacheError, ConcurrencyAcquireOutcome, ConcurrencyLeaseOutcome, ConcurrencyWaitOutcome,
    MAX_CONCURRENCY_CLEANUP_BATCH_SIZE, RedisConcurrencyLease, RedisConcurrencyStore,
    RedisConcurrencyWait,
};
use af_domain::{ConcurrencyLimit, CredentialId, GatewayPrincipal, TokenId, UserId};
use af_protocol::Usage;
use af_relay::{
    GenerationCompletionFuture, GenerationCompletionHook, RelayAttemptGate, RelayAttemptGateError,
    RelayAttemptGateFuture, RelayAttemptPermit, RelayAttemptReleaseFuture, UsageResolutionError,
};
use af_scheduler::{ConcurrencyWaitBackoff, concurrency_wait_queue_limit};
use thiserror::Error;
use tokio::{
    sync::{Mutex, mpsc},
    time::{Instant, sleep},
};

use crate::credential_order::CredentialLoad;

/// 并发槽位异步释放队列容量；饱和时仍由 Redis TTL 保证最终收敛。
pub(crate) const CONCURRENCY_RELEASE_QUEUE_CAPACITY: usize = 4_096;
/// 孤儿槽位索引的周期清理间隔；首次扫描在任务启动后立即执行。
pub(crate) const CONCURRENCY_CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

/// 生产请求使用的 Redis 槽位、等待策略与非阻塞释放端口。
#[derive(Clone)]
pub(crate) struct ConcurrencyRuntime {
    store: RedisConcurrencyStore,
    release_queue: ConcurrencyReleaseQueue,
}

impl ConcurrencyRuntime {
    /// 绑定已通过启动健康检查的 Redis 存储并创建受监督释放 worker。
    pub(crate) fn new(
        store: RedisConcurrencyStore,
    ) -> (Self, ConcurrencyReleaseWorker, ConcurrencyCleanupWorker) {
        let (sender, receiver) = mpsc::channel(CONCURRENCY_RELEASE_QUEUE_CAPACITY);
        let release_queue = ConcurrencyReleaseQueue { sender };
        (
            Self {
                store: store.clone(),
                release_queue,
            },
            ConcurrencyReleaseWorker {
                receiver: Arc::new(Mutex::new(receiver)),
            },
            ConcurrencyCleanupWorker {
                store,
                interval: CONCURRENCY_CLEANUP_INTERVAL,
            },
        )
    }

    /// 在进入上游候选循环前获取用户槽位并追踪令牌层。
    pub(crate) async fn acquire_user(
        &self,
        principal: GatewayPrincipal,
        limit: Option<ConcurrencyLimit>,
        timeout: Duration,
    ) -> Result<RuntimeConcurrencyPermit, ConcurrencyRuntimeError> {
        self.acquire_with_wait(
            RuntimeSlotTarget::User {
                user_id: principal.user_id(),
                token_id: principal.token_id(),
                limit,
            },
            timeout,
        )
        .await
    }

    /// 为单个运行时凭据创建候选许可门；等待时间由路由计划提前固定。
    pub(crate) fn account_gate(
        &self,
        credential_id: CredentialId,
        limit: Option<ConcurrencyLimit>,
        timeout: Duration,
    ) -> Arc<dyn RelayAttemptGate> {
        Arc::new(AccountConcurrencyGate {
            runtime: self.clone(),
            credential_id,
            limit,
            timeout,
        })
    }

    /// 批量读取账号负载，供普通回退候选做负载感知排序。
    pub(crate) async fn account_loads(
        &self,
        accounts: &[(CredentialId, Option<ConcurrencyLimit>)],
    ) -> Result<BTreeMap<i64, CredentialLoad>, CacheError> {
        self.store.account_loads(accounts).await.map(|loads| {
            loads
                .into_iter()
                .map(|load| {
                    (
                        load.credential_id().get(),
                        CredentialLoad::new(load.active(), load.waiting(), load.limit()),
                    )
                })
                .collect()
        })
    }

    async fn acquire_with_wait(
        &self,
        target: RuntimeSlotTarget,
        timeout: Duration,
    ) -> Result<RuntimeConcurrencyPermit, ConcurrencyRuntimeError> {
        if timeout.is_zero() {
            return Err(ConcurrencyRuntimeError::Internal);
        }
        if let Some(lease) = self.try_acquire(target).await? {
            return Ok(self.permit(lease));
        }
        let Some(limit) = target.limit() else {
            return Err(ConcurrencyRuntimeError::Internal);
        };
        let waiting = self
            .enter_wait(target, concurrency_wait_queue_limit(limit))
            .await?;
        let deadline = Instant::now() + timeout;
        let mut backoff = ConcurrencyWaitBackoff::new();

        loop {
            let now = Instant::now();
            if now >= deadline {
                leave_wait(waiting).await;
                return Err(ConcurrencyRuntimeError::Limited);
            }
            sleep(backoff.next_delay(deadline.saturating_duration_since(now))).await;
            match waiting.renew().await {
                Ok(ConcurrencyLeaseOutcome::Applied) => {}
                Ok(ConcurrencyLeaseOutcome::Lost) | Err(_) => {
                    leave_wait(waiting).await;
                    return Err(ConcurrencyRuntimeError::Internal);
                }
            }
            if let Some(lease) = self.try_acquire(target).await? {
                leave_wait(waiting).await;
                return Ok(self.permit(lease));
            }
        }
    }

    async fn try_acquire(
        &self,
        target: RuntimeSlotTarget,
    ) -> Result<Option<RedisConcurrencyLease>, ConcurrencyRuntimeError> {
        let outcome = match target {
            RuntimeSlotTarget::User {
                user_id,
                token_id,
                limit,
            } => {
                self.store
                    .acquire_user_token(user_id, limit, token_id)
                    .await
            }
            RuntimeSlotTarget::Account {
                credential_id,
                limit,
            } => self.store.acquire_account(credential_id, limit).await,
        }
        .map_err(|_| ConcurrencyRuntimeError::Internal)?;
        Ok(match outcome {
            ConcurrencyAcquireOutcome::Acquired(lease) => Some(lease),
            ConcurrencyAcquireOutcome::Limited => None,
        })
    }

    async fn enter_wait(
        &self,
        target: RuntimeSlotTarget,
        max_waiting: ConcurrencyLimit,
    ) -> Result<RedisConcurrencyWait, ConcurrencyRuntimeError> {
        let outcome = match target {
            RuntimeSlotTarget::User { user_id, .. } => {
                self.store.enter_user_wait(user_id, max_waiting).await
            }
            RuntimeSlotTarget::Account { credential_id, .. } => {
                self.store
                    .enter_account_wait(credential_id, max_waiting)
                    .await
            }
        }
        .map_err(|_| ConcurrencyRuntimeError::Internal)?;
        match outcome {
            ConcurrencyWaitOutcome::Entered(waiting) => Ok(waiting),
            ConcurrencyWaitOutcome::Full => Err(ConcurrencyRuntimeError::Limited),
        }
    }

    fn permit(&self, lease: RedisConcurrencyLease) -> RuntimeConcurrencyPermit {
        RuntimeConcurrencyPermit {
            lease: Some(lease),
            release_queue: self.release_queue.clone(),
        }
    }
}

impl fmt::Debug for ConcurrencyRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConcurrencyRuntime")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
enum RuntimeSlotTarget {
    User {
        user_id: UserId,
        token_id: TokenId,
        limit: Option<ConcurrencyLimit>,
    },
    Account {
        credential_id: CredentialId,
        limit: Option<ConcurrencyLimit>,
    },
}

impl RuntimeSlotTarget {
    const fn limit(self) -> Option<ConcurrencyLimit> {
        match self {
            Self::User { limit, .. } | Self::Account { limit, .. } => limit,
        }
    }
}

/// 请求等待结束时可安全公开给调度层的闭合结果。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ConcurrencyRuntimeError {
    /// 槽位或等待队列在截止时间内仍满。
    #[error("并发槽位已达上限")]
    Limited,
    /// Redis 或运行期不变量失败。
    #[error("并发槽位运行时失败")]
    Internal,
}

struct AccountConcurrencyGate {
    runtime: ConcurrencyRuntime,
    credential_id: CredentialId,
    limit: Option<ConcurrencyLimit>,
    timeout: Duration,
}

impl RelayAttemptGate for AccountConcurrencyGate {
    fn acquire(&self) -> RelayAttemptGateFuture<'_> {
        Box::pin(async move {
            self.runtime
                .acquire_with_wait(
                    RuntimeSlotTarget::Account {
                        credential_id: self.credential_id,
                        limit: self.limit,
                    },
                    self.timeout,
                )
                .await
                .map(|permit| Box::new(permit) as Box<dyn RelayAttemptPermit>)
                .map_err(|error| match error {
                    ConcurrencyRuntimeError::Limited => RelayAttemptGateError::Limited,
                    ConcurrencyRuntimeError::Internal => RelayAttemptGateError::Internal,
                })
        })
    }
}

/// 持有 Redis 槽位的请求级许可；Drop 只尝试非阻塞入队，绝不执行异步 IO。
pub(crate) struct RuntimeConcurrencyPermit {
    lease: Option<RedisConcurrencyLease>,
    release_queue: ConcurrencyReleaseQueue,
}

impl RuntimeConcurrencyPermit {
    pub(crate) async fn release(mut self) {
        if let Some(lease) = self.lease.take() {
            release_lease(lease).await;
        }
    }
}

impl RelayAttemptPermit for RuntimeConcurrencyPermit {
    fn release(self: Box<Self>) -> RelayAttemptReleaseFuture {
        Box::pin(async move { (*self).release().await })
    }
}

impl Drop for RuntimeConcurrencyPermit {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        if self.release_queue.sender.try_send(lease).is_err() {
            tracing::warn!(
                target: "af_server::concurrency",
                error_kind = "concurrency_release_queue_unavailable",
                "并发槽位释放未能入队，将依靠 Redis TTL 收敛"
            );
        }
    }
}

impl fmt::Debug for RuntimeConcurrencyPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeConcurrencyPermit(<已脱敏>)")
    }
}

/// 把用户/令牌许可绑定到完整生成流生命周期。
pub(crate) fn user_permit_completion_hook(
    permit: RuntimeConcurrencyPermit,
) -> Box<dyn GenerationCompletionHook> {
    Box::new(UserPermitCompletionHook {
        permit: Some(permit),
    })
}

struct UserPermitCompletionHook {
    permit: Option<RuntimeConcurrencyPermit>,
}

impl GenerationCompletionHook for UserPermitCompletionHook {
    fn on_complete(
        mut self: Box<Self>,
        _usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        let permit = self.permit.take();
        Box::pin(async move {
            if let Some(permit) = permit {
                permit.release().await;
            }
        })
    }
}

#[derive(Clone)]
struct ConcurrencyReleaseQueue {
    sender: mpsc::Sender<RedisConcurrencyLease>,
}

/// 处理取消与早退路径提交的异步槽位释放请求。
#[derive(Clone)]
pub(crate) struct ConcurrencyReleaseWorker {
    receiver: Arc<Mutex<mpsc::Receiver<RedisConcurrencyLease>>>,
}

impl ConcurrencyReleaseWorker {
    /// 持续释放直到收到统一关闭信号，关闭时排空当前已入队请求。
    pub(crate) async fn run_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        let mut receiver = self.receiver.lock().await;
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                () = &mut shutdown => {
                    while let Ok(lease) = receiver.try_recv() {
                        release_lease(lease).await;
                    }
                    return;
                }
                lease = receiver.recv() => match lease {
                    Some(lease) => release_lease(lease).await,
                    None => return,
                }
            }
        }
    }
}

impl fmt::Debug for ConcurrencyReleaseWorker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConcurrencyReleaseWorker(<受控>)")
    }
}

/// 周期清理活跃索引中的过期成员，不按进程前缀误删其他存活实例的槽位。
#[derive(Clone)]
pub(crate) struct ConcurrencyCleanupWorker {
    store: RedisConcurrencyStore,
    interval: Duration,
}

impl ConcurrencyCleanupWorker {
    /// 启动即清理一轮，随后按固定间隔运行到统一关闭信号。
    pub(crate) async fn run_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            match self
                .store
                .cleanup_expired(MAX_CONCURRENCY_CLEANUP_BATCH_SIZE)
                .await
            {
                Ok(report) if report.removed_members() > 0 => {
                    tracing::debug!(
                        target: "af_server::concurrency",
                        inspected_keys = report.inspected_keys(),
                        removed_members = report.removed_members(),
                        "已清理过期并发槽位成员"
                    );
                }
                Ok(_) => {}
                Err(_) => {
                    tracing::warn!(
                        target: "af_server::concurrency",
                        error_kind = "concurrency_cleanup_failed",
                        "清理过期并发槽位失败，将在下一周期重试"
                    );
                }
            }
            tokio::select! {
                () = &mut shutdown => return,
                () = sleep(self.interval) => {}
            }
        }
    }
}

impl fmt::Debug for ConcurrencyCleanupWorker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConcurrencyCleanupWorker(<受控>)")
    }
}

async fn release_lease(lease: RedisConcurrencyLease) {
    if lease.release().await.is_err() {
        tracing::warn!(
            target: "af_server::concurrency",
            error_kind = "concurrency_release_failed",
            "释放 Redis 并发槽位失败，将依靠 TTL 收敛"
        );
    }
}

async fn leave_wait(waiting: RedisConcurrencyWait) {
    if waiting.leave().await.is_err() {
        tracing::warn!(
            target: "af_server::concurrency",
            error_kind = "concurrency_wait_release_failed",
            "清理并发等待登记失败，将依靠 TTL 收敛"
        );
    }
}
