use std::fmt;

use tracing::{Level, Span};

use crate::RequestIdError;

pub(crate) const REQUEST_SPAN_TARGET: &str = "af_request";
const MAX_REQUEST_ID_BYTES: usize = 64;

/// 经过严格校验的服务端 canonical 请求 ID。
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(String);

impl RequestId {
    /// 校验并保存服务端生成的请求 ID；不得直接信任客户端传入的 header。
    pub fn new(value: impl Into<String>) -> Result<Self, RequestIdError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_REQUEST_ID_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(RequestIdError::Invalid);
        }
        Ok(Self(value))
    }

    /// 返回已校验的请求 ID 文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("RequestId").field(&self.0).finish()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 创建请求根 span，但不自动进入；异步调用方应使用 `Instrument::instrument` 绑定。
#[must_use]
pub fn request_span(request_id: &RequestId) -> Span {
    tracing::span!(
        target: REQUEST_SPAN_TARGET,
        Level::INFO,
        "request",
        request_id = request_id.as_str()
    )
}
