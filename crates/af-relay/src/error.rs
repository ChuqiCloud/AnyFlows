use af_adapter::{AdaptorError, AdaptorTransportError, HeaderMap, StatusCode};
use af_domain::{NetworkFailureKind, RateLimitScope, UpstreamError, UpstreamRetryAfter};
use af_protocol::{
    anthropic::{AnthropicUpstreamErrorKind, classify_error_response as classify_anthropic_error},
    gemini::{GeminiUpstreamErrorKind, classify_error_response as classify_gemini_error},
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
        AdaptorError::Transport(error) => UpstreamError::network(match error {
            AdaptorTransportError::TargetResolution => NetworkFailureKind::Resolution,
            AdaptorTransportError::Connect => NetworkFailureKind::Connect,
            AdaptorTransportError::ConnectTimeout => NetworkFailureKind::ConnectTimeout,
            AdaptorTransportError::ReadTimeout => NetworkFailureKind::ReadTimeout,
            AdaptorTransportError::RequestTimeout => NetworkFailureKind::RequestTimeout,
            AdaptorTransportError::Request => NetworkFailureKind::Request,
            AdaptorTransportError::ResponseBody => NetworkFailureKind::ResponseBody,
            AdaptorTransportError::InvalidRequestTarget
            | AdaptorTransportError::UnsupportedRequestMethod
            | AdaptorTransportError::TargetAddressBlocked
            | AdaptorTransportError::RemoteDnsDenied => NetworkFailureKind::Policy,
            _ => NetworkFailureKind::Request,
        }),
        AdaptorError::InvalidResponseHeader | AdaptorError::ResponseBodyTooLarge => {
            UpstreamError::ProtocolError
        }
        _ => UpstreamError::ProtocolError,
    }
}

/// 从标准 `Retry-After` 的整数秒形式提取受上限保护的重试提示。
///
/// HTTP-date 和供应商私有 Header 暂不猜测；无法无歧义解析时返回 `None`，由状态仓储
/// 使用自身的保守默认冷却。
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<UpstreamRetryAfter> {
    let value = headers.get("retry-after")?.to_str().ok()?;
    if value.is_empty() || value.trim() != value {
        return None;
    }
    UpstreamRetryAfter::from_seconds(value.parse().ok()?)
}

/// 按状态码和已审定的上游结构化错误信号得出脱敏领域分类。
pub(crate) fn classify_upstream_status(
    status: StatusCode,
    retry_after: Option<UpstreamRetryAfter>,
    body: &[u8],
) -> UpstreamError {
    match classify_gemini_error(status.as_u16(), body) {
        GeminiUpstreamErrorKind::InvalidRequest => return UpstreamError::BadRequest,
        GeminiUpstreamErrorKind::Authentication => return UpstreamError::AuthExpired,
        GeminiUpstreamErrorKind::Permission => return UpstreamError::ProtocolError,
        GeminiUpstreamErrorKind::ModelUnsupported => {
            return UpstreamError::ModelUnsupported;
        }
        GeminiUpstreamErrorKind::RateLimited => {
            return rate_limited(RateLimitScope::Unknown, retry_after);
        }
        GeminiUpstreamErrorKind::Overloaded => return overloaded(retry_after),
        GeminiUpstreamErrorKind::Server => {
            return UpstreamError::ServerError {
                status: af_domain::UpstreamServerStatus::new(status.as_u16())
                    .expect("Gemini Server 分类必须携带 5xx 状态"),
            };
        }
        GeminiUpstreamErrorKind::Unknown => {}
    }

    let anthropic_signal = classify_anthropic_error(body);
    match (status, anthropic_signal) {
        (
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
            AnthropicUpstreamErrorKind::Authentication,
        ) => return UpstreamError::AuthExpired,
        (
            StatusCode::BAD_REQUEST | StatusCode::PAYMENT_REQUIRED | StatusCode::TOO_MANY_REQUESTS,
            AnthropicUpstreamErrorKind::Billing,
        ) => return UpstreamError::QuotaExhausted,
        (StatusCode::TOO_MANY_REQUESTS, AnthropicUpstreamErrorKind::RateLimited) => {
            return rate_limited(RateLimitScope::Window, retry_after);
        }
        (status, AnthropicUpstreamErrorKind::Overloaded)
            if matches!(status.as_u16(), 500..=599) =>
        {
            return overloaded(retry_after);
        }
        (
            StatusCode::BAD_REQUEST
            | StatusCode::PAYLOAD_TOO_LARGE
            | StatusCode::UNPROCESSABLE_ENTITY,
            AnthropicUpstreamErrorKind::InvalidRequest,
        ) => return UpstreamError::BadRequest,
        (StatusCode::NOT_FOUND, AnthropicUpstreamErrorKind::NotFound) => {
            return UpstreamError::ModelUnsupported;
        }
        _ => {}
    }

    let openai_signal = classify_openai_error(body);
    match (status, openai_signal) {
        (
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
            OpenAiUpstreamErrorKind::AuthRevoked,
        ) => return UpstreamError::AuthRevoked,
        (
            StatusCode::BAD_REQUEST
            | StatusCode::UNAUTHORIZED
            | StatusCode::PAYMENT_REQUIRED
            | StatusCode::FORBIDDEN,
            OpenAiUpstreamErrorKind::AccountDisabled,
        ) => return UpstreamError::AccountDisabled,
        (StatusCode::TOO_MANY_REQUESTS, OpenAiUpstreamErrorKind::QuotaExhausted) => {
            return UpstreamError::QuotaExhausted;
        }
        (StatusCode::TOO_MANY_REQUESTS, OpenAiUpstreamErrorKind::RateLimited { scope }) => {
            return rate_limited(scope, retry_after);
        }
        (StatusCode::NOT_FOUND, OpenAiUpstreamErrorKind::ModelUnsupported) => {
            return UpstreamError::ModelUnsupported;
        }
        _ => {}
    }

    match status.as_u16() {
        400 | 413 | 422 => UpstreamError::BadRequest,
        // 未知 401/403 可能来自 IP 或区域策略，不能据此永久禁用凭据。
        401 => UpstreamError::AuthExpired,
        403 => UpstreamError::ProtocolError,
        402 => UpstreamError::QuotaExhausted,
        408 => UpstreamError::network(NetworkFailureKind::RequestTimeout),
        // 未知 404 更可能是路径配置错误；未知 429 只按可恢复限流处理。
        404 => UpstreamError::ProtocolError,
        429 => rate_limited(RateLimitScope::Unknown, retry_after),
        529 => overloaded(retry_after),
        value @ 500..=599 => UpstreamError::ServerError {
            status: af_domain::UpstreamServerStatus::new(value)
                .expect("500..=599 必须始终满足上游服务器状态约束"),
        },
        _ => UpstreamError::ProtocolError,
    }
}

