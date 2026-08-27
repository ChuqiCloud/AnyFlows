use std::num::NonZeroU32;

use thiserror::Error;

use crate::{OrganizationId, OrganizationScimTokenKey};

/// SCIM 固定窗口允许的最大长度；具体计数由后续 Redis 适配器原子维护。
pub const MAX_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS: u32 = 7 * 24 * 60 * 60;

/// SCIM 限流主体；令牌主体必须在类型中携带所属企业。
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimRateLimitSubject {
    /// 企业级共享窗口。
    Organization(OrganizationId),
    /// 令牌级窗口；只保存非敏感的令牌公开标识。
    Token {
        organization_id: OrganizationId,
        token_key: OrganizationScimTokenKey,
    },
}

impl OrganizationScimRateLimitSubject {
    /// 返回限流主体所属企业，避免调用方从请求正文推导租户。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        match self {
            Self::Organization(organization_id)
            | Self::Token {
                organization_id, ..
            } => *organization_id,
        }
    }

    /// 返回稳定主体种类码，供后续缓存键适配器使用。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Organization(_) => "organization",
            Self::Token { .. } => "token",
        }
    }

    /// 返回已脱敏的令牌公开标识；企业级主体没有令牌标识。
    #[must_use]
    pub fn token_key(&self) -> Option<&OrganizationScimTokenKey> {
        match self {
            Self::Organization(_) => None,
            Self::Token { token_key, .. } => Some(token_key),
        }
    }
}

/// SCIM 限流窗口构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimRateLimitError {
    /// 窗口不在固定的正数范围内。
    #[error("企业 SCIM 限流窗口无效")]
    InvalidWindow,
    /// 持久化的计数或重置时间不满足不变量。
    #[error("企业 SCIM 限流快照无效")]
    InvalidSnapshot,
    /// 当前时间戳不是有效的 Unix 秒。
    #[error("企业 SCIM 限流时间戳无效")]
    InvalidTimestamp,
}

/// 受控的 SCIM 固定窗口长度。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrganizationScimRateLimitWindow(NonZeroU32);

impl OrganizationScimRateLimitWindow {
    /// 创建窗口；长度不能超过七天。
    pub const fn new(seconds: u32) -> Result<Self, OrganizationScimRateLimitError> {
        if seconds == 0 || seconds > MAX_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS {
            return Err(OrganizationScimRateLimitError::InvalidWindow);
        }
        match NonZeroU32::new(seconds) {
            Some(value) => Ok(Self(value)),
            None => Err(OrganizationScimRateLimitError::InvalidWindow),
        }
    }

    /// 从数据库整数恢复窗口，拒绝负数、零和超界值。
    pub const fn from_persistence(value: i64) -> Result<Self, OrganizationScimRateLimitError> {
        if value <= 0 || value > MAX_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS as i64 {
            return Err(OrganizationScimRateLimitError::InvalidWindow);
        }
        Self::new(value as u32)
    }

    /// 返回窗口秒数。
    #[must_use]
    pub const fn seconds(self) -> u32 {
        self.0.get()
    }
}

/// 单个企业或令牌主体的固定窗口规则。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationScimRateLimitRule {
    subject: OrganizationScimRateLimitSubject,
    limit: NonZeroU32,
    window: OrganizationScimRateLimitWindow,
}

impl OrganizationScimRateLimitRule {
    /// 组合限流主体、窗口请求上限和窗口长度。
    #[must_use]
    pub const fn new(
        subject: OrganizationScimRateLimitSubject,
        limit: NonZeroU32,
        window: OrganizationScimRateLimitWindow,
    ) -> Self {
        Self {
            subject,
            limit,
            window,
        }
    }

    /// 返回限流主体。
    #[must_use]
    pub fn subject(&self) -> &OrganizationScimRateLimitSubject {
        &self.subject
    }

    /// 返回窗口内允许的最大请求数。
    #[must_use]
    pub const fn limit(&self) -> NonZeroU32 {
        self.limit
    }

    /// 返回固定窗口。
    #[must_use]
    pub const fn window(&self) -> OrganizationScimRateLimitWindow {
        self.window
    }
}

/// 由原子存储读取的固定窗口计数快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationScimRateLimitSnapshot {
    request_count: u64,
    reset_at_unix_seconds: i64,
}

