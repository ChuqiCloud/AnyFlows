mod scripts;
mod types;

use std::{collections::BTreeSet, fmt, sync::Arc};

use af_domain::{ConcurrencyLimit, CredentialId, TokenId, UserId};

use crate::{
    CacheError, CacheOperation, RedisFailureKind, config::validate_cache_key,
    redis_backend::RedisBackend,
};
use scripts::{
    ACCOUNT_LOADS_SCRIPT, ACQUIRE_SCRIPT, CLEAN_KEY_SCRIPT, EXPIRED_KEYS_SCRIPT, RELEASE_SCRIPT,
    RENEW_SCRIPT,
};
use types::{RegistrationState, validate_bounded_ttl};

pub use types::{
    ConcurrencyAccountLoad, ConcurrencyAcquireOutcome, ConcurrencyCleanupReport,
    ConcurrencyLeaseOutcome, ConcurrencyWaitOutcome, DEFAULT_CONCURRENCY_SLOT_TTL,
    DEFAULT_CONCURRENCY_WAIT_TTL, MAX_CONCURRENCY_CLEANUP_BATCH_SIZE,
    MAX_CONCURRENCY_LOAD_BATCH_SIZE, RedisConcurrencyConfig,
};

const RANDOM_MEMBER_BYTES: usize = 16;
const ACQUIRE_OK: i64 = 0;
const ACQUIRE_LIMITED: i64 = 1;
const ACQUIRE_INCONSISTENT: i64 = 2;

/// 使用同一 Redis hash tag 原子维护账号、用户、令牌与等待 ZSET。
#[derive(Clone)]
pub struct RedisConcurrencyStore {
    backend: Arc<RedisBackend>,
    key_prefix: Arc<str>,
    active_index_key: Arc<str>,
    slot_ttl_millis: i64,
    wait_ttl_millis: i64,
}

impl RedisConcurrencyStore {
    /// 建立 Redis 连接并验证并发键空间配置。
    pub async fn connect(config: RedisConcurrencyConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let key_prefix = format!("{{{}}}:concurrency", config.namespace);
        validate_cache_key(&key_prefix)?;
        let active_index_key = format!("{key_prefix}:active");
        validate_cache_key(&active_index_key)?;
        Ok(Self {
            backend: Arc::new(RedisBackend::connect(&config.redis).await?),
            key_prefix: Arc::from(key_prefix),
            active_index_key: Arc::from(active_index_key),
            slot_ttl_millis: validate_bounded_ttl(config.slot_ttl)?,
            wait_ttl_millis: validate_bounded_ttl(config.wait_ttl)?,
        })
    }

    /// 原子获取用户限制槽位并同时追踪下游令牌；令牌层只统计、不设上限。
    pub async fn acquire_user_token(
        &self,
        user_id: UserId,
        user_limit: Option<ConcurrencyLimit>,
        token_id: TokenId,
    ) -> Result<ConcurrencyAcquireOutcome, CacheError> {
        self.acquire(
            vec![
                (self.slot_key("user", user_id.get())?, user_limit),
                (self.slot_key("token", token_id.get())?, None),
            ],
            self.slot_ttl_millis,
        )
        .await
    }

    /// 获取单个上游账号槽位；空限制仍写入 ZSET 供负载调度使用。
    pub async fn acquire_account(
        &self,
        credential_id: CredentialId,
        limit: Option<ConcurrencyLimit>,
    ) -> Result<ConcurrencyAcquireOutcome, CacheError> {
        self.acquire(
            vec![(self.slot_key("account", credential_id.get())?, limit)],
            self.slot_ttl_millis,
        )
        .await
    }

    /// 有界登记用户级等待成员；等待循环应定期续期该句柄。
    pub async fn enter_user_wait(
        &self,
        user_id: UserId,
        max_waiting: ConcurrencyLimit,
    ) -> Result<ConcurrencyWaitOutcome, CacheError> {
        self.enter_wait(self.wait_key("user", user_id.get())?, max_waiting)
            .await
    }

