use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_adapter::{
    Adaptor, AdaptorSettings, ChannelType, Credential, Method, OpenAiAdaptorSettings, Operation,
    PooledClient, RelayContext, ResponseMode, StatusCode, get_adaptor,
};
use af_domain::{AfError, GatewayPrincipal, MAX_MODEL_NAME_BYTES, Protocol, UpstreamError};
use af_protocol::{CanonicalRequestEnvelope, openai_chat::parse_response};

use crate::attempt_gate::permit_completion_hook;
use crate::{
    ChatResponse, OpenAiChatStream, RelayAttemptReport, RelayBuildError, RelayCandidate,
    RelayError, RelayRequest, RelayStateMachine,
    chat_response::encode_full_response,
    chat_stream_encoder::ChatStreamEncoder,
    error::{classify_upstream_status, map_adaptor_error, parse_retry_after},
    openai_chat_request::{PreparedOpenAiChatRequest, prepare_openai_chat_request},
    openai_chat_usage::OpenAiChatUsageEstimator,
};

const STREAM_FIRST_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 静态单 OpenAI-compatible 上游的完整与流式转发服务。
///
/// 本类型刻意不包含鉴权、计费、调度、重试或数据库渠道读取；这些能力将在 M1
/// 替换静态单上游装配，但 HTTP 层仍只依赖本 crate 的转发入口。
#[derive(Clone)]
pub struct RelayService {
    chat_adaptor: Arc<dyn Adaptor>,
    responses_adaptor: Arc<dyn Adaptor>,
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
        let chat_adaptor = get_adaptor(
            ChannelType::OpenAi,
            AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(vec![model.to_owned()])),
        )?;
        let responses_adaptor = get_adaptor(
            ChannelType::OpenAi,
            AdaptorSettings::OpenAi(OpenAiAdaptorSettings::for_protocol(
                Protocol::OpenAiResponses,
                vec![model.to_owned()],
            )?),
        )?;
        Ok(Self {
            chat_adaptor,
            responses_adaptor,
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
        request: impl Into<CanonicalRequestEnvelope>,
        response_protocol: Protocol,
        request_id: &str,
    ) -> Result<ChatResponse, AfError> {
        let request = request.into();
        if request.canonical().model != self.model {
            return Err(UpstreamError::ModelUnsupported.into());
        }
        let context = self
            .context
            .clone()
            .with_request_id(request_id.to_owned())
            .map_err(|_| AfError::Internal)?;
        let adaptor = match request.canonical().operation {
            Operation::Chat => self.chat_adaptor.clone(),
            Operation::Responses => self.responses_adaptor.clone(),
            _ => return Err(AfError::InvalidRequest),
        };
        let machine = RelayStateMachine::new(vec![RelayCandidate::new(
            adaptor,
            context,
            self.credential.clone(),
        )])
        .map_err(|_| AfError::Internal)?;
        match request.canonical().operation {
            Operation::Chat => {
                relay_openai_chat(&machine, request, response_protocol, request_id).await
            }
            Operation::Responses if response_protocol == Protocol::OpenAiResponses => {
                crate::relay_openai_responses(&machine, request, request_id).await
            }
            Operation::Responses => Err(AfError::InvalidRequest),
            _ => Err(AfError::InvalidRequest),
        }
    }
}

/// 使用已按请求固定顺序装配的候选状态机执行 OpenAI Chat 转发。
///
/// 调度、凭据解密和代理绑定必须在调用前完成；本函数只负责协议请求构造、候选故障转移、
/// 响应校验及 usage 提取，不读取数据库或持有计费会话。
pub async fn relay_openai_chat(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    response_protocol: Protocol,
    request_id: &str,
) -> Result<ChatResponse, AfError> {
    relay_openai_chat_with_report(machine, request, response_protocol, request_id)
        .await
        .into_result()
}

