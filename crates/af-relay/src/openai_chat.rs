use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_adapter::{
    Adaptor, AdaptorSettings, Bytes, ChannelType, Credential, Method, OpenAiAdaptorSettings,
    Operation, PooledClient, RelayContext, ResponseMode, StatusCode, get_adaptor,
};
use af_domain::{AfError, GatewayPrincipal, MAX_MODEL_NAME_BYTES, UpstreamError};
use af_protocol::{
    CanonicalRequest, StreamOptions,
    openai_chat::{OpenAiChatStreamEncoder, build_request, parse_response},
};

use crate::{
    OpenAiChatStream, OpenAiChatUsageHandle, RelayAttemptReport, RelayBuildError, RelayCandidate,
    RelayError, RelayRequest, RelayStateMachine, UsageResolutionError,
    error::{classify_upstream_status, map_adaptor_error},
    openai_chat_usage::OpenAiChatUsageEstimator,
};

const STREAM_FIRST_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 静态单 OpenAI-compatible 上游的完整与流式转发服务。
///
/// 本类型刻意不包含鉴权、计费、调度、重试或数据库渠道读取；这些能力将在 M1
/// 替换静态单上游装配，但 HTTP 层仍只依赖本 crate 的转发入口。
#[derive(Clone)]
pub struct RelayService {
    adaptor: Arc<dyn Adaptor>,
    context: RelayContext,
    credential: Credential,
    model: String,
}

impl RelayService {
    /// 使用已创建的受控 Client 和启动期静态上游配置构造 M0 转发服务。
    pub fn new(
        client: PooledClient,
        base_url: &str,
        model: &str,
        api_key: &str,
    ) -> Result<Self, RelayBuildError> {
        if !is_valid_model(model) {
            return Err(RelayBuildError::InvalidModel);
        }
        let context = RelayContext::new(client).with_base_url(base_url)?;
        let credential = Credential::api_key(api_key)?;
        let adaptor = get_adaptor(
            ChannelType::OpenAi,
            AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(vec![model.to_owned()])),
        )?;
        Ok(Self {
            adaptor,
            context,
            credential,
            model: model.to_owned(),
        })
    }

    /// 转发一次已认证且完成入站规范化的请求，并返回经过协议校验的同协议响应。
    ///
    /// 身份参数用于强制调用方保留请求归属，当前静态单上游阶段不会把它写入
    /// `RelayContext`、适配器或上游请求。
    pub async fn chat_completions(
        &self,
        _principal: &GatewayPrincipal,
        canonical: CanonicalRequest,
        request_id: &str,
    ) -> Result<OpenAiChatResponse, AfError> {
        if canonical.model != self.model {
            return Err(UpstreamError::ModelUnsupported.into());
        }
        let context = self
            .context
            .clone()
            .with_request_id(request_id.to_owned())
            .map_err(|_| AfError::Internal)?;
        let machine = RelayStateMachine::new(vec![RelayCandidate::new(
            self.adaptor.clone(),
            context,
            self.credential.clone(),
        )])
        .map_err(|_| AfError::Internal)?;
        relay_openai_chat(&machine, canonical, request_id).await
    }
}

/// 使用已按请求固定顺序装配的候选状态机执行 OpenAI Chat 转发。
///
/// 调度、凭据解密和代理绑定必须在调用前完成；本函数只负责协议请求构造、候选故障转移、
/// 响应校验及 usage 提取，不读取数据库或持有计费会话。
pub async fn relay_openai_chat(
    machine: &RelayStateMachine,
    canonical: CanonicalRequest,
    request_id: &str,
) -> Result<OpenAiChatResponse, AfError> {
    relay_openai_chat_with_report(machine, canonical, request_id)
        .await
        .into_result()
}

