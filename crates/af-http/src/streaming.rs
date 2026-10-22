use std::{
    error::Error,
    fmt,
    panic::AssertUnwindSafe,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_domain::UpstreamError;
use af_relay::{StreamDeliveryHook, StreamDeliveryOutcome as PumpOutcome};
use axum::body::{Body, Bytes};
use futures_core::Stream;
use futures_util::{FutureExt as _, StreamExt as _};
use tokio::{
    sync::{mpsc, oneshot},
    time::{sleep, timeout},
};
use tracing::Instrument as _;

/// 下游停止消费后允许单次背压持续的默认时间。
const DEFAULT_STREAM_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// 单个 HTTP SSE 响应允许存活的默认硬上限。
const DEFAULT_STREAM_MAX_DURATION: Duration = Duration::from_secs(900);
const STREAM_CHANNEL_CAPACITY: usize = 1;

/// SSE 下游交付的空闲写超时与整流硬期限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StreamDeliveryPolicy {
    write_timeout: Duration,
    max_duration: Duration,
}

impl StreamDeliveryPolicy {
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

/// 创建保持背压的 Axum Body；接收端释放会同步取消上游生产任务。
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
        if let Some(completion) = completion {
            completion.on_delivery(outcome).await;
        }
    }
    .in_current_span();
    // JoinHandle 丢弃后任务继续运行；生命周期由接收端关闭和硬期限共同约束。
    drop(tokio::spawn(task));
    Body::from_stream(ReceiverStream {
        receiver,
        terminal,
        acknowledged: Some(acknowledged),
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
    acknowledged: Option<oneshot::Sender<()>>,
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
        let outcome = match std::future::Future::poll(Pin::new(&mut self.terminal), context) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(result) => result.unwrap_or(PumpOutcome::Internal),
        };
        self.terminated = true;
        match outcome {
            PumpOutcome::Completed => {
                if let Some(sender) = self.acknowledged.take() {
                    let _ = sender.send(());
                }
                Poll::Ready(None)
            }
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
