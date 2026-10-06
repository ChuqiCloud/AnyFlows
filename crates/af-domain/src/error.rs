use std::{fmt, time::Duration};

use thiserror::Error;

/// 已校验的上游 5xx HTTP 状态码。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UpstreamServerStatus(u16);

impl UpstreamServerStatus {
    /// HTTP 服务器错误的最小状态码。
    pub const MIN: u16 = 500;
    /// HTTP 服务器错误的最大状态码。
    pub const MAX: u16 = 599;

    /// 校验并构造上游服务器错误状态码。
    #[must_use]
    pub const fn new(status: u16) -> Option<Self> {
        match status {
            Self::MIN..=Self::MAX => Some(Self(status)),
            _ => None,
        }
    }

    /// 返回已校验的 HTTP 状态码。
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl From<UpstreamServerStatus> for u16 {
    fn from(status: UpstreamServerStatus) -> Self {
        status.get()
    }
}

impl fmt::Display for UpstreamServerStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// 上游建议的相对重试等待时间。
///
/// 只接受整秒且最多七天，避免恶意或损坏的响应头制造不可恢复的远期冷却。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UpstreamRetryAfter(u32);

impl UpstreamRetryAfter {
    /// 支持的最大重试等待秒数，覆盖供应商最长七天额度窗口。
    pub const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;

    /// 校验并构造非零相对等待时间。
    #[must_use]
    pub const fn from_seconds(seconds: u64) -> Option<Self> {
        if seconds == 0 || seconds > Self::MAX_SECONDS {
            return None;
        }
        Some(Self(seconds as u32))
    }

    /// 返回已校验的相对等待秒数。
    #[must_use]
    pub const fn seconds(self) -> u32 {
        self.0
    }

    /// 返回可直接用于截止时间计算的标准时长。
    #[must_use]
    pub const fn duration(self) -> Duration {
        Duration::from_secs(self.0 as u64)
    }
}

/// 下游额度窗口的相对重试等待时间。
///
/// 只接受整秒且最多三十一天，完整覆盖 UTC 日历月窗口，同时不放宽上游响应头的安全边界。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QuotaWindowRetryAfter(u32);

impl QuotaWindowRetryAfter {
    /// 支持的最大等待秒数，覆盖最长三十一天的日历月。
    pub const MAX_SECONDS: u64 = 31 * 24 * 60 * 60;

    /// 校验并构造非零额度窗口等待时间。
    #[must_use]
    pub const fn from_seconds(seconds: u64) -> Option<Self> {
        if seconds == 0 || seconds > Self::MAX_SECONDS {
            return None;
        }
        Some(Self(seconds as u32))
    }

    /// 返回已校验的相对等待秒数。
    #[must_use]
    pub const fn seconds(self) -> u32 {
        self.0
    }

    /// 返回可直接用于截止时间计算的标准时长。
    #[must_use]
    pub const fn duration(self) -> Duration {
        Duration::from_secs(self.0 as u64)
    }
}

/// 限流影响的最小可确认范围。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RateLimitScope {
    /// 当前凭据或上游账号整体受限。
    Credential,
    /// 只有当前模型受限，账号上的其他模型仍可能可用。
    Model,
    /// 请求数、令牌数或供应商配额窗口受限。
    Window,
    /// 结构化信号不足，不能安全推断更小范围。
    Unknown,
}

/// 上游网络失败发生的稳定阶段。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NetworkFailureKind {
    /// 目标解析失败。
    Resolution,
    /// 建立连接失败。
    Connect,
    /// 建连阶段超过截止时间。
    ConnectTimeout,
    /// 读取响应阶段超过截止时间。
    ReadTimeout,
    /// 完整请求超过总截止时间。
    RequestTimeout,
    /// 请求发送失败。
    Request,
    /// 响应体传输中断或损坏。
    ResponseBody,
    /// 本地安全策略拒绝了目标或代理 DNS 行为。
    Policy,
}

