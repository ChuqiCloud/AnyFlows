use std::{fmt, sync::Arc, time::Duration};

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind,
    config::{validate_cache_key, validate_namespace, validate_ttl},
    redis_backend::RedisBackend,
};

const LEASE_OWNER_BYTES: usize = 16;

const COMPARE_AND_DELETE_SCRIPT: &str = r#"
if redis.call('GET', KEYS[1]) == ARGV[1] then
    return redis.call('DEL', KEYS[1])
end
return 0
"#;

/// 独立于 HybridCache 的分布式租约配置。
#[derive(Clone)]
pub struct DistributedLeaseConfig {
    namespace: String,
    redis: Option<RedisConfig>,
}

impl DistributedLeaseConfig {
    /// 创建本地模式配置；通过 `with_redis` 显式启用跨实例租约。
    pub fn new(namespace: impl Into<String>) -> Result<Self, CacheError> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        Ok(Self {
            namespace,
            redis: None,
        })
    }

    /// 启用 Redis；连接或命令故障会直接返回错误，不会降级为无锁执行。
    #[must_use]
    pub fn with_redis(mut self, redis: RedisConfig) -> Self {
        self.redis = Some(redis);
        self
    }

    /// 返回是否已显式启用 Redis。
    #[must_use]
    pub const fn redis_enabled(&self) -> bool {
        self.redis.is_some()
    }

    fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.namespace)?;
        if let Some(redis) = &self.redis {
            redis.validate()?;
        }
        Ok(())
    }
}

impl fmt::Debug for DistributedLeaseConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DistributedLeaseConfig")
            .field("namespace", &"<已脱敏>")
            .field("redis_enabled", &self.redis_enabled())
            .finish()
    }
}

/// 当前租约协调范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DistributedLeaseMode {
    /// 未配置 Redis，只保留调用方已有的单进程协调语义。
    LocalOnly,
    /// 使用 Redis 原子命令协调多个进程。
    Redis,
}

#[derive(Clone)]
enum LeaseBackend {
    LocalOnly,
    Redis(Arc<RedisBackend>),
}

/// 使用安全 owner 获取并释放短期租约的协调器。
#[derive(Clone)]
pub struct DistributedLeaseManager {
    namespace: Arc<str>,
    backend: LeaseBackend,
}

impl DistributedLeaseManager {
    /// 按配置创建租约协调器；Redis 模式会在返回前完成连接和健康检查。
    pub async fn new(config: DistributedLeaseConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let DistributedLeaseConfig { namespace, redis } = config;
        let backend = match redis {
            Some(redis) => LeaseBackend::Redis(Arc::new(RedisBackend::connect(&redis).await?)),
            None => LeaseBackend::LocalOnly,
        };
        Ok(Self {
            namespace: Arc::from(namespace),
            backend,
        })
    }

    /// 创建不访问 Redis 的显式本地模式，供已有 singleflight 的调用方使用。
    pub fn local_only(namespace: impl Into<String>) -> Result<Self, CacheError> {
        let config = DistributedLeaseConfig::new(namespace)?;
        Ok(Self {
            namespace: Arc::from(config.namespace),
            backend: LeaseBackend::LocalOnly,
        })
    }

    /// 返回当前协调范围，便于启动检查和有限指标使用。
    #[must_use]
    pub const fn mode(&self) -> DistributedLeaseMode {
        match &self.backend {
            LeaseBackend::LocalOnly => DistributedLeaseMode::LocalOnly,
            LeaseBackend::Redis(_) => DistributedLeaseMode::Redis,
        }
    }

    /// 尝试获取指定键的租约；Redis 模式使用 `SET key owner NX PX ttl` 原子竞争。
    pub async fn acquire(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<LeaseAcquireOutcome, CacheError> {
        let ttl_millis = validate_ttl(ttl)?;
        let key = self.namespaced_key(key)?;
        let LeaseBackend::Redis(backend) = &self.backend else {
            return Ok(LeaseAcquireOutcome::Acquired(DistributedLease {
                state: DistributedLeaseState::LocalOnly,
            }));
        };

        let mut owner = [0_u8; LEASE_OWNER_BYTES];
        getrandom::fill(&mut owner).map_err(|_| CacheError::EntropyUnavailable)?;
        let response: Option<String> = backend
            .query(
                CacheOperation::LeaseAcquire,
                redis::cmd("SET")
                    .arg(&key)
                    .arg(owner.as_slice())
                    .arg("NX")
                    .arg("PX")
                    .arg(ttl_millis),
            )
            .await?;

        match response.as_deref() {
            Some("OK") => Ok(LeaseAcquireOutcome::Acquired(DistributedLease {
                state: DistributedLeaseState::Redis {
                    backend: Arc::clone(backend),
                    key,
                    owner,
                },
            })),
            None => Ok(LeaseAcquireOutcome::Held),
            Some(_) => Err(CacheError::redis(
                CacheOperation::LeaseAcquire,
                RedisFailureKind::Protocol,
            )),
        }
    }

    fn namespaced_key(&self, key: &str) -> Result<String, CacheError> {
        validate_cache_key(key)?;
        let namespaced = format!("{}:{key}", self.namespace);
        validate_cache_key(&namespaced)?;
        Ok(namespaced)
    }
}

impl fmt::Debug for DistributedLeaseManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DistributedLeaseManager")
            .field("namespace", &"<已脱敏>")
            .field("mode", &self.mode())
            .finish()
    }
}

