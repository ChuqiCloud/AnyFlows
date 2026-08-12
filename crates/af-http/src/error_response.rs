use af_domain::{AfError, Protocol, PublicErrorCode, UpstreamError, UpstreamRetryAfter};
use af_protocol::{anthropic, gemini, openai_chat};
use axum::{Json, response::IntoResponse};
use http::{HeaderValue, StatusCode, header::RETRY_AFTER};

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
        protocol_error_response(Protocol::OpenAiChat, self.0)
    }
}

/// 将统一领域错误转换为 OpenAI Responses HTTP 响应。
#[derive(Debug)]
pub(crate) struct OpenAiResponsesHttpError(MappedError);

impl From<AfError> for OpenAiResponsesHttpError {
    fn from(error: AfError) -> Self {
        Self(classify_error(error))
    }
}

impl IntoResponse for OpenAiResponsesHttpError {
    fn into_response(self) -> axum::response::Response {
        protocol_error_response(Protocol::OpenAiResponses, self.0)
    }
}

/// 将统一领域错误转换为 Anthropic Messages HTTP 响应的本地封装。
#[derive(Debug)]
pub(crate) struct AnthropicHttpError(MappedError);

impl From<AfError> for AnthropicHttpError {
    fn from(error: AfError) -> Self {
        Self(classify_error(error))
    }
}

impl IntoResponse for AnthropicHttpError {
    fn into_response(self) -> axum::response::Response {
        protocol_error_response(Protocol::Anthropic, self.0)
    }
}

/// 将统一领域错误转换为 Gemini Google JSON API 响应的本地封装。
#[derive(Debug)]
pub(crate) struct GeminiHttpError(MappedError);

impl From<AfError> for GeminiHttpError {
    fn from(error: AfError) -> Self {
        Self(classify_error(error))
    }
}

impl IntoResponse for GeminiHttpError {
    fn into_response(self) -> axum::response::Response {
        protocol_error_response(Protocol::Gemini, self.0)
    }
}

/// 为路由级中间件按客户端协议构造错误响应。
pub(crate) fn api_error_response(protocol: Protocol, error: AfError) -> axum::response::Response {
    protocol_error_response(protocol, classify_error(error))
}

/// 返回生产请求限流响应，并把 Redis 计算的窗口恢复时间转换为标准头。
pub(crate) fn rate_limit_error_response(
    protocol: Protocol,
    retry_after: UpstreamRetryAfter,
) -> axum::response::Response {
    protocol_error_response(
        protocol,
        MappedError::with_retry_after(
            PublicErrorCode::RateLimited,
            "request_rate_limited",
            retry_after.seconds(),
        ),
    )
}

