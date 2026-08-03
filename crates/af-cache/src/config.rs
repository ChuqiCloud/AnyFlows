use std::{fmt, num::NonZeroUsize, time::Duration};

use crate::CacheError;

const MAX_NAMESPACE_BYTES: usize = 64;
const MAX_CACHE_KEY_BYTES: usize = 1024;

/// Redis 建连的默认超时。
pub const DEFAULT_REDIS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次 Redis 命令的默认超时。
pub const DEFAULT_REDIS_OPERATION_TIMEOUT: Duration = Duration::from_secs(1);
/// Hybrid 模式下单个 L1 副本的默认最长存活时间。
pub const DEFAULT_LOCAL_TTL_CAP: Duration = Duration::from_secs(30);
/// 单个缓存值默认最多占用 16 MiB。
pub const DEFAULT_MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;

/// HybridCache 的本地容量、命名空间与可选 Redis 配置。
#[derive(Clone, Debug)]
pub struct HybridCacheConfig {
    namespace: String,
    local_capacity: NonZeroUsize,
    local_ttl_cap: Duration,
    max_value_bytes: NonZeroUsize,
    redis: Option<RedisConfig>,
}

impl HybridCacheConfig {
    /// 创建纯本地配置；通过 `with_redis` 显式启用 Redis。
    pub fn new(namespace: impl Into<String>, local_capacity: usize) -> Result<Self, CacheError> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        let local_capacity =
            NonZeroUsize::new(local_capacity).ok_or(CacheError::InvalidCapacity)?;

        Ok(Self {
            namespace,
            local_capacity,
            local_ttl_cap: DEFAULT_LOCAL_TTL_CAP,
            max_value_bytes: NonZeroUsize::new(DEFAULT_MAX_VALUE_BYTES)
                .expect("默认缓存值上限必须大于零"),
            redis: None,
        })
    }

    /// 设置 Hybrid 模式下 L1 的最长 TTL，限制无失效广播阶段的跨实例陈旧窗口。
    #[must_use]
    pub const fn with_local_ttl_cap(mut self, local_ttl_cap: Duration) -> Self {
        self.local_ttl_cap = local_ttl_cap;
        self
    }

    /// 设置单个缓存值的字节上限，防止少量超大值耗尽进程内存。
    pub fn with_max_value_bytes(mut self, max_value_bytes: usize) -> Result<Self, CacheError> {
        self.max_value_bytes = validate_max_value_bytes(max_value_bytes)?;
        Ok(self)
    }

    /// 启用 Redis；配置后发生连接或命令故障时不会静默退回本地模式。
    #[must_use]
    pub fn with_redis(mut self, redis: RedisConfig) -> Self {
        self.redis = Some(redis);
        self
    }

    /// 返回用于隔离 Redis 键空间的命名空间。
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// 返回本地 LRU 可容纳的最大条目数。
    #[must_use]
    pub const fn local_capacity(&self) -> NonZeroUsize {
        self.local_capacity
    }

    /// 返回 Hybrid 模式下 L1 的最长 TTL。
    #[must_use]
    pub const fn local_ttl_cap(&self) -> Duration {
        self.local_ttl_cap
    }

    /// 返回单个缓存值的最大字节数。
    #[must_use]
    pub const fn max_value_bytes(&self) -> NonZeroUsize {
        self.max_value_bytes
    }

    /// 返回是否已显式配置 Redis。
    #[must_use]
    pub const fn redis_enabled(&self) -> bool {
        self.redis.is_some()
    }

    pub(crate) fn redis(&self) -> Option<&RedisConfig> {
        self.redis.as_ref()
    }

    pub(crate) fn mutation_timeout(&self) -> Duration {
        self.redis.as_ref().map_or(
            DEFAULT_REDIS_OPERATION_TIMEOUT,
            RedisConfig::operation_timeout,
        )
    }

    pub(crate) fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.namespace)?;
        validate_ttl(self.local_ttl_cap)?;
        validate_max_value_bytes(self.max_value_bytes.get())?;
        if let Some(redis) = &self.redis {
            redis.validate()?;
        }
        Ok(())
    }
}

/// Redis 单节点连接配置。
#[derive(Clone)]
pub struct RedisConfig {
    url: String,
    connect_timeout: Duration,
    operation_timeout: Duration,
}

