use std::{fmt, sync::Arc, time::Duration};

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind,
    config::{validate_cache_key, validate_namespace, validate_ttl},
    redis_backend::RedisBackend,
};

/// 粘性会话默认保留时间；每次命中都会通过 Redis 原子读取刷新。
pub const DEFAULT_STICKY_SESSION_TTL: Duration = Duration::from_secs(60 * 60);

const STICKY_DIGEST_BYTES: usize = 64;
const MAX_CHANNEL_ID_BYTES: usize = 19;
const READ_AND_REFRESH_SCRIPT: &str = r#"
local value = redis.call('GET', KEYS[1])
if not value then
    return {0, ''}
end
if string.len(value) == 0 or string.len(value) > 19 or string.find(value, '%D') then
    return {1, ''}
end
redis.call('PEXPIRE', KEYS[1], ARGV[1])
return {2, value}
"#;
const REFRESH_IF_MATCH_SCRIPT: &str = r#"
local value = redis.call('GET', KEYS[1])
if value == ARGV[1] then
    redis.call('PEXPIRE', KEYS[1], ARGV[2])
    return 1
end
return 0
"#;
const DELETE_IF_MATCH_SCRIPT: &str = r#"
if redis.call('GET', KEYS[1]) == ARGV[1] then
    return redis.call('DEL', KEYS[1])
end
return 0
"#;

/// Redis 粘性会话键空间和 TTL 配置。
#[derive(Clone)]
pub struct RedisStickySessionConfig {
    redis: RedisConfig,
    namespace: String,
    ttl: Duration,
}

impl RedisStickySessionConfig {
    /// 创建粘性会话配置；命名空间不应包含租户或请求正文等动态信息。
    pub fn new(redis: RedisConfig, namespace: impl Into<String>) -> Result<Self, CacheError> {
        let config = Self {
            redis,
            namespace: namespace.into(),
            ttl: DEFAULT_STICKY_SESSION_TTL,
        };
        config.validate()?;
        Ok(config)
    }

    /// 覆盖粘性绑定 TTL；零值、亚毫秒值和超出 Redis PX 范围的值会被拒绝。
    pub fn with_ttl(mut self, ttl: Duration) -> Result<Self, CacheError> {
        validate_ttl(ttl)?;
        self.ttl = ttl;
        Ok(self)
    }

    fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.namespace)?;
        validate_ttl(self.ttl)?;
        self.redis.validate()
    }
}

impl fmt::Debug for RedisStickySessionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisStickySessionConfig")
            .field("redis", &self.redis)
            .field("namespace", &"<已脱敏>")
            .field("ttl", &self.ttl)
            .finish()
    }
}

/// 使用 Redis 原子命令维护会话摘要到渠道标识的粘性映射。
#[derive(Clone)]
pub struct RedisStickySessionStore {
    backend: Arc<RedisBackend>,
    namespace: Arc<str>,
    ttl: Duration,
}

impl RedisStickySessionStore {
    /// 建立并验证 Redis 连接；显式配置 Redis 时连接失败必须阻止继续装配。
    pub async fn connect(config: RedisStickySessionConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let backend = Arc::new(RedisBackend::connect(&config.redis).await?);
        Ok(Self {
            backend,
            namespace: Arc::from(config.namespace),
            ttl: config.ttl,
        })
    }

    /// 读取并原子刷新 TTL；缺失键返回 `None`，Redis 中的非法值失败关闭。
    pub async fn get_and_refresh(&self, digest: &str) -> Result<Option<i64>, CacheError> {
        let key = self.namespaced_key(digest)?;
        let ttl_millis = validate_ttl(self.ttl)?;
        let (status, value): (i64, Vec<u8>) = self
            .backend
            .query(
                CacheOperation::StickyRead,
                redis::cmd("EVAL")
                    .arg(READ_AND_REFRESH_SCRIPT)
                    .arg(1)
                    .arg(key)
                    .arg(ttl_millis),
            )
            .await?;
        match status {
            0 => Ok(None),
            1 => Err(CacheError::redis(
                CacheOperation::StickyRead,
                RedisFailureKind::Protocol,
            )),
            2 => parse_channel_id(&value).map(Some).map_err(|_| {
                CacheError::redis(CacheOperation::StickyRead, RedisFailureKind::Protocol)
            }),
            _ => Err(CacheError::redis(
                CacheOperation::StickyRead,
                RedisFailureKind::Protocol,
            )),
        }
    }

