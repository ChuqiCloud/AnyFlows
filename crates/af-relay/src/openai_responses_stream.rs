use std::{
    collections::VecDeque,
    fmt,
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_adapter::{Bytes, UpstreamBody};
use af_domain::{AfError, UpstreamError};
use af_protocol::{
    CanonicalRequest, CanonicalStreamEvent, Usage,
    openai_responses::{OpenAiResponsesStreamDecoder, OpenAiResponsesStreamEncoder},
};
use futures_core::Stream;

use crate::error::map_adaptor_error;
use crate::generation_stream::StreamSource;
use crate::generation_stream::{append_completion_future, append_completion_hook};
use crate::openai_chat_usage::{
    OpenAiChatUsageEstimator, OpenAiChatUsageHandle, UsageResolutionError, UsageSender,
    usage_channel,
};
use crate::{GenerationCompletionFuture, GenerationCompletionHook, GenerationStream};

/// 已验证并重新生成公开身份的 OpenAI Responses SSE 流。
pub struct OpenAiResponsesStream {
    source: Option<StreamSource>,
    decoder: OpenAiResponsesStreamDecoder,
    encoder: OpenAiResponsesStreamEncoder,
    usage_estimator: OpenAiChatUsageEstimator,
    usage_sender: Option<UsageSender>,
    completion_hook: Option<Box<dyn GenerationCompletionHook>>,
    deferred_completion: Option<Result<Usage, UsageResolutionError>>,
    completion_future: Option<GenerationCompletionFuture>,
    pending_terminal_chunk: Option<Bytes>,
    pending_terminal_error: Option<UpstreamError>,
    prefetched: VecDeque<Bytes>,
    has_delivered_chunk: bool,
    terminated: bool,
}

impl OpenAiResponsesStream {
    /// 使用客户端公开身份创建 Responses 流；上游身份只在 decoder 内部校验。
    pub(crate) fn new(
        source: UpstreamBody,
        request: &CanonicalRequest,
        response_id: &str,
        client_model: &str,
        created_at: i64,
    ) -> Result<(Self, OpenAiChatUsageHandle), AfError> {
        let encoder = OpenAiResponsesStreamEncoder::new(response_id, client_model, created_at)
            .map_err(|_| AfError::Internal)?;
        let (usage_sender, usage_handle) = usage_channel();
        Ok((
            Self {
                source: Some(StreamSource::from(source)),
                decoder: OpenAiResponsesStreamDecoder::new(),
                encoder,
                usage_estimator: OpenAiChatUsageEstimator::new(request),
                usage_sender: Some(usage_sender),
                completion_hook: None,
                deferred_completion: None,
                completion_future: None,
                pending_terminal_chunk: None,
                pending_terminal_error: None,
                prefetched: VecDeque::new(),
                has_delivered_chunk: false,
                terminated: false,
            },
            usage_handle,
        ))
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

    /// 等待下一段已经完成身份清理与协议重编码的 SSE 字节。
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, UpstreamError>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }

    fn transform(&mut self, bytes: &[u8]) -> Result<Option<Bytes>, UpstreamError> {
        let events = self.decoder.push(bytes).map_err(|error| {
            // 只记录脱敏错误分类与流阶段，避免把上游帧、提示词或凭据写入日志。
            tracing::warn!(
                error_kind = ?error,
                decoder_state = ?self.decoder,
                delivered_chunk = self.has_delivered_chunk,
                "OpenAI Responses 上游流解码失败"
            );
            UpstreamError::ProtocolError
        })?;
        let mut output = Vec::new();
        let mut completed = false;
        let mut interrupted = false;
        let mut resolved_usage = None;
        for event in events {
            match event {
                CanonicalStreamEvent::Usage(usage) => {
                    self.usage_estimator
                        .observe(&CanonicalStreamEvent::Usage(usage));
                }
                CanonicalStreamEvent::Error(error) => {
                    self.usage_estimator
                        .observe(&CanonicalStreamEvent::Error(error));
                    output.extend(
                        self.encoder
                            .encode(CanonicalStreamEvent::Error(error))
                            .map_err(|_| UpstreamError::ProtocolError)?,
                    );
                    interrupted = true;
                }
                CanonicalStreamEvent::StreamEnd => {
                    let usage = self.usage_estimator.resolve();
                    if !interrupted && let Ok(final_usage) = usage {
                        output.extend(
                            self.encoder
                                .encode(CanonicalStreamEvent::Usage(final_usage))
                                .map_err(|_| UpstreamError::ProtocolError)?,
                        );
                    }
                    output.extend(
                        self.encoder
                            .encode(CanonicalStreamEvent::StreamEnd)
                            .map_err(|_| UpstreamError::ProtocolError)?,
                    );
                    resolved_usage = Some(usage);
                    completed = true;
                }
                event => {
                    self.usage_estimator.observe(&event);
                    output.extend(
                        self.encoder
                            .encode(event)
                            .map_err(|_| UpstreamError::ProtocolError)?,
                    );
                }
            }
        }
        if completed {
            self.begin_completion(resolved_usage.expect("流结束时必须固化 usage 结果"));
            // 官方终态事件已经定义逻辑终点，立即释放上游连接并传播取消。
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
        let terminal_chunk = self
            .has_delivered_chunk
            .then(|| self.encode_terminal_error(error))
            .flatten();
        self.begin_completion(Err(UsageResolutionError::Interrupted));
        self.source = None;
        if let Some(chunk) = terminal_chunk {
            self.pending_terminal_chunk = Some(chunk);
        } else {
            // 尚未提交响应或 encoder 已无法安全收尾时，保留 HTTP/传输层关闭失败语义。
            self.pending_terminal_error = Some(error);
        }
        self.poll_terminal_completion(context)
    }

    fn encode_terminal_error(&mut self, error: UpstreamError) -> Option<Bytes> {
        let output = self
            .encoder
            .encode(CanonicalStreamEvent::Error(error))
            .ok()?;
        self.encoder.encode(CanonicalStreamEvent::StreamEnd).ok()?;
        (!output.is_empty()).then(|| Bytes::from(output))
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
        if let Some(chunk) = self.pending_terminal_chunk.take() {
            return Poll::Ready(Some(Ok(chunk)));
        }
        if let Some(error) = self.pending_terminal_error.take() {
            return Poll::Ready(Some(Err(error)));
        }
        Poll::Ready(None)
    }
}

impl Stream for OpenAiResponsesStream {
    type Item = Result<Bytes, UpstreamError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(bytes) = self.prefetched.pop_front() {
            self.has_delivered_chunk = true;
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
                .expect("未终止的 Responses 流必须保留上游响应体")
                .poll_next(context);
            match next {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(bytes))) => match self.transform(&bytes) {
                    Ok(Some(output)) => {
                        self.has_delivered_chunk = true;
                        return Poll::Ready(Some(Ok(output)));
                    }
                    Ok(None) => {}
                    Err(error) => return self.fail(error, context),
                },
                Poll::Ready(Some(Err(error))) => {
                    return self.fail(map_adaptor_error(error), context);
                }
                Poll::Ready(None) => {
                    self.source = None;
                    if let Err(error) = self.decoder.finish() {
                        tracing::warn!(
                            error_kind = ?error,
                            delivered_chunk = self.has_delivered_chunk,
                            "OpenAI Responses 上游流终态校验失败"
                        );
                        return self.fail(UpstreamError::ProtocolError, context);
                    }
                    self.terminated = true;
                    return Poll::Ready(None);
                }
            }
        }
    }
}

