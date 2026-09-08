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
use af_protocol::{CanonicalStreamEvent, Usage, gemini::GeminiGenerateContentStreamDecoder};
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

/// 已验证并重建公开身份的 Gemini `streamGenerateContent` SSE 流。
pub struct GeminiGenerateContentStream {
    source: Option<StreamSource>,
    decoder: GeminiGenerateContentStreamDecoder,
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

impl GeminiGenerateContentStream {
    /// 创建以官方 EOF 或兼容 `[DONE]` 为逻辑终点的流转换器。
    pub(crate) fn new(
        source: UpstreamBody,
        encoder: ChatStreamEncoder,
        usage_estimator: OpenAiChatUsageEstimator,
    ) -> (Self, OpenAiChatUsageHandle) {
        let (usage_sender, usage_handle) = usage_channel();
        (
            Self {
                source: Some(StreamSource::from(source)),
                decoder: GeminiGenerateContentStreamDecoder::new(),
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

    /// 在提交 HTTP 200 前读取并验证首个可交付事件。
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

    /// 等待下一段已经完成身份清理和协议重编码的 SSE 字节。
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, UpstreamError>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }

    fn transform_bytes(&mut self, bytes: &[u8]) -> Result<Option<Bytes>, UpstreamError> {
        let events = self
            .decoder
            .push(bytes)
            .map_err(|_| UpstreamError::ProtocolError)?;
        self.transform_events(events)
    }

    fn transform_events(
        &mut self,
        events: Vec<CanonicalStreamEvent>,
    ) -> Result<Option<Bytes>, UpstreamError> {
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
                CanonicalStreamEvent::Error(error) => return Err(error),
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
            // Gemini 官方以 EOF 结束；兼容 `[DONE]` 命中后同样立即释放上游响应体。
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
                Poll::Ready(()) => self.completion_future = None,
            }
        }
        self.terminated = true;
        if let Some(error) = self.pending_terminal_error.take() {
            return Poll::Ready(Some(Err(error)));
        }
        Poll::Ready(None)
    }
}

impl Stream for GeminiGenerateContentStream {
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
                .expect("未终止的 Gemini 流必须保留上游响应体")
                .poll_next(context);
            match next {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(bytes))) => match self.transform_bytes(&bytes) {
                    Ok(Some(output)) => return Poll::Ready(Some(Ok(output))),
                    Ok(None) => {}
                    Err(error) => return self.fail(error, context),
                },
                Poll::Ready(Some(Err(error))) => {
                    return self.fail(map_adaptor_error(error), context);
                }
                Poll::Ready(None) => {
                    self.source = None;
                    let events = match self.decoder.finish() {
                        Ok(events) => events,
                        Err(_) => return self.fail(UpstreamError::ProtocolError, context),
                    };
                    match self.transform_events(events) {
                        Ok(Some(output)) => return Poll::Ready(Some(Ok(output))),
                        Ok(None) if self.completion_future.is_some() => {
                            return self.poll_terminal_completion(context);
                        }
                        Ok(None) => {
                            self.terminated = true;
                            return Poll::Ready(None);
                        }
                        Err(error) => return self.fail(error, context),
                    }
                }
            }
        }
    }
}

impl GenerationStream for GeminiGenerateContentStream {
    fn with_completion_hook(
        mut self: Box<Self>,
        hook: Box<dyn GenerationCompletionHook>,
    ) -> Box<dyn GenerationStream> {
        if let Some(usage) = self.deferred_completion {
            append_completion_future(&mut self.completion_future, hook.on_complete(usage));
        } else {
            append_completion_hook(&mut self.completion_hook, hook);
        }
        self
    }
}

