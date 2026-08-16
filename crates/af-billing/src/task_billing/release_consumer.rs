use std::{future::Future, panic::AssertUnwindSafe, pin::Pin, sync::Arc, time::Duration};

use af_db::{QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus};
use af_domain::BillingReservationId;
use futures_util::FutureExt as _;
use thiserror::Error;
use tokio::time::sleep;

use super::release_queue::TaskBillingReleaseReceiver;

/// 任务释放 sink 的对象安全异步调用。
pub type TaskBillingReleaseSinkFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), TaskBillingReleaseSinkError>> + Send + 'a>>;

/// 只处理批量任务冻结释放的持久化端口。
pub trait TaskBillingReleaseSink: Send + Sync + 'static {
    /// 对同一任务预留标识执行或重放 `release_batch_task`。
    fn release<'a>(
        &'a self,
        reservation_id: BillingReservationId,
    ) -> TaskBillingReleaseSinkFuture<'a>;
}

impl TaskBillingReleaseSink for QuotaRepository {
    fn release<'a>(
        &'a self,
        reservation_id: BillingReservationId,
    ) -> TaskBillingReleaseSinkFuture<'a> {
        Box::pin(async move {
            let outcome = QuotaRepository::release_batch_task(self, reservation_id)
                .await
                .map_err(map_release_error)?;
            match outcome {
                QuotaMutationOutcome::Applied
                | QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded) => Ok(()),
                QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::Reserved
                    | QuotaReservationStatus::SettlementPending
                    | QuotaReservationStatus::Settled,
                ) => Err(TaskBillingReleaseSinkError::Invariant),
            }
        })
    }
}

/// 任务释放消费者配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskBillingReleaseConsumerError {
    /// 重试退避必须大于零，避免数据库异常时忙等。
    #[error("任务额度释放消费者重试退避必须大于零")]
    ZeroRetryDelay,
}

/// 消费任务释放队列并调用批量任务专用持久化接口。
#[derive(Clone)]
pub struct TaskBillingReleaseConsumer {
    receiver: TaskBillingReleaseReceiver,
    sink: Arc<dyn TaskBillingReleaseSink>,
    retry_delay: Duration,
}

impl TaskBillingReleaseConsumer {
    /// 创建消费者；可由多个 worker 共享同一个队列和 sink。
    pub fn new(
        receiver: TaskBillingReleaseReceiver,
        sink: Arc<dyn TaskBillingReleaseSink>,
        retry_delay: Duration,
    ) -> Result<Self, TaskBillingReleaseConsumerError> {
        if retry_delay.is_zero() {
            return Err(TaskBillingReleaseConsumerError::ZeroRetryDelay);
        }
        Ok(Self {
            receiver,
            sink,
            retry_delay,
        })
    }

