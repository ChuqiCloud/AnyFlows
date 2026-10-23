use std::{
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

#[cfg(test)]
#[path = "tool_conversation_tests.rs"]
mod tests;

use af_adapter::{Bytes, Method, ResponseMode, UpstreamBody};
use af_domain::{AfError, NetworkFailureKind, Protocol, UpstreamError};
use af_protocol::{
    CanonicalRequestEnvelope, TokenCount, ToolCompatibilitySession, ToolDef, Usage, UsageDetails,
    UsageSemantics, UsageSource,
};

use crate::{
    ChatResponse, GenerationCompletionFuture, GenerationCompletionHook, OpenAiChatStream,
    RelayAttemptDiagnostic, RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine,
    UsageResolutionError,
    attempt_gate::permit_completion_hook,
    chat_stream_encoder::ChatStreamEncoder,
    conversation::{build_response, prepare_request},
    conversation_aggregate::Aggregate,
    conversation_source::ConversationSource,
    estimate_openai_request_upper_bound,
    openai_chat::unix_timestamp,
    openai_chat_usage::OpenAiChatUsageEstimator,
    tool_conversation_stream::ToolConversationStream,
};

/// 兼容模式的请求级执行计划。工具必须来自程序注册实现，不能使用未经校验的渠道正文。
/// 创建计划不会发起网络请求；上层须按 upper_bound 预扣后再消费执行。
pub struct ToolConversationPlan {
    pub(crate) session: ToolCompatibilitySession,
    pub(crate) upstream_protocol: Protocol,
    pub(crate) upper_bound: Usage,
    total_timeout: Duration,
    downstream_include_usage: bool,
}

impl ToolConversationPlan {
    /// 为两次上下文输入及共享输出上限预留额度；额外上下文只允许有界增长。
    /// 实际续答编码前再次估算，超过冻结输入预算则不发送第二轮。
    pub fn prepare(
        request: CanonicalRequestEnvelope,
        upstream_protocol: Protocol,
        tools: Vec<ToolDef>,
        continuation_input_allowance: TokenCount,
        total_timeout: Duration,
    ) -> Result<Self, AfError> {
        if !matches!(
            upstream_protocol,
            Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic
        ) || continuation_input_allowance.get() > 1_048_576
            || total_timeout.is_zero()
            || total_timeout > Duration::from_secs(600)
        {
            return Err(AfError::InvalidRequest);
        }
        let downstream_include_usage = request.canonical().stream_options.include_usage();
        let request = request
            .for_conversation_protocol(upstream_protocol)
            .ok_or(AfError::InvalidRequest)?;
        let session = ToolCompatibilitySession::prepare(request, tools)
            .map_err(|_| AfError::InvalidRequest)?;
        let first = estimate_openai_request_upper_bound(session.request().canonical())
            .map_err(|_| AfError::InvalidRequest)?;
        let input = first
            .input_tokens()
            .get()
            .checked_mul(2)
            .and_then(|tokens| tokens.checked_add(first.output_tokens().get()))
            .and_then(|tokens| tokens.checked_add(continuation_input_allowance.get()))
            .and_then(|tokens| TokenCount::new(tokens).ok())
            .ok_or(AfError::InvalidRequest)?;
        let upper_bound = Usage::new(
            input,
            first.output_tokens(),
            UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Estimated,
            UsageSemantics::Inclusive,
        )
        .map_err(|_| AfError::InvalidRequest)?;
        upper_bound
            .checked_total_tokens()
            .map_err(|_| AfError::InvalidRequest)?;
        Ok(Self {
            session,
            upstream_protocol,
            upper_bound,
            total_timeout,
            downstream_include_usage,
        })
    }

    #[must_use]
    pub const fn request(&self) -> &CanonicalRequestEnvelope {
        self.session.request()
    }

    /// 包含注入定义、最多两轮输入和一份总输出预算，按次计费仍只创建一次生命周期。
    #[must_use]
    pub const fn upper_bound(&self) -> Usage {
        self.upper_bound
    }

    pub(crate) const fn total_timeout(&self) -> Duration {
        self.total_timeout
    }
}

impl fmt::Debug for ToolConversationPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ToolConversationPlan")
            .field("protocol", &self.upstream_protocol)
            .field("upper_bound", &self.upper_bound)
            .field("total_timeout", &self.total_timeout)
            .finish_non_exhaustive()
    }
}

