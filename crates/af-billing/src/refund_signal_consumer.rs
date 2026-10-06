use std::{future::Future, panic::AssertUnwindSafe, pin::Pin, sync::Arc, time::Duration};

use af_db::{QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus};
use af_domain::BillingReservationId;
use futures_util::FutureExt as _;
use thiserror::Error;
use tokio::time::sleep;

use crate::RefundSignalReceiver;

/// 退款信号持久化 sink 的对象安全异步调用。
pub type RefundSignalSinkFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), RefundSignalSinkError>> + Send + 'a>>;

/// 只处理内部预扣补偿的持久化端口。
pub trait RefundSignalSink: Send + Sync + 'static {
    /// 对同一预留标识执行或重放全额退款补偿。
    fn refund<'a>(&'a self, reservation_id: BillingReservationId) -> RefundSignalSinkFuture<'a>;
}

impl RefundSignalSink for QuotaRepository {
    fn refund<'a>(&'a self, reservation_id: BillingReservationId) -> RefundSignalSinkFuture<'a> {
        Box::pin(async move {
            let outcome = QuotaRepository::refund(self, reservation_id)
                .await
                .map_err(map_quota_repository_refund_error)?;
            match outcome {
                QuotaMutationOutcome::Applied
                | QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded) => Ok(()),
                QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::Reserved
                    | QuotaReservationStatus::SettlementPending
                    | QuotaReservationStatus::Settled,
                ) => Err(RefundSignalSinkError::Invariant),
            }
        })
    }
}

/// 退款信号消费者配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundSignalConsumerError {
    /// 重试退避必须大于零，避免持久化失败时忙等数据库。
    #[error("退款信号消费者重试退避必须大于零")]
    ZeroRetryDelay,
}

/// 从内部队列消费 RAII 补偿信号并调用持久化退款仓储。
#[derive(Clone)]
pub struct RefundSignalConsumer {
    receiver: RefundSignalReceiver,
    sink: Arc<dyn RefundSignalSink>,
    retry_delay: Duration,
}

impl RefundSignalConsumer {
    /// 创建一个消费者；多个消费者可以安全共享同一队列和 sink。
    pub fn new(
        receiver: RefundSignalReceiver,
        sink: Arc<dyn RefundSignalSink>,
        retry_delay: Duration,
    ) -> Result<Self, RefundSignalConsumerError> {
        if retry_delay.is_zero() {
            return Err(RefundSignalConsumerError::ZeroRetryDelay);
        }
        Ok(Self {
            receiver,
            sink,
            retry_delay,
        })
    }