impl fmt::Debug for GeminiGenerateContentStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiGenerateContentStream")
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
    use af_adapter::{HeaderMap, StatusCode, UpstreamResponse};
    use af_domain::{Operation, Protocol, Role};
    use af_protocol::{
        CanonicalRequest, CanonicalStreamEvent, ContentBlock, ContentDelta, FinishReason, Message,
        TokenCount, UsageDetails, UsageSemantics, UsageSource,
        gemini::{GeminiGenerateContentStreamDecoder, GeminiGenerateContentStreamEncoder},
    };
    use tokio::sync::oneshot;

    use super::*;

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

    #[tokio::test]
    async fn official_eof_emits_final_usage_and_public_identity() {
        let final_usage = usage(3, 2);
        let upstream = upstream_stream(final_usage, false);
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let request = request();
        let encoder = ChatStreamEncoder::new(
            Protocol::Gemini,
            "request-public",
            "public-model",
            1_700_000_000,
            &request,
            false,
        )
        .unwrap();
        let (mut stream, usage_handle) = GeminiGenerateContentStream::new(
            response.into_body(),
            encoder,
            OpenAiChatUsageEstimator::new(&request),
        );

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let rendered = String::from_utf8(output.clone()).unwrap();
        assert!(rendered.contains("response-request-public"));
        assert!(rendered.contains("public-model"));
        assert!(!rendered.contains("private-response"));
        assert!(!rendered.contains("private-model"));

        let mut decoder = GeminiGenerateContentStreamDecoder::new();
        let mut events = decoder.push(&output).unwrap();
        events.extend(decoder.finish().unwrap());
        assert!(events.contains(&CanonicalStreamEvent::Usage(final_usage)));
        assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
        assert_eq!(usage_handle.resolve().await.unwrap(), final_usage);
    }

    #[tokio::test]
    async fn completion_hook_installed_after_compatible_done_receives_usage() {
        let final_usage = usage(3, 2);
        let upstream = upstream_stream(final_usage, true);
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let request = request();
        let encoder = ChatStreamEncoder::new(
            Protocol::Gemini,
            "request-public",
            "public-model",
            1_700_000_000,
            &request,
            false,
        )
        .unwrap();
        let (stream, usage_handle) = GeminiGenerateContentStream::new(
            response.into_body(),
            encoder,
            OpenAiChatUsageEstimator::new(&request),
        );
        let stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();
        let (sender, completed) = oneshot::channel();
        let mut stream = GenerationStream::with_completion_hook(
            Box::new(stream),
            Box::new(RecordingHook { sender }),
        );

        while let Some(chunk) =
            poll_fn(|context| Pin::new(stream.as_mut()).poll_next(context)).await
        {
            chunk.unwrap();
        }

        assert_eq!(usage_handle.resolve().await.unwrap(), final_usage);
        assert_eq!(completed.await.unwrap().unwrap(), final_usage);
    }

    #[tokio::test]
    async fn in_stream_google_error_fails_closed_and_interrupts_usage() {
        let body = Bytes::from_static(
            b"data: {\"error\":{\"code\":429,\"message\":\"private\",\"status\":\"RESOURCE_EXHAUSTED\",\"details\":[]}}\n\n",
        );
        let response = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), body).unwrap();
        let request = request();
        let encoder = ChatStreamEncoder::new(
            Protocol::Gemini,
            "request-public",
            "public-model",
            1_700_000_000,
            &request,
            false,
        )
        .unwrap();
        let (mut stream, usage_handle) = GeminiGenerateContentStream::new(
            response.into_body(),
            encoder,
            OpenAiChatUsageEstimator::new(&request),
        );

        assert_eq!(
            stream.next_chunk().await,
            Some(Err(UpstreamError::rate_limited(
                af_domain::RateLimitScope::Unknown,
            )))
        );
        assert_eq!(
            usage_handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    fn request() -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Chat,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello".to_owned())],
            )],
            true,
        )
    }

    fn upstream_stream(final_usage: Usage, compatible_done: bool) -> Vec<u8> {
        let mut encoder =
            GeminiGenerateContentStreamEncoder::new("private-response", "private-model").unwrap();
        let mut upstream = Vec::new();
        for event in [
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("answer".to_owned()),
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::Stop,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(final_usage),
            CanonicalStreamEvent::StreamEnd,
        ] {
            upstream.extend(encoder.encode(event).unwrap());
        }
        if compatible_done {
            upstream.extend_from_slice(b"data: [DONE]\n\n");
        }
        upstream
    }

    fn usage(input: i64, output: i64) -> Usage {
        Usage::new(
            TokenCount::new(input).unwrap(),
            TokenCount::new(output).unwrap(),
            UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Upstream,
            UsageSemantics::Inclusive,
        )
        .unwrap()
    }
}
