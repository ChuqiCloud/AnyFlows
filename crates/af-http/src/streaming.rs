use std::{
    error::Error,
    fmt,
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_domain::{AfError, NetworkFailureKind, Protocol, UpstreamError};
use af_relay::{StreamDeliveryHook, StreamDeliveryOutcome as PumpOutcome};
use axum::{
    body::{Body, Bytes},
    response::{IntoResponse, Response},
};
use futures_core::Stream;
use futures_util::{FutureExt as _, StreamExt as _};
use tokio::{
    sync::{mpsc, oneshot},
    time::{
        Instant, Interval, MissedTickBehavior, Sleep, interval_at, sleep, sleep_until, timeout,
    },
};
use tracing::Instrument as _;

/// 下游停止消费后允许单次背压持续的默认时间。
const DEFAULT_STREAM_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// 单个 HTTP SSE 响应允许存活的默认硬上限。
const DEFAULT_STREAM_MAX_DURATION: Duration = Duration::from_secs(900);
const STREAM_CHANNEL_CAPACITY: usize = 1;
const STREAM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// SSE 下游交付的空闲写超时与整流硬期限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StreamDeliveryPolicy {
    write_timeout: Duration,
    max_duration: Duration,
}

impl StreamDeliveryPolicy {
    pub(crate) fn after_wait(self, elapsed: Duration) -> Self {
        Self {
            max_duration: self.max_duration.saturating_sub(elapsed),
            ..self
        }
    }

    #[cfg(test)]
    fn new(write_timeout: Duration, max_duration: Duration) -> Option<Self> {
        if write_timeout.is_zero() || max_duration < write_timeout {
            return None;
        }
        Some(Self {
            write_timeout,
            max_duration,
        })
    }
}

impl Default for StreamDeliveryPolicy {
    fn default() -> Self {
        Self {
            write_timeout: DEFAULT_STREAM_WRITE_TIMEOUT,
            max_duration: DEFAULT_STREAM_MAX_DURATION,
        }
    }
}

/// Keep ordinary admission failures as HTTP errors, then protect slow routing
/// and upstream first-output waits from reverse-proxy idle timeouts.
pub(crate) async fn await_chat_response<F>(
    execution: F,
    stream: bool,
    protocol: Protocol,
    policy: StreamDeliveryPolicy,
) -> Result<Response, AfError>
where
    F: Future<Output = Result<Response, AfError>> + Send + 'static,
{
    await_chat_response_with_interval(
        execution,
        stream,
        protocol,
        policy,
        STREAM_HEARTBEAT_INTERVAL,
    )
    .await
}

async fn await_chat_response_with_interval<F>(
    execution: F,
    stream: bool,
    protocol: Protocol,
    policy: StreamDeliveryPolicy,
    heartbeat_interval: Duration,
) -> Result<Response, AfError>
where
    F: Future<Output = Result<Response, AfError>> + Send + 'static,
{
    if !stream {
        return execution.await;
    }
    let deadline = Instant::now() + policy.max_duration;
    let mut execution = Box::pin(execution);
    match timeout(heartbeat_interval.min(policy.max_duration), &mut execution).await {
        Ok(Ok(response)) => {
            if !response
                .headers()
                .get(http::header::CONTENT_TYPE)
                .is_some_and(|value| value.as_bytes().starts_with(b"text/event-stream"))
            {
                return Ok(response);
            }
            Ok(response.map(|body| {
                Body::from_stream(PendingResponseStream::new(
                    None,
                    Some(Box::pin(body.into_data_stream())),
                    protocol,
                    heartbeat_interval,
                    deadline,
                ))
            }))
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Ok(sse_response(Body::from_stream(PendingResponseStream::new(
            Some(execution),
            None,
            protocol,
            heartbeat_interval,
            deadline,
        )))),
    }
}

impl PendingResponseStream {
    fn new(
        execution: Option<PendingExecution>,
        body: Option<ResponseBytes>,
        protocol: Protocol,
        heartbeat_interval: Duration,
        deadline: Instant,
    ) -> Self {
        let mut heartbeat = interval_at(Instant::now(), heartbeat_interval);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        Self {
            execution,
            body,
            protocol,
            heartbeat,
            waiting_deadline: Box::pin(sleep_until(deadline)),
            finished: false,
        }
    }
}

pub(crate) fn sse_response(body: Body) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-cache, no-transform"),
    );
    response.headers_mut().insert(
        http::HeaderName::from_static("x-accel-buffering"),
        http::HeaderValue::from_static("no"),
    );
    response
}

