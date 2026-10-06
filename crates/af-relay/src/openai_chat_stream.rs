use std::{
    collections::VecDeque,
    fmt,
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_adapter::{Bytes, UpstreamBody};
use af_domain::UpstreamError;
use af_protocol::{CanonicalStreamEvent, Usage, openai_chat::OpenAiChatStreamDecoder};
use futures_core::Stream;

use crate::chat_stream_encoder::ChatStreamEncoder;
use crate::error::map_adaptor_error;
use crate::generation_stream::StreamSource;
use crate::generation_stream::{append_completion_future, append_completion_hook};
use crate::openai_chat_usage::{
    OpenAiChatUsageEstimator, OpenAiChatUsageHandle, UsageResolutionError, UsageSender,
    usage_channel,
};
use crate::{GenerationCompletionFuture, GenerationCompletionHook, GenerationStream};

/// 已完成 OpenAI Chat 协议转换的下游 SSE 字节流。
///
/// 每段上游字节都会先解码为 Canonical 事件再重新编码；任一错误都会终止实例，
/// 且只向调用方暴露闭合的脱敏错误分类。
pub struct OpenAiChatStream {
    source: Option<StreamSource>,
    decoder: OpenAiChatStreamDecoder,
    encoder: ChatStreamEncoder,
    usage_estimator: OpenAiChatUsageEstimator,
    usage_sender: Option<UsageSender>,
    completion_hook: Option<Box<dyn GenerationCompletionHook>>,
    deferred_completion: Option<Result<Usage, UsageResolutionError>>,
    completion_future: Option<GenerationCompletionFuture>,
    pending_terminal_error: Option<UpstreamError>,
    prefetched: VecDeque<Bytes>,
    terminated: bool,
}

impl OpenAiChatStream {
    pub(crate) fn new(
        source: UpstreamBody,
        encoder: ChatStreamEncoder,
        usage_estimator: OpenAiChatUsageEstimator,
    ) -> (Self, OpenAiChatUsageHandle) {
        let (usage_sender, usage_handle) = usage_channel();
        (
            Self {
                source: Some(StreamSource::from(source)),
                decoder: OpenAiChatStreamDecoder::new(),
                encoder,
                usage_estimator,
                usage_sender: Some(usage_sender),
                completion_hook: None,
                deferred_completion: None,
                completion_future: None,
                pending_terminal_error: None,
                prefetched: VecDeque::new(),
                terminated: false,
            },
            usage_handle,
        )
    }

    /// 为流结束时的计费或审计收尾安装一次性回调。
    pub fn with_completion_hook(mut self, hook: Box<dyn GenerationCompletionHook>) -> Self {
        if let Some(usage) = self.deferred_completion {
            append_completion_future(&mut self.completion_future, hook.on_complete(usage));
        } else {
            append_completion_hook(&mut self.completion_hook, hook);
        }
        self
    }

    /// 在 HTTP 成功响应提交前读取并验证首个可交付 SSE 事件。
    pub(crate) async fn prefetch_first(
        mut self,
        first_event_timeout: Duration,
    ) -> Result<Self, UpstreamError> {
        let first = tokio::time::timeout(first_event_timeout, self.next_chunk())
            .await
            .map_err(|_| UpstreamError::network(af_domain::NetworkFailureKind::ResponseBody))?
            .ok_or(UpstreamError::ProtocolError)??;
        self.prefetched.push_front(first);
        Ok(self)
    }

    /// 等待下一段已经完成协议转换的 SSE 字节。
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, UpstreamError>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }

    fn transform(&mut self, bytes: &[u8]) -> Result<Option<Bytes>, UpstreamError> {
        let events = self
            .decoder
            .push(bytes)
            .map_err(|_| UpstreamError::ProtocolError)?;
        let mut output = Vec::new();
        let mut completed = false;
        let mut resolved_usage = None;
        for event in events {
            match event {
                CanonicalStreamEvent::Usage(usage) => {
                    self.usage_estimator
                        .observe(&CanonicalStreamEvent::Usage(usage));
                    output.extend(
                        self.encoder
                            .encode_event(CanonicalStreamEvent::Usage(usage))
                            .map_err(|_| UpstreamError::ProtocolError)?,
                    );
                }
                CanonicalStreamEvent::StreamEnd => {
                    let upstream_usage_seen = self.usage_estimator.has_upstream_usage();
                    let usage = self.usage_estimator.resolve();
                    output.extend(
                        self.encoder
                            .finish(usage, upstream_usage_seen)
                            .map_err(|_| UpstreamError::ProtocolError)?,
                    );
                    resolved_usage = Some(usage);
                    completed = true;
                }
                event => {
                    self.usage_estimator.observe(&event);
                    output.extend(
                        self.encoder
                            .encode_event(event)
                            .map_err(|_| UpstreamError::ProtocolError)?,
                    );
                }
            }
        }
        if completed {
            self.begin_completion(resolved_usage.expect("流结束时必须固化 usage 结果"));
            // `[DONE]` 已经定义逻辑终点，立即丢弃底层响应体以传播取消。
            self.source = None;
            if self.completion_future.is_none() {
                self.terminated = true;
            }
        }
        Ok((!output.is_empty()).then(|| Bytes::from(output)))
    }

    fn fail(
        &mut self,
        error: UpstreamError,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, UpstreamError>>> {
        self.begin_completion(Err(UsageResolutionError::Interrupted));
        self.source = None;
        self.pending_terminal_error = Some(error);
        self.poll_terminal_completion(context)
    }

    fn begin_completion(&mut self, usage: Result<Usage, UsageResolutionError>) {
        if let Some(sender) = self.usage_sender.take() {
            let _ = sender.send(usage);
        }
        self.deferred_completion = Some(usage);
        if let Some(hook) = self.completion_hook.take() {
            append_completion_future(&mut self.completion_future, hook.on_complete(usage));
        }
    }

    fn poll_terminal_completion(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, UpstreamError>>> {
        if let Some(future) = &mut self.completion_future {
            match future.as_mut().poll(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(()) => {
                    self.completion_future = None;
                }
            }
        }
        self.terminated = true;
        if let Some(error) = self.pending_terminal_error.take() {
            return Poll::Ready(Some(Err(error)));
        }
        Poll::Ready(None)
    }
}

impl Stream for OpenAiChatStream {
    type Item = Result<Bytes, UpstreamError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(bytes) = self.prefetched.pop_front() {
            return Poll::Ready(Some(Ok(bytes)));
        }
        if self.completion_future.is_some() {
            return self.poll_terminal_completion(context);
        }
        if self.terminated {
            return Poll::Ready(None);
        }

        loop {
            let next = self
                .source
                .as_mut()
                .expect("未终止的 OpenAI Chat 流必须保留上游响应体")
                .poll_next(context);
            match next {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(bytes))) => match self.transform(&bytes) {
                    Ok(Some(output)) => return Poll::Ready(Some(Ok(output))),
                    Ok(None) => {}
                    Err(error) => return self.fail(error, context),
                },
                Poll::Ready(Some(Err(error))) => {
                    return self.fail(map_adaptor_error(error), context);
                }
                Poll::Ready(None) => {
                    self.source = None;
                    if self.decoder.finish().is_err() {
                        return self.fail(UpstreamError::ProtocolError, context);
                    }
                    self.terminated = true;
                    return Poll::Ready(None);
                }
            }
        }
    }
}