impl GenerationStream for OpenAiResponsesStream {
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

impl fmt::Debug for OpenAiResponsesStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesStream")
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
    use af_adapter::{
        AdaptorError, AdaptorResult, AdaptorTransportError, HeaderMap, StatusCode, UpstreamResponse,
    };
    use af_domain::{Operation, Role};
    use af_protocol::{
        ContentBlock, ContentDelta, FinishReason, Message, TokenCount, UsageDetails,
        UsageSemantics, UsageSource,
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
    async fn stream_rebuilds_public_identity_and_closes_usage() {
        let expected_usage = usage(3, 2);
        let mut upstream_encoder = OpenAiResponsesStreamEncoder::new(
            "resp_private_upstream",
            "private-upstream-model",
            1_700_000_000,
        )
        .unwrap();
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
            CanonicalStreamEvent::Usage(expected_usage),
            CanonicalStreamEvent::StreamEnd,
        ] {
            upstream.extend(upstream_encoder.encode(event).unwrap());
        }
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let request = request();
        let (mut stream, usage_handle) = OpenAiResponsesStream::new(
            response.into_body(),
            &request,
            "resp_public",
            "public-model",
            1_700_000_001,
        )
        .unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let rendered = String::from_utf8(output.clone()).unwrap();
        assert!(rendered.contains("resp_public"));
        assert!(rendered.contains("public-model"));
        assert!(!rendered.contains("resp_private_upstream"));
        assert!(!rendered.contains("private-upstream-model"));

        let mut decoder = OpenAiResponsesStreamDecoder::new();
        let events = decoder.push(&output).unwrap();
        decoder.finish().unwrap();
        assert!(events.contains(&CanonicalStreamEvent::Usage(expected_usage)));
        assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
        assert_eq!(usage_handle.resolve().await.unwrap(), expected_usage);
    }