type PendingExecution = Pin<Box<dyn Future<Output = Result<Response, AfError>> + Send>>;
type ResponseBytes = Pin<Box<dyn Stream<Item = Result<Bytes, axum::Error>> + Send>>;

struct PendingResponseStream {
    execution: Option<PendingExecution>,
    body: Option<ResponseBytes>,
    protocol: Protocol,
    heartbeat: Interval,
    waiting_deadline: Pin<Box<Sleep>>,
    finished: bool,
}

impl PendingResponseStream {
    fn fail(&mut self, error: AfError) -> Poll<Option<Result<Bytes, axum::Error>>> {
        self.execution = None;
        self.body = None;
        self.finished = true;
        Poll::Ready(Some(Ok(crate::error_response::stream_error_frame(
            self.protocol,
            error,
        ))))
    }
}

impl Stream for PendingResponseStream {
    type Item = Result<Bytes, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }
        if let Some(execution) = self.execution.as_mut() {
            match execution.as_mut().poll(context) {
                Poll::Ready(Ok(response)) => {
                    self.execution = None;
                    self.body = Some(Box::pin(response.into_body().into_data_stream()));
                }
                Poll::Ready(Err(error)) => return self.fail(error),
                Poll::Pending => {
                    if self.waiting_deadline.as_mut().poll(context).is_ready() {
                        return self.fail(AfError::Upstream(UpstreamError::network(
                            NetworkFailureKind::RequestTimeout,
                        )));
                    }
                }
            }
        }
        if let Some(body) = self.body.as_mut() {
            match body.as_mut().poll_next(context) {
                Poll::Ready(item) => {
                    if item.is_none() || item.as_ref().is_some_and(Result::is_err) {
                        self.finished = true;
                        self.body = None;
                    }
                    return Poll::Ready(item);
                }
                Poll::Pending => {}
            }
        }
        if self.heartbeat.poll_tick(context).is_ready() {
            return Poll::Ready(Some(Ok(Bytes::from_static(b": keep-alive\n\n"))));
        }
        Poll::Pending
    }
}

/// 创建保持背压的 Axum Body；接收端释放会通知生产任务停止读取上游。
/// 正常交付先确认所有数据已消费，再等待完成回调结束后返回 EOF。
pub(crate) fn openai_chat_body<S>(
    source: S,
    policy: StreamDeliveryPolicy,
    completion: Option<Box<dyn StreamDeliveryHook>>,
) -> Body
where
    S: Stream<Item = Result<Bytes, UpstreamError>> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
    let (terminal_sender, terminal) = oneshot::channel();
    let (acknowledged, delivered) = oneshot::channel();
    let (completion_sender, completion_done) = oneshot::channel();
    let task = async move {
        let mut source = Box::pin(source);
        let mut hard_deadline = Box::pin(sleep(policy.max_duration));
        let result = AssertUnwindSafe(pump_stream(
            &mut source,
            &sender,
            policy,
            &mut hard_deadline,
        ))
        .catch_unwind()
        .await;
        let mut outcome = match result {
            Ok(outcome) => outcome,
            Err(_) => {
                tracing::error!(
                    error_kind = "stream_delivery_panic",
                    "SSE 下游交付任务发生 panic"
                );
                PumpOutcome::Internal
            }
        };
        let _ = terminal_sender.send(outcome);
        // 关闭通道后接收端才能观察 EOF；排队成功不等于 Body 已完整消费。
        drop(sender);
        drop(source);
        if outcome == PumpOutcome::Completed {
            outcome = tokio::select! {
                biased;
                () = hard_deadline.as_mut() => PumpOutcome::MaxDuration,
                result = timeout(policy.write_timeout, delivered) => match result {
                    Ok(Ok(())) => PumpOutcome::Completed,
                    Ok(Err(_)) => PumpOutcome::ClientClosed,
                    Err(_) => PumpOutcome::WriteTimeout,
                },
            };
        }
        log_outcome(outcome);
        let completion_result = AssertUnwindSafe(async move {
            if let Some(completion) = completion {
                completion.on_delivery(outcome).await;
            }
        })
        .catch_unwind()
        .await;
        if completion_result.is_err() {
            tracing::error!(
                error_kind = "stream_completion_panic",
                "SSE 交付完成回调发生 panic"
            );
            outcome = PumpOutcome::Internal;
        }
        let _ = completion_sender.send(outcome);
    }
    .in_current_span();
    // 上游读取和交付确认受关闭通知及硬期限约束；正常 EOF 还会等待回调结束。
    // 接收端提前释放时，回调仍在后台收尾，不由此 Body 提供全局关停追踪。
    drop(tokio::spawn(task));
    Body::from_stream(ReceiverStream {
        receiver,
        terminal,
        terminal_outcome: None,
        acknowledged: Some(acknowledged),
        completion_done,
        terminated: false,
    })
}

