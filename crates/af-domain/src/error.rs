use std::fmt;

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

/// 上游故障的闭合领域分类。
///
/// 该类型不得保存上游响应正文、错误文本、URL、Header、凭据或底层错误链；
/// 重试、冷却与禁用等调度动作由后续分类层决定。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UpstreamError {
    /// 上游对当前请求实施限流。
    #[error("上游请求受到限流")]
    RateLimited,
    /// 上游当前过载。
    #[error("上游服务过载")]
    Overloaded,
    /// 上游认证已过期，可能需要刷新凭据。
    #[error("上游认证已过期")]
    AuthExpired,
    /// 上游认证已被永久撤销。
    #[error("上游认证已被撤销")]
    AuthRevoked,
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
    Network,
}

/// AnyFlows 业务层统一错误骨架。
///
/// 这里只聚合已脱敏的领域分类；公开错误码与默认英文文案由 HTTP 边界穷尽映射。
#[derive(Debug, Eq, Error, PartialEq)]
pub enum AfError {
    /// 客户端请求无法进入后续业务流程。
    #[error("请求无效")]
    InvalidRequest,
    /// 已归一化且不含原始上游内容的故障。
    #[error("{0}")]
    Upstream(#[from] UpstreamError),
    /// 不应向客户端暴露实现细节的内部故障。
    #[error("服务内部错误")]
    Internal,
}
