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
use af_protocol::{
    CanonicalStreamEvent, Usage,
    anthropic::{AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder},
};
use futures_core::Stream;

use crate::error::map_adaptor_error;
use crate::generation_stream::StreamSource;
use crate::generation_stream::{append_completion_future, append_completion_hook};
use crate::openai_chat_usage::{
    OpenAiChatUsageHandle, UsageResolutionError, UsageSender, usage_channel,
};
use crate::{GenerationCompletionFuture, GenerationCompletionHook, GenerationStream};

/// 已验证并重建公开身份的 Anthropic Messages SSE 流。
pub struct AnthropicMessagesStream {
    source: Option<StreamSource>,
    decoder: AnthropicMessagesStreamDecoder,
    encoder: Option<Box<AnthropicMessagesStreamEncoder>>,
    response_id: String,
    client_model: String,
    final_usage: Option<Usage>,
    usage_sender: Option<UsageSender>,
    completion_hook: Option<Box<dyn GenerationCompletionHook>>,
    deferred_completion: Option<Result<Usage, UsageResolutionError>>,
    completion_future: Option<GenerationCompletionFuture>,
    pending_terminal_error: Option<UpstreamError>,
    prefetched: VecDeque<Bytes>,
    terminated: bool,
}

impl AnthropicMessagesStream {
    /// 创建等待上游 `message_start` 真实初始 usage 的流转换器。
    pub(crate) fn new(
        source: UpstreamBody,
        response_id: impl Into<String>,
        client_model: impl Into<String>,
    ) -> (Self, OpenAiChatUsageHandle) {
        let (usage_sender, usage_handle) = usage_channel();
        (
            Self {
                source: Some(StreamSource::from(source)),
                decoder: AnthropicMessagesStreamDecoder::new(),
                encoder: None,
                response_id: response_id.into(),
                client_model: client_model.into(),
                final_usage: None,
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

    fn transform(&mut self, bytes: &[u8]) -> Result<Option<Bytes>, UpstreamError> {
        let events = self
            .decoder
            .push(bytes)
            .map_err(|_| UpstreamError::ProtocolError)?;
        if self.encoder.is_none() {
            let Some(initial_usage) = self.decoder.initial_usage() else {
                for event in events {
                    match event {
                        CanonicalStreamEvent::Ping => {}
                        CanonicalStreamEvent::Error(error) => return Err(error),
                        _ => return Err(UpstreamError::ProtocolError),
                    }
                }
                return Ok(None);
            };
            self.encoder = Some(Box::new(
                AnthropicMessagesStreamEncoder::new(
                    self.response_id.clone(),
                    self.client_model.clone(),
                    initial_usage,
                )
                .map_err(|_| UpstreamError::ProtocolError)?,
            ));
        }

        let mut output = Vec::new();
        let mut completed = false;
        for event in events {
            match event {
                CanonicalStreamEvent::Usage(usage) => {
                    if self.final_usage.replace(usage).is_some() {
                        return Err(UpstreamError::ProtocolError);
                    }
                    output.extend(self.encode_event(CanonicalStreamEvent::Usage(usage))?);
                }
                CanonicalStreamEvent::StreamEnd => {
                    let usage = self.final_usage.ok_or(UpstreamError::ProtocolError)?;
                    output.extend(self.encode_event(CanonicalStreamEvent::StreamEnd)?);
                    self.begin_completion(Ok(usage));
                    completed = true;
                }
                CanonicalStreamEvent::Error(error) => return Err(error),
                event => output.extend(self.encode_event(event)?),
            }
        }
        if completed {
            // `message_stop` 是 Anthropic 的逻辑终点，立即释放底层响应体以传播取消。
            self.source = None;
            if self.completion_future.is_none() {
                self.terminated = true;
            }
        }
        Ok((!output.is_empty()).then(|| Bytes::from(output)))
    }

    fn encode_event(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, UpstreamError> {
        self.encoder
            .as_mut()
            .expect("已取得 Anthropic 初始 usage 时必须创建下游编码器")
            .encode(event)
            .map_err(|_| UpstreamError::ProtocolError)
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

impl Stream for AnthropicMessagesStream {
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
                .expect("未终止的 Anthropic 流必须保留上游响应体")
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

impl GenerationStream for AnthropicMessagesStream {
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

impl fmt::Debug for AnthropicMessagesStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicMessagesStream")
            .field("has_encoder", &self.encoder.is_some())
            .field("has_final_usage", &self.final_usage.is_some())
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
    use af_domain::Role;
    use af_protocol::{
        CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount, UsageDetails, UsageSemantics,
        UsageSource,
        anthropic::{AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder},
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
    async fn stream_rebuilds_public_identity_and_uses_real_snapshots() {
        let initial_usage = usage(3, 0);
        let final_usage = usage(3, 2);
        let mut upstream_encoder = AnthropicMessagesStreamEncoder::new(
            "msg_private_upstream",
            "private-upstream-model",
            initial_usage,
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
            CanonicalStreamEvent::Usage(final_usage),
            CanonicalStreamEvent::StreamEnd,
        ] {
            upstream.extend(upstream_encoder.encode(event).unwrap());
        }
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let (mut stream, usage_handle) =
            AnthropicMessagesStream::new(response.into_body(), "msg_public", "public-model");

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let rendered = String::from_utf8(output.clone()).unwrap();
        assert!(rendered.contains("msg_public"));
        assert!(rendered.contains("public-model"));
        assert!(!rendered.contains("msg_private_upstream"));
        assert!(!rendered.contains("private-upstream-model"));

        let mut decoder = AnthropicMessagesStreamDecoder::new();
        let events = decoder.push(&output).unwrap();
        decoder.finish().unwrap();
        assert_eq!(decoder.initial_usage(), Some(initial_usage));
        assert!(events.contains(&CanonicalStreamEvent::Usage(final_usage)));
        assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
        assert_eq!(usage_handle.resolve().await.unwrap(), final_usage);
    }

    #[tokio::test]
    async fn stream_without_message_stop_fails_closed() {
        let mut upstream_encoder = AnthropicMessagesStreamEncoder::new(
            "msg_private_upstream",
            "private-upstream-model",
            usage(1, 0),
        )
        .unwrap();
        let body = upstream_encoder
            .encode(CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            })
            .unwrap();
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(body)).unwrap();
        let (stream, usage_handle) =
            AnthropicMessagesStream::new(response.into_body(), "msg_public", "public-model");
        let mut stream = stream.prefetch_first(Duration::from_secs(1)).await.unwrap();

        assert!(matches!(stream.next_chunk().await, Some(Ok(_))));
        assert_eq!(
            stream.next_chunk().await,
            Some(Err(UpstreamError::ProtocolError))
        );
        assert_eq!(
            usage_handle.resolve().await,
            Err(UsageResolutionError::Interrupted)
        );
    }

    #[tokio::test]
    async fn completion_hook_installed_after_prefetch_receives_terminal_usage() {
        let initial_usage = usage(3, 0);
        let final_usage = usage(3, 2);
        let mut encoder = AnthropicMessagesStreamEncoder::new(
            "msg_private_upstream",
            "private-upstream-model",
            initial_usage,
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
            CanonicalStreamEvent::Usage(final_usage),
            CanonicalStreamEvent::StreamEnd,
        ] {
            upstream.extend(encoder.encode(event).unwrap());
        }
        let response =
            UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(upstream))
                .unwrap();
        let (stream, usage_handle) =
            AnthropicMessagesStream::new(response.into_body(), "msg_public", "public-model");
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
            UsageSemantics::CacheSeparated,
        )
        .unwrap()
    }
}
