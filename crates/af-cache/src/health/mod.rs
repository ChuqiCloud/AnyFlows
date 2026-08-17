mod scripts;
mod types;

use std::{fmt, sync::Arc};

use crate::{
    CacheError, CacheOperation, RedisFailureKind, config::validate_cache_key,
    redis_backend::RedisBackend,
};
use scripts::{APPLY_EVENTS_SCRIPT, READ_STATES_SCRIPT};

pub use types::{
    HEALTH_PENALTY_SCALE, MAX_HEALTH_BATCH_SIZE, RedisHealthConfig, RedisHealthEvent,
    RedisHealthFailure, RedisHealthSnapshot, RedisHealthTarget,
};

const STATE_FIELDS_PER_RESULT: usize = 4;

/// 使用 Redis 服务端时间原子维护渠道与凭据的共享健康状态。
#[derive(Clone)]
pub struct RedisHealthStore {
    backend: Arc<RedisBackend>,
    key_prefix: Arc<str>,
    half_life_millis: i64,
    transient_streak_window_millis: i64,
    state_ttl_millis: i64,
    breaker_threshold: u32,
    cooldown_millis: [i64; 3],
}

impl RedisHealthStore {
    /// 建立 Redis 连接并验证熔断键空间与策略边界。
    pub async fn connect(config: RedisHealthConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let key_prefix = format!("{{{}}}:health", config.namespace());
        validate_cache_key(&key_prefix)?;
        Ok(Self {
            backend: Arc::new(RedisBackend::connect(config.redis()).await?),
            key_prefix: Arc::from(key_prefix),
            half_life_millis: config.half_life_millis()?,
            transient_streak_window_millis: config.transient_streak_window_millis()?,
            state_ttl_millis: config.state_ttl_millis()?,
            breaker_threshold: config.breaker_threshold(),
            cooldown_millis: config.cooldown_millis()?,
        })
    }

    /// 一次读取最多 128 个目标，并使用 Redis 服务端时间计算当前衰减分与冷却余量。
    pub async fn states(
        &self,
        targets: &[RedisHealthTarget],
    ) -> Result<Vec<RedisHealthSnapshot>, CacheError> {
        validate_read_targets(targets)?;
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let keys = targets
            .iter()
            .copied()
            .map(|target| self.target_key(target))
            .collect::<Result<Vec<_>, _>>()?;
        let values: Vec<i64> = self
            .backend
            .query(
                CacheOperation::HealthRead,
                redis::cmd("EVAL")
                    .arg(READ_STATES_SCRIPT)
                    .arg(keys.len())
                    .arg(&keys)
                    .arg(self.half_life_millis),
            )
            .await?;
        decode_snapshots(CacheOperation::HealthRead, targets, &values)
    }