/// HTTP 200 仍属于进行中；只有完整协议终态才记为完成。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolConversationState {
    Pending,
    Completed,
    Failed,
}

/// 仅保存安全诊断与已知用量；失败不补零，也不把部分消耗当作成功结算。
#[derive(Clone, Debug)]
pub struct ToolConversationReport {
    pub report: RelayAttemptReport,
    pub state: ToolConversationState,
    pub observed_usage: [Option<Usage>; 2],
    pub sent_rounds: u8,
    pub continuation_diagnostic: Option<RelayAttemptDiagnostic>,
}

#[derive(Clone)]
pub struct ToolConversationReportHandle(Arc<Mutex<ToolConversationReport>>);

impl ToolConversationReportHandle {
    fn new(report: RelayAttemptReport) -> Self {
        Self(Arc::new(Mutex::new(ToolConversationReport {
            report,
            state: ToolConversationState::Pending,
            observed_usage: [None, None],
            sent_rounds: 0,
            continuation_diagnostic: None,
        })))
    }

    #[must_use]
    pub fn snapshot(&self) -> ToolConversationReport {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 消费方取消时关闭尚未完成的报告，避免将初始 HTTP 200 记为成功。
    pub fn cancel(&self) {
        self.update(|snapshot| {
            if snapshot.state == ToolConversationState::Pending {
                snapshot.state = ToolConversationState::Failed;
                snapshot
                    .report
                    .reject_success(UpstreamError::network(NetworkFailureKind::ResponseBody));
            }
        });
    }

    pub(crate) fn update(&self, update: impl FnOnce(&mut ToolConversationReport)) {
        update(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()));
    }

    pub(crate) fn fail(&self, error: UpstreamError) {
        self.update(|snapshot| {
            snapshot.state = ToolConversationState::Failed;
            snapshot.report.reject_success(error);
        });
    }
}

impl fmt::Debug for ToolConversationReportHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ToolConversationReportHandle")
            .field(&self.snapshot())
            .finish()
    }
}

/// 显式消费已预扣的工具兼容计划。普通对话入口不会启用该路径。
/// 调用方应在流结束回调中读取最终报告，不能用初始 HTTP 状态更新健康或审计。
pub async fn relay_tool_conversation(
    machine: &RelayStateMachine,
    plan: ToolConversationPlan,
    response_protocol: Protocol,
    request_id: &str,
) -> (Result<ChatResponse, AfError>, ToolConversationReportHandle) {
    let handle = ToolConversationReportHandle::new(RelayAttemptReport::default());
    let result = execute(machine, plan, response_protocol, request_id, &handle).await;
    if result.is_err() {
        handle.fail(UpstreamError::ProtocolError);
    }
    (result, handle)
}