/// 执行 OpenAI Chat 转发并保留通过协议验证后的脱敏候选报告。
pub async fn relay_openai_chat_with_report(
    machine: &RelayStateMachine,
    request: impl Into<CanonicalRequestEnvelope>,
    response_protocol: Protocol,
    request_id: &str,
) -> RelayOpenAiChatOutcome {
    let PreparedOpenAiChatRequest {
        canonical,
        body: outbound_body,
        include_usage,
        client_model,
    } = match prepare_openai_chat_request(request.into()) {
        Ok(prepared) => prepared,
        Err(error) => return RelayOpenAiChatOutcome::failed(error),
    };
    // 先由协议构造器完成全部容量与结构校验，再让 tokenizer 处理规范化请求。
    let usage_estimator = OpenAiChatUsageEstimator::new(&canonical);
    if let Err(error) = validate_downstream_request(response_protocol, &canonical) {
        return RelayOpenAiChatOutcome::failed(error);
    }
    let stream_encoder = if canonical.stream {
        let created_at = match unix_timestamp() {
            Ok(created_at) => created_at,
            Err(error) => return RelayOpenAiChatOutcome::failed(error),
        };
        match ChatStreamEncoder::new(
            response_protocol,
            request_id,
            &client_model,
            created_at,
            &canonical,
            include_usage,
        ) {
            Ok(encoder) => Some(encoder),
            Err(error) => return RelayOpenAiChatOutcome::failed(error),
        }
    } else {
        None
    };
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
    let (response, mut report, mut permit) = match machine.execute_with_report(request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiChatOutcome::new(Err(map_relay_error(error)), report);
        }
    };
    let status = response.status();
    if status != StatusCode::OK {
        let retry_after = parse_retry_after(response.headers());
        let body = match response.into_body().into_bytes().await {
            Ok(body) => body,
            Err(error) => {
                let error = map_adaptor_error(error);
                report.reject_success(error);
                return RelayOpenAiChatOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiChatOutcome::new(Err(error.into()), report);
    }

    if canonical.stream {
        let encoder = stream_encoder.expect("流式请求必须已经创建下游编码器");
        let (stream, usage) = OpenAiChatStream::new(response.into_body(), encoder, usage_estimator);
        return match stream.prefetch_first(STREAM_FIRST_EVENT_TIMEOUT).await {
            Ok(stream) => {
                let mut body: Box<dyn crate::GenerationStream> = Box::new(stream);
                if let Some(permit) = permit.take() {
                    body = body.with_completion_hook(permit_completion_hook(permit));
                }
                RelayOpenAiChatOutcome::new(Ok(ChatResponse::Stream { body, usage }), report)
            }
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
    match encode_full_response(
        response_protocol,
        body,
        &canonical_response,
        usage,
        &client_model,
        request_id,
    ) {
        Ok(response) => RelayOpenAiChatOutcome::new(Ok(response), report),
        Err(error) => {
            report.reject_success(UpstreamError::ProtocolError);
            RelayOpenAiChatOutcome::new(Err(error), report)
        }
    }
}

/// OpenAI Chat 业务结果及其脱敏候选尝试报告。
pub struct RelayOpenAiChatOutcome {
    result: Result<ChatResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiChatOutcome {
    fn new(result: Result<ChatResponse, AfError>, report: RelayAttemptReport) -> Self {
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
    pub fn into_result(self) -> Result<ChatResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与尝试报告。
    pub fn into_parts(self) -> (Result<ChatResponse, AfError>, RelayAttemptReport) {
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
            .field("adaptors", &"<OpenAI-compatible>")
            .field("context", &self.context)
            .field("credential", &self.credential)
            .field("model", &"<已脱敏>")
            .finish()
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

/// 在发送上游请求前拒绝目标协议无法可靠构造的响应语义。
fn validate_downstream_request(
    protocol: Protocol,
    request: &af_protocol::CanonicalRequest,
) -> Result<(), AfError> {
    match protocol {
        Protocol::OpenAiChat => Ok(()),
        Protocol::Anthropic => {
            // OpenAI Chat 不提供 Anthropic thinking 签名，不能在响应阶段伪造或静默丢弃。
            if request
                .reasoning
                .is_some_and(|reasoning| reasoning.include_thinking())
            {
                return Err(AfError::InvalidRequest);
            }
            Ok(())
        }
        Protocol::Gemini => Ok(()),
        Protocol::OpenAiResponses
        | Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech
        | Protocol::JinaRerank
        | Protocol::CohereRerank
        | Protocol::XaiVideo => Err(AfError::Internal),
    }
}

pub(crate) fn unix_timestamp() -> Result<i64, AfError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AfError::Internal)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| AfError::Internal)
}

fn map_relay_error(error: RelayError) -> AfError {
    match error {
        RelayError::InvalidModel => AfError::InvalidRequest,
        RelayError::Request(_) | RelayError::Adaptor(_) | RelayError::AttemptGateFailed => {
            AfError::Internal
        }
        RelayError::ConcurrencyUnavailable => AfError::ConcurrencyLimited,
        RelayError::Upstream(error) => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use af_adapter::Bytes;
    use af_domain::{RateLimitScope, UpstreamRetryAfter, UpstreamServerStatus};

    use super::*;
    use crate::UsageResolutionError;

    #[test]
    fn model_validation_matches_openai_protocol_boundary() {
        assert!(is_valid_model("gpt-test"));
        for model in ["", " gpt-test", "gpt-test ", "gpt\ntest"] {
            assert!(!is_valid_model(model));
        }
        assert!(!is_valid_model(&"x".repeat(MAX_MODEL_NAME_BYTES + 1)));
    }

    #[test]
    fn anthropic_downstream_rejects_unsigned_openai_thinking_before_dispatch() {
        let request = af_protocol::anthropic::parse_request(
            br#"{"model":"gpt-test","max_tokens":2048,"messages":[{"role":"user","content":"hello"}],"thinking":{"type":"enabled","budget_tokens":1024}}"#,
        )
        .unwrap();

        assert_eq!(
            validate_downstream_request(Protocol::Anthropic, &request),
            Err(AfError::InvalidRequest)
        );
        assert_eq!(
            validate_downstream_request(Protocol::OpenAiChat, &request),
            Ok(())
        );
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
                StatusCode::UNAUTHORIZED,
                br#"{"type":"error","error":{"type":"authentication_error","message":"private"}}"#
                    .as_slice(),
                UpstreamError::AuthExpired,
            ),
            (
                StatusCode::PAYMENT_REQUIRED,
                br#"{"detail":{"code":"deactivated_workspace","message":"private"}}"#.as_slice(),
                UpstreamError::AccountDisabled,
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
                StatusCode::BAD_REQUEST,
                br#"{"type":"error","error":{"type":"billing_error","message":"private"}}"#
                    .as_slice(),
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
                UpstreamError::rate_limited(RateLimitScope::Window),
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"type":"error","error":{"type":"rate_limit_error","message":"private"}}"#
                    .as_slice(),
                UpstreamError::rate_limited(RateLimitScope::Window),
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"code":"model_rate_limit_exceeded"}}"#.as_slice(),
                UpstreamError::rate_limited(RateLimitScope::Model),
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"type":"insufficient_quota"}}"#.as_slice(),
                UpstreamError::QuotaExhausted,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                b"{}".as_slice(),
                UpstreamError::rate_limited(RateLimitScope::Unknown),
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
            assert_eq!(classify_upstream_status(status, None, body), expected);
        }
    }

    #[test]
    fn retry_after_hint_is_attached_only_as_a_bounded_value() {
        let retry_after = UpstreamRetryAfter::from_seconds(90).unwrap();
        assert_eq!(
            classify_upstream_status(
                StatusCode::TOO_MANY_REQUESTS,
                Some(retry_after),
                br#"{"error":{"code":"rate_limit_exceeded"}}"#,
            ),
            UpstreamError::rate_limited_after(RateLimitScope::Window, retry_after)
        );
        assert_eq!(
            classify_upstream_status(StatusCode::from_u16(529).unwrap(), Some(retry_after), b"{}",),
            UpstreamError::overloaded_after(retry_after)
        );
    }

    #[test]
    fn verified_response_debug_never_contains_body() {
        let response = ChatResponse::Full {
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
