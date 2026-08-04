use af_adapter::{AdaptorError, StatusCode};
use af_domain::UpstreamError;
use af_protocol::{
    anthropic::{AnthropicUpstreamErrorKind, classify_error_response as classify_anthropic_error},
    openai_chat::{OpenAiUpstreamErrorKind, classify_error_response as classify_openai_error},
};
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
    /// 同一渠道的候选没有连续排列，无法安全跳过渠道内剩余凭据。
    #[error("转发候选渠道分组不连续")]
    NonContiguousChannelGroup,
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
    /// 所有候选都在各自等待期限内达到账号并发上限。
    #[error("转发候选并发槽位均不可用")]
    ConcurrencyUnavailable,
    /// 候选并发协调依赖发生内部故障。
    #[error("转发候选并发协调失败")]
    AttemptGateFailed,
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

/// 按状态码和已审定的上游结构化错误信号得出脱敏领域分类。
pub(crate) fn classify_upstream_status(status: StatusCode, body: &[u8]) -> UpstreamError {
    if let Some(error) = classify_gemini_error(status, body) {
        return error;
    }

    let anthropic_signal = classify_anthropic_error(body);
    match (status, anthropic_signal) {
        (
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
            AnthropicUpstreamErrorKind::Authentication,
        ) => return UpstreamError::AuthRevoked,
        (
            StatusCode::BAD_REQUEST | StatusCode::PAYMENT_REQUIRED | StatusCode::TOO_MANY_REQUESTS,
            AnthropicUpstreamErrorKind::Billing,
        ) => return UpstreamError::QuotaExhausted,
        (StatusCode::TOO_MANY_REQUESTS, AnthropicUpstreamErrorKind::RateLimited) => {
            return UpstreamError::RateLimited;
        }
        _ => {}
    }

    let openai_signal = classify_openai_error(body);
    match (status, openai_signal) {
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

/// 只读取 Google JSON API 的状态码与稳定状态名，不保留 message 或 details。
fn classify_gemini_error(status: StatusCode, body: &[u8]) -> Option<UpstreamError> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let error = value.as_object()?.get("error")?.as_object()?;
    let code = error.get("code")?.as_u64()?;
    let code = u16::try_from(code).ok()?;
    if code != status.as_u16() {
        return None;
    }
    let upstream_status = error.get("status")?.as_str()?;
    match (code, upstream_status) {
        (400, "INVALID_ARGUMENT") => Some(UpstreamError::BadRequest),
        (401, "UNAUTHENTICATED") => Some(UpstreamError::AuthExpired),
        (403, "PERMISSION_DENIED") => Some(UpstreamError::AuthRevoked),
        (404, "NOT_FOUND") => Some(UpstreamError::ModelUnsupported),
        (429, "RESOURCE_EXHAUSTED") => Some(UpstreamError::RateLimited),
        (503, "UNAVAILABLE") => Some(UpstreamError::Overloaded),
        (500..=599, _) => Some(UpstreamError::ServerError {
            status: af_domain::UpstreamServerStatus::new(code)
                .expect("500..=599 必须始终满足上游服务器状态约束"),
        }),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_error_classifier_uses_only_structured_code_and_status() {
        for (status, signal, expected) in [
            (400, "INVALID_ARGUMENT", UpstreamError::BadRequest),
            (401, "UNAUTHENTICATED", UpstreamError::AuthExpired),
            (403, "PERMISSION_DENIED", UpstreamError::AuthRevoked),
            (404, "NOT_FOUND", UpstreamError::ModelUnsupported),
            (429, "RESOURCE_EXHAUSTED", UpstreamError::RateLimited),
            (503, "UNAVAILABLE", UpstreamError::Overloaded),
        ] {
            let status = StatusCode::from_u16(status).unwrap();
            let body = format!(
                r#"{{"error":{{"code":{},"message":"private-canary","status":"{signal}","details":[{{"private":"canary"}}]}}}}"#,
                status.as_u16()
            );
            assert_eq!(
                classify_gemini_error(status, body.as_bytes()),
                Some(expected)
            );
        }
    }

    #[test]
    fn gemini_error_classifier_rejects_mismatched_or_unstructured_signals() {
        assert_eq!(
            classify_gemini_error(
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"code":503,"status":"UNAVAILABLE"}}"#,
            ),
            None
        );
        assert_eq!(
            classify_gemini_error(
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"code":429,"message":"RESOURCE_EXHAUSTED"}}"#,
            ),
            None
        );
    }
}