fn protocol_error_response(protocol: Protocol, mapped: MappedError) -> axum::response::Response {
    log_error(protocol, mapped);
    let body = match protocol {
        Protocol::Anthropic => anthropic::encode_error(mapped.code, mapped.message()),
        Protocol::Gemini => gemini::encode_error(mapped.code, mapped.message()),
        Protocol::OpenAiChat
        | Protocol::OpenAiResponses
        | Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech
        | Protocol::JinaRerank
        | Protocol::CohereRerank
        | Protocol::XaiVideo => openai_chat::encode_error(mapped.code, mapped.message()),
    };
    let mut response = (mapped.status(), Json(body)).into_response();
    if let Some(retry_after) = mapped.retry_after {
        response.headers_mut().insert(
            RETRY_AFTER,
            HeaderValue::from_str(&retry_after.to_string())
                .expect("受控正整数秒数必须始终是有效响应头"),
        );
    }
    response
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MappedError {
    code: PublicErrorCode,
    error_kind: &'static str,
    retry_after: Option<u32>,
}

impl MappedError {
    pub(super) const fn new(code: PublicErrorCode, error_kind: &'static str) -> Self {
        Self {
            code,
            error_kind,
            retry_after: None,
        }
    }

    /// 构造带标准相对等待时间的限流响应映射。
    pub(super) const fn with_retry_after(
        code: PublicErrorCode,
        error_kind: &'static str,
        retry_after: u32,
    ) -> Self {
        Self {
            code,
            error_kind,
            retry_after: Some(retry_after),
        }
    }

    pub(super) const fn status(self) -> StatusCode {
        match self.code {
            PublicErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            PublicErrorCode::InvalidApiKey => StatusCode::UNAUTHORIZED,
            PublicErrorCode::InsufficientQuota | PublicErrorCode::RateLimited => {
                StatusCode::TOO_MANY_REQUESTS
            }
            PublicErrorCode::ModelNotFound | PublicErrorCode::TaskNotFound => StatusCode::NOT_FOUND,
            PublicErrorCode::IdempotencyConflict => StatusCode::CONFLICT,
            PublicErrorCode::RequestOutcomeUnknown | PublicErrorCode::UpstreamUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
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
            PublicErrorCode::TaskNotFound => "The requested task was not found.",
            PublicErrorCode::IdempotencyConflict => {
                "The idempotency key is already associated with a different request."
            }
            PublicErrorCode::RequestOutcomeUnknown => {
                "The request outcome is unknown. Retry with the same idempotency key."
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
        AfError::ModelNotAllowed => {
            MappedError::new(PublicErrorCode::ModelNotFound, "token_model_not_allowed")
        }
        AfError::TaskNotFound => MappedError::new(PublicErrorCode::TaskNotFound, "task_not_found"),
        AfError::IdempotencyConflict => {
            MappedError::new(PublicErrorCode::IdempotencyConflict, "idempotency_conflict")
        }
        AfError::RequestOutcomeUnknown => MappedError::new(
            PublicErrorCode::RequestOutcomeUnknown,
            "request_outcome_unknown",
        ),
        AfError::InsufficientQuota => {
            MappedError::new(PublicErrorCode::InsufficientQuota, "insufficient_quota")
        }
        AfError::QuotaWindowLimited { retry_after } => MappedError::with_retry_after(
            PublicErrorCode::RateLimited,
            "quota_window_limited",
            retry_after.seconds(),
        ),
        AfError::ConcurrencyLimited => {
            MappedError::new(PublicErrorCode::RateLimited, "concurrency_limited")
        }
        AfError::Upstream(error) => classify_upstream_error(error),
        AfError::Internal => MappedError::new(PublicErrorCode::InternalError, "internal"),
    }
}

/// 上游认证和额度属于供应侧状态，绝不能伪装成客户端 API Key 或用户额度错误。
fn classify_upstream_error(error: UpstreamError) -> MappedError {
    match error {
        UpstreamError::RateLimited { .. } => {
            MappedError::new(PublicErrorCode::RateLimited, "upstream_rate_limited")
        }
        UpstreamError::Overloaded { .. } => {
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
        UpstreamError::AccountDisabled => MappedError::new(
            PublicErrorCode::UpstreamUnavailable,
            "upstream_account_disabled",
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
        UpstreamError::Network { .. } => {
            MappedError::new(PublicErrorCode::UpstreamUnavailable, "upstream_network")
        }
    }
}

/// 错误事件只记录闭合分类，避免内部诊断和外部输入进入结构化日志。
fn log_error(protocol: Protocol, mapped: MappedError) {
    let status_code = u64::from(mapped.status().as_u16());
    if mapped.status().is_server_error() {
        tracing::error!(
            target: "af_http::error_response",
            status_code,
            protocol = protocol.as_str(),
            error_code = mapped.code.as_str(),
            error_kind = mapped.error_kind,
            "返回协议错误响应"
        );
    } else {
        tracing::warn!(
            target: "af_http::error_response",
            status_code,
            protocol = protocol.as_str(),
            error_code = mapped.code.as_str(),
            error_kind = mapped.error_kind,
            "返回协议错误响应"
        );
    }
}