    /// 有界登记账号级等待成员；等待循环应定期续期该句柄。
    pub async fn enter_account_wait(
        &self,
        credential_id: CredentialId,
        max_waiting: ConcurrencyLimit,
    ) -> Result<ConcurrencyWaitOutcome, CacheError> {
        self.enter_wait(self.wait_key("account", credential_id.get())?, max_waiting)
            .await
    }

    /// 一次 Redis 调用读取最多 64 个账号的活跃与等待负载。
    pub async fn account_loads(
        &self,
        accounts: &[(CredentialId, Option<ConcurrencyLimit>)],
    ) -> Result<Vec<ConcurrencyAccountLoad>, CacheError> {
        if accounts.len() > MAX_CONCURRENCY_LOAD_BATCH_SIZE
            || accounts
                .iter()
                .map(|(credential_id, _)| credential_id.get())
                .collect::<BTreeSet<_>>()
                .len()
                != accounts.len()
        {
            return Err(CacheError::InvalidConcurrencyBatch);
        }
        if accounts.is_empty() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::with_capacity(1 + accounts.len() * 2);
        keys.push(self.active_index_key.to_string());
        for (credential_id, _) in accounts {
            keys.push(self.slot_key("account", credential_id.get())?);
        }
        for (credential_id, _) in accounts {
            keys.push(self.wait_key("account", credential_id.get())?);
        }
        let values: Vec<i64> = self
            .backend
            .query(
                CacheOperation::ConcurrencyLoad,
                redis::cmd("EVAL")
                    .arg(ACCOUNT_LOADS_SCRIPT)
                    .arg(keys.len())
                    .arg(&keys)
                    .arg(accounts.len())
                    .arg(self.slot_ttl_millis),
            )
            .await?;
        if values.len() != accounts.len() * 2 {
            return Err(protocol_error(CacheOperation::ConcurrencyLoad));
        }
        accounts
            .iter()
            .zip(values.chunks_exact(2))
            .map(|((credential_id, limit), counts)| {
                let active = u32::try_from(counts[0])
                    .map_err(|_| protocol_error(CacheOperation::ConcurrencyLoad))?;
                let waiting = u32::try_from(counts[1])
                    .map_err(|_| protocol_error(CacheOperation::ConcurrencyLoad))?;
                Ok(ConcurrencyAccountLoad::new(
                    *credential_id,
                    active,
                    waiting,
                    *limit,
                ))
            })
            .collect()
    }

    /// 清理活跃索引中已经到期的受控键，不扫描 Redis 全局键空间。
    pub async fn cleanup_expired(
        &self,
        max_keys: usize,
    ) -> Result<ConcurrencyCleanupReport, CacheError> {
        if !(1..=MAX_CONCURRENCY_CLEANUP_BATCH_SIZE).contains(&max_keys) {
            return Err(CacheError::InvalidConcurrencyBatch);
        }
        let keys: Vec<String> = self
            .backend
            .query(
                CacheOperation::ConcurrencyCleanup,
                redis::cmd("EVAL")
                    .arg(EXPIRED_KEYS_SCRIPT)
                    .arg(1)
                    .arg(self.active_index_key.as_ref())
                    .arg(max_keys),
            )
            .await?;
        let mut removed_members = 0_u64;
        for key in &keys {
            self.validate_indexed_key(key)?;
            let removed: i64 = self
                .backend
                .query(
                    CacheOperation::ConcurrencyCleanup,
                    redis::cmd("EVAL")
                        .arg(CLEAN_KEY_SCRIPT)
                        .arg(2)
                        .arg(self.active_index_key.as_ref())
                        .arg(key)
                        .arg(self.slot_ttl_millis),
                )
                .await?;
            removed_members = removed_members
                .checked_add(
                    u64::try_from(removed)
                        .map_err(|_| protocol_error(CacheOperation::ConcurrencyCleanup))?,
                )
                .ok_or_else(|| protocol_error(CacheOperation::ConcurrencyCleanup))?;
        }
        Ok(ConcurrencyCleanupReport::new(keys.len(), removed_members))
    }