const fn rate_limited(
    scope: RateLimitScope,
    retry_after: Option<UpstreamRetryAfter>,
) -> UpstreamError {
    UpstreamError::RateLimited { scope, retry_after }
}

const fn overloaded(retry_after: Option<UpstreamRetryAfter>) -> UpstreamError {
    UpstreamError::Overloaded { retry_after }
}

/// 判定当前候选失败后能否继续选路；健康惩罚、刷新和禁用由调度层处理。
pub(crate) const fn is_retryable(error: UpstreamError) -> bool {
    matches!(
        error,
        UpstreamError::RateLimited { .. }
            | UpstreamError::Overloaded { .. }
            | UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::AccountDisabled
            | UpstreamError::QuotaExhausted
            | UpstreamError::ModelUnsupported
            | UpstreamError::Network { .. }
            | UpstreamError::ServerError { .. }
    )
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;

    use af_adapter::{HeaderName, HeaderValue};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use url::Url;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    #[test]
    fn adaptor_transport_errors_keep_only_the_stable_failure_stage() {
        for (transport, expected) in [
            (
                AdaptorTransportError::TargetResolution,
                NetworkFailureKind::Resolution,
            ),
            (
                AdaptorTransportError::ConnectTimeout,
                NetworkFailureKind::ConnectTimeout,
            ),
            (
                AdaptorTransportError::ReadTimeout,
                NetworkFailureKind::ReadTimeout,
            ),
            (
                AdaptorTransportError::RequestTimeout,
                NetworkFailureKind::RequestTimeout,
            ),
            (
                AdaptorTransportError::TargetAddressBlocked,
                NetworkFailureKind::Policy,
            ),
            (
                AdaptorTransportError::ResponseBody,
                NetworkFailureKind::ResponseBody,
            ),
        ] {
            assert_eq!(
                map_adaptor_error(AdaptorError::Transport(transport)),
                UpstreamError::network(expected)
            );
        }
    }

    #[test]
    fn gemini_error_classifier_uses_only_structured_code_and_status() {
        for (status, signal, expected) in [
            (400, "INVALID_ARGUMENT", UpstreamError::BadRequest),
            (401, "UNAUTHENTICATED", UpstreamError::AuthExpired),
            (403, "PERMISSION_DENIED", UpstreamError::ProtocolError),
            (404, "NOT_FOUND", UpstreamError::ModelUnsupported),
            (
                429,
                "RESOURCE_EXHAUSTED",
                UpstreamError::rate_limited(RateLimitScope::Unknown),
            ),
            (503, "UNAVAILABLE", UpstreamError::overloaded()),
        ] {
            let status = StatusCode::from_u16(status).unwrap();
            let body = format!(
                r#"{{"error":{{"code":{},"message":"private-canary","status":"{signal}","details":[{{"private":"canary"}}]}}}}"#,
                status.as_u16()
            );
            assert_eq!(
                classify_upstream_status(status, None, body.as_bytes()),
                expected
            );
        }
    }

    #[test]
    fn gemini_error_classifier_rejects_mismatched_or_unstructured_signals() {
        assert_eq!(
            classify_gemini_error(
                StatusCode::TOO_MANY_REQUESTS.as_u16(),
                br#"{"error":{"code":503,"status":"UNAVAILABLE"}}"#,
            ),
            GeminiUpstreamErrorKind::Unknown
        );
        assert_eq!(
            classify_gemini_error(
                StatusCode::TOO_MANY_REQUESTS.as_u16(),
                br#"{"error":{"code":429,"message":"RESOURCE_EXHAUSTED"}}"#,
            ),
            GeminiUpstreamErrorKind::Unknown
        );
        assert_eq!(
            classify_upstream_status(
                StatusCode::TOO_MANY_REQUESTS,
                None,
                br#"{"error":{"code":503,"status":"UNAVAILABLE"}}"#,
            ),
            UpstreamError::rate_limited(RateLimitScope::Unknown)
        );
    }

    #[test]
    fn retry_after_accepts_only_bounded_integer_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "90".parse().unwrap());
        assert_eq!(parse_retry_after(&headers).unwrap().seconds(), 90);

        for invalid in ["0", " 90", "Wed, 21 Oct 2015 07:28:00 GMT", "604801"] {
            headers.insert("retry-after", invalid.parse().unwrap());
            assert_eq!(parse_retry_after(&headers), None, "未拒绝 {invalid}");
        }
    }

    #[tokio::test]
    async fn wiremock_matrix_keeps_provider_errors_structured_and_redacted() {
        struct Case {
            path: &'static str,
            status: u16,
            body: &'static str,
            retry_after: Option<&'static str>,
            expected: UpstreamError,
        }

        let retry_after = UpstreamRetryAfter::from_seconds(17).unwrap();
        let cases = [
            Case {
                path: "/openai-rate-limit",
                status: 429,
                body: r#"{"error":{"code":"rate_limit_exceeded","message":"openai-secret"}}"#,
                retry_after: Some("17"),
                expected: UpstreamError::rate_limited_after(RateLimitScope::Window, retry_after),
            },
            Case {
                path: "/openai-revoked",
                status: 401,
                body: r#"{"error":{"code":"invalid_api_key","message":"revoked-secret"}}"#,
                retry_after: None,
                expected: UpstreamError::AuthRevoked,
            },
            Case {
                path: "/anthropic-overloaded",
                status: 529,
                body: r#"{"type":"error","error":{"type":"overloaded_error","message":"anthropic-secret"}}"#,
                retry_after: Some("17"),
                expected: UpstreamError::overloaded_after(retry_after),
            },
            Case {
                path: "/anthropic-billing",
                status: 400,
                body: r#"{"type":"error","error":{"type":"billing_error","message":"billing-secret"}}"#,
                retry_after: None,
                expected: UpstreamError::QuotaExhausted,
            },
            Case {
                path: "/gemini-rate-limit",
                status: 429,
                body: r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","message":"gemini-secret"}}"#,
                retry_after: Some("17"),
                expected: UpstreamError::rate_limited_after(RateLimitScope::Unknown, retry_after),
            },
            Case {
                path: "/generic-server-error",
                status: 503,
                body: r#"{"error":{"message":"server-secret"}}"#,
                retry_after: None,
                expected: UpstreamError::ServerError {
                    status: af_domain::UpstreamServerStatus::new(503).unwrap(),
                },
            },
        ];

        let server = MockServer::start().await;
        for case in &cases {
            let mut response = ResponseTemplate::new(case.status).set_body_string(case.body);
            if let Some(retry_after) = case.retry_after {
                response = response.insert_header("retry-after", retry_after);
            }
            Mock::given(method("GET"))
                .and(wiremock::matchers::path(case.path))
                .respond_with(response)
                .mount(&server)
                .await;
        }

        for case in cases {
            let (status, headers, body) = fetch_wiremock_response(&server, case.path).await;
            let retry_after = parse_retry_after(&headers);
            let classified = classify_upstream_status(status, retry_after, &body);

            assert_eq!(classified, case.expected, "mock path {}", case.path);
            let debug = format!("{classified:?}");
            assert!(!debug.contains("secret"), "分类泄露了 mock 正文");
        }
    }

    async fn fetch_wiremock_response(
        server: &MockServer,
        path: &str,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let endpoint = Url::parse(&format!("{}{}", server.uri(), path)).unwrap();
        let address = SocketAddr::new(
            endpoint.host().unwrap().to_string().parse().unwrap(),
            endpoint.port().unwrap(),
        );
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                    endpoint.host_str().unwrap()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap()
            + 4;
        let header_text = std::str::from_utf8(&response[..header_end]).unwrap();
        let status = StatusCode::from_u16(
            header_text
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse()
                .unwrap(),
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        if let Some(value) = header_text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("retry-after")
                .then_some(value.trim())
        }) {
            headers.insert(
                HeaderName::from_static("retry-after"),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        (status, headers, response[header_end..].to_vec())
    }
}
