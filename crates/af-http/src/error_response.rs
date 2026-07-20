use af_domain::{AfError, PublicErrorCode, UpstreamError};
use af_protocol::openai_chat;
use axum::{Json, response::IntoResponse};
use http::StatusCode;

use crate::ApiKeyExtractionError;

/// 将统一领域错误转换为 OpenAI 兼容 HTTP 响应的本地封装。
///
/// 业务 handler 应返回该类型，框架自身产生的 CORS、路由和正文限制错误不在此处重写。
#[derive(Debug)]
pub struct OpenAiHttpError(MappedError);

impl From<AfError> for OpenAiHttpError {
    fn from(error: AfError) -> Self {
        Self(classify_error(error))
    }
}

impl From<ApiKeyExtractionError> for OpenAiHttpError {
    fn from(_: ApiKeyExtractionError) -> Self {
        Self::from(AfError::InvalidApiKey)
    }
}

impl IntoResponse for OpenAiHttpError {
    fn into_response(self) -> axum::response::Response {
        let mapped = self.0;
        log_error(mapped);
        (
            mapped.status(),
            Json(openai_chat::encode_error(mapped.code, mapped.message())),
        )
            .into_response()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MappedError {
    code: PublicErrorCode,
    error_kind: &'static str,
}

impl MappedError {
    pub(super) const fn new(code: PublicErrorCode, error_kind: &'static str) -> Self {
        Self { code, error_kind }
    }

    pub(super) const fn status(self) -> StatusCode {
        match self.code {
            PublicErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            PublicErrorCode::InvalidApiKey => StatusCode::UNAUTHORIZED,
            PublicErrorCode::InsufficientQuota | PublicErrorCode::RateLimited => {
                StatusCode::TOO_MANY_REQUESTS
            }
            PublicErrorCode::ModelNotFound => StatusCode::NOT_FOUND,
            PublicErrorCode::UpstreamUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            PublicErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(super) const fn message(self) -> &'static str {
        match self.code {
            PublicErrorCode::InvalidRequest => "Invalid request.",
            PublicErrorCode::InvalidApiKey => "Invalid API key.",
            PublicErrorCode::InsufficientQuota => "Insufficient quota.",
            PublicErrorCode::ModelNotFound => {
                "The requested model was not found or is unavailable."
            }
            PublicErrorCode::UpstreamUnavailable => {
                "The service is temporarily unavailable. Please retry later."
            }
            PublicErrorCode::RateLimited => "Rate limit exceeded. Please retry later.",
            PublicErrorCode::InternalError => "An internal server error occurred.",
        }
    }
}

/// 穷尽归一现有领域错误；新增领域变体时必须在编译期补齐公开分类。
fn classify_error(error: AfError) -> MappedError {
    match error {
        AfError::InvalidRequest => {
            MappedError::new(PublicErrorCode::InvalidRequest, "invalid_request")
        }
        AfError::InvalidApiKey => {
            MappedError::new(PublicErrorCode::InvalidApiKey, "invalid_api_key")
        }
        AfError::Upstream(error) => classify_upstream_error(error),
        AfError::Internal => MappedError::new(PublicErrorCode::InternalError, "internal"),
    }
}

/// 上游认证和额度属于供应侧状态，绝不能伪装成客户端 API Key 或用户额度错误。
fn classify_upstream_error(error: UpstreamError) -> MappedError {
    match error {
        UpstreamError::RateLimited => {
            MappedError::new(PublicErrorCode::RateLimited, "upstream_rate_limited")
        }
        UpstreamError::Overloaded => {
            MappedError::new(PublicErrorCode::UpstreamUnavailable, "upstream_overloaded")
        }
        UpstreamError::AuthExpired => MappedError::new(
            PublicErrorCode::UpstreamUnavailable,
            "upstream_auth_expired",
        ),
        UpstreamError::AuthRevoked => MappedError::new(
            PublicErrorCode::UpstreamUnavailable,
            "upstream_auth_revoked",
        ),
        UpstreamError::QuotaExhausted => MappedError::new(
            PublicErrorCode::UpstreamUnavailable,
            "upstream_quota_exhausted",
        ),
        UpstreamError::ModelUnsupported => {
            MappedError::new(PublicErrorCode::ModelNotFound, "upstream_model_unsupported")
        }
        UpstreamError::ProtocolError => MappedError::new(
            PublicErrorCode::UpstreamUnavailable,
            "upstream_protocol_error",
        ),
        UpstreamError::ServerError { .. } => MappedError::new(
            PublicErrorCode::UpstreamUnavailable,
            "upstream_server_error",
        ),
        UpstreamError::BadRequest => {
            MappedError::new(PublicErrorCode::InvalidRequest, "upstream_bad_request")
        }
        UpstreamError::Network => {
            MappedError::new(PublicErrorCode::UpstreamUnavailable, "upstream_network")
        }
    }
}

/// 错误事件只记录闭合分类，避免内部诊断和外部输入进入结构化日志。
fn log_error(mapped: MappedError) {
    let status_code = u64::from(mapped.status().as_u16());
    if mapped.status().is_server_error() {
        tracing::error!(
            target: "af_http::error_response",
            status_code,
            error_code = mapped.code.as_str(),
            error_kind = mapped.error_kind,
            "返回 OpenAI 错误响应"
        );
    } else {
        tracing::warn!(
            target: "af_http::error_response",
            status_code,
            error_code = mapped.code.as_str(),
            error_kind = mapped.error_kind,
            "返回 OpenAI 错误响应"
        );
    }
}
