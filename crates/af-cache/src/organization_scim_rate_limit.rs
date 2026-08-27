use std::sync::Arc;

use af_domain::{
    OrganizationScimRateLimitDecision, OrganizationScimRateLimitFuture,
    OrganizationScimRateLimitRejection, OrganizationScimRateLimitRule,
    OrganizationScimRateLimitStore, OrganizationScimRateLimitSubject,
};

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind, config::validate_cache_key,
    redis_backend::RedisBackend,
};

const ADMIT_SCRIPT: &str = r#"
redis.replicate_commands()
local now = tonumber(redis.call('TIME')[1])
if #KEYS == 0 or #ARGV ~= (#KEYS * 2) then return {2, 0, 0, 0} end
local buckets, counts, resets = {}, {}, {}
local remaining = 2147483647
for index = 1, #KEYS do
  local arg = (index - 1) * 2
  local limit, window = tonumber(ARGV[arg + 1]), tonumber(ARGV[arg + 2])
  if not limit or not window or limit <= 0 or window <= 0 or limit % 1 ~= 0 or window % 1 ~= 0 then
    return {2, index, 0, 0}
  end
  local bucket = math.floor(now / window)
  local reset = (bucket + 1) * window
  local stored_bucket, stored_count = redis.call('HMGET', KEYS[index], 'bucket', 'count')[1], redis.call('HGET', KEYS[index], 'count')
  local count = 0
  if stored_bucket ~= false or stored_count ~= false then
    if stored_bucket == false or stored_count == false then return {2, index, 0, 0} end
    stored_bucket, stored_count = tonumber(stored_bucket), tonumber(stored_count)
    if not stored_bucket or not stored_count or stored_bucket % 1 ~= 0 or stored_count % 1 ~= 0 or stored_bucket < 0 or stored_count < 0 or stored_bucket > bucket then return {2, index, 0, 0} end
    if stored_bucket == bucket then count = stored_count end
  end
  if count >= limit then return {1, index, reset - now, reset} end
  buckets[index], counts[index], resets[index] = bucket, count, reset
  remaining = math.min(remaining, limit - count - 1)
end
for index = 1, #KEYS do
  redis.call('HSET', KEYS[index], 'bucket', buckets[index], 'count', counts[index] + 1)
  redis.call('EXPIREAT', KEYS[index], resets[index] + 1)
end
return {0, 0, remaining, resets[1]}
"#;

/// SCIM 企业/令牌固定窗口的 Redis 原子适配器。
#[derive(Clone)]
pub struct RedisOrganizationScimRateLimitStore {
    backend: Arc<RedisBackend>,
    key_prefix: Arc<str>,
}

impl RedisOrganizationScimRateLimitStore {
    /// 建立 Redis 连接并校验 SCIM 限流命名空间。
    pub async fn connect(
        config: RedisConfig,
        namespace: impl Into<String>,
    ) -> Result<Self, CacheError> {
        let namespace = namespace.into();
        let key_prefix = format!("{{{namespace}}}:organization-scim-rate-limit");
        validate_cache_key(&key_prefix)?;
        Ok(Self {
            backend: Arc::new(RedisBackend::connect(&config).await?),
            key_prefix: Arc::from(key_prefix),
        })
    }

    fn key(&self, rule: OrganizationScimRateLimitRule) -> Result<String, CacheError> {
        let subject = rule.subject();
        let suffix = match subject {
            OrganizationScimRateLimitSubject::Organization(id) => {
                format!("organization:{}:w{}", id.get(), rule.window().seconds())
            }
            OrganizationScimRateLimitSubject::Token {
                organization_id,
                token_key,
            } => format!(
                "token:{}:{}:w{}",
                organization_id.get(),
                token_key.as_str(),
                rule.window().seconds()
            ),
        };
        let key = format!("{}:{suffix}", self.key_prefix);
        validate_cache_key(&key)?;
        Ok(key)
    }
}

impl OrganizationScimRateLimitStore for RedisOrganizationScimRateLimitStore {
    fn admit<'a>(
        &'a self,
        rules: &'a [OrganizationScimRateLimitRule],
    ) -> OrganizationScimRateLimitFuture<'a> {
        Box::pin(async move {
            if rules.is_empty() || rules.len() > 2 {
                return OrganizationScimRateLimitDecision::Rejected(
                    OrganizationScimRateLimitRejection::InvalidFact,
                );
            }
            let keys = match rules
                .iter()
                .copied()
                .map(|rule| self.key(rule))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(keys) => keys,
                Err(_) => {
                    return OrganizationScimRateLimitDecision::Rejected(
                        OrganizationScimRateLimitRejection::InvalidFact,
                    );
                }
            };
            let mut command = redis::cmd("EVAL");
            command.arg(ADMIT_SCRIPT).arg(keys.len()).arg(&keys);
            for rule in rules {
                command.arg(rule.limit().get()).arg(rule.window().seconds());
            }
            let response: Result<(i64, i64, i64, i64), CacheError> = self
                .backend
                .query(CacheOperation::RateLimitCheck, &mut command)
                .await;
            let (status, index, retry_after, reset_at) = match response {
                Ok(value) => value,
                Err(error) => {
                    let reason = match error {
                        CacheError::Redis {
                            kind: RedisFailureKind::Protocol,
                            ..
                        } => OrganizationScimRateLimitRejection::InvalidFact,
                        _ => OrganizationScimRateLimitRejection::StorageUnavailable,
                    };
                    return OrganizationScimRateLimitDecision::Rejected(reason);
                }
            };
            match status {
                0 if index == 0 && retry_after >= 0 && reset_at > 0 => {
                    OrganizationScimRateLimitDecision::Admitted {
                        remaining: u32::try_from(retry_after).unwrap_or(0),
                        reset_at_unix_seconds: reset_at,
                    }
                }
                1 if index > 0 && retry_after > 0 && reset_at > 0 => {
                    OrganizationScimRateLimitDecision::Limited {
                        retry_after_seconds: u32::try_from(retry_after).unwrap_or(0),
                        reset_at_unix_seconds: reset_at,
                    }
                }
                _ => OrganizationScimRateLimitDecision::Rejected(
                    OrganizationScimRateLimitRejection::InvalidFact,
                ),
            }
        })
    }
}

impl std::fmt::Debug for RedisOrganizationScimRateLimitStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RedisOrganizationScimRateLimitStore(<redacted>)")
    }
}
