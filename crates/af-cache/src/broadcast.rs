use std::{fmt, num::NonZeroUsize, sync::Arc};

use futures_util::StreamExt as _;
use redis::aio::PubSub;
use tokio::time;

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind,
    config::validate_namespace,
    redis_backend::{RedisBackend, redis_error},
};

/// 单条 Redis 广播消息默认最多占用 4 KiB。
pub const DEFAULT_BROADCAST_MAX_MESSAGE_BYTES: usize = 4 * 1024;

/// 独立 Redis pub/sub 通道的连接与消息边界。
#[derive(Clone)]
pub struct RedisBroadcastConfig {
    redis: RedisConfig,
    channel: String,
    max_message_bytes: NonZeroUsize,
}

impl RedisBroadcastConfig {
    /// 创建使用固定 Redis 通道的广播配置。
    pub fn new(redis: RedisConfig, channel: impl Into<String>) -> Result<Self, CacheError> {
        let config = Self {
            redis,
            channel: channel.into(),
            max_message_bytes: NonZeroUsize::new(DEFAULT_BROADCAST_MAX_MESSAGE_BYTES)
                .expect("固定广播消息上限必须为正数"),
        };
        config.validate()?;
        Ok(config)
    }

    /// 覆盖单条广播消息的非零字节上限。
    pub fn with_max_message_bytes(mut self, max_message_bytes: usize) -> Result<Self, CacheError> {
        self.max_message_bytes =
            NonZeroUsize::new(max_message_bytes).ok_or(CacheError::InvalidMaxValueSize)?;
        if max_message_bytes as u128 > i64::MAX as u128 {
            return Err(CacheError::InvalidMaxValueSize);
        }
        Ok(self)
    }

    fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.channel).map_err(|_| CacheError::InvalidBroadcastChannel)?;
        self.redis.validate()
    }
}

impl fmt::Debug for RedisBroadcastConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisBroadcastConfig")
            .field("redis", &self.redis)
            .field("channel", &self.channel)
            .field("max_message_bytes", &self.max_message_bytes)
            .finish()
    }
}

/// 使用可重连命令连接向固定 Redis 通道发布有界消息。
#[derive(Clone)]
pub struct RedisBroadcastPublisher {
    backend: Arc<RedisBackend>,
    channel: Arc<str>,
    max_message_bytes: usize,
}

impl RedisBroadcastPublisher {
    /// 建立并验证发布连接；配置了 Redis 时连接失败必须由调用方显式处理。
    pub async fn connect(config: RedisBroadcastConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let backend = Arc::new(RedisBackend::connect(&config.redis).await?);
        Ok(Self {
            backend,
            channel: Arc::from(config.channel),
            max_message_bytes: config.max_message_bytes.get(),
        })
    }

    /// 发布一条消息，并返回命令执行时 Redis 报告的订阅者数量。
    ///
    /// 零订阅者仍表示 `PUBLISH` 命令已经成功；pub/sub 不是持久队列，调用方必须另有
    /// 全量校正机制覆盖订阅断线窗口。
    pub async fn publish(&self, payload: &[u8]) -> Result<u64, CacheError> {
        validate_payload(payload, self.max_message_bytes)?;
        self.backend
            .query(
                CacheOperation::Publish,
                redis::cmd("PUBLISH")
                    .arg(self.channel.as_ref())
                    .arg(payload),
            )
            .await
    }
}

impl fmt::Debug for RedisBroadcastPublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisBroadcastPublisher")
            .field("channel", &self.channel)
            .field("max_message_bytes", &self.max_message_bytes)
            .finish_non_exhaustive()
    }
}

/// 持有 Redis 专用 pub/sub 连接并接收固定通道的有界消息。
pub struct RedisBroadcastSubscriber {
    pubsub: PubSub,
    channel: String,
    max_message_bytes: usize,
}

impl RedisBroadcastSubscriber {
    /// 建立专用连接并等待 Redis 确认订阅，返回后发布方才可依赖本实例已进入订阅窗口。
    pub async fn connect(config: RedisBroadcastConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let client =
            redis::Client::open(config.redis.url()).map_err(|_| CacheError::InvalidRedisUrl)?;
        let mut pubsub = time::timeout(config.redis.connect_timeout(), client.get_async_pubsub())
            .await
            .map_err(|_| CacheError::redis(CacheOperation::Connect, RedisFailureKind::Timeout))?
            .map_err(|error| redis_error(CacheOperation::Connect, &error))?;
        time::timeout(
            config.redis.operation_timeout(),
            pubsub.subscribe(config.channel.as_str()),
        )
        .await
        .map_err(|_| CacheError::redis(CacheOperation::Subscribe, RedisFailureKind::Timeout))?
        .map_err(|error| redis_error(CacheOperation::Subscribe, &error))?;
        Ok(Self {
            pubsub,
            channel: config.channel,
            max_message_bytes: config.max_message_bytes.get(),
        })
    }

    /// 等待下一条消息；连接关闭时返回稳定分类，由上层监督器重建订阅并重新校正快照。
    pub async fn recv(&mut self) -> Result<Vec<u8>, CacheError> {
        let mut messages = self.pubsub.on_message();
        let message = messages.next().await.ok_or(CacheError::BroadcastClosed)?;
        if message.get_channel_name() != self.channel {
            return Err(CacheError::redis(
                CacheOperation::Receive,
                RedisFailureKind::Protocol,
            ));
        }
        let payload = message.get_payload_bytes();
        validate_payload(payload, self.max_message_bytes)?;
        Ok(payload.to_vec())
    }
}

impl fmt::Debug for RedisBroadcastSubscriber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisBroadcastSubscriber")
            .field("channel", &self.channel)
            .field("max_message_bytes", &self.max_message_bytes)
            .finish_non_exhaustive()
    }
}

fn validate_payload(payload: &[u8], max_message_bytes: usize) -> Result<(), CacheError> {
    if payload.is_empty() || payload.len() > max_message_bytes {
        return Err(CacheError::InvalidBroadcastPayload);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejects_invalid_channels_and_message_limits() {
        let redis = RedisConfig::new("redis://127.0.0.1:6379/");
        assert_eq!(
            RedisBroadcastConfig::new(redis.clone(), "bad channel").unwrap_err(),
            CacheError::InvalidBroadcastChannel
        );
        assert_eq!(
            RedisBroadcastConfig::new(redis, "scheduler.invalidate.v1")
                .unwrap()
                .with_max_message_bytes(0)
                .unwrap_err(),
            CacheError::InvalidMaxValueSize
        );
    }

    #[test]
    fn publisher_rejects_empty_and_oversized_payloads_before_io() {
        assert_eq!(
            validate_payload(&[], 8),
            Err(CacheError::InvalidBroadcastPayload)
        );
        assert_eq!(
            validate_payload(b"123456789", 8),
            Err(CacheError::InvalidBroadcastPayload)
        );
        assert!(validate_payload(b"event-v1", 8).is_ok());
    }

    #[test]
    fn debug_output_redacts_redis_credentials() {
        let config = RedisBroadcastConfig::new(
            RedisConfig::new("redis://private-user:private-secret@127.0.0.1:6379/"),
            "scheduler.invalidate.v1",
        )
        .unwrap();
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("private-user"));
        assert!(!rendered.contains("private-secret"));
    }
}