    /// 持续消费，直到队列关闭且已接收信号全部排空。
    pub async fn run(&self) {
        loop {
            let Some(delivery) = self.receiver.recv().await else {
                return;
            };
            let release = AssertUnwindSafe(self.sink.release(delivery.reservation_id()))
                .catch_unwind()
                .await;
            match release {
                Ok(Ok(())) => {
                    if !delivery.acknowledge() {
                        tracing::error!(
                            error_kind = "task_release_acknowledge",
                            "任务额度释放确认边界不一致，将保留信号等待重投"
                        );
                        sleep(self.retry_delay).await;
                    }
                }
                Ok(Err(error)) => {
                    tracing::error!(
                        error_kind = error.error_kind(),
                        "任务额度释放失败，将按同一预留标识重试"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
                Err(_) => {
                    tracing::error!(
                        error_kind = "task_release_sink_panic",
                        "任务额度释放 sink 异常，将按同一预留标识重试"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
            }
        }
    }
}

/// 任务释放 sink 的脱敏错误分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskBillingReleaseSinkError {
    /// 持久化结果未知，只能重放同一标识。
    #[error("任务额度释放结果未知")]
    OutcomeUnknown,
    /// 数据库当前不可用。
    #[error("任务额度释放暂不可用")]
    Unavailable,
    /// 预留状态冲突；不确认信号，等待人工或后续修复。
    #[error("任务额度释放状态冲突")]
    Conflict,
    /// 持久化不变量损坏；不确认信号。
    #[error("任务额度释放状态损坏")]
    Invariant,
}

impl TaskBillingReleaseSinkError {
    /// 返回固定日志分类，不携带数据库诊断。
    pub(crate) const fn error_kind(self) -> &'static str {
        match self {
            Self::OutcomeUnknown => "task_release_outcome_unknown",
            Self::Unavailable => "task_release_unavailable",
            Self::Conflict => "task_release_conflict",
            Self::Invariant => "task_release_invariant",
        }
    }
}

#[allow(unreachable_patterns)]
const fn map_release_error(error: QuotaRepositoryError) -> TaskBillingReleaseSinkError {
    match error {
        QuotaRepositoryError::OutcomeUnknown => TaskBillingReleaseSinkError::OutcomeUnknown,
        QuotaRepositoryError::Query => TaskBillingReleaseSinkError::Unavailable,
        QuotaRepositoryError::NotFound | QuotaRepositoryError::Conflict => {
            TaskBillingReleaseSinkError::Conflict
        }
        QuotaRepositoryError::InvalidConfiguration
        | QuotaRepositoryError::ZeroAmount
        | QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::OrganizationQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient
        | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::ActualExceedsReservation
        | QuotaRepositoryError::Invariant => TaskBillingReleaseSinkError::Invariant,
        _ => TaskBillingReleaseSinkError::Invariant,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tokio::sync::Notify;

    use super::*;
    use crate::task_billing::{
        TaskBillingReleaseQueue, TaskBillingReleaseSignalOutcome, TaskBillingReleaseSignalPort,
    };

    struct ScriptedSink {
        outcomes: Mutex<VecDeque<Result<(), TaskBillingReleaseSinkError>>>,
        calls: AtomicUsize,
        called: Notify,
    }

    impl ScriptedSink {
        fn new(
            outcomes: impl IntoIterator<Item = Result<(), TaskBillingReleaseSinkError>>,
        ) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into_iter().collect()),
                calls: AtomicUsize::new(0),
                called: Notify::new(),
            }
        }

        async fn wait_for_calls(&self, expected: usize) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while self.calls.load(Ordering::Acquire) < expected {
                    self.called.notified().await;
                }
            })
            .await
            .unwrap();
        }
    }

    impl TaskBillingReleaseSink for ScriptedSink {
        fn release<'a>(
            &'a self,
            _reservation_id: BillingReservationId,
        ) -> TaskBillingReleaseSinkFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::AcqRel);
                self.called.notify_waiters();
                self.outcomes.lock().unwrap().pop_front().unwrap_or(Ok(()))
            })
        }
    }

    #[tokio::test]
    async fn retryable_and_invariant_failures_keep_the_same_signal() {
        let (queue, receiver) = TaskBillingReleaseQueue::bounded(1).unwrap();
        let sink = Arc::new(ScriptedSink::new([
            Err(TaskBillingReleaseSinkError::OutcomeUnknown),
            Err(TaskBillingReleaseSinkError::Invariant),
            Ok(()),
        ]));
        let consumer =
            TaskBillingReleaseConsumer::new(receiver, sink.clone(), Duration::from_millis(1))
                .unwrap();
        let id = BillingReservationId::new([1; 16]).unwrap();
        assert_eq!(
            queue.try_signal_release(id),
            TaskBillingReleaseSignalOutcome::Accepted
        );
        let task = tokio::spawn(async move { consumer.run().await });
        sink.wait_for_calls(3).await;
        queue.close();
        task.await.unwrap();
        assert_eq!(sink.calls.load(Ordering::Acquire), 3);
        assert!(queue.is_drained());
    }

    #[test]
    fn zero_retry_delay_is_rejected() {
        let (_queue, receiver) = TaskBillingReleaseQueue::bounded(1).unwrap();
        assert!(matches!(
            TaskBillingReleaseConsumer::new(
                receiver,
                Arc::new(ScriptedSink::new([])),
                Duration::ZERO,
            ),
            Err(TaskBillingReleaseConsumerError::ZeroRetryDelay)
        ));
    }

    #[test]
    fn repository_errors_never_expose_raw_diagnostics() {
        for error in [
            TaskBillingReleaseSinkError::OutcomeUnknown,
            TaskBillingReleaseSinkError::Unavailable,
            TaskBillingReleaseSinkError::Conflict,
            TaskBillingReleaseSinkError::Invariant,
        ] {
            let text = format!("{error:?} {error}");
            assert!(!text.contains("sql"));
            assert!(error.error_kind().starts_with("task_release_"));
        }
    }
}
