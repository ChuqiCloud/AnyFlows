use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use af_cache::{
    CacheError, RedisBroadcastConfig, RedisBroadcastSubscriber, RedisVersionedProjectionStore,
};
use af_db::{SchedulerCatalogSubject, SchedulerRuntimeProjection};
use af_scheduler::InMemoryChannelIndex;
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use tokio::time::sleep;

use super::{
    SchedulerInvalidationRuntimeError,
    projection::{decode_stored_projection, scheduler_projection_key},
    wire::{
        SchedulerInvalidationSignal, SchedulerInvalidationWireError, decode_scheduler_invalidation,
    },
};

const PROJECTION_READ_CONCURRENCY: usize = 16;
const MAX_INVALIDATION_SUBJECTS_PER_BATCH: usize = 4_096;

type InvalidationReceiveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, CacheError>> + Send + 'a>>;
pub(super) type ProjectionReadFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Option<StoredProjection>, SchedulerInvalidationRuntimeError>>
            + Send
            + 'a,
    >,
>;

trait InvalidationSubscription: Send {
    fn recv(&mut self) -> InvalidationReceiveFuture<'_>;
}

impl InvalidationSubscription for RedisBroadcastSubscriber {
    fn recv(&mut self) -> InvalidationReceiveFuture<'_> {
        Box::pin(RedisBroadcastSubscriber::recv(self))
    }
}

#[derive(Clone)]
pub(super) struct StoredProjection {
    version: u64,
    payload: Vec<u8>,
}

pub(super) trait SchedulerProjectionReader: Send + Sync {
    fn read<'a>(&'a self, subject_key: &'a str) -> ProjectionReadFuture<'a>;
}

impl SchedulerProjectionReader for RedisVersionedProjectionStore {
    fn read<'a>(&'a self, subject_key: &'a str) -> ProjectionReadFuture<'a> {
        Box::pin(async move {
            Ok(self.get(subject_key).await?.map(|entry| StoredProjection {
                version: entry.version(),
                payload: entry.payload().to_vec(),
            }))
        })
    }
}

/// Redis 失效订阅器；每次建连后先全量校正，再批量应用主体级物化投影。
pub(super) struct SchedulerInvalidationSubscriber {
    config: RedisBroadcastConfig,
    projection_reader: Arc<dyn SchedulerProjectionReader>,
    channel_index: InMemoryChannelIndex,
    debounce: Duration,
}

impl SchedulerInvalidationSubscriber {
    pub(super) fn new(
        config: RedisBroadcastConfig,
        projection_reader: impl SchedulerProjectionReader + 'static,
        channel_index: InMemoryChannelIndex,
        debounce: Duration,
    ) -> Self {
        Self {
            config,
            projection_reader: Arc::new(projection_reader),
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
            catalog_version = initial.catalog_version(),
            key_count = initial.key_count(),
            candidate_count = initial.candidate_count(),
            "调度快照失效订阅完成启动校正"
        );

        loop {
            let payload = tokio::select! {
                () = &mut shutdown => return Ok(()),
                result = subscription.recv() => result?,
            };
            let Some(signal) = accept_payload(&payload) else {
                continue;
            };
            let mut signals = BTreeMap::new();
            insert_signal(&mut signals, signal)?;

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
                                if let Some(signal) = accept_payload(&payload) {
                                    insert_signal(&mut signals, signal)?;
                                }
                            }
                            Err(error) => {
                                // 已收到的有效信号必须先应用，再让监督器重建断开的订阅。
                                deferred_error = Some(error);
                                break;
                            }
                        }
                    }
                }
            }

            let projections = tokio::select! {
                () = &mut shutdown => return Ok(()),
                result = self.load_projections(signals) => result?,
            };
            let report = tokio::select! {
                () = &mut shutdown => return Ok(()),
                result = self.channel_index.apply_projections(projections) => result?,
            };
            tracing::debug!(
                generation = report.generation(),
                applied_subject_count = report.applied_subject_count(),
                key_count = report.key_count(),
                candidate_count = report.candidate_count(),
                "调度快照按 Redis 物化投影完成主体级增量应用"
            );
            if let Some(error) = deferred_error {
                return Err(error.into());
            }
        }
    }

    async fn load_projections(
        &self,
        signals: BTreeMap<SchedulerCatalogSubject, u64>,
    ) -> Result<Vec<SchedulerRuntimeProjection>, SchedulerInvalidationRuntimeError> {
        stream::iter(signals.into_iter().map(|(subject, signaled_version)| {
            let reader = Arc::clone(&self.projection_reader);
            async move {
                let key = scheduler_projection_key(subject);
                let stored = reader
                    .read(&key)
                    .await?
                    .ok_or(SchedulerInvalidationRuntimeError::ProjectionState)?;
                decode_stored_projection(stored.version, signaled_version, subject, &stored.payload)
            }
        }))
        .buffer_unordered(PROJECTION_READ_CONCURRENCY)
        .try_collect()
        .await
    }
}

