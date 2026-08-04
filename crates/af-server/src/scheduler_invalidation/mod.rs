use std::time::Duration;

use af_cache::{
    CacheError, RedisBroadcastConfig, RedisBroadcastPublisher, RedisBroadcastSubscriber,
    RedisConfig,
};
use af_config::AppConfig;
use af_db::{
    DatabasePool, SchedulerOutboxRepository, SchedulerOutboxRepositoryConfigError,
    SchedulerOutboxRepositoryError,
};
use af_scheduler::{ChannelIndexCacheError, InMemoryChannelIndex};
use thiserror::Error;

use crate::BackgroundTaskSupervisor;

use self::{
    publisher::{SchedulerInvalidationDelivery, SchedulerOutboxPublisher},
    subscriber::SchedulerInvalidationSubscriber,
    wire::SchedulerInvalidationWireError,
};

mod publisher;
mod subscriber;
mod wire;

const SCHEDULER_INVALIDATION_CHANNEL: &str = "anyflows.scheduler.invalidate.v1";
const SCHEDULER_INVALIDATION_MAX_MESSAGE_BYTES: usize = 512;
const OUTBOX_DATABASE_TIMEOUT: Duration = Duration::from_secs(5);
const OUTBOX_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const INVALIDATION_DEBOUNCE: Duration = Duration::from_millis(50);

/// 已装配的调度 outbox 发布器与可选 Redis 订阅配置。
pub(crate) struct PreparedSchedulerInvalidationRuntime {
    publisher: SchedulerOutboxPublisher,
    subscription: Option<RedisBroadcastConfig>,
    channel_index: InMemoryChannelIndex,
}

/// 初始化或运行调度失效链路时的稳定错误分类。
#[derive(Debug, Error)]
pub enum SchedulerInvalidationRuntimeError {
    /// outbox 仓储截止时间配置无效。
    #[error("初始化调度 outbox 仓储失败")]
    RepositoryConfig(#[from] SchedulerOutboxRepositoryConfigError),
    /// outbox 领取、确认或退避失败。
    #[error("推进调度 outbox 状态失败")]
    Repository(#[from] SchedulerOutboxRepositoryError),
    /// Redis 广播连接、发布或接收失败。
    #[error("调度快照 Redis 广播失败")]
    Cache(#[from] CacheError),
    /// 本地渠道索引全量校正失败。
    #[error("调度快照本地校正失败")]
    ChannelIndex(#[from] ChannelIndexCacheError),
    /// 失效消息无法按闭合版本契约编码或解析。
    #[error("调度快照失效消息无效")]
    Wire,
}

impl From<SchedulerInvalidationWireError> for SchedulerInvalidationRuntimeError {
    fn from(_: SchedulerInvalidationWireError) -> Self {
        Self::Wire
    }
}

impl SchedulerInvalidationRuntimeError {
    pub(super) const fn error_kind(&self) -> &'static str {
        match self {
            Self::RepositoryConfig(_) => "scheduler_outbox_configuration",
            Self::Repository(_) => "scheduler_outbox_repository",
            Self::Cache(_) => "scheduler_invalidation_redis",
            Self::ChannelIndex(_) => "scheduler_invalidation_index",
            Self::Wire => "scheduler_invalidation_wire",
        }
    }
}

/// 根据可选 Redis 配置装配发布模式；显式配置 Redis 时连接失败会阻止启动。
pub(crate) async fn prepare_scheduler_invalidation_runtime(
    config: &AppConfig,
    database: DatabasePool,
    channel_index: InMemoryChannelIndex,
) -> Result<PreparedSchedulerInvalidationRuntime, SchedulerInvalidationRuntimeError> {
    let repository = SchedulerOutboxRepository::new(database, OUTBOX_DATABASE_TIMEOUT)?;
    let (delivery, subscription) = if let Some(redis_url) = config.redis().url() {
        let broadcast = RedisBroadcastConfig::new(
            RedisConfig::new(redis_url.expose().to_owned()),
            SCHEDULER_INVALIDATION_CHANNEL,
        )?
        .with_max_message_bytes(SCHEDULER_INVALIDATION_MAX_MESSAGE_BYTES)?;
        let publisher = RedisBroadcastPublisher::connect(broadcast.clone()).await?;
        // 启动期同时验证 SUBSCRIBE 权限；运行任务会重新建连并先全量校正。
        drop(RedisBroadcastSubscriber::connect(broadcast.clone()).await?);
        (
            SchedulerInvalidationDelivery::Redis(publisher),
            Some(broadcast),
        )
    } else {
        (
            SchedulerInvalidationDelivery::Local(channel_index.clone()),
            None,
        )
    };
    Ok(PreparedSchedulerInvalidationRuntime {
        publisher: SchedulerOutboxPublisher::new(repository, delivery, OUTBOX_IDLE_POLL_INTERVAL),
        subscription,
        channel_index,
    })
}

/// 注册可重建的 outbox 发布任务；Redis 模式额外注册订阅和启动校正任务。
pub(crate) fn register_scheduler_invalidation_tasks(
    supervisor: &mut BackgroundTaskSupervisor,
    runtime: PreparedSchedulerInvalidationRuntime,
) {
    let PreparedSchedulerInvalidationRuntime {
        publisher,
        subscription,
        channel_index,
    } = runtime;
    if let Some(config) = subscription {
        supervisor.spawn("scheduler-index-invalidation", move |task_shutdown| {
            let runner = SchedulerInvalidationSubscriber::new(
                config.clone(),
                channel_index.clone(),
                INVALIDATION_DEBOUNCE,
            );
            async move {
                if let Err(error) = runner.run_until(task_shutdown.cancelled()).await {
                    tracing::warn!(
                        error_kind = error.error_kind(),
                        "调度快照失效订阅已中断，等待监督器重建"
                    );
                }
            }
        });
    }
    supervisor.spawn("scheduler-outbox-publish", move |task_shutdown| {
        let publisher = publisher.clone();
        async move {
            publisher.run_until(task_shutdown.cancelled()).await;
        }
    });
}
