use std::{fmt, time::Duration};

use af_domain::{ChannelId, CredentialId};

use crate::{CacheError, CacheOperation, RedisConfig, RedisFailureKind, config::validate_ttl};

/// 惩罚分使用百万分之一固定精度，避免跨 Redis 与 Rust 边界传播浮点状态。
pub const HEALTH_PENALTY_SCALE: u64 = 1_000_000;
/// 单次健康读取或反馈批次的硬上限，覆盖 64 个 Relay 候选的渠道与凭据双层状态。
pub const MAX_HEALTH_BATCH_SIZE: usize = 128;

const DEFAULT_HALF_LIFE: Duration = Duration::from_secs(10 * 60);
const DEFAULT_TRANSIENT_STREAK_WINDOW: Duration = Duration::from_secs(5 * 60);
const DEFAULT_STATE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const DEFAULT_BREAKER_THRESHOLD: u32 = 3;
const DEFAULT_COOLDOWNS: [Duration; 3] = [
    Duration::from_secs(60),
    Duration::from_secs(5 * 60),
    Duration::from_secs(30 * 60),
];
const MAX_BREAKER_THRESHOLD: u32 = 64;

/// Redis 健康状态的稳定作用目标，不包含模型、URL 或请求内容。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RedisHealthTarget {
    Channel(ChannelId),
    Credential(CredentialId),
    /// 引用同一密钥的逻辑凭据共享认证、撤销和账号停用健康。
    CredentialShared(CredentialId),
}

/// 已归一化故障对健康分与连续瞬时故障窗口的影响。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedisHealthFailure {
    /// 当前渠道不支持请求模型，只施加轻惩罚且不推进瞬时故障连续计数。
    ModelUnsupported,
    /// 上游成功响应不符合目标协议，施加轻惩罚但不触发瞬时熔断。
    Protocol,
    /// 凭据认证失败；永久或临时停用仍由凭据状态仓储决定。
    Authentication,
    /// 上游额度不可用；具体恢复窗口仍由凭据状态仓储决定。
    Quota,
    /// 上游限流，施加较重惩罚并推进瞬时熔断计数。
    RateLimited,
    /// 上游过载，施加重惩罚并推进瞬时熔断计数。
    Overloaded,
    /// 连接、读取或传输故障，推进瞬时熔断计数。
    Network,
    /// 上游服务器错误，施加重惩罚并推进瞬时熔断计数。
    Server,
}

impl RedisHealthFailure {
    pub(super) const fn penalty_micros(self) -> u64 {
        match self {
            Self::ModelUnsupported | Self::Quota => 900_000,
            Self::Protocol => 600_000,
            Self::Authentication => 1_800_000,
            Self::RateLimited => 2_200_000,
            Self::Overloaded | Self::Server => 2_500_000,
            Self::Network => 1_200_000,
        }
    }

    pub(super) const fn is_transient(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::Overloaded | Self::Network | Self::Server
        )
    }
}

/// 一条按调用顺序应用的脱敏健康反馈。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedisHealthEvent {
    target: RedisHealthTarget,
    failure: Option<RedisHealthFailure>,
}

impl RedisHealthEvent {
    /// 构造成功事件；无历史故障状态时 Redis 脚本不会产生写入。
    #[must_use]
    pub const fn succeeded(target: RedisHealthTarget) -> Self {
        Self {
            target,
            failure: None,
        }
    }

    /// 构造已结构化分类的失败事件。
    #[must_use]
    pub const fn failed(target: RedisHealthTarget, failure: RedisHealthFailure) -> Self {
        Self {
            target,
            failure: Some(failure),
        }
    }

    #[must_use]
    pub const fn target(self) -> RedisHealthTarget {
        self.target
    }

    pub(super) const fn is_failure(self) -> bool {
        self.failure.is_some()
    }

    pub(super) const fn penalty_micros(self) -> u64 {
        match self.failure {
            Some(failure) => failure.penalty_micros(),
            None => 0,
        }
    }