fn insert_signal(
    signals: &mut BTreeMap<SchedulerCatalogSubject, u64>,
    signal: SchedulerInvalidationSignal,
) -> Result<(), SchedulerInvalidationRuntimeError> {
    if !signals.contains_key(&signal.subject())
        && signals.len() >= MAX_INVALIDATION_SUBJECTS_PER_BATCH
    {
        return Err(SchedulerInvalidationRuntimeError::ProjectionState);
    }
    signals
        .entry(signal.subject())
        .and_modify(|version| *version = (*version).max(signal.event_id()))
        .or_insert(signal.event_id());
    Ok(())
}

fn accept_payload(payload: &[u8]) -> Option<SchedulerInvalidationSignal> {
    match decode_scheduler_invalidation(payload) {
        Ok(signal) => Some(signal),
        Err(error) => {
            tracing::warn!(
                error_kind = wire_error_kind(error),
                "忽略无效的调度快照失效消息"
            );
            None
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
    async fn startup_resync_and_message_burst_publish_one_incremental_generation() {
        let index = empty_index().await;
        let channel_subject = SchedulerCatalogSubject::Channel(ChannelId::new(7).unwrap());
        let group_subject = SchedulerCatalogSubject::Group(GroupId::new(8).unwrap());
        let subscription = FakeSubscription::new([
            encode_scheduler_invalidation(1, channel_subject).unwrap(),
            b"invalid-private-payload".to_vec(),
            encode_scheduler_invalidation(2, group_subject).unwrap(),
        ]);
        let runner = SchedulerInvalidationSubscriber::new(
            config(),
            FakeProjectionReader::new([
                (channel_subject, projection_entry(1, channel_subject)),
                (group_subject, projection_entry(2, group_subject)),
            ]),
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
    async fn invalid_messages_do_not_read_projection_or_trigger_incremental_apply() {
        let index = empty_index().await;
        let subscription = FakeSubscription::new([b"private-invalid-message".to_vec()]);
        let runner = SchedulerInvalidationSubscriber::new(
            config(),
            FakeProjectionReader::new([]),
            index.clone(),
            Duration::from_millis(1),
        );

        runner
            .run_subscription_until(subscription, std::future::pending())
            .await
            .expect_err("断开的测试订阅必须交由监督器重建");
        assert_eq!(index.snapshot().unwrap().generation(), 2);
    }

    #[tokio::test]
    async fn projection_older_than_signal_fails_closed_after_startup_resync() {
        let index = empty_index().await;
        let subject = SchedulerCatalogSubject::Channel(ChannelId::new(7).unwrap());
        let runner = SchedulerInvalidationSubscriber::new(
            config(),
            FakeProjectionReader::new([(subject, projection_entry(1, subject))]),
            index.clone(),
            Duration::from_millis(1),
        );
        let error = runner
            .run_subscription_until(
                FakeSubscription::new([encode_scheduler_invalidation(2, subject).unwrap()]),
                std::future::pending(),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            SchedulerInvalidationRuntimeError::ProjectionState
        ));
        assert_eq!(index.snapshot().unwrap().generation(), 2);
    }

    fn config() -> RedisBroadcastConfig {
        RedisBroadcastConfig::new(
            af_cache::RedisConfig::new("redis://127.0.0.1:6379/"),
            "scheduler.test.v1",
        )
        .unwrap()
    }

    fn projection_entry(version: u64, subject: SchedulerCatalogSubject) -> StoredProjection {
        let projection = SchedulerRuntimeProjection::new(version, subject, Vec::new()).unwrap();
        StoredProjection {
            version,
            payload: projection.encode().unwrap(),
        }
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

    struct FakeProjectionReader {
        entries: BTreeMap<String, StoredProjection>,
    }

    impl FakeProjectionReader {
        fn new(
            entries: impl IntoIterator<Item = (SchedulerCatalogSubject, StoredProjection)>,
        ) -> Self {
            Self {
                entries: entries
                    .into_iter()
                    .map(|(subject, entry)| (scheduler_projection_key(subject), entry))
                    .collect(),
            }
        }
    }

    impl SchedulerProjectionReader for FakeProjectionReader {
        fn read<'a>(&'a self, subject_key: &'a str) -> ProjectionReadFuture<'a> {
            let entry = self.entries.get(subject_key).cloned();
            Box::pin(async move { Ok(entry) })
        }
    }
}
