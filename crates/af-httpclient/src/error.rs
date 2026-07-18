use std::fmt;

use thiserror::Error;

/// HTTP 超时阶段，用于配置校验与稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TimeoutPhase {
    Connect,
    Read,
    Request,
}

impl fmt::Display for TimeoutPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Connect => "连接",
            Self::Read => "读取",
            Self::Request => "整体请求",
        };
        formatter.write_str(name)
    }
}

/// HTTP Client 配置、构建与缓存错误。
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum HttpClientError {
    #[error("HTTP Client 缓存容量必须大于零")]
    InvalidPoolCapacity,
    #[error("HTTP {phase}超时必须大于零")]
    InvalidTimeout { phase: TimeoutPhase },
    #[error("代理地址无效")]
    InvalidProxyUrl,
    #[error("代理协议仅支持 http、https、socks5 或 socks5h")]
    UnsupportedProxyScheme,
    #[error("TLS 加密提供者不可用")]
    TlsProviderUnavailable,
    #[error("HTTP Client 构建失败")]
    ClientBuild,
    #[error("HTTP Client 缓存锁不可用")]
    PoolUnavailable,
}

/// 不携带 URL、Header、请求体或底层错误链的 HTTP 传输错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum HttpTransportError {
    #[error("HTTP 请求目标无效")]
    InvalidRequestTarget,
    #[error("HTTP 请求方法不受支持")]
    UnsupportedRequestMethod,
    #[error("HTTP 建连超时")]
    ConnectTimeout,
    #[error("HTTP 读取超时")]
    ReadTimeout,
    #[error("HTTP 请求超过总时限")]
    RequestTimeout,
    #[error("HTTP 连接失败")]
    Connect,
    #[error("HTTP 请求发送失败")]
    Request,
    #[error("HTTP 响应体读取失败")]
    ResponseBody,
}

impl HttpTransportError {
    /// 返回错误是否由任一传输超时触发。
    #[must_use]
    pub const fn is_timeout(&self) -> bool {
        matches!(
            self,
            Self::ConnectTimeout | Self::ReadTimeout | Self::RequestTimeout
        )
    }

    /// 返回错误是否发生在建连阶段。
    #[must_use]
    pub const fn is_connect(&self) -> bool {
        matches!(self, Self::ConnectTimeout | Self::Connect)
    }

    pub(crate) fn from_send(error: &reqwest::Error) -> Self {
        if error.is_timeout() && error.is_connect() {
            Self::ConnectTimeout
        } else if error.is_timeout() {
            Self::ReadTimeout
        } else if error.is_connect() {
            Self::Connect
        } else if error.is_builder() {
            Self::InvalidRequestTarget
        } else {
            Self::Request
        }
    }

    pub(crate) fn from_response_body(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::ReadTimeout
        } else {
            Self::ResponseBody
        }
    }
}