/// 上游故障的闭合领域分类。
///
/// 该类型不得保存上游响应正文、错误文本、URL、Header、凭据或底层错误链；
/// 重试、冷却与禁用等调度动作由后续分类层决定。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UpstreamError {
    /// 上游对当前请求实施限流。
    #[error("上游请求受到限流")]
    RateLimited {
        /// 由结构化信号确认的最小限流范围。
        scope: RateLimitScope,
        /// 由受限响应头解析的可选相对等待时间。
        retry_after: Option<UpstreamRetryAfter>,
    },
    /// 上游当前过载。
    #[error("上游服务过载")]
    Overloaded {
        /// 由受限响应头解析的可选相对等待时间。
        retry_after: Option<UpstreamRetryAfter>,
    },
    /// 上游认证已过期，可能需要刷新凭据。
    #[error("上游认证已过期")]
    AuthExpired,
    /// 上游认证已被永久撤销。
    #[error("上游认证已被撤销")]
    AuthRevoked,
    /// 上游账号、组织或工作区被明确停用。
    #[error("上游账号或组织已被停用")]
    AccountDisabled,
    /// 上游账号或组织额度已耗尽。
    #[error("上游额度已耗尽")]
    QuotaExhausted,
    /// 上游不支持请求的模型。
    #[error("上游不支持请求的模型")]
    ModelUnsupported,
    /// 上游响应不符合预期协议。
    #[error("上游协议响应无效")]
    ProtocolError,
    /// 上游返回服务器错误。
    #[error("上游服务返回服务器错误（HTTP {status}）")]
    ServerError {
        /// 已解析且不含响应正文的 HTTP 状态码。
        status: UpstreamServerStatus,
    },
    /// 上游判定请求参数无效。
    #[error("上游拒绝了无效请求")]
    BadRequest,
    /// 上游网络传输失败。
    #[error("上游网络传输失败")]
    Network {
        /// 不含底层错误文本的稳定失败阶段。
        kind: NetworkFailureKind,
    },
}

impl UpstreamError {
    /// 构造不携带等待提示的限流错误。
    #[must_use]
    pub const fn rate_limited(scope: RateLimitScope) -> Self {
        Self::RateLimited {
            scope,
            retry_after: None,
        }
    }

    /// 构造带受控等待提示的限流错误。
    #[must_use]
    pub const fn rate_limited_after(
        scope: RateLimitScope,
        retry_after: UpstreamRetryAfter,
    ) -> Self {
        Self::RateLimited {
            scope,
            retry_after: Some(retry_after),
        }
    }

    /// 构造不携带等待提示的过载错误。
    #[must_use]
    pub const fn overloaded() -> Self {
        Self::Overloaded { retry_after: None }
    }

    /// 构造带受控等待提示的过载错误。
    #[must_use]
    pub const fn overloaded_after(retry_after: UpstreamRetryAfter) -> Self {
        Self::Overloaded {
            retry_after: Some(retry_after),
        }
    }

    /// 构造不携带底层错误文本的网络错误。
    #[must_use]
    pub const fn network(kind: NetworkFailureKind) -> Self {
        Self::Network { kind }
    }
}

/// AnyFlows 业务层统一错误骨架。
///
/// 这里只聚合已脱敏的领域分类；公开错误码与默认英文文案由 HTTP 边界穷尽映射。
#[derive(Debug, Eq, Error, PartialEq)]
pub enum AfError {
    /// 客户端请求无法进入后续业务流程。
    #[error("请求无效")]
    InvalidRequest,
    /// 下游 API Key 缺失、格式无效或未通过认证。
    #[error("下游 API Key 无效")]
    InvalidApiKey,
    /// 请求模型未被当前下游令牌策略允许。
    #[error("下游令牌不允许请求的模型")]
    ModelNotAllowed,
    /// 当前认证用户范围内不存在指定异步任务。
    #[error("异步任务不存在")]
    TaskNotFound,
    /// 相同幂等键已经绑定到不同请求载荷。
    #[error("幂等键与已有请求冲突")]
    IdempotencyConflict,
    /// 提交或持久化结果未知，调用方必须使用相同幂等键重试。
    #[error("请求结果未知")]
    RequestOutcomeUnknown,
    /// 用户钱包或下游令牌额度不足，无法继续发送请求。
    #[error("下游可用额度不足")]
    InsufficientQuota,
    /// 下游令牌或分组额度窗口已满，可在指定秒数后重新准入。
    #[error("下游额度窗口已达上限")]
    QuotaWindowLimited {
        /// 当前所有阻塞窗口中最长的剩余等待时间。
        retry_after: QuotaWindowRetryAfter,
    },
    /// 用户级或上游账号级并发槽位在等待期限内不可用。
    #[error("并发请求已达上限")]
    ConcurrencyLimited,
    /// 已归一化且不含原始上游内容的故障。
    #[error("{0}")]
    Upstream(#[from] UpstreamError),
    /// 不应向客户端暴露实现细节的内部故障。
    #[error("服务内部错误")]
    Internal,
}
