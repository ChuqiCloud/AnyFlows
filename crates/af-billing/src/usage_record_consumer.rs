use std::{future::Future, panic::AssertUnwindSafe, pin::Pin, sync::Arc, time::Duration};

use futures_util::FutureExt as _;
use thiserror::Error;
use tokio::time::sleep;

use crate::UsageRecordReceiver;

/// 用量记录持久化 sink 的对象安全异步调用。
pub type UsageRecordSinkFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UsageRecordSinkError>> + Send + 'a>>;

/// 只在可靠下游确认后释放队列投递的持久化端口。
pub trait UsageRecordSink: Send + Sync + 'static {
    /// 持久化一条用量事实；相同幂等键的 Existing 也必须返回成功。
    fn persist<'a>(&'a self, record: crate::BillingUsageRecord) -> UsageRecordSinkFuture<'a>;
}

/// 用量记录消费者配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsageRecordConsumerError {
    /// 重试退避必须大于零，避免持久化失败时忙等数据库。
    #[error("用量记录消费者重试退避必须大于零")]
    ZeroRetryDelay,
}

/// 持久化失败后自动重投并退避的用量记录消费者。
#[derive(Clone)]
pub struct UsageRecordConsumer {
    receiver: UsageRecordReceiver,
    sink: Arc<dyn UsageRecordSink>,
    retry_delay: Duration,
}

impl UsageRecordConsumer {
    /// 创建一个消费者；多个消费者可以安全共享同一队列和 sink。
    pub fn new(
        receiver: UsageRecordReceiver,
        sink: Arc<dyn UsageRecordSink>,
        retry_delay: Duration,
    ) -> Result<Self, UsageRecordConsumerError> {
        if retry_delay.is_zero() {
            return Err(UsageRecordConsumerError::ZeroRetryDelay);
        }
        Ok(Self {
            receiver,
            sink,
            retry_delay,
        })
    }

    /// 持续消费直到队列入口关闭且所有已接收投递均完成。
    ///
    /// sink 失败或 panic 时不返回错误，也不确认当前 delivery；退避后继续重试同一事实。
    /// supervisor 仍隔离消费者自身 panic，队列 delivery 的 Drop 负责把未确认事实重新入队。
    pub async fn run(&self) {
        loop {
            let Some(delivery) = self.receiver.recv().await else {
                return;
            };
            let persistence = AssertUnwindSafe(self.sink.persist(delivery.record()))
                .catch_unwind()
                .await;
            match persistence {
                Ok(Ok(())) => {
                    if !delivery.acknowledge() {
                        tracing::error!(
                            error_kind = "usage_record_acknowledge",
                            "用量记录确认边界不一致，将保留事实等待重投"
                        );
                        sleep(self.retry_delay).await;
                    }
                }
                Ok(Err(error)) => {
                    tracing::error!(
                        error_kind = error.error_kind(),
                        "用量记录持久化失败，将按同一幂等键重试"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
                Err(_) => {
                    tracing::error!(
                        error_kind = "usage_record_sink_panic",
                        "用量记录持久化 sink 异常，将按同一幂等键重试"
                    );
                    drop(delivery);
                    sleep(self.retry_delay).await;
                }
            }
        }
    }
}

/// 持久化 sink 的脱敏错误分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsageRecordSinkError {
    /// 持久化结果未知，只能重放同一事实。
    #[error("用量记录持久化结果未知")]
    OutcomeUnknown,
    /// 数据库当前不可用。
    #[error("用量记录持久化暂不可用")]
    Unavailable,
    /// 同一幂等键绑定了不同事实。
    #[error("用量记录持久化幂等冲突")]
    Conflict,
    /// 持久化数据违反不变量，继续重试前必须保留原事实等待运维处理。
    #[error("用量记录持久化不变量损坏")]
    Invariant,
}

impl UsageRecordSinkError {
    /// 返回固定日志分类，不携带底层数据库细节。
    pub(crate) const fn error_kind(self) -> &'static str {
        match self {
            Self::OutcomeUnknown => "usage_record_outcome_unknown",
            Self::Unavailable => "usage_record_unavailable",
            Self::Conflict => "usage_record_conflict",
            Self::Invariant => "usage_record_invariant",
        }
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

