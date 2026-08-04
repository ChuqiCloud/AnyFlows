use std::{future::Future, pin::Pin, time::Duration};

use af_cache::{CacheError, RedisBroadcastConfig, RedisBroadcastSubscriber};
use af_scheduler::InMemoryChannelIndex;
use tokio::time::sleep;

use super::{
    SchedulerInvalidationRuntimeError,
    wire::{SchedulerInvalidationWireError, decode_scheduler_invalidation},
};

type InvalidationReceiveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, CacheError>> + Send + 'a>>;

trait InvalidationSubscription: Send {
    fn recv(&mut self) -> InvalidationReceiveFuture<'_>;
}

impl InvalidationSubscription for RedisBroadcastSubscriber {
    fn recv(&mut self) -> InvalidationReceiveFuture<'_> {
        Box::pin(RedisBroadcastSubscriber::recv(self))
    }
}

/// Redis 失效订阅器；每次建连后先全量校正，再把突发消息合并为一次刷新。
pub(super) struct SchedulerInvalidationSubscriber {
    config: RedisBroadcastConfig,
    channel_index: InMemoryChannelIndex,
    debounce: Duration,
}

impl SchedulerInvalidationSubscriber {
    pub(super) const fn new(
        config: RedisBroadcastConfig,
        channel_index: InMemoryChannelIndex,
        debounce: Duration,
    ) -> Self {
        Self {
            config,
            channel_index,
            debounce,
        }
    }

    /// 建立订阅并运行到关闭；连接中断会返回错误，由外层监督器重新订阅和校正。
    pub(super) async fn run_until<F>(
        &self,
        shutdown: F,
    ) -> Result<(), SchedulerInvalidationRuntimeError>
    where
        F: Future<Output = ()> + Send,
    {
        let subscription = RedisBroadcastSubscriber::connect(self.config.clone()).await?;
        self.run_subscription_until(subscription, shutdown).await
    }

    async fn run_subscription_until<S, F>(
        &self,
        mut subscription: S,
        shutdown: F,
    ) -> Result<(), SchedulerInvalidationRuntimeError>
    where
        S: InvalidationSubscription,
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        // 先订阅再读取数据库真相，封住首轮快照加载与订阅确认之间的漏信号窗口。
        let initial = tokio::select! {
            () = &mut shutdown => return Ok(()),
            result = self.channel_index.refresh() => result?,
        };
        tracing::debug!(
            generation = initial.generation(),
            key_count = initial.key_count(),
            candidate_count = initial.candidate_count(),
            "调度快照失效订阅完成启动校正"
        );

        loop {
            let payload = tokio::select! {
                () = &mut shutdown => return Ok(()),
                result = subscription.recv() => result?,
            };
            if !accept_payload(&payload) {
                continue;
            }

            let deadline = sleep(self.debounce);
            tokio::pin!(deadline);
            let mut deferred_error = None;
            loop {
                tokio::select! {
                    () = &mut shutdown => return Ok(()),
                    () = &mut deadline => break,
                    result = subscription.recv() => {
                        match result {
                            Ok(payload) => {
                                let _ = accept_payload(&payload);
                            }
                            Err(error) => {
                                // 已收到的有效信号必须先触发校正，再让监督器重建断开的订阅。
                                deferred_error = Some(error);
                                break;
                            }
                        }
                    }
                }
            }

            let refreshed = tokio::select! {
                () = &mut shutdown => return Ok(()),
                result = self.channel_index.refresh() => result?,
            };
            tracing::debug!(
                generation = refreshed.generation(),
                key_count = refreshed.key_count(),
                candidate_count = refreshed.candidate_count(),
                "调度快照按 Redis 失效广播完成合并刷新"
            );
            if let Some(error) = deferred_error {
                return Err(error.into());
            }
        }
    }
}

fn accept_payload(payload: &[u8]) -> bool {
    match decode_scheduler_invalidation(payload) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                error_kind = wire_error_kind(error),
                "忽略无效的调度快照失效消息"
            );
            false
        }
    }
}

const fn wire_error_kind(error: SchedulerInvalidationWireError) -> &'static str {
    match error {
        SchedulerInvalidationWireError::Encode => "scheduler_invalidation_encode",
        SchedulerInvalidationWireError::Decode => "scheduler_invalidation_decode",
        SchedulerInvalidationWireError::Invariant => "scheduler_invalidation_invariant",
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Arc};

    use af_domain::{ChannelId, GroupId};
    use af_scheduler::{ChannelIndexSource, ChannelIndexSourceFuture, ChannelIndexSourceRecord};

    use super::*;
    use crate::scheduler_invalidation::wire::encode_scheduler_invalidation;

    #[tokio::test]
    async fn startup_resync_and_message_burst_each_publish_one_generation() {
        let index = empty_index().await;
        let subscription = FakeSubscription::new([
            encode_scheduler_invalidation(
                1,
                af_db::SchedulerCatalogSubject::Channel(ChannelId::new(7).unwrap()),
            )
            .unwrap(),
            b"invalid-private-payload".to_vec(),
            encode_scheduler_invalidation(
                2,
                af_db::SchedulerCatalogSubject::Group(GroupId::new(8).unwrap()),
            )
            .unwrap(),
        ]);
        let runner = SchedulerInvalidationSubscriber::new(
            RedisBroadcastConfig::new(
                af_cache::RedisConfig::new("redis://127.0.0.1:6379/"),
                "scheduler.test.v1",
            )
            .unwrap(),
            index.clone(),
            Duration::from_millis(1),
        );

        runner
            .run_subscription_until(subscription, std::future::pending())
            .await
            .expect_err("断开的测试订阅必须交由监督器重建");
        assert_eq!(index.snapshot().unwrap().generation(), 3);
    }

    #[tokio::test]
    async fn invalid_messages_do_not_trigger_an_extra_refresh() {
        let index = empty_index().await;
        let subscription = FakeSubscription::new([b"private-invalid-message".to_vec()]);
        let runner = SchedulerInvalidationSubscriber::new(
            RedisBroadcastConfig::new(
                af_cache::RedisConfig::new("redis://127.0.0.1:6379/"),
                "scheduler.test.v1",
            )
            .unwrap(),
            index.clone(),
            Duration::from_millis(1),
        );

        runner
            .run_subscription_until(subscription, std::future::pending())
            .await
            .expect_err("断开的测试订阅必须交由监督器重建");
        assert_eq!(index.snapshot().unwrap().generation(), 2);
    }

    async fn empty_index() -> InMemoryChannelIndex {
        InMemoryChannelIndex::load(Arc::new(EmptySource))
            .await
            .unwrap()
    }

    struct EmptySource;

    impl ChannelIndexSource for EmptySource {
        fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
            Box::pin(async { Ok(Vec::<ChannelIndexSourceRecord>::new()) })
        }
    }

    struct FakeSubscription {
        payloads: VecDeque<Vec<u8>>,
    }

    impl FakeSubscription {
        fn new(payloads: impl IntoIterator<Item = Vec<u8>>) -> Self {
            Self {
                payloads: payloads.into_iter().collect(),
            }
        }
    }

    impl InvalidationSubscription for FakeSubscription {
        fn recv(&mut self) -> InvalidationReceiveFuture<'_> {
            let payload = self.payloads.pop_front();
            Box::pin(async move { payload.ok_or(CacheError::BroadcastClosed) })
        }
    }
}