    pub(super) const fn is_transient(self) -> bool {
        match self.failure {
            Some(failure) => failure.is_transient(),
            None => false,
        }
    }
}

/// 以 Redis 服务端当前时间归一化后的单目标健康快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedisHealthSnapshot {
    target: RedisHealthTarget,
    penalty_micros: u64,
    cooldown_remaining: Duration,
    breaker_level: u8,
}

impl RedisHealthSnapshot {
    pub(super) fn decode(
        target: RedisHealthTarget,
        values: &[i64],
        operation: CacheOperation,
    ) -> Result<Self, CacheError> {
        let [
            penalty_micros,
            cooling,
            cooldown_remaining_millis,
            breaker_level,
        ] = values
        else {
            return Err(protocol_error(operation));
        };
        if *penalty_micros < 0
            || !matches!(*cooling, 0 | 1)
            || *cooldown_remaining_millis < 0
            || !(0..=3).contains(breaker_level)
            || (*cooling == 0 && *cooldown_remaining_millis != 0)
            || (*cooling == 1 && *cooldown_remaining_millis == 0)
        {
            return Err(protocol_error(operation));
        }
        Ok(Self {
            target,
            penalty_micros: u64::try_from(*penalty_micros)
                .map_err(|_| protocol_error(operation))?,
            cooldown_remaining: Duration::from_millis(
                u64::try_from(*cooldown_remaining_millis).map_err(|_| protocol_error(operation))?,
            ),
            breaker_level: u8::try_from(*breaker_level).map_err(|_| protocol_error(operation))?,
        })
    }

    #[must_use]
    pub const fn target(self) -> RedisHealthTarget {
        self.target
    }

    /// 返回按半衰期折算到本次 Redis 读取时刻的固定精度惩罚分。
    #[must_use]
    pub const fn penalty_micros(self) -> u64 {
        self.penalty_micros
    }

    /// 返回当前目标是否仍处于熔断冷却期。
    #[must_use]
    pub const fn is_cooling(self) -> bool {
        !self.cooldown_remaining.is_zero()
    }

    /// 返回以 Redis 服务端时间计算的冷却余量。
    #[must_use]
    pub const fn cooldown_remaining(self) -> Duration {
        self.cooldown_remaining
    }

    /// 返回当前分级冷却层，零表示尚未触发过熔断。
    #[must_use]
    pub const fn breaker_level(self) -> u8 {
        self.breaker_level
    }
}

/// Redis 调度健康状态的命名空间和时间策略。
#[derive(Clone)]
pub struct RedisHealthConfig {
    redis: RedisConfig,
    namespace: String,
    half_life: Duration,
    transient_streak_window: Duration,
    breaker_threshold: u32,
    cooldowns: [Duration; 3],
    state_ttl: Duration,
}

impl RedisHealthConfig {
    /// 使用设计基线创建十分钟半衰期与 1/5/30 分钟三级冷却策略。
    pub fn new(redis: RedisConfig, namespace: impl Into<String>) -> Result<Self, CacheError> {
        let config = Self {
            redis,
            namespace: namespace.into(),
            half_life: DEFAULT_HALF_LIFE,
            transient_streak_window: DEFAULT_TRANSIENT_STREAK_WINDOW,
            breaker_threshold: DEFAULT_BREAKER_THRESHOLD,
            cooldowns: DEFAULT_COOLDOWNS,
            state_ttl: DEFAULT_STATE_TTL,
        };
        config.validate()?;
        Ok(config)
    }

    /// 覆盖完整健康策略；主要用于受控部署配置与快速真实 Redis 回归。
    pub fn with_policy(
        mut self,
        half_life: Duration,
        transient_streak_window: Duration,
        breaker_threshold: u32,
        cooldowns: [Duration; 3],
        state_ttl: Duration,
    ) -> Result<Self, CacheError> {
        self.half_life = half_life;
        self.transient_streak_window = transient_streak_window;
        self.breaker_threshold = breaker_threshold;
        self.cooldowns = cooldowns;
        self.state_ttl = state_ttl;
        self.validate()?;
        Ok(self)
    }