/// 执行 OpenAI Chat 转发并保留通过协议验证后的脱敏候选报告。
pub async fn relay_openai_chat_with_report(
    machine: &RelayStateMachine,
    mut canonical: CanonicalRequest,
    request_id: &str,
) -> RelayOpenAiChatOutcome {
    let include_usage = canonical.stream_options.include_usage();
    if canonical.stream {
        // 上游 usage 是内部结算优先数据；客户端是否可见由 include_usage 单独控制。
        canonical.stream_options = StreamOptions::new(true);
    }
    let outbound_value = match build_request(&canonical) {
        Ok(value) => value,
        Err(_) => return RelayOpenAiChatOutcome::failed(AfError::InvalidRequest),
    };
    let outbound_body = match serde_json::to_vec(&outbound_value).map(Bytes::from) {
        Ok(body) => body,
        Err(_) => return RelayOpenAiChatOutcome::failed(AfError::Internal),
    };
    // 先由协议构造器完成全部容量与结构校验，再让 tokenizer 处理规范化请求。
    let usage_estimator = OpenAiChatUsageEstimator::new(&canonical);
    let response_mode = if canonical.stream {
        ResponseMode::Stream
    } else {
        ResponseMode::Full
    };
    let request = match RelayRequest::new(
        canonical.model.clone(),
        Operation::Chat,
        Method::POST,
        Some(outbound_body),
    )
    .map(|request| request.with_response_mode(response_mode))
    {
        Ok(request) => request,
        Err(error) => return RelayOpenAiChatOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report) = match machine.execute_with_report(request).await {
        Ok(response) => response.into_parts(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiChatOutcome::new(Err(map_relay_error(error)), report);
        }
    };
    let status = response.status();
    if status != StatusCode::OK {
        let body = match response.into_body().into_bytes().await {
            Ok(body) => body,
            Err(error) => {
                let error = map_adaptor_error(error);
                report.reject_success(error);
                return RelayOpenAiChatOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, &body);
        report.reject_success(error);
        return RelayOpenAiChatOutcome::new(Err(error.into()), report);
    }

    if canonical.stream {
        let created = match unix_timestamp() {
            Ok(created) => created,
            Err(error) => return RelayOpenAiChatOutcome::new(Err(error), report),
        };
        let response_id = format!("chatcmpl-{request_id}");
        let encoder = match OpenAiChatStreamEncoder::new(response_id, &canonical.model, created) {
            Ok(encoder) => encoder,
            Err(_) => return RelayOpenAiChatOutcome::new(Err(AfError::Internal), report),
        };
        let (stream, usage) = OpenAiChatStream::new(
            response.into_body(),
            encoder,
            usage_estimator,
            include_usage,
        );
        return match stream.prefetch_first(STREAM_FIRST_EVENT_TIMEOUT).await {
            Ok(stream) => RelayOpenAiChatOutcome::new(
                Ok(OpenAiChatResponse::Stream {
                    body: Box::new(stream),
                    usage,
                }),
                report,
            ),
            Err(error) => {
                report.reject_success(error);
                RelayOpenAiChatOutcome::new(Err(error.into()), report)
            }
        };
    }

    let body = match response.into_body().into_bytes().await {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiChatOutcome::new(Err(error.into()), report);
        }
    };
    let canonical_response = match parse_response(&body) {
        Ok(response) => response,
        Err(_) => {
            let error = UpstreamError::ProtocolError;
            report.reject_success(error);
            return RelayOpenAiChatOutcome::new(Err(error.into()), report);
        }
    };
    let usage = usage_estimator.resolve_response(&canonical_response);
    RelayOpenAiChatOutcome::new(Ok(OpenAiChatResponse::Full { body, usage }), report)
}

/// OpenAI Chat 业务结果及其脱敏候选尝试报告。
pub struct RelayOpenAiChatOutcome {
    result: Result<OpenAiChatResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiChatOutcome {
    fn new(result: Result<OpenAiChatResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 返回只含候选索引与闭合错误分类的尝试报告。
    #[must_use]
    pub const fn report(&self) -> &RelayAttemptReport {
        &self.report
    }

    /// 消费包装并返回原有业务结果。
    pub fn into_result(self) -> Result<OpenAiChatResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与尝试报告。
    pub fn into_parts(self) -> (Result<OpenAiChatResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayOpenAiChatOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiChatOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

impl fmt::Debug for RelayService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayService")
            .field("adaptor", &"<OpenAI-compatible>")
            .field("context", &self.context)
            .field("credential", &self.credential)
            .field("model", &"<已脱敏>")
            .finish()
    }
}

/// 已完成 OpenAI Chat 协议校验的完整或流式同协议响应。
pub enum OpenAiChatResponse {
    /// 完整解析通过后保留的非流式 JSON 字节。
    Full {
        /// 写入下游的原始同协议响应。
        body: Bytes,
        /// 上游 usage 或本地受控估算结果。
        usage: Result<af_protocol::Usage, UsageResolutionError>,
    },
    /// 按 Canonical 事件逐段转换的 SSE 字节流。
    Stream {
        /// 写入下游的 Canonical 重编码流。
        body: Box<OpenAiChatStream>,
        /// 流完整结束后只可消费一次的 usage 结果。
        usage: OpenAiChatUsageHandle,
    },
}

impl fmt::Debug for OpenAiChatResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full { body, usage: _ } => formatter
                .debug_struct("OpenAiChatResponse")
                .field("mode", &"full")
                .field("body_bytes", &body.len())
                .finish(),
            Self::Stream { body, usage: _ } => formatter
                .debug_struct("OpenAiChatResponse")
                .field("mode", &"stream")
                .field("body", body)
                .finish(),
        }
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn unix_timestamp() -> Result<i64, AfError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AfError::Internal)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| AfError::Internal)
}