async fn pump_stream<S>(
    source: &mut Pin<Box<S>>,
    sender: &mpsc::Sender<Bytes>,
    policy: StreamDeliveryPolicy,
    hard_deadline: &mut Pin<Box<tokio::time::Sleep>>,
) -> PumpOutcome
where
    S: Stream<Item = Result<Bytes, UpstreamError>> + Send + 'static,
{
    loop {
        let item = tokio::select! {
            biased;
            () = sender.closed() => return PumpOutcome::ClientClosed,
            () = hard_deadline.as_mut() => return PumpOutcome::MaxDuration,
            item = source.next() => item,
        };
        let bytes = match item {
            Some(Ok(bytes)) => bytes,
            Some(Err(_)) => return PumpOutcome::UpstreamFailed,
            None => return PumpOutcome::Completed,
        };

        let send = sender.send(bytes);
        let result = tokio::select! {
            biased;
            () = sender.closed() => return PumpOutcome::ClientClosed,
            () = hard_deadline.as_mut() => return PumpOutcome::MaxDuration,
            result = timeout(policy.write_timeout, send) => result,
        };
        match result {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return PumpOutcome::ClientClosed,
            Err(_) => return PumpOutcome::WriteTimeout,
        }
    }
}

fn log_outcome(outcome: PumpOutcome) {
    match outcome {
        PumpOutcome::Completed | PumpOutcome::ClientClosed | PumpOutcome::Internal => {}
        PumpOutcome::UpstreamFailed => tracing::warn!(
            error_kind = "stream_upstream_failed",
            "SSE 已提交后上游流失败"
        ),
        PumpOutcome::WriteTimeout => tracing::warn!(
            error_kind = "stream_write_timeout",
            "SSE 下游背压超过写入时限"
        ),
        PumpOutcome::MaxDuration => tracing::warn!(
            error_kind = "stream_max_duration",
            "SSE 响应超过生命周期硬上限"
        ),
    }
}

#[derive(Clone, Copy, Debug)]
enum StreamBodyError {
    Upstream,
    Internal,
}

impl fmt::Display for StreamBodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream => formatter.write_str("SSE 上游流失败"),
            Self::Internal => formatter.write_str("SSE 交付任务失败"),
        }
    }
}

impl Error for StreamBodyError {}

struct ReceiverStream {
    receiver: mpsc::Receiver<Bytes>,
    terminal: oneshot::Receiver<PumpOutcome>,
    terminal_outcome: Option<PumpOutcome>,
    acknowledged: Option<oneshot::Sender<()>>,
    completion_done: oneshot::Receiver<PumpOutcome>,
    terminated: bool,
}