    async fn enter_wait(
        &self,
        key: String,
        max_waiting: ConcurrencyLimit,
    ) -> Result<ConcurrencyWaitOutcome, CacheError> {
        match self
            .acquire(vec![(key, Some(max_waiting))], self.wait_ttl_millis)
            .await?
        {
            ConcurrencyAcquireOutcome::Acquired(lease) => {
                Ok(ConcurrencyWaitOutcome::Entered(RedisConcurrencyWait {
                    inner: lease.inner,
                }))
            }
            ConcurrencyAcquireOutcome::Limited => Ok(ConcurrencyWaitOutcome::Full),
        }
    }

    async fn acquire(
        &self,
        slots: Vec<(String, Option<ConcurrencyLimit>)>,
        ttl_millis: i64,
    ) -> Result<ConcurrencyAcquireOutcome, CacheError> {
        let member = random_member()?;
        let mut keys = Vec::with_capacity(slots.len() + 1);
        keys.push(self.active_index_key.to_string());
        keys.extend(slots.iter().map(|(key, _)| key.clone()));
        let mut command = redis::cmd("EVAL");
        command
            .arg(ACQUIRE_SCRIPT)
            .arg(keys.len())
            .arg(&keys)
            .arg(ttl_millis)
            .arg(member.as_str());
        for (_, limit) in &slots {
            command.arg(limit.map_or(0, ConcurrencyLimit::get));
        }
        let status: i64 = self
            .backend
            .query(CacheOperation::ConcurrencyAcquire, &mut command)
            .await?;
        match status {
            ACQUIRE_OK => Ok(ConcurrencyAcquireOutcome::Acquired(RedisConcurrencyLease {
                inner: RegistrationState {
                    store: self.clone(),
                    keys: Arc::from(keys.into_iter().skip(1).collect::<Vec<_>>()),
                    member: Arc::from(member),
                    ttl_millis,
                },
            })),
            ACQUIRE_LIMITED => Ok(ConcurrencyAcquireOutcome::Limited),
            ACQUIRE_INCONSISTENT => Err(protocol_error(CacheOperation::ConcurrencyAcquire)),
            _ => Err(protocol_error(CacheOperation::ConcurrencyAcquire)),
        }
    }

    async fn renew_registration(
        &self,
        state: &RegistrationState,
    ) -> Result<ConcurrencyLeaseOutcome, CacheError> {
        let keys = self.command_keys(&state.keys);
        let renewed: i64 = self
            .backend
            .query(
                CacheOperation::ConcurrencyRenew,
                redis::cmd("EVAL")
                    .arg(RENEW_SCRIPT)
                    .arg(keys.len())
                    .arg(&keys)
                    .arg(state.ttl_millis)
                    .arg(state.member.as_ref()),
            )
            .await?;
        match renewed {
            0 => Ok(ConcurrencyLeaseOutcome::Lost),
            1 => Ok(ConcurrencyLeaseOutcome::Applied),
            _ => Err(protocol_error(CacheOperation::ConcurrencyRenew)),
        }
    }

    async fn release_registration(
        &self,
        state: RegistrationState,
    ) -> Result<ConcurrencyLeaseOutcome, CacheError> {
        let keys = self.command_keys(&state.keys);
        let removed: i64 = self
            .backend
            .query(
                CacheOperation::ConcurrencyRelease,
                redis::cmd("EVAL")
                    .arg(RELEASE_SCRIPT)
                    .arg(keys.len())
                    .arg(&keys)
                    .arg(state.ttl_millis)
                    .arg(state.member.as_ref()),
            )
            .await?;
        let expected = i64::try_from(state.keys.len())
            .map_err(|_| protocol_error(CacheOperation::ConcurrencyRelease))?;
        match removed {
            value if value == expected => Ok(ConcurrencyLeaseOutcome::Applied),
            value if (0..expected).contains(&value) => Ok(ConcurrencyLeaseOutcome::Lost),
            _ => Err(protocol_error(CacheOperation::ConcurrencyRelease)),
        }
    }

    fn command_keys(&self, keys: &[String]) -> Vec<String> {
        let mut command_keys = Vec::with_capacity(keys.len() + 1);
        command_keys.push(self.active_index_key.to_string());
        command_keys.extend(keys.iter().cloned());
        command_keys
    }