    use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
    use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
    use tokio::sync::Notify;

    use super::*;
    use crate::{
        BillingMode, BillingUsageRecord, UsageRecordOutcome, UsageRecordPort, UsageRecordQueue,
    };

    struct ScriptedSink {
        outcomes: Mutex<VecDeque<Result<(), UsageRecordSinkError>>>,
        calls: AtomicUsize,
        called: Notify,
    }

    impl ScriptedSink {
        fn new(outcomes: impl IntoIterator<Item = Result<(), UsageRecordSinkError>>) -> Self {
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

    impl UsageRecordSink for ScriptedSink {
        fn persist<'a>(&'a self, _record: BillingUsageRecord) -> UsageRecordSinkFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::AcqRel);
                self.called.notify_waiters();
                self.outcomes.lock().unwrap().pop_front().unwrap_or(Ok(()))
            })
        }
    }

    #[tokio::test]
    async fn failed_persistence_is_retried_before_queue_entry_is_released() {
        let (queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        let sink = Arc::new(ScriptedSink::new([
            Err(UsageRecordSinkError::OutcomeUnknown),
            Ok(()),
        ]));
        let consumer =
            UsageRecordConsumer::new(receiver, sink.clone(), Duration::from_millis(1)).unwrap();
        assert_eq!(queue.try_record(record(1)), UsageRecordOutcome::Accepted);
        assert_eq!(queue.try_record(record(2)), UsageRecordOutcome::Saturated);
        let task = tokio::spawn(async move { consumer.run().await });

        sink.wait_for_calls(2).await;
        assert_eq!(queue.try_record(record(2)), UsageRecordOutcome::Accepted);
        queue.close();
        task.await.unwrap();
        assert_eq!(sink.calls.load(Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn sink_panic_requeues_delivery_for_the_same_consumer_retry() {
        let (queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        assert_eq!(queue.try_record(record(3)), UsageRecordOutcome::Accepted);
        let sink = Arc::new(ScriptedSink::new([Ok(())]));
        let consumer = UsageRecordConsumer::new(
            receiver,
            Arc::new(PanicThenSuccessSink {
                panicking: AtomicUsize::new(1),
                success: sink.clone(),
            }),
            Duration::from_millis(1),
        )
        .unwrap();
        queue.close();
        consumer.run().await;
        assert_eq!(sink.calls.load(Ordering::Acquire), 1);
    }

    struct PanicThenSuccessSink {
        panicking: AtomicUsize,
        success: Arc<ScriptedSink>,
    }

    impl UsageRecordSink for PanicThenSuccessSink {
        fn persist<'a>(&'a self, record: BillingUsageRecord) -> UsageRecordSinkFuture<'a> {
            if self.panicking.swap(0, Ordering::AcqRel) > 0 {
                return Box::pin(async { panic!("受控测试 sink panic") });
            }
            self.success.persist(record)
        }
    }

    fn record(marker: u8) -> BillingUsageRecord {
        BillingUsageRecord::new(
            BillingReservationId::new([marker; 16]).unwrap(),
            GatewayPrincipal::new(
                TokenId::new(1).unwrap(),
                UserId::new(2).unwrap(),
                GroupId::new(3).unwrap(),
            ),
            Usage::new(
                TokenCount::new(1).unwrap(),
                TokenCount::new(1).unwrap(),
                UsageDetails::new(
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                    TokenCount::ZERO,
                ),
                UsageSource::Estimated,
                UsageSemantics::Inclusive,
            )
            .unwrap(),
            BillingMode::PerToken,
            Quota::new(1).unwrap(),
        )
    }
}