    /// 写入新的渠道绑定并覆盖旧 TTL；写入只保存正整数渠道标识。
    pub async fn bind(&self, digest: &str, channel_id: i64) -> Result<(), CacheError> {
        let key = self.namespaced_key(digest)?;
        let channel_id = encode_channel_id(channel_id)?;
        let ttl_millis = validate_ttl(self.ttl)?;
        self.backend
            .query(
                CacheOperation::StickyWrite,
                redis::cmd("SET")
                    .arg(key)
                    .arg(channel_id)
                    .arg("PX")
                    .arg(ttl_millis),
            )
            .await
    }

    /// 仅当当前值仍属于指定渠道时原子刷新 TTL，避免覆盖并发改绑。
    pub async fn refresh_if_channel(
        &self,
        digest: &str,
        channel_id: i64,
    ) -> Result<bool, CacheError> {
        let key = self.namespaced_key(digest)?;
        let channel_id = encode_channel_id(channel_id)?;
        let ttl_millis = validate_ttl(self.ttl)?;
        let refreshed: i64 = self
            .backend
            .query(
                CacheOperation::StickyRefresh,
                redis::cmd("EVAL")
                    .arg(REFRESH_IF_MATCH_SCRIPT)
                    .arg(1)
                    .arg(key)
                    .arg(channel_id)
                    .arg(ttl_millis),
            )
            .await?;
        match refreshed {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CacheError::redis(
                CacheOperation::StickyRefresh,
                RedisFailureKind::Protocol,
            )),
        }
    }

    /// 仅当当前值仍等于指定渠道时原子删除，迟到清理不能删除新绑定。
    pub async fn delete_if_channel(
        &self,
        digest: &str,
        channel_id: i64,
    ) -> Result<bool, CacheError> {
        let key = self.namespaced_key(digest)?;
        let channel_id = encode_channel_id(channel_id)?;
        let deleted: i64 = self
            .backend
            .query(
                CacheOperation::StickyDelete,
                redis::cmd("EVAL")
                    .arg(DELETE_IF_MATCH_SCRIPT)
                    .arg(1)
                    .arg(key)
                    .arg(channel_id),
            )
            .await?;
        match deleted {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CacheError::redis(
                CacheOperation::StickyDelete,
                RedisFailureKind::Protocol,
            )),
        }
    }

    fn namespaced_key(&self, digest: &str) -> Result<String, CacheError> {
        validate_digest(digest)?;
        let key = format!("{}:{digest}", self.namespace);
        validate_cache_key(&key)?;
        Ok(key)
    }
}

impl fmt::Debug for RedisStickySessionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisStickySessionStore")
            .field("namespace", &"<已脱敏>")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

fn validate_digest(digest: &str) -> Result<(), CacheError> {
    if digest.len() != STICKY_DIGEST_BYTES || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CacheError::InvalidStickyDigest);
    }
    Ok(())
}

fn encode_channel_id(channel_id: i64) -> Result<String, CacheError> {
    if channel_id <= 0 || channel_id.to_string().len() > MAX_CHANNEL_ID_BYTES {
        return Err(CacheError::InvalidStickyChannelId);
    }
    Ok(channel_id.to_string())
}

fn parse_channel_id(value: &[u8]) -> Result<i64, CacheError> {
    if value.is_empty() || value.len() > MAX_CHANNEL_ID_BYTES {
        return Err(CacheError::InvalidStickyChannelId);
    }
    let value = std::str::from_utf8(value).map_err(|_| CacheError::InvalidStickyChannelId)?;
    let channel_id = value
        .parse::<i64>()
        .map_err(|_| CacheError::InvalidStickyChannelId)?;
    if channel_id <= 0 {
        return Err(CacheError::InvalidStickyChannelId);
    }
    Ok(channel_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_boundaries_reject_untrusted_digest_and_channel_values() {
        assert_eq!(
            validate_digest("short"),
            Err(CacheError::InvalidStickyDigest)
        );
        assert_eq!(
            encode_channel_id(0),
            Err(CacheError::InvalidStickyChannelId)
        );
        assert_eq!(
            parse_channel_id(b"-1"),
            Err(CacheError::InvalidStickyChannelId)
        );
        assert!(validate_digest(&"a".repeat(STICKY_DIGEST_BYTES)).is_ok());
        assert_eq!(parse_channel_id(b"41").unwrap(), 41);
    }

    #[test]
    fn debug_output_never_contains_dynamic_key_or_binding() {
        let config = RedisStickySessionConfig::new(
            RedisConfig::new("redis://user:secret@redis.internal:6379/0"),
            "private.sticky.v1",
        )
        .unwrap();
        let rendered = format!("{config:?}");
        for private in ["private.sticky.v1", "user", "secret", "redis.internal"] {
            assert!(!rendered.contains(private));
        }
    }
}