    pub(super) fn validate(&self) -> Result<(), CacheError> {
        crate::config::validate_namespace(&self.namespace)?;
        self.redis.validate()?;
        for duration in [
            self.half_life,
            self.transient_streak_window,
            self.state_ttl,
            self.cooldowns[0],
            self.cooldowns[1],
            self.cooldowns[2],
        ] {
            validate_ttl(duration)?;
        }
        if !(1..=MAX_BREAKER_THRESHOLD).contains(&self.breaker_threshold)
            || self.cooldowns[0] >= self.cooldowns[1]
            || self.cooldowns[1] >= self.cooldowns[2]
            || self.state_ttl <= self.cooldowns[2]
        {
            return Err(CacheError::InvalidHealthPolicy);
        }
        Ok(())
    }

    pub(super) const fn redis(&self) -> &RedisConfig {
        &self.redis
    }

    pub(super) fn namespace(&self) -> &str {
        &self.namespace
    }

    pub(super) const fn breaker_threshold(&self) -> u32 {
        self.breaker_threshold
    }

    pub(super) fn half_life_millis(&self) -> Result<i64, CacheError> {
        validate_ttl(self.half_life)
    }

    pub(super) fn transient_streak_window_millis(&self) -> Result<i64, CacheError> {
        validate_ttl(self.transient_streak_window)
    }

    pub(super) fn state_ttl_millis(&self) -> Result<i64, CacheError> {
        validate_ttl(self.state_ttl)
    }

    pub(super) fn cooldown_millis(&self) -> Result<[i64; 3], CacheError> {
        Ok([
            validate_ttl(self.cooldowns[0])?,
            validate_ttl(self.cooldowns[1])?,
            validate_ttl(self.cooldowns[2])?,
        ])
    }
}

impl fmt::Debug for RedisHealthConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisHealthConfig")
            .field("redis", &self.redis)
            .field("namespace", &"<已脱敏>")
            .field("half_life", &self.half_life)
            .field("transient_streak_window", &self.transient_streak_window)
            .field("breaker_threshold", &self.breaker_threshold)
            .field("cooldowns", &self.cooldowns)
            .field("state_ttl", &self.state_ttl)
            .finish()
    }
}

fn protocol_error(operation: CacheOperation) -> CacheError {
    CacheError::redis(operation, RedisFailureKind::Protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_weights_and_transient_boundary_match_the_closed_policy() {
        assert_eq!(RedisHealthFailure::Server.penalty_micros(), 2_500_000);
        assert_eq!(RedisHealthFailure::RateLimited.penalty_micros(), 2_200_000);
        assert!(RedisHealthFailure::Network.is_transient());
        assert!(!RedisHealthFailure::ModelUnsupported.is_transient());
        assert!(!RedisHealthFailure::Protocol.is_transient());
    }

    #[test]
    fn policy_rejects_non_increasing_cooldowns_and_short_state_ttl() {
        let base = || {
            RedisHealthConfig::new(RedisConfig::new("redis://127.0.0.1:6379/0"), "health").unwrap()
        };
        assert_eq!(
            base()
                .with_policy(
                    Duration::from_secs(1),
                    Duration::from_secs(1),
                    3,
                    [
                        Duration::from_secs(2),
                        Duration::from_secs(2),
                        Duration::from_secs(3),
                    ],
                    Duration::from_secs(10),
                )
                .unwrap_err(),
            CacheError::InvalidHealthPolicy
        );
        assert_eq!(
            base()
                .with_policy(
                    Duration::from_secs(1),
                    Duration::from_secs(1),
                    3,
                    [
                        Duration::from_secs(1),
                        Duration::from_secs(2),
                        Duration::from_secs(3),
                    ],
                    Duration::from_secs(3),
                )
                .unwrap_err(),
            CacheError::InvalidHealthPolicy
        );
    }
}
