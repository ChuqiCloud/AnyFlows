use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use af_adapter::{MAX_UPSTREAM_RESPONSE_BODY_BYTES, StatusCode, UpstreamResponse};
use af_domain::{NetworkFailureKind, Protocol, UpstreamError};
use af_protocol::{CanonicalStreamEvent, ToolCompatibilitySession, ToolCompatibilityStep, Usage};

use crate::{
    RelayAttemptDiagnostic,
    conversation_decoder::ConversationDecoder,
    error::map_adaptor_error,
    estimate_openai_request_upper_bound,
    frozen_candidate::FrozenCandidate,
    generation_stream::StreamSource,
    tool_conversation::{
        ToolConversationPlan, ToolConversationReportHandle, ToolConversationState,
        encode_tool_request,
    },
};

type ContinuationFuture = Pin<
    Box<
        dyn Future<
                Output = (
                    Result<UpstreamResponse, UpstreamError>,
                    Option<RelayAttemptDiagnostic>,
                ),
            > + Send,
    >,
>;

/// 惰性读取和发送，没有后台任务；丢弃下游流会取消在途请求和续答。
pub(crate) struct ToolConversationStream {
    session: ToolCompatibilitySession,
    protocol: Protocol,
    upper_bound: Usage,
    source: Option<StreamSource>,
    decoder: ConversationDecoder,
    events: VecDeque<CanonicalStreamEvent>,
    continuation: Option<FrozenCandidate>,
    sending: Option<ContinuationFuture>,
    deadline: Pin<Box<tokio::time::Sleep>>,
    started_at: tokio::time::Instant,
    bytes_seen: usize,
    terminal: bool,
    report: ToolConversationReportHandle,
}

impl ToolConversationStream {
    pub(crate) fn new(
        plan: ToolConversationPlan,
        response: UpstreamResponse,
        continuation: FrozenCandidate,
        deadline: tokio::time::Instant,
        report: ToolConversationReportHandle,
    ) -> Result<Self, UpstreamError> {
        validate_response(&response)?;
        let started_at = deadline - plan.total_timeout();
        Ok(Self {
            decoder: ConversationDecoder::new(plan.upstream_protocol)?,
            session: plan.session,
            protocol: plan.upstream_protocol,
            upper_bound: plan.upper_bound,
            source: Some(response.into_body().into()),
            events: VecDeque::new(),
            continuation: Some(continuation),
            sending: None,
            deadline: Box::pin(tokio::time::sleep_until(deadline)),
            started_at,
            bytes_seen: 0,
            terminal: false,
            report,
        })
    }

    fn fail(
        &mut self,
        error: UpstreamError,
    ) -> Poll<Option<Result<Vec<CanonicalStreamEvent>, UpstreamError>>> {
        self.session.abort();
        self.source = None;
        self.sending = None;
        self.continuation = None;
        self.events.clear();
        self.terminal = true;
        self.report
            .update(|snapshot| snapshot.report.complete_success(self.started_at.elapsed()));
        self.report.fail(error);
        Poll::Ready(Some(Err(error)))
    }

    fn record_usage(&self) -> Result<(), UpstreamError> {
        let observed = *self.session.observed_usage();
        self.report
            .update(|snapshot| snapshot.observed_usage = observed);
        let mut input = 0_i64;
        let mut output = 0_i64;
        for usage in observed.into_iter().flatten() {
            input = input
                .checked_add(
                    usage
                        .checked_input_tokens()
                        .map_err(|_| UpstreamError::ProtocolError)?
                        .get(),
                )
                .ok_or(UpstreamError::ProtocolError)?;
            output = output
                .checked_add(usage.output_tokens().get())
                .ok_or(UpstreamError::ProtocolError)?;
        }
        if input > self.upper_bound.input_tokens().get()
            || output > self.upper_bound.output_tokens().get()
        {
            return Err(UpstreamError::ProtocolError);
        }
        Ok(())
    }