    /// 按调用顺序原子应用成功或失败事件；同一目标可在一批内先失败后成功。
    pub async fn apply(
        &self,
        events: &[RedisHealthEvent],
    ) -> Result<Vec<RedisHealthSnapshot>, CacheError> {
        if events.is_empty() || events.len() > MAX_HEALTH_BATCH_SIZE {
            return Err(CacheError::InvalidHealthBatch);
        }
        let keys = events
            .iter()
            .map(|event| self.target_key(event.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut command = redis::cmd("EVAL");
        command
            .arg(APPLY_EVENTS_SCRIPT)
            .arg(keys.len())
            .arg(&keys)
            .arg(self.half_life_millis)
            .arg(self.transient_streak_window_millis)
            .arg(self.state_ttl_millis)
            .arg(self.breaker_threshold)
            .arg(self.cooldown_millis[0])
            .arg(self.cooldown_millis[1])
            .arg(self.cooldown_millis[2]);
        for event in events {
            command
                .arg(if event.is_failure() { 1 } else { 0 })
                .arg(event.penalty_micros())
                .arg(if event.is_transient() { 1 } else { 0 });
        }
        let values: Vec<i64> = self
            .backend
            .query(CacheOperation::HealthWrite, &mut command)
            .await?;
        let targets = events
            .iter()
            .map(|event| event.target())
            .collect::<Vec<_>>();
        decode_snapshots(CacheOperation::HealthWrite, &targets, &values)
    }

    fn target_key(&self, target: RedisHealthTarget) -> Result<String, CacheError> {
        let key = match target {
            RedisHealthTarget::Channel(channel_id) => {
                format!("{}:channel:{}", self.key_prefix, channel_id.get())
            }
            RedisHealthTarget::Credential(credential_id) => {
                format!("{}:credential:{}", self.key_prefix, credential_id.get())
            }
            RedisHealthTarget::CredentialShared(credential_id) => {
                format!(
                    "{}:credential-shared:{}",
                    self.key_prefix,
                    credential_id.get()
                )
            }
        };
        validate_cache_key(&key)?;
        Ok(key)
    }
}

impl fmt::Debug for RedisHealthStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisHealthStore")
            .field("half_life_millis", &self.half_life_millis)
            .field(
                "transient_streak_window_millis",
                &self.transient_streak_window_millis,
            )
            .field("state_ttl_millis", &self.state_ttl_millis)
            .field("breaker_threshold", &self.breaker_threshold)
            .field("cooldown_millis", &self.cooldown_millis)
            .finish_non_exhaustive()
    }
}

fn validate_read_targets(targets: &[RedisHealthTarget]) -> Result<(), CacheError> {
    if targets.len() > MAX_HEALTH_BATCH_SIZE {
        return Err(CacheError::InvalidHealthBatch);
    }
    let unique = targets
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != targets.len() {
        return Err(CacheError::InvalidHealthBatch);
    }
    Ok(())
}

fn decode_snapshots(
    operation: CacheOperation,
    targets: &[RedisHealthTarget],
    values: &[i64],
) -> Result<Vec<RedisHealthSnapshot>, CacheError> {
    if values.len()
        != targets
            .len()
            .checked_mul(STATE_FIELDS_PER_RESULT)
            .ok_or_else(|| protocol_error(operation))?
    {
        return Err(protocol_error(operation));
    }
    targets
        .iter()
        .copied()
        .zip(values.chunks_exact(STATE_FIELDS_PER_RESULT))
        .map(|(target, state)| RedisHealthSnapshot::decode(target, state, operation))
        .collect()
}

fn protocol_error(operation: CacheOperation) -> CacheError {
    CacheError::redis(operation, RedisFailureKind::Protocol)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use af_domain::{ChannelId, CredentialId};

    use super::*;
    use crate::RedisConfig;

    #[test]
    fn read_batch_rejects_duplicates_and_excess_capacity() {
        let channel = RedisHealthTarget::Channel(ChannelId::new(1).unwrap());
        assert_eq!(
            validate_read_targets(&[channel, channel]),
            Err(CacheError::InvalidHealthBatch)
        );
        let oversized = (1_i64..=129)
            .map(|id| RedisHealthTarget::Credential(CredentialId::new(id).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            validate_read_targets(&oversized),
            Err(CacheError::InvalidHealthBatch)
        );
    }

    #[test]
    fn debug_output_does_not_expose_namespace_or_redis_url() {
        let config = RedisHealthConfig::new(
            RedisConfig::new("redis://user:secret@redis.internal:6379/0"),
            "private.scheduler.health",
        )
        .unwrap()
        .with_policy(
            Duration::from_secs(10),
            Duration::from_secs(5),
            2,
            [
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(3),
            ],
            Duration::from_secs(30),
        )
        .unwrap();
        let rendered = format!("{config:?}");
        for private in [
            "private.scheduler.health",
            "user",
            "secret",
            "redis.internal",
        ] {
            assert!(!rendered.contains(private));
        }
    }
}