fn map_relay_error(error: RelayError) -> AfError {
    match error {
        RelayError::InvalidModel => AfError::InvalidRequest,
        RelayError::Request(_) | RelayError::Adaptor(_) => AfError::Internal,
        RelayError::Upstream(error) => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use af_domain::UpstreamServerStatus;

    use super::*;

    #[test]
    fn model_validation_matches_openai_protocol_boundary() {
        assert!(is_valid_model("gpt-test"));
        for model in ["", " gpt-test", "gpt-test ", "gpt\ntest"] {
            assert!(!is_valid_model(model));
        }
        assert!(!is_valid_model(&"x".repeat(MAX_MODEL_NAME_BYTES + 1)));
    }

    #[test]
    fn status_and_structured_signals_map_to_closed_domain_errors() {
        let cases = [
            (
                StatusCode::BAD_REQUEST,
                b"{}".as_slice(),
                UpstreamError::BadRequest,
            ),
            (
                StatusCode::UNAUTHORIZED,
                b"{}".as_slice(),
                UpstreamError::AuthExpired,
            ),
            (
                StatusCode::UNAUTHORIZED,
                br#"{"error":{"code":"invalid_api_key"}}"#.as_slice(),
                UpstreamError::AuthRevoked,
            ),
            (
                StatusCode::FORBIDDEN,
                b"{}".as_slice(),
                UpstreamError::ProtocolError,
            ),
            (
                StatusCode::PAYMENT_REQUIRED,
                b"{}".as_slice(),
                UpstreamError::QuotaExhausted,
            ),
            (
                StatusCode::NOT_FOUND,
                br#"{"error":{"code":"model_not_found"}}"#.as_slice(),
                UpstreamError::ModelUnsupported,
            ),
            (
                StatusCode::NOT_FOUND,
                b"{}".as_slice(),
                UpstreamError::ProtocolError,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"code":"rate_limit_exceeded"}}"#.as_slice(),
                UpstreamError::RateLimited,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"type":"insufficient_quota"}}"#.as_slice(),
                UpstreamError::QuotaExhausted,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                b"{}".as_slice(),
                UpstreamError::Overloaded,
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                b"{}".as_slice(),
                UpstreamError::ServerError {
                    status: UpstreamServerStatus::new(503).unwrap(),
                },
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                br#"{"error":{"code":"model_not_found"}}"#.as_slice(),
                UpstreamError::ServerError {
                    status: UpstreamServerStatus::new(503).unwrap(),
                },
            ),
            (
                StatusCode::NO_CONTENT,
                b"{}".as_slice(),
                UpstreamError::ProtocolError,
            ),
        ];
        for (status, body, expected) in cases {
            assert_eq!(classify_upstream_status(status, body), expected);
        }
    }

    #[test]
    fn verified_response_debug_never_contains_body() {
        let response = OpenAiChatResponse::Full {
            body: Bytes::from_static(b"response-body-secret"),
            usage: Err(UsageResolutionError::EstimationFailed),
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("body_bytes"));
        assert!(!debug.contains("response-body-secret"));
    }

    #[test]
    fn response_boundary_errors_map_without_payloads() {
        assert_eq!(
            AfError::from(map_adaptor_error(
                af_adapter::AdaptorError::InvalidResponseHeader
            )),
            AfError::from(UpstreamError::ProtocolError)
        );
        assert_eq!(
            AfError::from(map_adaptor_error(
                af_adapter::AdaptorError::ResponseBodyTooLarge
            )),
            AfError::from(UpstreamError::ProtocolError)
        );
    }
}
