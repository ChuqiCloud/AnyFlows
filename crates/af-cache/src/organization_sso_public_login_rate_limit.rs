use std::sync::Arc;

use af_domain::{
    ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IDENTIFIER,
    ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IP,
    ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_WINDOW, OrganizationSsoPublicLoginRateLimitDecision,
    OrganizationSsoPublicLoginRateLimitFuture, OrganizationSsoPublicLoginRateLimitKey,
    OrganizationSsoPublicLoginRateLimitStore,
};

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind, config::validate_cache_key,
    redis_backend::RedisBackend,
};

const ADMIT_SCRIPT: &str = r#"
redis.replicate_commands()
if #KEYS ~= 2 or #ARGV ~= 4 then return {2, 0} end
local now = tonumber(redis.call('TIME')[1])
local buckets, counts, resets = {}, {}, {}
for index = 1, 2 do
  local arg = (index - 1) * 2
  local limit, window = tonumber(ARGV[arg + 1]), tonumber(ARGV[arg + 2])
  if not limit or not window or limit <= 0 or window <= 0 or limit % 1 ~= 0 or window % 1 ~= 0 then return {2, index} end
  local bucket = math.floor(now / window)
  local reset = (bucket + 1) * window
  local stored = redis.call('HMGET', KEYS[index], 'bucket', 'count')
  local count = 0
  if stored[1] ~= false or stored[2] ~= false then
    if stored[1] == false or stored[2] == false then return {2, index} end
    local stored_bucket, stored_count = tonumber(stored[1]), tonumber(stored[2])
    if not stored_bucket or not stored_count or stored_bucket % 1 ~= 0 or stored_count % 1 ~= 0 or stored_bucket < 0 or stored_count < 0 or stored_bucket > bucket then return {2, index} end
    if stored_bucket == bucket then count = stored_count end
  end
  if count >= limit then return {1, reset - now} end
  buckets[index], counts[index], resets[index] = bucket, count, reset
end
for index = 1, 2 do
  redis.call('HSET', KEYS[index], 'bucket', buckets[index], 'count', counts[index] + 1)
  redis.call('EXPIREAT', KEYS[index], resets[index] + 1)
end
return {0, 0}
"#;

/// 企业 SSO 公开启动的 Redis 原子组合限流适配器。
#[derive(Clone)]
pub struct RedisOrganizationSsoPublicLoginRateLimitStore {
    backend: Arc<RedisBackend>,
    key_prefix: Arc<str>,
}

impl RedisOrganizationSsoPublicLoginRateLimitStore {
    /// 建立 Redis 连接并校验公开登录限流命名空间。
    pub async fn connect(
        config: RedisConfig,
        namespace: impl Into<String>,
    ) -> Result<Self, CacheError> {
        config.validate()?;
        let namespace = namespace.into();
        let key_prefix = format!("{{{namespace}}}:organization-sso-public-login-rate-limit");
        validate_cache_key(&key_prefix)?;
        Ok(Self {
            backend: Arc::new(RedisBackend::connect(&config).await?),
            key_prefix: Arc::from(key_prefix),
        })
    }

    fn key(&self, kind: &str, fingerprint: [u8; 32]) -> Result<String, CacheError> {
        let mut suffix = String::with_capacity(66);
        suffix.push_str(kind);
        suffix.push(':');
        for byte in fingerprint {
            suffix.push_str(&format!("{byte:02x}"));
        }
        let key = format!("{}:{suffix}", self.key_prefix);
        validate_cache_key(&key)?;
        Ok(key)
    }
}

impl OrganizationSsoPublicLoginRateLimitStore for RedisOrganizationSsoPublicLoginRateLimitStore {
    fn admit<'a>(
        &'a self,
        key: OrganizationSsoPublicLoginRateLimitKey,
    ) -> OrganizationSsoPublicLoginRateLimitFuture<'a> {
        Box::pin(async move {
            let keys = match [
                self.key("ip", key.client_ip_fingerprint()),
                self.key("identifier", key.identifier_fingerprint()),
            ]
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            {
                Ok(keys) => keys,
                Err(_) => return OrganizationSsoPublicLoginRateLimitDecision::Rejected,
            };
            let window = ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_WINDOW.as_secs();
            let mut command = redis::cmd("EVAL");
            command
                .arg(ADMIT_SCRIPT)
                .arg(2)
                .arg(&keys)
                .arg(ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IP)
                .arg(window)
                .arg(ORGANIZATION_SSO_PUBLIC_LOGIN_RATE_LIMIT_PER_IDENTIFIER)
                .arg(window);
            let response: Result<(i64, i64), CacheError> = self
                .backend
                .query(CacheOperation::RateLimitCheck, &mut command)
                .await;
            match response {
                Ok((0, 0)) => OrganizationSsoPublicLoginRateLimitDecision::Admitted,
                Ok((1, retry_after)) if retry_after > 0 => match u32::try_from(retry_after) {
                    Ok(retry_after_seconds) => {
                        OrganizationSsoPublicLoginRateLimitDecision::Limited {
                            retry_after_seconds,
                        }
                    }
                    Err(_) => OrganizationSsoPublicLoginRateLimitDecision::Rejected,
                },
                Ok(_) => OrganizationSsoPublicLoginRateLimitDecision::Rejected,
                Err(CacheError::Redis {
                    kind: RedisFailureKind::Protocol,
                    ..
                })
                | Err(_) => OrganizationSsoPublicLoginRateLimitDecision::Rejected,
            }
        })
    }
}

impl std::fmt::Debug for RedisOrganizationSsoPublicLoginRateLimitStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RedisOrganizationSsoPublicLoginRateLimitStore(<redacted>)")
    }
}