    fn slot_key(&self, kind: &str, id: i64) -> Result<String, CacheError> {
        self.dynamic_key("slot", kind, id)
    }

    fn wait_key(&self, kind: &str, id: i64) -> Result<String, CacheError> {
        self.dynamic_key("wait", kind, id)
    }

    fn dynamic_key(&self, family: &str, kind: &str, id: i64) -> Result<String, CacheError> {
        if id <= 0 {
            return Err(CacheError::InvalidConcurrencyIdentifier);
        }
        let key = format!("{}:{family}:{kind}:{id}", self.key_prefix);
        validate_cache_key(&key)?;
        Ok(key)
    }

    fn validate_indexed_key(&self, key: &str) -> Result<(), CacheError> {
        validate_cache_key(key)?;
        let prefix = format!("{}:", self.key_prefix);
        if !key.starts_with(&prefix)
            || !(key.contains(":slot:account:")
                || key.contains(":slot:user:")
                || key.contains(":slot:token:")
                || key.contains(":wait:account:")
                || key.contains(":wait:user:"))
        {
            return Err(protocol_error(CacheOperation::ConcurrencyCleanup));
        }
        Ok(())
    }
}

impl fmt::Debug for RedisConcurrencyStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisConcurrencyStore")
            .field("namespace", &"<已脱敏>")
            .field("slot_ttl_millis", &self.slot_ttl_millis)
            .field("wait_ttl_millis", &self.wait_ttl_millis)
            .finish_non_exhaustive()
    }
}

/// 已取得的请求槽位；调用方负责在请求或流结束时显式释放。
pub struct RedisConcurrencyLease {
    inner: RegistrationState,
}

impl RedisConcurrencyLease {
    /// 原子续期全部层级；任一成员丢失时清理残余并返回 `Lost`。
    pub async fn renew(&self) -> Result<ConcurrencyLeaseOutcome, CacheError> {
        self.inner.store.renew_registration(&self.inner).await
    }

    /// 仅删除当前随机成员，不会影响并发接管后的其他请求。
    pub async fn release(self) -> Result<ConcurrencyLeaseOutcome, CacheError> {
        self.inner
            .store
            .clone()
            .release_registration(self.inner)
            .await
    }
}

impl fmt::Debug for RedisConcurrencyLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedisConcurrencyLease(<已脱敏>)")
    }
}

/// 已登记的等待成员；续期可避免长队列中的计数提前过期。
pub struct RedisConcurrencyWait {
    inner: RegistrationState,
}

impl RedisConcurrencyWait {
    /// 刷新等待登记 TTL；成员已过期时不会重新创建。
    pub async fn renew(&self) -> Result<ConcurrencyLeaseOutcome, CacheError> {
        self.inner.store.renew_registration(&self.inner).await
    }

    /// 离开等待队列并条件删除自己的成员。
    pub async fn leave(self) -> Result<ConcurrencyLeaseOutcome, CacheError> {
        self.inner
            .store
            .clone()
            .release_registration(self.inner)
            .await
    }
}

impl fmt::Debug for RedisConcurrencyWait {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedisConcurrencyWait(<已脱敏>)")
    }
}

fn random_member() -> Result<String, CacheError> {
    let mut bytes = [0_u8; RANDOM_MEMBER_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| CacheError::EntropyUnavailable)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(RANDOM_MEMBER_BYTES * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(output)
}

fn protocol_error(operation: CacheOperation) -> CacheError {
    CacheError::redis(operation, RedisFailureKind::Protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_and_debug_keep_namespace_private() {
        let config = RedisConcurrencyConfig::new(
            crate::RedisConfig::new("redis://user:secret@redis.internal:6379/0"),
            "private.concurrency.v1",
        )
        .unwrap();
        let rendered = format!("{config:?}");
        for private in ["private.concurrency.v1", "user", "secret", "redis.internal"] {
            assert!(!rendered.contains(private));
        }
    }

    #[test]
    fn random_members_are_fixed_width_lowercase_hex() {
        let first = random_member().unwrap();
        let second = random_member().unwrap();
        assert_eq!(first.len(), RANDOM_MEMBER_BYTES * 2);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        assert_ne!(first, second);
    }
}
