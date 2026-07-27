use af_adapter::{AdaptorError, StatusCode};
use af_domain::UpstreamError;
use af_protocol::openai_chat::{OpenAiUpstreamErrorKind, classify_error_response};
use thiserror::Error;

/// 转发服务启动或候选装配阶段的构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RelayBuildError {
    /// 静态模型配置不符合 OpenAI Chat 协议边界。
    #[error("单上游模型配置无效")]
    InvalidModel,
    /// 基础地址或凭据无法构造安全适配器上下文。
    #[error("单上游适配器配置无效")]
    Adaptor(#[source] AdaptorError),
    /// 转发状态机没有可供尝试的候选。
    #[error("转发候选不能为空")]
    NoCandidates,
    /// 候选数量超过单次转发的安全上限。
    #[error("转发候选超过容量上限")]
    TooManyCandidates,
}

impl From<AdaptorError> for RelayBuildError {
    fn from(error: AdaptorError) -> Self {
        Self::Adaptor(error)
    }
}

/// 转发运行期错误；不保留 URL、凭据、请求体或上游原始错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum RelayError {
    /// 请求模型不符合 Canonical 模型名边界。
    #[error("转发模型无效")]
    InvalidModel,
    /// 请求在适配层边界被拒绝。
    #[error("转发请求无效")]
    Request(#[source] AdaptorError),
    /// 单个候选的适配器构造或凭据阶段失败。
    #[error("转发适配器失败")]
    Adaptor(#[source] AdaptorError),
    /// 所有候选都未能完成请求。
    #[error("转发候选均不可用")]
    Upstream(#[from] UpstreamError),
}

/// 将发送和响应边界错误收敛到可供候选切换使用的脱敏分类。
pub(crate) fn map_adaptor_error(error: AdaptorError) -> UpstreamError {
    match error {
        AdaptorError::Transport(_) => UpstreamError::Network,
        AdaptorError::InvalidResponseHeader | AdaptorError::ResponseBodyTooLarge => {
            UpstreamError::ProtocolError
        }
        _ => UpstreamError::ProtocolError,
    }
}

/// 按状态码和 OpenAI 结构化错误信号得出脱敏的领域错误。
pub(crate) fn classify_upstream_status(status: StatusCode, body: &[u8]) -> UpstreamError {
    let signal = classify_error_response(body);
    match (status, signal) {
        (
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
            OpenAiUpstreamErrorKind::AuthRevoked,
        ) => return UpstreamError::AuthRevoked,
        (StatusCode::TOO_MANY_REQUESTS, OpenAiUpstreamErrorKind::QuotaExhausted) => {
            return UpstreamError::QuotaExhausted;
        }
        (StatusCode::TOO_MANY_REQUESTS, OpenAiUpstreamErrorKind::RateLimited) => {
            return UpstreamError::RateLimited;
        }
        (StatusCode::NOT_FOUND, OpenAiUpstreamErrorKind::ModelUnsupported) => {
            return UpstreamError::ModelUnsupported;
        }
        _ => {}
    }

    match status.as_u16() {
        400 | 422 => UpstreamError::BadRequest,
        // 未知 401/403 可能来自 IP 或区域策略，不能据此永久禁用凭据。
        401 => UpstreamError::AuthExpired,
        403 => UpstreamError::ProtocolError,
        402 => UpstreamError::QuotaExhausted,
        408 => UpstreamError::Network,
        // 未知 404 更可能是路径配置错误；未知 429 才按临时过载处理。
        404 => UpstreamError::ProtocolError,
        429 => UpstreamError::Overloaded,
        529 => UpstreamError::Overloaded,
        value @ 500..=599 => UpstreamError::ServerError {
            status: af_domain::UpstreamServerStatus::new(value)
                .expect("500..=599 必须始终满足上游服务器状态约束"),
        },
        _ => UpstreamError::ProtocolError,
    }
}

/// 判定当前候选失败后能否继续选路；健康惩罚、刷新和禁用由调度层处理。
pub(crate) const fn is_retryable(error: UpstreamError) -> bool {
    matches!(
        error,
        UpstreamError::RateLimited
            | UpstreamError::Overloaded
            | UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::QuotaExhausted
            | UpstreamError::ModelUnsupported
            | UpstreamError::Network
            | UpstreamError::ServerError { .. }
    )
}
