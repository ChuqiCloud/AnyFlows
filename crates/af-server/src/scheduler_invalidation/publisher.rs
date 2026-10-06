use std::{future::Future, time::Duration};

use af_cache::{RedisBroadcastPublisher, RedisVersionedProjectionStore};
use af_db::{
    DatabaseTimestamp, SchedulerOutboxClaimOutcome, SchedulerOutboxCompletionOutcome,
    SchedulerOutboxRepository, SchedulerRuntimeProjection, SchedulerRuntimeRepository,
};
use af_scheduler::InMemoryChannelIndex;
use tokio::time::sleep;

use super::{
    SchedulerInvalidationRuntimeError,
    projection::{load_database_projection, scheduler_projection_key},
    wire::encode_scheduler_invalidation,
};

/// 单实例模式直接应用主体投影；Redis 模式先原子写投影，再由广播驱动各实例应用。
#[derive(Clone)]
pub(super) enum SchedulerInvalidationDelivery {
    Local(InMemoryChannelIndex),
    Redis {
        publisher: RedisBroadcastPublisher,
        projection_store: RedisVersionedProjectionStore,
    },
}

impl SchedulerInvalidationDelivery {
    async fn deliver(
        &self,
        projection: SchedulerRuntimeProjection,
    ) -> Result<Option<u64>, SchedulerInvalidationRuntimeError> {
        match self {
            Self::Local(index) => {
                index.apply_projections(vec![projection]).await?;
                Ok(None)
            }
            Self::Redis {
                publisher,
                projection_store,
            } => {
                let version = projection.version();
                let subject = projection.subject();
                let projection_payload = projection.encode()?;
                projection_store
                    .put_if_newer(
                        &scheduler_projection_key(subject),
                        version,
                        &projection_payload,
                    )
                    .await?;
                // 只有投影原子可读后才广播轻量信号，订阅端永远不接收正文。
                let payload = encode_scheduler_invalidation(version, subject)?;
                publisher
                    .publish(&payload)
                    .await
                    .map(Some)
                    .map_err(Into::into)
            }
        }
    }
}

/// 单轮 outbox 发布结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchedulerOutboxPublishOutcome {
    Published { subscriber_count: Option<u64> },
    Stale,
    Empty,
}

/// 领取并可靠发布调度失效事件的长期任务。
#[derive(Clone)]
pub(super) struct SchedulerOutboxPublisher {
    repository: SchedulerOutboxRepository,
    runtime_repository: SchedulerRuntimeRepository,
    delivery: SchedulerInvalidationDelivery,
    idle_poll_interval: Duration,
}

impl SchedulerOutboxPublisher {
    pub(super) const fn new(
        repository: SchedulerOutboxRepository,
        runtime_repository: SchedulerRuntimeRepository,
        delivery: SchedulerInvalidationDelivery,
        idle_poll_interval: Duration,
    ) -> Self {
        Self {
            repository,
            runtime_repository,
            delivery,
            idle_poll_interval,
        }
    }

    /// 持续清空到期 outbox；空队列或失败时有界休眠，积压成功时立即处理下一条。
    pub(super) async fn run_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            let result = tokio::select! {
                () = &mut shutdown => return,
                result = self.run_once() => result,
            };
            let should_wait = match result {
                Ok(SchedulerOutboxPublishOutcome::Published { subscriber_count }) => {
                    if let Some(subscriber_count) = subscriber_count {
                        tracing::debug!(subscriber_count, "调度快照失效事件广播成功");
                    } else {
                        tracing::debug!("调度快照失效事件已应用到本地索引");
                    }
                    false
                }
                Ok(SchedulerOutboxPublishOutcome::Stale) => false,
                Ok(SchedulerOutboxPublishOutcome::Empty) => true,
                Err(error) => {
                    tracing::warn!(
                        error_kind = error.error_kind(),
                        "调度快照失效事件发布失败，已保留重试边界"
                    );
                    true
                }
            };
            if should_wait {
                tokio::select! {
                    () = &mut shutdown => return,
                    () = sleep(self.idle_poll_interval) => {}
                }
            }
        }
    }

    async fn run_once(
        &self,
    ) -> Result<SchedulerOutboxPublishOutcome, SchedulerInvalidationRuntimeError> {
        let now = DatabaseTimestamp::now_utc();
        let SchedulerOutboxClaimOutcome::Claimed(lease) = self.repository.claim_next(now).await?
        else {
            return Ok(SchedulerOutboxPublishOutcome::Empty);
        };
        let delivery = async {
            let projection = load_database_projection(
                &self.runtime_repository,
                lease.event_id(),
                lease.subject(),
            )
            .await?;
            self.delivery.deliver(projection).await
        }
        .await;
        let subscriber_count = match delivery {
            Ok(subscriber_count) => subscriber_count,
            Err(delivery_error) => {
                return match self
                    .repository
                    .record_failure(&lease, DatabaseTimestamp::now_utc())
                    .await?
                {
                    SchedulerOutboxCompletionOutcome::Completed => Err(delivery_error),
                    SchedulerOutboxCompletionOutcome::Stale => {
                        Ok(SchedulerOutboxPublishOutcome::Stale)
                    }
                };
            }
        };
        match self
            .repository
            .mark_published(&lease, DatabaseTimestamp::now_utc())
            .await?
        {
            SchedulerOutboxCompletionOutcome::Completed => {
                Ok(SchedulerOutboxPublishOutcome::Published { subscriber_count })
            }
            SchedulerOutboxCompletionOutcome::Stale => Ok(SchedulerOutboxPublishOutcome::Stale),
        }
    }
}
