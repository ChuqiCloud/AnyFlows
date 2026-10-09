use std::{fmt, num::NonZeroU32, time::Duration};

use af_domain::{GroupId, TokenId, UserId};

use crate::{
    CacheError, RedisConfig,
    config::{validate_namespace, validate_ttl},
};

/// 单次原子准入允许检查的最大规则数。
pub const MAX_REQUEST_RATE_LIMIT_RULES: usize = 8;
const MAX_REQUEST_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// A caller-derived, domain-separated fingerprint with a fixed-window limit.
/// Raw IP addresses and login identifiers must be hashed before construction.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct FingerprintRateLimitRule {
    fingerprint: [u8; 32],
    limit: NonZeroU32,
    window_millis: i64,
}

impl FingerprintRateLimitRule {
    pub fn new(
        fingerprint: [u8; 32],
        limit: NonZeroU32,
        window: Duration,
    ) -> Result<Self, CacheError> {
        Ok(Self {
            fingerprint,
            limit,
            window_millis: validate_window(window)?,
        })
    }

    pub(super) const fn fingerprint(self) -> [u8; 32] {
        self.fingerprint
    }

    pub(super) const fn limit(self) -> u32 {
        self.limit.get()
    }

    pub(super) const fn window_millis(self) -> i64 {
        self.window_millis
    }
}

impl fmt::Debug for FingerprintRateLimitRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FingerprintRateLimitRule")
            .field("fingerprint", &"<redacted>")
            .field("limit", &self.limit)
            .field("window_millis", &self.window_millis)
            .finish()
    }
}

fn validate_window(window: Duration) -> Result<i64, CacheError> {
    let milliseconds = validate_ttl(window)?;
    if window > MAX_REQUEST_RATE_LIMIT_WINDOW {
        return Err(CacheError::InvalidRateLimitWindow);
    }
    Ok(milliseconds)
}

/// Redis 请求限流的连接与键空间配置。
#[derive(Clone)]
pub struct RedisRequestRateLimitConfig {
    pub(super) redis: RedisConfig,
    pub(super) namespace: String,
}

impl RedisRequestRateLimitConfig {
    /// 创建请求限流配置；命名空间必须满足共享缓存键约束。
    pub fn new(redis: RedisConfig, namespace: impl Into<String>) -> Result<Self, CacheError> {
        let config = Self {
            redis,
            namespace: namespace.into(),
        };
        config.validate()?;
        Ok(config)
    }

    pub(super) fn validate(&self) -> Result<(), CacheError> {
        validate_namespace(&self.namespace)?;
        self.redis.validate()
    }
}

impl fmt::Debug for RedisRequestRateLimitConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisRequestRateLimitConfig")
            .field("redis", &self.redis)
            .field("namespace", &"<已脱敏>")
            .finish()
    }
}

/// 请求计数所属的稳定业务主体。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestRateLimitSubject {
    User(UserId),
    Group(GroupId),
    Token(TokenId),
}

impl RequestRateLimitSubject {
    pub(super) const fn kind(self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Group(_) => "group",
            Self::Token(_) => "token",
        }
    }

    pub(super) const fn identifier(self) -> i64 {
        match self {
            Self::User(value) => value.get(),
            Self::Group(value) => value.get(),
            Self::Token(value) => value.get(),
        }
    }

    pub(super) const fn duplicate_key(self) -> (u8, i64) {
        match self {
            Self::User(value) => (1, value.get()),
            Self::Group(value) => (2, value.get()),
            Self::Token(value) => (3, value.get()),
        }
    }
}

/// 单个主体的固定窗口请求上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestRateLimitRule {
    subject: RequestRateLimitSubject,
    limit: NonZeroU32,
    window: Duration,
    window_millis: i64,
}

impl RequestRateLimitRule {
    /// 创建固定窗口规则；窗口必须为整毫秒且不超过七天。
    pub fn new(
        subject: RequestRateLimitSubject,
        limit: NonZeroU32,
        window: Duration,
    ) -> Result<Self, CacheError> {
        let window_millis = validate_window(window)?;
        Ok(Self {
            subject,
            limit,
            window,
            window_millis,
        })
    }

    /// 返回规则约束的业务主体。
    #[must_use]
    pub const fn subject(self) -> RequestRateLimitSubject {
        self.subject
    }

    /// 返回窗口内允许的最大请求数。
    #[must_use]
    pub const fn limit(self) -> NonZeroU32 {
        self.limit
    }

    /// 返回固定窗口长度。
    #[must_use]
    pub const fn window(self) -> Duration {
        self.window
    }

    pub(super) const fn window_millis(self) -> i64 {
        self.window_millis
    }
}

/// 原子请求准入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestRateLimitOutcome {
    /// 所有主体均有余量，并已在同一 Redis 脚本内计数。
    Admitted,
    /// 至少一个主体已经达到窗口上限，任何主体都没有写入。
    Limited(RequestRateLimitRejection),
}

/// 请求被固定窗口拒绝时的安全诊断。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestRateLimitRejection {
    subject: RequestRateLimitSubject,
    retry_after: Duration,
}

impl RequestRateLimitRejection {
    pub(super) const fn new(subject: RequestRateLimitSubject, retry_after: Duration) -> Self {
        Self {
            subject,
            retry_after,
        }
    }

    /// 返回最先达到上限的业务主体。
    #[must_use]
    pub const fn subject(self) -> RequestRateLimitSubject {
        self.subject
    }

    /// 返回按 Redis 服务端时间计算的剩余等待时间。
    #[must_use]
    pub const fn retry_after(self) -> Duration {
        self.retry_after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_rejects_sub_millisecond_and_overlong_windows() {
        let subject = RequestRateLimitSubject::User(UserId::new(1).unwrap());
        let limit = NonZeroU32::new(1).unwrap();
        assert_eq!(
            RequestRateLimitRule::new(subject, limit, Duration::from_micros(1)).unwrap_err(),
            CacheError::InvalidTtl
        );
        assert_eq!(
            RequestRateLimitRule::new(
                subject,
                limit,
                MAX_REQUEST_RATE_LIMIT_WINDOW + Duration::from_millis(1),
            )
            .unwrap_err(),
            CacheError::InvalidRateLimitWindow
        );
    }

    #[test]
    fn rule_accepts_maximum_window_and_limit() {
        let rule = RequestRateLimitRule::new(
            RequestRateLimitSubject::Group(GroupId::new(2).unwrap()),
            NonZeroU32::new(u32::MAX).unwrap(),
            MAX_REQUEST_RATE_LIMIT_WINDOW,
        )
        .unwrap();
        assert_eq!(rule.limit().get(), u32::MAX);
        assert_eq!(rule.window(), MAX_REQUEST_RATE_LIMIT_WINDOW);
        assert_eq!(rule.window_millis(), 7 * 24 * 60 * 60 * 1_000);
    }

    #[test]
    fn config_debug_redacts_namespace_and_redis_url() {
        let config = RedisRequestRateLimitConfig::new(
            RedisConfig::new("redis://user:secret@127.0.0.1/"),
            "private-tenant",
        )
        .unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("private-tenant"));
    }
}