impl RedisConfig {
    /// 创建 Redis 配置；当前连接器接受 redis-rs 支持的非 TLS 地址。
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            connect_timeout: DEFAULT_REDIS_CONNECT_TIMEOUT,
            operation_timeout: DEFAULT_REDIS_OPERATION_TIMEOUT,
        }
    }

    /// 覆盖建连与单次命令超时；零值会在构建缓存时被拒绝。
    #[must_use]
    pub const fn with_timeouts(
        mut self,
        connect_timeout: Duration,
        operation_timeout: Duration,
    ) -> Self {
        self.connect_timeout = connect_timeout;
        self.operation_timeout = operation_timeout;
        self
    }

    /// 返回建连超时。
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// 返回单次命令超时。
    #[must_use]
    pub const fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn validate(&self) -> Result<(), CacheError> {
        if self.url.trim().is_empty() {
            return Err(CacheError::InvalidRedisUrl);
        }
        if self.connect_timeout.is_zero() || self.operation_timeout.is_zero() {
            return Err(CacheError::InvalidTimeout);
        }
        Ok(())
    }
}

impl fmt::Debug for RedisConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisConfig")
            .field("url", &"<redacted>")
            .field("connect_timeout", &self.connect_timeout)
            .field("operation_timeout", &self.operation_timeout)
            .finish()
    }
}

pub(crate) fn validate_cache_key(key: &str) -> Result<(), CacheError> {
    if key.is_empty() || key.len() > MAX_CACHE_KEY_BYTES || key.chars().any(char::is_control) {
        return Err(CacheError::InvalidKey);
    }
    Ok(())
}

pub(crate) fn validate_ttl(ttl: Duration) -> Result<i64, CacheError> {
    let milliseconds = ttl.as_millis();
    if milliseconds == 0
        || milliseconds > i64::MAX as u128
        || !ttl.subsec_nanos().is_multiple_of(1_000_000)
    {
        return Err(CacheError::InvalidTtl);
    }
    i64::try_from(milliseconds).map_err(|_| CacheError::InvalidTtl)
}

pub(crate) fn validate_namespace(namespace: &str) -> Result<(), CacheError> {
    let valid = !namespace.is_empty()
        && namespace.len() <= MAX_NAMESPACE_BYTES
        && namespace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if !valid {
        return Err(CacheError::InvalidNamespace);
    }
    Ok(())
}

fn validate_max_value_bytes(max_value_bytes: usize) -> Result<NonZeroUsize, CacheError> {
    let value = NonZeroUsize::new(max_value_bytes).ok_or(CacheError::InvalidMaxValueSize)?;
    if value.get() as u128 > i64::MAX as u128 {
        return Err(CacheError::InvalidMaxValueSize);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_namespace_and_capacity() {
        assert_eq!(
            HybridCacheConfig::new("", 1).unwrap_err(),
            CacheError::InvalidNamespace
        );
        assert_eq!(
            HybridCacheConfig::new("valid", 0).unwrap_err(),
            CacheError::InvalidCapacity
        );
        assert!(HybridCacheConfig::new("scheduler.v1", 16).is_ok());
    }

    #[test]
    fn redis_debug_output_redacts_credentials() {
        let config = RedisConfig::new("redis://user:secret@127.0.0.1:6379/0");
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("user"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn rejects_sub_millisecond_and_unbounded_ttl() {
        assert_eq!(
            validate_ttl(Duration::from_nanos(1)).unwrap_err(),
            CacheError::InvalidTtl
        );
        assert_eq!(
            validate_ttl(Duration::from_micros(1_500)).unwrap_err(),
            CacheError::InvalidTtl
        );
        assert_eq!(
            validate_ttl(Duration::MAX).unwrap_err(),
            CacheError::InvalidTtl
        );
        assert_eq!(validate_ttl(Duration::from_millis(1)).unwrap(), 1);
    }

    #[test]
    fn rejects_zero_timeouts_and_local_ttl_cap() {
        let redis = RedisConfig::new("redis://127.0.0.1:6379/")
            .with_timeouts(Duration::ZERO, Duration::from_secs(1));
        let config = HybridCacheConfig::new("cache.v1", 1)
            .unwrap()
            .with_redis(redis);
        assert_eq!(config.validate().unwrap_err(), CacheError::InvalidTimeout);

        let config = HybridCacheConfig::new("cache.v1", 1)
            .unwrap()
            .with_local_ttl_cap(Duration::ZERO);
        assert_eq!(config.validate().unwrap_err(), CacheError::InvalidTtl);

        assert_eq!(
            HybridCacheConfig::new("cache.v1", 1)
                .unwrap()
                .with_max_value_bytes(0)
                .unwrap_err(),
            CacheError::InvalidMaxValueSize
        );
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            HybridCacheConfig::new("cache.v1", 1)
                .unwrap()
                .with_max_value_bytes(i64::MAX as usize + 1)
                .unwrap_err(),
            CacheError::InvalidMaxValueSize
        );
    }

    #[test]
    fn rejects_invalid_cache_keys() {
        assert_eq!(validate_cache_key("").unwrap_err(), CacheError::InvalidKey);
        assert_eq!(
            validate_cache_key("line\nbreak").unwrap_err(),
            CacheError::InvalidKey
        );
        assert!(validate_cache_key("tenant:42:model:gpt-5").is_ok());
    }
}
