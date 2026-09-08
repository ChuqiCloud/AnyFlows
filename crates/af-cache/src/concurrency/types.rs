use std::{fmt, sync::Arc, time::Duration};

use af_domain::{ConcurrencyLimit, CredentialId};

use crate::{
    CacheError, RedisConfig,
    config::{validate_namespace, validate_ttl},
};

/// 活跃请求槽位默认最多保留二十分钟，覆盖渠道允许的十五分钟请求硬期限及收尾余量。
pub const DEFAULT_CONCURRENCY_SLOT_TTL: Duration = Duration::from_secs(20 * 60);
/// 等待登记默认保留一分钟；等待循环每次重试都会续期。
pub const DEFAULT_CONCURRENCY_WAIT_TTL: Duration = Duration::from_secs(60);
/// 单次批量账号负载读取的硬上限，与运行时凭据池容量一致。
pub const MAX_CONCURRENCY_LOAD_BATCH_SIZE: usize = 64;
/// 单轮孤儿清理最多处理的键数量。
pub const MAX_CONCURRENCY_CLEANUP_BATCH_SIZE: usize = 256;
const MAX_CONCURRENCY_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Redis 并发槽位的键空间与租约期限配置。
#[derive(Clone)]
pub struct RedisConcurrencyConfig {
    pub(super) redis: RedisConfig,
    pub(super) namespace: String,
    pub(super) slot_ttl: Duration,
    pub(super) wait_ttl: Duration,
}

impl RedisConcurrencyConfig {
    /// 创建默认二十分钟槽位和一分钟等待登记配置。
    pub fn new(redis: RedisConfig, namespace: impl Into<String>) -> Result<Self, CacheError> {
        let config = Self {
            redis,
            namespace: namespace.into(),
            slot_ttl: DEFAULT_CONCURRENCY_SLOT_TTL,
            wait_ttl: DEFAULT_CONCURRENCY_WAIT_TTL,
        };
        config.validate()?;
        Ok(config)
    }

    /// 覆盖槽位与等待登记 TTL；两者必须为整毫秒且不超过一天。
    pub fn with_ttls(mut self, slot_ttl: Duration, wait_ttl: Duration) -> Result<Self, CacheError> {
        self.slot_ttl = slot_ttl;
        self.wait_ttl = wait_ttl;
        self.validate()?;
        Ok(self)
    }

    pub(super) fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.namespace)?;
        validate_bounded_ttl(self.slot_ttl)?;
        validate_bounded_ttl(self.wait_ttl)?;
        self.redis.validate()
    }
}

impl fmt::Debug for RedisConcurrencyConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisConcurrencyConfig")
            .field("redis", &self.redis)
            .field("namespace", &"<已脱敏>")
            .field("slot_ttl", &self.slot_ttl)
            .field("wait_ttl", &self.wait_ttl)
            .finish()
    }
}

pub(super) fn validate_bounded_ttl(ttl: Duration) -> Result<i64, CacheError> {
    let ttl_millis = validate_ttl(ttl)?;
    if ttl > MAX_CONCURRENCY_TTL {
        return Err(CacheError::InvalidTtl);
    }
    Ok(ttl_millis)
}

/// 一次原子槽位竞争结果。
pub enum ConcurrencyAcquireOutcome {
    /// 已取得全部请求层级的槽位。
    Acquired(super::RedisConcurrencyLease),
    /// 至少一个受限层级已经达到并发上限。
    Limited,
}

impl fmt::Debug for ConcurrencyAcquireOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Acquired(_) => {
                formatter.write_str("ConcurrencyAcquireOutcome::Acquired(<已脱敏>)")
            }
            Self::Limited => formatter.write_str("ConcurrencyAcquireOutcome::Limited"),
        }
    }
}

/// 一次等待队列登记结果。
pub enum ConcurrencyWaitOutcome {
    /// 已登记等待成员并取得可续期句柄。
    Entered(super::RedisConcurrencyWait),
    /// 当前等待人数已经达到有界上限。
    Full,
}

impl fmt::Debug for ConcurrencyWaitOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entered(_) => formatter.write_str("ConcurrencyWaitOutcome::Entered(<已脱敏>)"),
            Self::Full => formatter.write_str("ConcurrencyWaitOutcome::Full"),
        }
    }
}

/// 显式释放或续期时的所有权结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcurrencyLeaseOutcome {
    /// 当前成员仍存在，操作已作用于自己的槽位。
    Applied,
    /// 成员已经过期、被清理或状态不完整，未破坏其他请求。
    Lost,
}

/// 单个账号的实时槽位与等待负载。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConcurrencyAccountLoad {
    credential_id: CredentialId,
    active: u32,
    waiting: u32,
    limit: Option<ConcurrencyLimit>,
}

impl ConcurrencyAccountLoad {
    pub(super) const fn new(
        credential_id: CredentialId,
        active: u32,
        waiting: u32,
        limit: Option<ConcurrencyLimit>,
    ) -> Self {
        Self {
            credential_id,
            active,
            waiting,
            limit,
        }
    }

    /// 返回账号凭据标识。
    #[must_use]
    pub const fn credential_id(self) -> CredentialId {
        self.credential_id
    }

    /// 返回仍在 TTL 内的活跃请求数。
    #[must_use]
    pub const fn active(self) -> u32 {
        self.active
    }

    /// 返回仍在 TTL 内的等待请求数。
    #[must_use]
    pub const fn waiting(self) -> u32 {
        self.waiting
    }

    /// 返回当前运行时快照中的账号并发上限。
    #[must_use]
    pub const fn limit(self) -> Option<ConcurrencyLimit> {
        self.limit
    }
}

/// 一轮过期成员清理报告。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConcurrencyCleanupReport {
    inspected_keys: usize,
    removed_members: u64,
}

impl ConcurrencyCleanupReport {
    pub(super) const fn new(inspected_keys: usize, removed_members: u64) -> Self {
        Self {
            inspected_keys,
            removed_members,
        }
    }

    /// 返回本轮检查的受控槽位键数量。
    #[must_use]
    pub const fn inspected_keys(self) -> usize {
        self.inspected_keys
    }

    /// 返回本轮删除的已过期成员数量。
    #[must_use]
    pub const fn removed_members(self) -> u64 {
        self.removed_members
    }
}

pub(super) struct RegistrationState {
    pub(super) store: super::RedisConcurrencyStore,
    pub(super) keys: Arc<[String]>,
    pub(super) member: Arc<str>,
    pub(super) ttl_millis: i64,
}