    /// 持续消费直到队列入口关闭且所有已接收信号均完成。
    ///
    /// 结果未知、暂时不可用或 sink panic 时保留当前信号重试；不可恢复的不变量错误不会
    /// 被确认，避免把可能仍处于预扣状态的额度补偿责任静默丢弃。
    pub async fn run(&self) {
        loop {
            let Some(delivery) = self.receiver.recv().await else {
                return;
            };
            let refund = AssertUnwindSafe(self.sink.refund(delivery.reservation_id()))
                .catch_unwind()
                .await;
            match refund {
                Ok(Ok(())) => {
                    if !delivery.acknowledge() {
                        tracing::error!(
                            error_kind = "refund_signal_acknowledge",
                            "退款信号确认边界不一致，将保留信号等待重投"
                        );
                        sleep(self.retry_delay).await;
                    }
                }
                Ok(Err(error)) if error.is_retryable() => {
                    tracing::error!(
                        error_kind = error.error_kind(),
                        "内部退款补偿失败，将按同一预留标识重试"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
                Ok(Err(error)) => {
                    tracing::error!(
                        error_kind = error.error_kind(),
                        "内部退款补偿遇到不可确认错误，将保留信号等待人工处理"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
                Err(_) => {
                    tracing::error!(
                        error_kind = "refund_signal_sink_panic",
                        "退款信号持久化 sink 异常，将按同一预留标识重试"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
            }
        }
    }
}

/// 退款信号 sink 的脱敏错误分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundSignalSinkError {
    /// 持久化结果未知，只能重放同一预留标识。
    #[error("退款补偿结果未知")]
    OutcomeUnknown,
    /// 数据库当前不可用。
    #[error("退款补偿暂不可用")]
    Unavailable,
    /// 预留标识不存在或已不处于可退款状态。
    #[error("退款补偿状态冲突")]
    Conflict,
    /// 持久化状态违反不变量，继续重试前必须保留原信号等待运维处理。
    #[error("退款补偿不变量损坏")]
    Invariant,
}

impl RefundSignalSinkError {
    /// 返回固定日志分类，不携带底层数据库细节。
    pub(crate) const fn error_kind(self) -> &'static str {
        match self {
            Self::OutcomeUnknown => "refund_signal_outcome_unknown",
            Self::Unavailable => "refund_signal_unavailable",
            Self::Conflict => "refund_signal_conflict",
            Self::Invariant => "refund_signal_invariant",
        }
    }

    const fn is_retryable(self) -> bool {
        matches!(self, Self::OutcomeUnknown | Self::Unavailable)
    }
}

#[allow(unreachable_patterns)]
const fn map_quota_repository_refund_error(error: QuotaRepositoryError) -> RefundSignalSinkError {
    match error {
        QuotaRepositoryError::OutcomeUnknown => RefundSignalSinkError::OutcomeUnknown,
        QuotaRepositoryError::Query => RefundSignalSinkError::Unavailable,
        QuotaRepositoryError::NotFound | QuotaRepositoryError::Conflict => {
            RefundSignalSinkError::Conflict
        }
        QuotaRepositoryError::InvalidConfiguration
        | QuotaRepositoryError::ZeroAmount
        | QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::OrganizationQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient
        | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::Invariant => RefundSignalSinkError::Invariant,
        _ => RefundSignalSinkError::Invariant,
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

    use tokio::sync::Notify;

    use super::*;
    use crate::{RefundSignalOutcome, RefundSignalPort, RefundSignalQueue};

    struct ScriptedSink {
        outcomes: Mutex<VecDeque<Result<(), RefundSignalSinkError>>>,
        calls: AtomicUsize,
        called: Notify,
    }

    impl ScriptedSink {
        fn new(outcomes: impl IntoIterator<Item = Result<(), RefundSignalSinkError>>) -> Self {
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

    impl RefundSignalSink for ScriptedSink {
        fn refund<'a>(
            &'a self,
            _reservation_id: BillingReservationId,
        ) -> RefundSignalSinkFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::AcqRel);
                self.called.notify_waiters();
                self.outcomes.lock().unwrap().pop_front().unwrap_or(Ok(()))
            })
        }
    }

    struct PanicThenSuccessSink {
        panicking: AtomicUsize,
        success: Arc<ScriptedSink>,
    }

    impl RefundSignalSink for PanicThenSuccessSink {
        fn refund<'a>(
            &'a self,
            reservation_id: BillingReservationId,
        ) -> RefundSignalSinkFuture<'a> {
            if self.panicking.swap(0, Ordering::AcqRel) > 0 {
                return Box::pin(async { panic!("受控测试退款 sink panic") });
            }
            self.success.refund(reservation_id)
        }
    }

    #[tokio::test]
    async fn retryable_failure_is_retried_before_signal_is_released() {
        let (queue, receiver) = RefundSignalQueue::bounded(1).unwrap();
        let sink = Arc::new(ScriptedSink::new([
            Err(RefundSignalSinkError::OutcomeUnknown),
            Ok(()),
        ]));
        let consumer =
            RefundSignalConsumer::new(receiver, sink.clone(), Duration::from_millis(1)).unwrap();
        assert_eq!(
            queue.try_signal_refund(reservation_id(1)),
            RefundSignalOutcome::Accepted
        );
        assert_eq!(
            queue.try_signal_refund(reservation_id(2)),
            RefundSignalOutcome::Saturated
        );
        let task = tokio::spawn(async move { consumer.run().await });

        sink.wait_for_calls(2).await;
        assert_eq!(
            queue.try_signal_refund(reservation_id(2)),
            RefundSignalOutcome::Accepted
        );
        queue.close();
        task.await.unwrap();
        assert_eq!(sink.calls.load(Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn sink_panic_requeues_the_same_signal() {
        let (queue, receiver) = RefundSignalQueue::bounded(1).unwrap();
        let success = Arc::new(ScriptedSink::new([Ok(())]));
        let consumer = RefundSignalConsumer::new(
            receiver,
            Arc::new(PanicThenSuccessSink {
                panicking: AtomicUsize::new(1),
                success: success.clone(),
            }),
            Duration::from_millis(1),
        )
        .unwrap();
        assert_eq!(
            queue.try_signal_refund(reservation_id(3)),
            RefundSignalOutcome::Accepted
        );
        queue.close();

        consumer.run().await;

        assert_eq!(success.calls.load(Ordering::Acquire), 1);
        assert!(queue.is_drained());
    }

    #[test]
    fn zero_retry_delay_is_rejected() {
        let (_queue, receiver) = RefundSignalQueue::bounded(1).unwrap();
        assert!(matches!(
            RefundSignalConsumer::new(receiver, Arc::new(ScriptedSink::new([])), Duration::ZERO,),
            Err(RefundSignalConsumerError::ZeroRetryDelay)
        ));
    }

    fn reservation_id(marker: u8) -> BillingReservationId {
        BillingReservationId::new([marker; 16]).unwrap()
    }
}
