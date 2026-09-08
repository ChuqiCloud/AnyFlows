use std::{fmt, num::NonZeroUsize, sync::Arc};

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind,
    config::{DEFAULT_MAX_VALUE_BYTES, validate_cache_key, validate_namespace},
    redis_backend::RedisBackend,
};

const VERSION_WIDTH: usize = 20;
const PUT_IF_NEWER_SCRIPT: &str = r#"
local current = redis.call('HGET', KEYS[1], 'version')
if current then
    if string.len(current) ~= 20 or string.find(current, '%D') then
        return -1
    end
    if current >= ARGV[1] then
        return 0
    end
end
redis.call('HSET', KEYS[1], 'version', ARGV[1], 'payload', ARGV[2])
return 1
"#;

/// 版本化 Redis 投影的键空间与单值容量配置。
#[derive(Clone)]
pub struct RedisProjectionConfig {
    redis: RedisConfig,
    namespace: String,
    max_payload_bytes: NonZeroUsize,
}

impl RedisProjectionConfig {
    /// 创建使用独立命名空间的版本化投影配置。
    pub fn new(redis: RedisConfig, namespace: impl Into<String>) -> Result<Self, CacheError> {
        let config = Self {
            redis,
            namespace: namespace.into(),
            max_payload_bytes: NonZeroUsize::new(DEFAULT_MAX_VALUE_BYTES)
                .expect("默认投影容量必须为正数"),
        };
        config.validate()?;
        Ok(config)
    }

    /// 覆盖单个主体投影的非零字节上限。
    pub fn with_max_payload_bytes(mut self, max_payload_bytes: usize) -> Result<Self, CacheError> {
        self.max_payload_bytes =
            NonZeroUsize::new(max_payload_bytes).ok_or(CacheError::InvalidMaxValueSize)?;
        if max_payload_bytes as u128 > i64::MAX as u128 {
            return Err(CacheError::InvalidMaxValueSize);
        }
        Ok(self)
    }

    fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.namespace)?;
        self.redis.validate()
    }
}

impl fmt::Debug for RedisProjectionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisProjectionConfig")
            .field("redis", &self.redis)
            .field("namespace", &self.namespace)
            .field("max_payload_bytes", &self.max_payload_bytes)
            .finish()
    }
}

/// 原子版本比较写入的结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionWriteOutcome {
    /// 传入版本大于 Redis 当前版本，投影已原子替换。
    Applied,
    /// Redis 已存在相同或更新版本，旧投影未覆盖当前事实。
    Stale,
}

/// 从 Redis 原子读取的版本化投影正文。
pub struct RedisProjectionEntry {
    version: u64,
    payload: Vec<u8>,
}

impl RedisProjectionEntry {
    /// 返回投影的主体单调版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回受容量保护的投影正文；调用方不得写入日志。
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl fmt::Debug for RedisProjectionEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisProjectionEntry")
            .field("version", &self.version)
            .field("payload_bytes", &self.payload.len())
            .finish()
    }
}

/// 使用 Lua/CAS 防止旧事件覆盖新主体事实的 Redis 投影存储。
#[derive(Clone)]
pub struct RedisVersionedProjectionStore {
    backend: Arc<RedisBackend>,
    namespace: Arc<str>,
    max_payload_bytes: usize,
}

impl RedisVersionedProjectionStore {
    /// 建立并验证 Redis 连接；配置后连接失败必须阻止调用方继续装配。
    pub async fn connect(config: RedisProjectionConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let backend = Arc::new(RedisBackend::connect(&config.redis).await?);
        Ok(Self {
            backend,
            namespace: Arc::from(config.namespace),
            max_payload_bytes: config.max_payload_bytes.get(),
        })
    }