async fn execute(
    machine: &RelayStateMachine,
    plan: ToolConversationPlan,
    response_protocol: Protocol,
    request_id: &str,
    handle: &ToolConversationReportHandle,
) -> Result<ChatResponse, AfError> {
    let deadline = tokio::time::Instant::now() + plan.total_timeout;
    let canonical = plan.request().canonical();
    if response_protocol == Protocol::Anthropic
        && !matches!(
            plan.upstream_protocol,
            Protocol::Anthropic | Protocol::OpenAiChat
        )
        && canonical
            .reasoning
            .is_some_and(|reasoning| reasoning.include_thinking())
    {
        return Err(AfError::InvalidRequest);
    }
    let client_model = plan.request().requested_model().to_owned();
    let include_usage = plan.downstream_include_usage;
    let downstream_stream = canonical.stream;
    let estimator = OpenAiChatUsageEstimator::new(canonical);
    let created_at = unix_timestamp()?;
    let encoder = if downstream_stream {
        Some(
            ChatStreamEncoder::new(
                response_protocol,
                request_id,
                &client_model,
                created_at,
                canonical,
                include_usage,
            )?
            .with_upstream_protocol(plan.upstream_protocol),
        )
    } else {
        None
    };
    let body = encode_tool_request(plan.request().clone(), plan.upstream_protocol)?;
    let outbound = RelayRequest::new(
        canonical.model.clone(),
        canonical.operation,
        Method::POST,
        Some(body),
    )
    .map_err(|_| AfError::Internal)?
    .with_response_mode(ResponseMode::Stream);
    let initial = tokio::time::timeout_at(
        deadline,
        machine.execute_tool_request(outbound, plan.upstream_protocol),
    )
    .await
    .map_err(|_| UpstreamError::network(NetworkFailureKind::ResponseBody))?;
    let response = match initial {
        Ok(response) => response,
        Err(error) => {
            let (error, report) = error.into_parts();
            handle.update(|snapshot| {
                snapshot.sent_rounds = 0;
                snapshot.report = report;
            });
            return Err(match error {
                RelayError::Upstream(error) => error.into(),
                RelayError::ConcurrencyUnavailable => AfError::ConcurrencyLimited,
                _ => AfError::Internal,
            });
        }
    };
    let (response, report, permit, continuation) = response.into_tool_parts();
    handle.update(|snapshot| {
        snapshot.report = report;
        snapshot.sent_rounds = 1;
    });
    let upstream_protocol = plan.upstream_protocol;
    let source = continuation
        .ok_or(UpstreamError::ProtocolError)
        .and_then(|continuation| {
            ToolConversationStream::new(plan, response, continuation, deadline, handle.clone())
        });
    let source = match source {
        Ok(source) => source,
        Err(error) => {
            if let Some(permit) = permit {
                permit.release().await;
            }
            return Err(error.into());
        }
    };
    if let Some(encoder) = encoder {
        let (stream, usage) =
            OpenAiChatStream::new(UpstreamBody::Full(Bytes::new()), encoder, estimator);
        let mut stream = stream.with_source(ConversationSource::Tools(Box::new(source)));
        stream = stream.with_completion_hook(Box::new(ReportCompletion(handle.clone())));
        if let Some(permit) = permit {
            stream = stream.with_completion_hook(permit_completion_hook(permit));
        }
        let stream = stream.prefetch_first(Duration::from_secs(30)).await?;
        return Ok(ChatResponse::Stream {
            body: Box::new(stream),
            usage,
        });
    }
    let result = collect(source).await.and_then(|mut response| {
        let usage = estimator
            .resolve_response(&response)
            .map_err(|_| UpstreamError::ProtocolError)?;
        crate::anthropic_chat_bridge::adapt_response(
            upstream_protocol,
            response_protocol,
            &mut response,
        )?;
        let body = build_response(
            response_protocol,
            response,
            usage,
            request_id,
            &client_model,
            created_at,
        )?;
        Ok(ChatResponse::Full {
            body,
            usage: Ok(usage),
        })
    });
    if let Some(permit) = permit {
        permit.release().await;
    }
    result.map_err(Into::into)
}

async fn collect(
    mut source: ToolConversationStream,
) -> Result<af_protocol::CanonicalResponse, UpstreamError> {
    let mut aggregate = Aggregate::default();
    while let Some(events) = std::future::poll_fn(|context| source.poll_next(context)).await {
        aggregate.push(events?)?;
    }
    aggregate.finish()
}

struct ReportCompletion(ToolConversationReportHandle);

pub(crate) fn encode_tool_request(
    request: CanonicalRequestEnvelope,
    protocol: Protocol,
) -> Result<Bytes, AfError> {
    // Transport streaming must retain source context, native capability guards,
    // and output format preferences through both controlled tool rounds.
    prepare_request(request.with_stream_mode(true), protocol).map(|(_, body)| body)
}

impl GenerationCompletionHook for ReportCompletion {
    fn on_complete(
        self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        if usage.is_err() {
            self.0.fail(UpstreamError::ProtocolError);
        }
        Box::pin(async {})
    }
}