    fn process(
        &mut self,
        event: CanonicalStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, UpstreamError> {
        // 让逐轮观测、续答预算和最终合计共用 Anthropic 协议解码器计算的计费用量。
        let event = self.decoder.with_billing_usage(event);
        let step = self.session.push(event);
        self.record_usage()?;
        match step.map_err(|_| UpstreamError::ProtocolError)? {
            ToolCompatibilityStep::Forward(events) => Ok(events),
            ToolCompatibilityStep::Complete { events, .. } => {
                self.finish_round()?;
                self.terminal = true;
                self.continuation = None;
                self.report.update(|snapshot| {
                    snapshot.state = ToolConversationState::Completed;
                    snapshot.report.complete_success(self.started_at.elapsed());
                });
                Ok(events)
            }
            ToolCompatibilityStep::Continue { request, usage } => {
                self.finish_round()?;
                let second = estimate_openai_request_upper_bound(request.canonical())
                    .map_err(|_| UpstreamError::ProtocolError)?;
                let cumulative_input = usage
                    .checked_input_tokens()
                    .map_err(|_| UpstreamError::ProtocolError)?
                    .get()
                    .checked_add(second.input_tokens().get())
                    .ok_or(UpstreamError::ProtocolError)?;
                if cumulative_input > self.upper_bound.input_tokens().get() {
                    return Err(UpstreamError::ProtocolError);
                }
                let body = encode_tool_request(*request, self.protocol)
                    .map_err(|_| UpstreamError::ProtocolError)?;
                let candidate = self
                    .continuation
                    .take()
                    .ok_or(UpstreamError::ProtocolError)?;
                self.sending = Some(Box::pin(candidate.send(body)));
                self.report.update(|snapshot| snapshot.sent_rounds = 2);
                Ok(Vec::new())
            }
        }
    }

    fn finish_round(&mut self) -> Result<(), UpstreamError> {
        if !self.events.is_empty() || !self.decoder.finish()?.is_empty() {
            return Err(UpstreamError::ProtocolError);
        }
        self.source = None;
        Ok(())
    }

    pub(crate) fn poll_next(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Vec<CanonicalStreamEvent>, UpstreamError>>> {
        if self.terminal {
            return Poll::Ready(None);
        }
        if self.deadline.as_mut().poll(context).is_ready() {
            return self.fail(UpstreamError::network(NetworkFailureKind::ResponseBody));
        }
        for _ in 0..64 {
            if let Some(event) = self.events.pop_front() {
                match self.process(event) {
                    Ok(events) if !events.is_empty() => return Poll::Ready(Some(Ok(events))),
                    Ok(_) => continue,
                    Err(error) => return self.fail(error),
                }
            }
            if let Some(sending) = self.sending.as_mut() {
                let (result, diagnostic) = match sending.as_mut().poll(context) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(result) => result,
                };
                self.sending = None;
                self.report
                    .update(|snapshot| snapshot.continuation_diagnostic = diagnostic);
                let response = match result.and_then(|response| {
                    validate_response(&response)?;
                    Ok(response)
                }) {
                    Ok(response) => response,
                    Err(error) => return self.fail(error),
                };
                self.decoder = match ConversationDecoder::new(self.protocol) {
                    Ok(decoder) => decoder,
                    Err(error) => return self.fail(error),
                };
                self.source = Some(response.into_body().into());
            }
            let Some(source) = self.source.as_mut() else {
                return self.fail(UpstreamError::ProtocolError);
            };
            match source.poll_next(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(bytes))) => {
                    self.bytes_seen = match self.bytes_seen.checked_add(bytes.len()) {
                        Some(size) if size <= MAX_UPSTREAM_RESPONSE_BODY_BYTES => size,
                        _ => return self.fail(UpstreamError::ProtocolError),
                    };
                    match self.decoder.push(&bytes) {
                        Ok(events) => self.events.extend(events),
                        Err(error) => return self.fail(error),
                    }
                }
                Poll::Ready(Some(Err(error))) => return self.fail(map_adaptor_error(error)),
                // 这三个上游协议必须在 EOF 前产生逻辑终态；断流不能作为续答条件。
                Poll::Ready(None) => return self.fail(UpstreamError::ProtocolError),
            }
        }
        context.waker().wake_by_ref();
        Poll::Pending
    }
}

impl Drop for ToolConversationStream {
    fn drop(&mut self) {
        if !self.terminal {
            self.session.abort();
            self.report
                .update(|snapshot| snapshot.report.complete_success(self.started_at.elapsed()));
            self.report
                .fail(UpstreamError::network(NetworkFailureKind::ResponseBody));
        }
    }
}

fn validate_response(response: &UpstreamResponse) -> Result<(), UpstreamError> {
    if response.status() != StatusCode::OK
        || !response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
    {
        return Err(UpstreamError::ProtocolError);
    }
    Ok(())
}
