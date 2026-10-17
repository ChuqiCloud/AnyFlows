//! Bounded ephemeral node registry. Redis time avoids cross-host clock skew.

use crate::{
    CacheError, CacheOperation, RedisConfig, config::validate_namespace,
    redis_backend::RedisBackend,
};

const USAGE_LEASE_TTL_MS: i64 = 55_000;
const USAGE_CACHE_TTL_MS: i64 = 130_000;

const EXCHANGE: &str = r#"
local now = tonumber(redis.call('TIME')[1])
local expired = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', now - 300, 'LIMIT', 0, 256)
for _, id in ipairs(expired) do
    redis.call('ZREM', KEYS[1], id)
    redis.call('HDEL', KEYS[2], id)
end
local admitted = redis.call('ZSCORE', KEYS[1], ARGV[1]) or redis.call('ZCARD', KEYS[1]) < 256
if admitted then
    redis.call('ZADD', KEYS[1], now, ARGV[1])
    redis.call('HSET', KEYS[2], ARGV[1], ARGV[2])
end
redis.call('EXPIRE', KEYS[1], 600)
redis.call('EXPIRE', KEYS[2], 600)
local result = {admitted and 1 or 0}
local ids = redis.call('ZRANGE', KEYS[1], 0, 255, 'WITHSCORES')
for i = 1, #ids, 2 do
    local value = redis.call('HGET', KEYS[2], ids[i])
    if value and string.len(value) <= 16384 then
        table.insert(result, {ids[i], now - tonumber(ids[i + 1]), value})
    end
end
return result
"#;

/// One registry entry and its age according to the shared Redis clock.
pub struct RedisNodeEntry {
    pub instance_id: String,
    pub age_seconds: u64,
    pub payload: Vec<u8>,
}

/// Registry exchange response, bounded to 256 nodes and 16 KiB per node.
pub struct RedisNodeExchange {
    pub admitted: bool,
    pub entries: Vec<RedisNodeEntry>,
}

/// Stores no credentials, request content, or permanent node records.
pub struct RedisNodeRegistry {
    backend: RedisBackend,
    namespace: String,
    heartbeat_key: String,
    payload_key: String,
}

impl RedisNodeRegistry {
    pub async fn connect(config: RedisConfig, namespace: &str) -> Result<Self, CacheError> {
        validate_namespace(namespace)?;
        config.validate()?;
        Ok(Self {
            backend: RedisBackend::connect(&config).await?,
            namespace: namespace.to_owned(),
            heartbeat_key: format!("{{{namespace}}}:nodes:heartbeat"),
            payload_key: format!("{{{namespace}}}:nodes:payload"),
        })
    }

    pub async fn exchange(
        &self,
        instance_id: &str,
        payload: &[u8],
    ) -> Result<RedisNodeExchange, CacheError> {
        if instance_id.len() != 32 || !instance_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CacheError::InvalidKey);
        }
        if payload.len() > 16_384 {
            return Err(CacheError::ValueTooLarge);
        }
        // Redis Lua arrays may contain mixed values. Decode through the driver's Value type.
        let values: Vec<redis::Value> = self
            .backend
            .query(
                CacheOperation::Write,
                redis::cmd("EVAL")
                    .arg(EXCHANGE)
                    .arg(2)
                    .arg(&self.heartbeat_key)
                    .arg(&self.payload_key)
                    .arg(instance_id)
                    .arg(payload),
            )
            .await?;
        let admitted = values
            .first()
            .and_then(|v| redis::from_redis_value::<i64>(v.clone()).ok())
            == Some(1);
        let mut entries = Vec::new();
        for value in values.into_iter().skip(1).take(256) {
            if let Ok((instance_id, age_seconds, payload)) =
                redis::from_redis_value::<(String, u64, Vec<u8>)>(value)
            {
                entries.push(RedisNodeEntry {
                    instance_id,
                    age_seconds,
                    payload,
                });
            }
        }
        Ok(RedisNodeExchange { admitted, entries })
    }

    /// Claims the short-lived cluster lease used for the once-per-minute usage scan.
    pub async fn claim_usage_refresh(&self) -> Result<bool, CacheError> {
        let key = format!("{}:usage:refresh", self.namespace_tag());
        let result: Option<String> = self
            .backend
            .query(
                CacheOperation::LeaseAcquire,
                redis::cmd("SET")
                    .arg(&key)
                    .arg("1")
                    .arg("NX")
                    .arg("PX")
                    .arg(USAGE_LEASE_TTL_MS),
            )
            .await?;
        Ok(result.is_some())
    }

    /// Publishes a bounded usage snapshot with an explicit expiry.
    pub async fn publish_usage(&self, payload: &[u8]) -> Result<(), CacheError> {
        if payload.is_empty() || payload.len() > 16_384 {
            return Err(CacheError::ValueTooLarge);
        }
        let key = format!("{}:usage:snapshot", self.namespace_tag());
        let _: () = self
            .backend
            .query(
                CacheOperation::Publish,
                redis::cmd("PSETEX")
                    .arg(key)
                    .arg(USAGE_CACHE_TTL_MS)
                    .arg(payload),
            )
            .await?;
        Ok(())
    }

    /// Reads the shared usage snapshot; expiry is enforced by Redis.
    pub async fn read_usage(&self) -> Result<Option<Vec<u8>>, CacheError> {
        let key = format!("{}:usage:snapshot", self.namespace_tag());
        self.backend
            .query(CacheOperation::Read, redis::cmd("GET").arg(key))
            .await
    }

    fn namespace_tag(&self) -> &str {
        &self.namespace
    }
}