impl GenerationStream for OpenAiChatStream {
    fn with_completion_hook(
        self: Box<Self>,
        hook: Box<dyn GenerationCompletionHook>,
    ) -> Box<dyn GenerationStream> {
        Box::new((*self).with_completion_hook(hook))
    }
}

impl fmt::Debug for OpenAiChatStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatStream")
            .field(
                "has_deferred_completion",
                &self.deferred_completion.is_some(),
            )
            .field("prefetched_chunks", &self.prefetched.len())
            .field("terminated", &self.terminated)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use af_adapter::{AdaptorResult, HeaderMap, StatusCode, UpstreamResponse};
    use af_domain::{Operation, Protocol, Role};
    use af_protocol::{CanonicalRequest, ContentBlock, Message, UsageSource};
    use tokio::sync::{Notify, oneshot};

    use super::*;

    fn encoder(include_usage: bool) -> ChatStreamEncoder {
        ChatStreamEncoder::new(
            Protocol::OpenAiChat,
            "relay-test",
            "gpt-relay-test",
            1_700_000_000,
            &request(),
            include_usage,
        )
        .unwrap()
    }

    fn anthropic_encoder() -> ChatStreamEncoder {
        ChatStreamEncoder::new(
            Protocol::Anthropic,
            "relay-test",
            "public-model",
            1_700_000_000,
            &request(),
            false,
        )
        .unwrap()
    }

    fn gemini_encoder() -> ChatStreamEncoder {
        ChatStreamEncoder::new(
            Protocol::Gemini,
            "relay-test",
            "public-model",
            1_700_000_000,
            &request(),
            false,
        )
        .unwrap()
    }

    fn request() -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Chat,
            "gpt-4o".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("question".to_owned())],
            )],
            true,
        )
    }

    fn estimator() -> OpenAiChatUsageEstimator {
        OpenAiChatUsageEstimator::new(&request())
    }

    struct RecordingHook {
        sender: oneshot::Sender<Result<Usage, UsageResolutionError>>,
    }

    impl GenerationCompletionHook for RecordingHook {
        fn on_complete(
            self: Box<Self>,
            usage: Result<Usage, UsageResolutionError>,
        ) -> GenerationCompletionFuture {
            Box::pin(async move {
                let Self { sender } = *self;
                let _ = sender.send(usage);
            })
        }
    }

    struct DelayedHook {
        entered: Arc<Notify>,
        release: Arc<Notify>,
        completed: Arc<AtomicBool>,
    }

    impl GenerationCompletionHook for DelayedHook {
        fn on_complete(
            self: Box<Self>,
            _usage: Result<Usage, UsageResolutionError>,
        ) -> GenerationCompletionFuture {
            Box::pin(async move {
                self.entered.notify_waiters();
                self.release.notified().await;
                self.completed.store(true, Ordering::Release);
            })
        }
    }

    #[tokio::test]
    async fn fragmented_upstream_is_canonicalized_before_delivery() {
        let body = [
            b"data: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n".as_slice(),
            b"data: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"answer\"},\"finish_reason\":null}]}\n\n",
            b"data: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ]
        .concat();
        let split = body.len() / 3;
        let response = UpstreamResponse::stream(
            StatusCode::OK,
            HeaderMap::new(),
            TestStream::new(vec![
                Ok(Bytes::copy_from_slice(&body[..split])),
                Ok(Bytes::copy_from_slice(&body[split..split + 1])),
                Ok(Bytes::copy_from_slice(&body[split + 1..])),
            ]),
        )
        .unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("chatcmpl-relay-test"));
        assert!(output.contains("gpt-relay-test"));
        assert!(output.contains("answer"));
        assert!(output.ends_with("data: [DONE]\n\n"));
        assert!(!output.contains("\"usage\""));
        assert!(!output.contains("upstream-private-id"));
        assert!(!output.contains("upstream-private-model"));
        assert_eq!(
            usage.resolve().await.unwrap().source(),
            UsageSource::Estimated
        );
    }

    #[tokio::test]
    async fn openai_upstream_stream_is_encoded_as_anthropic_messages() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\ndata: [DONE]\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), anthropic_encoder(), estimator());
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("event: message_start"));
        assert!(output.contains("event: content_block_delta"));
        assert!(output.contains("event: message_delta"));
        assert!(output.contains("event: message_stop"));
        assert!(output.contains("public-model"));
        assert!(output.contains("answer"));
        assert!(!output.contains("[DONE]"));
        assert!(!output.contains("upstream-private-id"));
        assert!(!output.contains("upstream-private-model"));

        let usage = usage.resolve().await.unwrap();
        assert_eq!(usage.input_tokens().get(), 11);
        assert_eq!(usage.output_tokens().get(), 7);
    }

    #[tokio::test]
    async fn openai_upstream_stream_is_encoded_as_gemini_sse() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"upstream-private-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"upstream-private-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\ndata: [DONE]\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), gemini_encoder(), estimator());
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("response-relay-test"));
        assert!(output.contains("public-model"));
        assert!(output.contains("answer"));
        assert!(output.contains("usageMetadata"));
        assert!(output.contains("promptTokenCount"));
        assert!(!output.contains("[DONE]"));
        assert!(!output.contains("upstream-private-id"));
        assert!(!output.contains("upstream-private-model"));

        let usage = usage.resolve().await.unwrap();
        assert_eq!(usage.input_tokens().get(), 11);
        assert_eq!(usage.output_tokens().get(), 7);
    }

    #[tokio::test]
    async fn completion_hook_receives_the_same_terminal_usage_once() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let (sender, completed) = oneshot::channel();
        let mut stream = stream
            .with_completion_hook(Box::new(RecordingHook { sender }))
            .prefetch_first(Duration::from_secs(1))
            .await
            .unwrap();

        while let Some(chunk) = stream.next_chunk().await {
            chunk.unwrap();
        }
        let from_handle = usage.resolve().await.unwrap();
        let from_hook = completed.await.unwrap().unwrap();
        assert_eq!(from_hook, from_handle);
    }

    #[tokio::test]
    async fn completion_hook_installed_after_prefetch_receives_terminal_usage() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();
        let (sender, completed) = oneshot::channel();
        let mut stream = stream.with_completion_hook(Box::new(RecordingHook { sender }));

        while let Some(chunk) = stream.next_chunk().await {
            chunk.unwrap();
        }

        let from_handle = usage.resolve().await.unwrap();
        let from_hook = completed.await.unwrap().unwrap();
        assert_eq!(from_hook, from_handle);
    }

    #[tokio::test]
    async fn multiple_completion_hooks_survive_prefetch_and_run_once_each() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();
        let (first_sender, first_completed) = oneshot::channel();
        let (second_sender, second_completed) = oneshot::channel();
        let mut stream = stream
            .with_completion_hook(Box::new(RecordingHook {
                sender: first_sender,
            }))
            .with_completion_hook(Box::new(RecordingHook {
                sender: second_sender,
            }));

        while let Some(chunk) = stream.next_chunk().await {
            chunk.unwrap();
        }

        let from_handle = usage.resolve().await.unwrap();
        assert_eq!(first_completed.await.unwrap().unwrap(), from_handle);
        assert_eq!(second_completed.await.unwrap().unwrap(), from_handle);
    }

    #[tokio::test]
    async fn stream_eof_waits_for_completion_hook() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, _usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let completed = Arc::new(AtomicBool::new(false));
        let mut stream = stream
            .with_completion_hook(Box::new(DelayedHook {
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
                completed: Arc::clone(&completed),
            }))
            .prefetch_first(Duration::from_secs(1))
            .await
            .unwrap();

        assert!(matches!(stream.next_chunk().await, Some(Ok(_))));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), stream.next_chunk())
                .await
                .is_err()
        );
        assert!(!completed.load(Ordering::Acquire));
        release.notify_one();
        assert_eq!(stream.next_chunk().await, None);
        assert!(completed.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn invalid_first_event_fails_before_stream_is_returned() {
        let response = UpstreamResponse::full(
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"data: {\"private\":\"payload\"}\n\n"),
        )
        .unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let error = stream
            .prefetch_first(Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(error, UpstreamError::ProtocolError);
        assert_eq!(
            usage.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn first_event_timeout_cancels_pending_upstream() {
        let response =
            UpstreamResponse::stream(StatusCode::OK, HeaderMap::new(), TestStream::pending())
                .unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let error = stream
            .prefetch_first(Duration::from_millis(20))
            .await
            .unwrap_err();
        assert_eq!(
            error,
            UpstreamError::network(af_domain::NetworkFailureKind::ResponseBody)
        );
        assert_eq!(
            usage.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn unexpected_eof_resolves_usage_as_interrupted() {
        let body = Bytes::from_static(
            b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        assert!(matches!(stream.next_chunk().await, Some(Ok(_))));
        assert_eq!(
            stream.next_chunk().await,
            Some(Err(UpstreamError::ProtocolError))
        );
        assert_eq!(
            usage.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn requested_usage_emits_estimated_chunk_before_done() {
        let body = b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from_static(body))
                .unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(true), estimator());
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\"choices\":[]"));
        assert!(output.contains("\"prompt_tokens\""));
        assert!(output.ends_with("data: [DONE]\n\n"));
        assert_eq!(
            usage.resolve().await.unwrap().source(),
            UsageSource::Estimated
        );
    }

    #[tokio::test]
    async fn upstream_usage_is_preferred_and_hidden_when_client_did_not_request_it() {
        let body = b"data: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"upstream-id\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\ndata: [DONE]\n\n";
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from_static(body))
                .unwrap();
        let (stream, usage) =
            OpenAiChatStream::new(response.into_body(), encoder(false), estimator());
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("\"prompt_tokens\":11"));
        let usage = usage.resolve().await.unwrap();
        assert_eq!(usage.source(), UsageSource::Upstream);
        assert_eq!(usage.input_tokens().get(), 11);
        assert_eq!(usage.output_tokens().get(), 7);
    }

    struct TestStream {
        chunks: std::collections::VecDeque<AdaptorResult<Bytes>>,
        stay_pending: bool,
    }

    impl TestStream {
        fn new(chunks: Vec<AdaptorResult<Bytes>>) -> Self {
            Self {
                chunks: chunks.into(),
                stay_pending: false,
            }
        }

        fn pending() -> Self {
            Self {
                chunks: std::collections::VecDeque::new(),
                stay_pending: true,
            }
        }
    }

    impl Stream for TestStream {
        type Item = AdaptorResult<Bytes>;

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
}