impl Stream for ReceiverStream {
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminated {
            return Poll::Ready(None);
        }
        match self.receiver.poll_recv(context) {
            Poll::Ready(Some(bytes)) => return Poll::Ready(Some(Ok(bytes))),
            Poll::Pending => return Poll::Pending,
            Poll::Ready(None) => {}
        }
        let mut outcome = match self.terminal_outcome {
            Some(outcome) => outcome,
            None => {
                let outcome = match Pin::new(&mut self.terminal).poll(context) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(result) => result.unwrap_or(PumpOutcome::Internal),
                };
                self.terminal_outcome = Some(outcome);
                outcome
            }
        };
        if outcome == PumpOutcome::Completed {
            if let Some(sender) = self.acknowledged.take() {
                let _ = sender.send(());
            }
            // ack 表示数据已排空；EOF 必须等依赖交付结果的计费等收尾完成。
            // 若确认已超时，使用回调接收到的最终失败结果，不能伪装为成功 EOF。
            outcome = match Pin::new(&mut self.completion_done).poll(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(result) => result.unwrap_or(PumpOutcome::Internal),
            };
        }
        self.terminated = true;
        match outcome {
            PumpOutcome::Completed => Poll::Ready(None),
            PumpOutcome::UpstreamFailed => Poll::Ready(Some(Err(StreamBodyError::Upstream))),
            _ => Poll::Ready(Some(Err(StreamBodyError::Internal))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use af_relay::{GenerationCompletionFuture, StreamDeliveryHook};
    use tokio::sync::oneshot;

    use super::*;

    #[test]
    fn policy_rejects_zero_or_inverted_deadlines() {
        assert!(StreamDeliveryPolicy::new(Duration::ZERO, Duration::from_secs(1)).is_none());
        assert!(
            StreamDeliveryPolicy::new(Duration::from_secs(2), Duration::from_secs(1)).is_none()
        );
        assert!(
            StreamDeliveryPolicy::new(Duration::from_secs(1), Duration::from_secs(1)).is_some()
        );
    }

    #[tokio::test]
    async fn fast_errors_and_non_streaming_responses_keep_http_semantics() {
        let error = await_chat_response_with_interval(
            async { Err(AfError::InvalidApiKey) },
            true,
            Protocol::OpenAiChat,
            StreamDeliveryPolicy::default(),
            Duration::from_millis(1),
        )
        .await;
        assert!(matches!(error, Err(AfError::InvalidApiKey)));
        let response = await_chat_response_with_interval(
            async { Ok(Body::from("{\"ok\":true}").into_response()) },
            false,
            Protocol::OpenAiChat,
            StreamDeliveryPolicy::default(),
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap(),
            "{\"ok\":true}"
        );
    }

    #[tokio::test]
    async fn waiting_stream_sends_heartbeat_then_preserves_delivery_and_payload() {
        let (send_response, receive_response) = oneshot::channel();
        let response = await_chat_response_with_interval(
            async move { receive_response.await.unwrap() },
            true,
            Protocol::OpenAiChat,
            StreamDeliveryPolicy::default(),
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        assert_eq!(response.headers()["x-accel-buffering"], "no");
        let mut stream = response.into_body().into_data_stream();
        assert_eq!(stream.next().await.unwrap().unwrap(), ": keep-alive\n\n");
        let (delivered, outcome) = oneshot::channel();
        let source = futures_util::stream::iter([Ok(Bytes::from_static(b"data: [DONE]\n\n"))]);
        let body = openai_chat_body(
            source,
            StreamDeliveryPolicy::default(),
            Some(Box::new(RecordingDelivery(delivered))),
        );
        send_response.send(Ok(sse_response(body))).unwrap();
        let chunks = stream.collect::<Vec<_>>().await;
        let payload: Vec<u8> = chunks
            .into_iter()
            .map(Result::unwrap)
            .filter(|chunk| !chunk.starts_with(b":"))
            .flatten()
            .collect();
        assert_eq!(payload, b"data: [DONE]\n\n");
        assert_eq!(outcome.await.unwrap(), PumpOutcome::Completed);
    }

    #[tokio::test]
    async fn dropping_waiting_response_cancels_execution() {
        let (source, dropped) = DropAwareStream::pending();
        let response = await_chat_response_with_interval(
            async move {
                let _source = source;
                std::future::pending::<Result<Response, AfError>>().await
            },
            true,
            Protocol::OpenAiChat,
            StreamDeliveryPolicy::default(),
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        drop(response);
        timeout(Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn slow_execution_failure_is_a_protocol_error_without_success_terminal() {
        for protocol in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
            Protocol::Gemini,
        ] {
            let (sender, receiver) = oneshot::channel();
            let response = await_chat_response_with_interval(
                async move { receiver.await.unwrap() },
                true,
                protocol,
                StreamDeliveryPolicy::default(),
                Duration::from_millis(1),
            )
            .await
            .unwrap();
            let mut body = response.into_body().into_data_stream();
            assert_eq!(body.next().await.unwrap().unwrap(), ": keep-alive\n\n");
            sender.send(Err(AfError::Internal)).unwrap();
            let frame = body.next().await.unwrap().unwrap();
            let text = std::str::from_utf8(&frame).unwrap();
            let data = text
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            let value: serde_json::Value = serde_json::from_str(data).unwrap();
            match protocol {
                Protocol::OpenAiResponses => {
                    assert!(text.starts_with("event: error\n"));
                    assert_eq!(value["type"], "error");
                    assert_eq!(value["code"], "internal_error");
                    assert_eq!(value["sequence_number"], 0);
                }
                Protocol::Anthropic => {
                    assert!(text.starts_with("event: error\n"));
                    assert_eq!(value["type"], "error");
                }
                _ => assert!(value["error"].is_object()),
            }
            assert!(!text.contains("[DONE]"));
            assert!(body.next().await.is_none());
        }
    }

    #[tokio::test]
    async fn waiting_execution_has_a_hard_deadline_and_is_dropped() {
        let (source, dropped) = DropAwareStream::pending();
        let policy =
            StreamDeliveryPolicy::new(Duration::from_millis(5), Duration::from_millis(20)).unwrap();
        let response = await_chat_response_with_interval(
            async move {
                let _source = source;
                std::future::pending::<Result<Response, AfError>>().await
            },
            true,
            Protocol::OpenAiChat,
            policy,
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        let body = timeout(
            Duration::from_secs(1),
            axum::body::to_bytes(response.into_body(), 16384),
        )
        .await
        .unwrap()
        .unwrap();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("upstream_unavailable"));
        assert!(!text.contains("[DONE]"));
        dropped.await.unwrap();
    }

    #[tokio::test]
    async fn fast_stream_with_idle_body_still_sends_heartbeats_and_cancels() {
        let (source, dropped) = DropAwareStream::pending();
        let body = openai_chat_body(source, StreamDeliveryPolicy::default(), None);
        let response = await_chat_response_with_interval(
            async move { Ok(sse_response(body)) },
            true,
            Protocol::OpenAiChat,
            StreamDeliveryPolicy::default(),
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        let mut stream = response.into_body().into_data_stream();
        assert_eq!(stream.next().await.unwrap().unwrap(), ": keep-alive\n\n");
        assert_eq!(stream.next().await.unwrap().unwrap(), ": keep-alive\n\n");
        drop(stream);
        timeout(Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn unconsumed_body_hits_write_timeout_and_drops_source() {
        let (stream, dropped) = DropAwareStream::ready_chunks(3);
        let policy =
            StreamDeliveryPolicy::new(Duration::from_millis(20), Duration::from_secs(1)).unwrap();
        let _body = openai_chat_body(stream, policy, None);

        tokio::time::timeout(Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn dropping_body_cancels_pending_source() {
        let (stream, dropped) = DropAwareStream::pending();
        let body = openai_chat_body(stream, StreamDeliveryPolicy::default(), None);
        drop(body);

        tokio::time::timeout(Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn hard_duration_drops_pending_source_even_with_live_client() {
        let (stream, dropped) = DropAwareStream::pending();
        let policy =
            StreamDeliveryPolicy::new(Duration::from_millis(100), Duration::from_millis(100))
                .unwrap();
        let _body = openai_chat_body(stream, policy, None);

        tokio::time::timeout(Duration::from_secs(1), dropped)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn completed_outcome_waits_until_body_eof_is_consumed() {
        let (sender, receiver) = oneshot::channel();
        let body = openai_chat_body(
            futures_util::stream::iter([Ok(Bytes::from_static(b"data: [DONE]\n\n"))]),
            StreamDeliveryPolicy::default(),
            Some(Box::new(RecordingDelivery(sender))),
        );
        let chunks = body.into_data_stream().collect::<Vec<_>>().await;
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].as_ref().unwrap().as_ref(), b"data: [DONE]\n\n");
        assert_eq!(receiver.await.unwrap(), PumpOutcome::Completed);
    }

    #[tokio::test]
    async fn completed_body_delivers_chunks_but_waits_for_callback_before_eof() {
        let (started, callback_started) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let body = openai_chat_body(
            futures_util::stream::iter([Ok(Bytes::from_static(b"data: [DONE]\n\n"))]),
            StreamDeliveryPolicy::default(),
            Some(Box::new(GatedDelivery { started, released })),
        );
        let mut stream = body.into_data_stream();
        let chunk = timeout(Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(chunk.as_ref(), b"data: [DONE]\n\n");

        let mut eof = Box::pin(stream.next());
        assert!(futures_util::poll!(eof.as_mut()).is_pending());
        let outcome = tokio::select! {
            outcome = timeout(Duration::from_secs(1), callback_started) => outcome.unwrap().unwrap(),
            _ = eof.as_mut() => panic!("EOF must wait for the gated completion callback"),
        };
        assert_eq!(outcome, PumpOutcome::Completed);
        // 再次 poll 必须复用缓存终态，不能重复 poll 已消费的 terminal oneshot。
        assert!(futures_util::poll!(eof.as_mut()).is_pending());
        release.send(()).unwrap();
        assert!(
            timeout(Duration::from_secs(1), eof)
                .await
                .unwrap()
                .is_none()
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn completion_hook_panics_surface_body_error_without_hanging() {
        for on_call in [true, false] {
            let body = openai_chat_body(
                futures_util::stream::iter([Ok(Bytes::from_static(b"data: [DONE]\n\n"))]),
                StreamDeliveryPolicy::default(),
                Some(Box::new(PanickingDelivery { on_call })),
            );
            let chunks = timeout(
                Duration::from_secs(1),
                body.into_data_stream().collect::<Vec<_>>(),
            )
            .await
            .unwrap();
            assert_eq!(chunks.len(), 2);
            assert_eq!(chunks[0].as_ref().unwrap().as_ref(), b"data: [DONE]\n\n");
            assert!(chunks[1].is_err());
        }
    }

    #[tokio::test]
    async fn missing_completion_notification_surfaces_internal_error_without_hanging() {
        let (sender, receiver) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
        drop(sender);
        let (terminal_sender, terminal) = oneshot::channel();
        terminal_sender.send(PumpOutcome::Completed).unwrap();
        let (acknowledged, _delivered) = oneshot::channel();
        let (completion_sender, completion_done) = oneshot::channel();
        drop(completion_sender);
        let mut stream = ReceiverStream {
            receiver,
            terminal,
            terminal_outcome: None,
            acknowledged: Some(acknowledged),
            completion_done,
            terminated: false,
        };

        assert!(matches!(
            timeout(Duration::from_secs(1), stream.next())
                .await
                .unwrap(),
            Some(Err(StreamBodyError::Internal))
        ));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn expired_delivery_ack_does_not_become_successful_body_eof() {
        let (sender, receiver) = oneshot::channel();
        let policy =
            StreamDeliveryPolicy::new(Duration::from_millis(20), Duration::from_secs(1)).unwrap();
        let body = openai_chat_body(
            futures_util::stream::iter([Ok(Bytes::from_static(b"data: [DONE]\n\n"))]),
            policy,
            Some(Box::new(RecordingDelivery(sender))),
        );
        // 保持 Body 存活但不消费，直到交付确认超时并结束回调。
        assert_eq!(
            timeout(Duration::from_secs(1), receiver)
                .await
                .unwrap()
                .unwrap(),
            PumpOutcome::WriteTimeout
        );
        let chunks = timeout(
            Duration::from_secs(1),
            body.into_data_stream().collect::<Vec<_>>(),
        )
        .await
        .unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].as_ref().unwrap().as_ref(), b"data: [DONE]\n\n");
        assert!(chunks[1].is_err());
    }

    #[tokio::test]
    async fn incomplete_upstream_stream_surfaces_body_error_and_failure_outcome() {
        let (sender, receiver) = oneshot::channel();
        let body = openai_chat_body(
            futures_util::stream::iter([Err(UpstreamError::ProtocolError)]),
            StreamDeliveryPolicy::default(),
            Some(Box::new(RecordingDelivery(sender))),
        );
        let chunks = body.into_data_stream().collect::<Vec<_>>().await;
        assert!(chunks.iter().any(Result::is_err));
        assert_eq!(receiver.await.unwrap(), PumpOutcome::UpstreamFailed);
    }

    struct GatedDelivery {
        started: oneshot::Sender<PumpOutcome>,
        released: oneshot::Receiver<()>,
    }

    impl StreamDeliveryHook for GatedDelivery {
        fn on_delivery(self: Box<Self>, outcome: PumpOutcome) -> GenerationCompletionFuture {
            Box::pin(async move {
                let _ = self.started.send(outcome);
                let _ = self.released.await;
            })
        }
    }

    struct PanickingDelivery {
        on_call: bool,
    }

    impl StreamDeliveryHook for PanickingDelivery {
        fn on_delivery(self: Box<Self>, _outcome: PumpOutcome) -> GenerationCompletionFuture {
            assert!(!self.on_call, "delivery hook creation panic");
            Box::pin(async { panic!("delivery hook future panic") })
        }
    }

    struct RecordingDelivery(oneshot::Sender<PumpOutcome>);

    impl StreamDeliveryHook for RecordingDelivery {
        fn on_delivery(self: Box<Self>, outcome: PumpOutcome) -> GenerationCompletionFuture {
            Box::pin(async move {
                let _ = self.0.send(outcome);
            })
        }
    }

    struct DropAwareStream {
        chunks: VecDeque<Result<Bytes, UpstreamError>>,
        stay_pending: bool,
        dropped: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    }

    impl DropAwareStream {
        fn ready_chunks(count: usize) -> (Self, oneshot::Receiver<()>) {
            Self::new(
                (0..count)
                    .map(|_| Ok(Bytes::from_static(b"data: test\n\n")))
                    .collect(),
                false,
            )
        }

        fn pending() -> (Self, oneshot::Receiver<()>) {
            Self::new(VecDeque::new(), true)
        }

        fn new(
            chunks: VecDeque<Result<Bytes, UpstreamError>>,
            stay_pending: bool,
        ) -> (Self, oneshot::Receiver<()>) {
            let (sender, receiver) = oneshot::channel();
            (
                Self {
                    chunks,
                    stay_pending,
                    dropped: Arc::new(Mutex::new(Some(sender))),
                },
                receiver,
            )
        }
    }

    impl Stream for DropAwareStream {
        type Item = Result<Bytes, UpstreamError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            match self.chunks.pop_front() {
                Some(item) => Poll::Ready(Some(item)),
                None if self.stay_pending => Poll::Pending,
                None => Poll::Ready(None),
            }
        }
    }

    impl Drop for DropAwareStream {
        fn drop(&mut self) {
            if let Some(sender) = self.dropped.lock().unwrap().take() {
                let _ = sender.send(());
            }
        }
    }
}