/// 一次非阻塞租约竞争的闭合结果。
#[derive(Debug)]
pub enum LeaseAcquireOutcome {
    /// 当前调用者取得租约所有权。
    Acquired(DistributedLease),
    /// 该键正由其他所有者持有。
    Held,
}

enum DistributedLeaseState {
    LocalOnly,
    Redis {
        backend: Arc<RedisBackend>,
        key: String,
        owner: [u8; LEASE_OWNER_BYTES],
    },
}

/// 已取得的租约所有权；显式释放会消费 handle，取消或崩溃依靠 TTL 收敛。
pub struct DistributedLease {
    state: DistributedLeaseState,
}

impl DistributedLease {
    /// 仅当 Redis 中的 owner 仍匹配时删除租约，避免迟到释放破坏新所有者。
    pub async fn release(self) -> Result<LeaseReleaseOutcome, CacheError> {
        let DistributedLeaseState::Redis {
            backend,
            key,
            owner,
        } = self.state
        else {
            return Ok(LeaseReleaseOutcome::Released);
        };
        let deleted: i64 = backend
            .query(
                CacheOperation::LeaseRelease,
                redis::cmd("EVAL")
                    .arg(COMPARE_AND_DELETE_SCRIPT)
                    .arg(1)
                    .arg(key)
                    .arg(owner.as_slice()),
            )
            .await?;
        match deleted {
            0 => Ok(LeaseReleaseOutcome::Lost),
            1 => Ok(LeaseReleaseOutcome::Released),
            _ => Err(CacheError::redis(
                CacheOperation::LeaseRelease,
                RedisFailureKind::Protocol,
            )),
        }
    }
}

impl fmt::Debug for DistributedLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match &self.state {
            DistributedLeaseState::LocalOnly => DistributedLeaseMode::LocalOnly,
            DistributedLeaseState::Redis { .. } => DistributedLeaseMode::Redis,
        };
        formatter
            .debug_struct("DistributedLease")
            .field("mode", &mode)
            .field("key", &"<已脱敏>")
            .field("owner", &"<已脱敏>")
            .finish()
    }
}

/// 显式释放后的所有权结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseReleaseOutcome {
    /// 当前 owner 已删除自己的租约。
    Released,
    /// 租约已过期或所有权已经转移，未删除任何新租约。
    Lost,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_manager_debug_hide_namespace_and_redis_url() {
        let config = DistributedLeaseConfig::new("private.namespace")
            .unwrap()
            .with_redis(RedisConfig::new(
                "redis://user:secret@redis.internal:6379/0",
            ));
        let rendered = format!("{config:?}");
        for private in ["private.namespace", "user", "secret", "redis.internal"] {
            assert!(!rendered.contains(private));
        }

        let manager = DistributedLeaseManager::local_only("private.namespace").unwrap();
        let rendered = format!("{manager:?}");
        assert!(!rendered.contains("private.namespace"));
        assert!(rendered.contains("LocalOnly"));
    }

    #[tokio::test]
    async fn local_mode_preserves_existing_single_process_behavior() {
        let manager = DistributedLeaseManager::local_only("oauth.refresh.v1").unwrap();
        let acquired = manager
            .acquire("channel:7:credential:9:revision:2", Duration::from_secs(1))
            .await
            .unwrap();
        let LeaseAcquireOutcome::Acquired(lease) = acquired else {
            panic!("本地模式不得伪造跨实例竞争");
        };
        let rendered = format!("{lease:?}");
        assert!(!rendered.contains("channel:7"));
        assert!(!rendered.contains("credential:9"));
        assert_eq!(
            lease.release().await.unwrap(),
            LeaseReleaseOutcome::Released
        );
    }

    #[tokio::test]
    async fn rejects_invalid_key_and_ttl_before_acquisition() {
        let manager = DistributedLeaseManager::local_only("oauth.refresh.v1").unwrap();
        assert_eq!(
            manager
                .acquire("line\nbreak", Duration::from_secs(1))
                .await
                .unwrap_err(),
            CacheError::InvalidKey
        );
        assert_eq!(
            manager.acquire("valid", Duration::ZERO).await.unwrap_err(),
            CacheError::InvalidTtl
        );
    }
}