    #[tokio::test]
    async fn completion_hook_installed_after_prefetch_receives_terminal_usage() {
        let expected_usage = usage(3, 2);
        let mut encoder = OpenAiResponsesStreamEncoder::new(
            "resp_private_upstream",
            "private-upstream-model",
            1_700_000_000,
        )
        .unwrap();
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
            CanonicalStreamEvent::Usage(expected_usage),
            CanonicalStreamEvent::StreamEnd,
        ] {
            upstream.extend(encoder.encode(event).unwrap());
        }
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let request = request();
        let (stream, usage_handle) = OpenAiResponsesStream::new(
            response.into_body(),
            &request,
            "resp_public",
            "public-model",
            1_700_000_001,
        )
        .unwrap();
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

        assert_eq!(usage_handle.resolve().await.unwrap(), expected_usage);
        assert_eq!(completed.await.unwrap().unwrap(), expected_usage);
    }

    #[tokio::test]
    async fn upstream_error_event_is_preserved_without_raw_stream_failure() {
        let mut upstream_encoder = OpenAiResponsesStreamEncoder::new(
            "resp_private_upstream",
            "private-upstream-model",
            1_700_000_000,
        )
        .unwrap();
        let mut upstream = upstream_encoder
            .encode(CanonicalStreamEvent::Error(UpstreamError::rate_limited(
                af_domain::RateLimitScope::Window,
            )))
            .unwrap();
        upstream.extend(
            upstream_encoder
                .encode(CanonicalStreamEvent::StreamEnd)
                .unwrap(),
        );
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let (stream, usage_handle) = OpenAiResponsesStream::new(
            response.into_body(),
            &request(),
            "resp_public",
            "public-model",
            1_700_000_001,
        )
        .unwrap();
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.expect("标准 Responses 错误事件不得降级为裸连接错误"));
        }
        let mut decoder = OpenAiResponsesStreamDecoder::new();
        let events = decoder.push(&output).unwrap();
        decoder.finish().unwrap();
        assert_eq!(
            events,
            vec![
                CanonicalStreamEvent::Error(UpstreamError::rate_limited(
                    af_domain::RateLimitScope::Window,
                )),
                CanonicalStreamEvent::StreamEnd,
            ]
        );
        assert_eq!(
            usage_handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn transport_failure_after_text_emits_protocol_error_and_closes_cleanly() {
        let mut upstream_encoder = OpenAiResponsesStreamEncoder::new(
            "resp_private_upstream",
            "private-upstream-model",
            1_700_000_000,
        )
        .unwrap();
        let mut partial = Vec::new();
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
        ] {
            partial.extend(upstream_encoder.encode(event).unwrap());
        }
        let response = UpstreamResponse::stream(
            StatusCode::OK,
            HeaderMap::new(),
            TestStream::new(vec![
                Ok(Bytes::from(partial)),
                Err(AdaptorError::Transport(AdaptorTransportError::ResponseBody)),
            ]),
        )
        .unwrap();
        let (stream, usage_handle) = OpenAiResponsesStream::new(
            response.into_body(),
            &request(),
            "resp_public",
            "public-model",
            1_700_000_001,
        )
        .unwrap();
        let stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();
        let (sender, completed) = oneshot::channel();
        let mut stream = GenerationStream::with_completion_hook(
            Box::new(stream),
            Box::new(RecordingHook { sender }),
        );

        let mut output = Vec::new();
        while let Some(chunk) =
            poll_fn(|context| Pin::new(stream.as_mut()).poll_next(context)).await
        {
            output.extend(chunk.expect("已提交的 Responses 流必须使用协议内错误正常收尾"));
        }

        let mut decoder = OpenAiResponsesStreamDecoder::new();
        let events = decoder.push(&output).unwrap();
        decoder.finish().unwrap();
        assert!(events.contains(&CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("answer".to_owned()),
        }));
        assert!(contains_public_server_error(&events));
        assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
        assert_eq!(
            usage_handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
        assert_eq!(
            completed.await.unwrap(),
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn transport_failure_before_first_event_remains_http_error() {
        let response = UpstreamResponse::stream(
            StatusCode::OK,
            HeaderMap::new(),
            TestStream::new(vec![Err(AdaptorError::Transport(
                AdaptorTransportError::ResponseBody,
            ))]),
        )
        .unwrap();
        let (stream, usage_handle) = OpenAiResponsesStream::new(
            response.into_body(),
            &request(),
            "resp_public",
            "public-model",
            1_700_000_001,
        )
        .unwrap();

        let error = stream
            .prefetch_first(Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(
            error,
            UpstreamError::network(af_domain::NetworkFailureKind::ResponseBody)
        );
        assert_eq!(
            usage_handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn stream_without_an_official_terminal_event_emits_protocol_error() {
        let response = UpstreamResponse::full(
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b": heartbeat\n\n"),
        )
        .unwrap();
        let (stream, usage_handle) = OpenAiResponsesStream::new(
            response.into_body(),
            &request(),
            "resp_public",
            "public-model",
            1_700_000_001,
        )
        .unwrap();
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.expect("已提交的 Responses 流必须使用协议内错误正常收尾"));
        }
        let mut decoder = OpenAiResponsesStreamDecoder::new();
        let events = decoder.push(&output).unwrap();
        decoder.finish().unwrap();
        assert!(events.contains(&CanonicalStreamEvent::Ping));
        assert!(contains_public_server_error(&events));
        assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
        assert_eq!(
            usage_handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    fn request() -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Responses,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello".to_owned())],
            )],
            true,
        )
    }

    fn contains_public_server_error(events: &[CanonicalStreamEvent]) -> bool {
        events.iter().any(|event| {
            matches!(
                event,
                CanonicalStreamEvent::Error(UpstreamError::ServerError { status })
                    if status.get() == 500
            )
        })
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

    struct TestStream {
        chunks: VecDeque<AdaptorResult<Bytes>>,
    }

    impl TestStream {
        fn new(chunks: Vec<AdaptorResult<Bytes>>) -> Self {
            Self {
                chunks: chunks.into(),
            }
        }
    }

    impl Stream for TestStream {
        type Item = AdaptorResult<Bytes>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.chunks.pop_front())
        }
    }
}