impl OrganizationScimRateLimitSnapshot {
    /// 构造快照；计数允许超过规则上限，以便损坏或并发超额时保持拒绝。
    pub const fn new(
        request_count: u64,
        reset_at_unix_seconds: i64,
    ) -> Result<Self, OrganizationScimRateLimitError> {
        if reset_at_unix_seconds <= 0 {
            return Err(OrganizationScimRateLimitError::InvalidSnapshot);
        }
        Ok(Self {
            request_count,
            reset_at_unix_seconds,
        })
    }

    /// 返回当前窗口计数。
    #[must_use]
    pub const fn request_count(self) -> u64 {
        self.request_count
    }

    /// 返回服务端计算的窗口重置时间。
    #[must_use]
    pub const fn reset_at_unix_seconds(self) -> i64 {
        self.reset_at_unix_seconds
    }
}

/// 原子存储返回的限流事实；不可用或损坏时显式进入失败关闭路径。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimRateLimitState {
    /// 存储返回了可校验的固定窗口快照。
    Ready(OrganizationScimRateLimitSnapshot),
    /// 存储不可用，不能安全放行请求。
    Unavailable,
    /// 存储返回了损坏或无法解释的事实。
    Invalid,
}

/// 限流失败关闭的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimRateLimitRejection {
    /// 无法确认计数时拒绝请求，避免绕过限流。
    #[error("企业 SCIM 限流存储不可用")]
    StorageUnavailable,
    /// 计数或时间快照损坏。
    #[error("企业 SCIM 限流事实无效")]
    InvalidFact,
    /// 调用方提供的当前时间无效。
    #[error("企业 SCIM 限流当前时间无效")]
    InvalidTimestamp,
}

/// SCIM 原子限流判定；Admitted 只表示后续适配器可以执行一次原子递增。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimRateLimitDecision {
    /// 当前请求可进入窗口，剩余数为本次递增后的值。
    Admitted {
        remaining: u32,
        reset_at_unix_seconds: i64,
    },
    /// 当前窗口已满，调用方只能等待指定秒数。
    Limited {
        retry_after_seconds: u32,
        reset_at_unix_seconds: i64,
    },
    /// 无法安全确认限流状态，必须失败关闭。
    Rejected(OrganizationScimRateLimitRejection),
}

/// 根据固定窗口快照判定 SCIM 请求；不执行 Redis、数据库或网络 IO。
#[must_use]
pub fn classify_organization_scim_rate_limit(
    rule: &OrganizationScimRateLimitRule,
    state: OrganizationScimRateLimitState,
    now_unix_seconds: i64,
) -> OrganizationScimRateLimitDecision {
    if now_unix_seconds <= 0 {
        return OrganizationScimRateLimitDecision::Rejected(
            OrganizationScimRateLimitRejection::InvalidTimestamp,
        );
    }

    let snapshot = match state {
        OrganizationScimRateLimitState::Ready(snapshot) => snapshot,
        OrganizationScimRateLimitState::Unavailable => {
            return OrganizationScimRateLimitDecision::Rejected(
                OrganizationScimRateLimitRejection::StorageUnavailable,
            );
        }
        OrganizationScimRateLimitState::Invalid => {
            return OrganizationScimRateLimitDecision::Rejected(
                OrganizationScimRateLimitRejection::InvalidFact,
            );
        }
    };

    if snapshot.reset_at_unix_seconds() <= now_unix_seconds {
        let Some(reset_at) = now_unix_seconds.checked_add(i64::from(rule.window().seconds()))
        else {
            return OrganizationScimRateLimitDecision::Rejected(
                OrganizationScimRateLimitRejection::InvalidTimestamp,
            );
        };
        return OrganizationScimRateLimitDecision::Admitted {
            remaining: rule.limit().get().saturating_sub(1),
            reset_at_unix_seconds: reset_at,
        };
    }

    let remaining_until_limit =
        u64::from(rule.limit().get()).saturating_sub(snapshot.request_count());
    if remaining_until_limit == 0 {
        let retry_after = snapshot.reset_at_unix_seconds() - now_unix_seconds;
        let Ok(retry_after_seconds) = u32::try_from(retry_after) else {
            return OrganizationScimRateLimitDecision::Rejected(
                OrganizationScimRateLimitRejection::InvalidFact,
            );
        };
        return OrganizationScimRateLimitDecision::Limited {
            retry_after_seconds,
            reset_at_unix_seconds: snapshot.reset_at_unix_seconds(),
        };
    }

    OrganizationScimRateLimitDecision::Admitted {
        remaining: (remaining_until_limit - 1) as u32,
        reset_at_unix_seconds: snapshot.reset_at_unix_seconds(),
    }
}