    /// 仅当传入版本严格更新时原子替换主体投影。
    pub async fn put_if_newer(
        &self,
        subject_key: &str,
        version: u64,
        payload: &[u8],
    ) -> Result<ProjectionWriteOutcome, CacheError> {
        validate_version(version)?;
        validate_payload(payload, self.max_payload_bytes)?;
        let key = self.namespaced_key(subject_key)?;
        let encoded_version = encode_version(version);
        let status: i64 = self
            .backend
            .query(
                CacheOperation::ProjectionWrite,
                redis::cmd("EVAL")
                    .arg(PUT_IF_NEWER_SCRIPT)
                    .arg(1)
                    .arg(key)
                    .arg(encoded_version)
                    .arg(payload),
            )
            .await?;
        match status {
            1 => Ok(ProjectionWriteOutcome::Applied),
            0 => Ok(ProjectionWriteOutcome::Stale),
            _ => Err(CacheError::redis(
                CacheOperation::ProjectionWrite,
                RedisFailureKind::Protocol,
            )),
        }
    }

    /// 原子读取主体版本与正文；缺少任一 hash 字段都视为 Redis 状态损坏。
    pub async fn get(&self, subject_key: &str) -> Result<Option<RedisProjectionEntry>, CacheError> {
        let key = self.namespaced_key(subject_key)?;
        let (version, payload): (Option<String>, Option<Vec<u8>>) = self
            .backend
            .query(
                CacheOperation::ProjectionRead,
                redis::cmd("HMGET").arg(key).arg("version").arg("payload"),
            )
            .await?;
        match (version, payload) {
            (None, None) => Ok(None),
            (Some(version), Some(payload)) => {
                let version = decode_version(&version)?;
                validate_payload(&payload, self.max_payload_bytes)?;
                Ok(Some(RedisProjectionEntry { version, payload }))
            }
            _ => Err(CacheError::redis(
                CacheOperation::ProjectionRead,
                RedisFailureKind::Protocol,
            )),
        }
    }

    fn namespaced_key(&self, subject_key: &str) -> Result<String, CacheError> {
        build_namespaced_key(&self.namespace, subject_key)
    }
}

impl fmt::Debug for RedisVersionedProjectionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisVersionedProjectionStore")
            .field("namespace", &self.namespace)
            .field("max_payload_bytes", &self.max_payload_bytes)
            .finish_non_exhaustive()
    }
}

fn validate_version(version: u64) -> Result<(), CacheError> {
    if version == 0 {
        return Err(CacheError::InvalidProjectionVersion);
    }
    Ok(())
}

fn encode_version(version: u64) -> String {
    format!("{version:0width$}", width = VERSION_WIDTH)
}

fn decode_version(encoded: &str) -> Result<u64, CacheError> {
    if encoded.len() != VERSION_WIDTH || !encoded.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CacheError::redis(
            CacheOperation::ProjectionRead,
            RedisFailureKind::Protocol,
        ));
    }
    let version = encoded.parse().map_err(|_| {
        CacheError::redis(CacheOperation::ProjectionRead, RedisFailureKind::Protocol)
    })?;
    validate_version(version)?;
    Ok(version)
}

fn validate_payload(payload: &[u8], max_payload_bytes: usize) -> Result<(), CacheError> {
    if payload.is_empty() {
        return Err(CacheError::InvalidProjectionPayload);
    }
    if payload.len() > max_payload_bytes {
        return Err(CacheError::ValueTooLarge);
    }
    Ok(())
}

fn build_namespaced_key(namespace: &str, subject_key: &str) -> Result<String, CacheError> {
    validate_cache_key(subject_key)?;
    let key = format!("{namespace}:{subject_key}");
    validate_cache_key(&key)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_width_versions_preserve_full_u64_order() {
        let versions = [1, (1_u64 << 53) + 1, u64::MAX];
        let encoded = versions.map(encode_version);
        assert!(encoded[0] < encoded[1]);
        assert!(encoded[1] < encoded[2]);
        for (version, encoded) in versions.into_iter().zip(encoded) {
            assert_eq!(decode_version(&encoded).unwrap(), version);
        }
    }

    #[test]
    fn projection_boundaries_reject_empty_values_and_invalid_keys() {
        assert_eq!(
            validate_version(0),
            Err(CacheError::InvalidProjectionVersion)
        );
        assert_eq!(
            validate_payload(&[], 8),
            Err(CacheError::InvalidProjectionPayload)
        );
        assert_eq!(
            build_namespaced_key("scheduler.projection.v1", "bad\nkey"),
            Err(CacheError::InvalidKey)
        );
    }
}
