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
            .map_err(|_| {
                tracing::warn!(stage = "prefetch", "Anthropic 上游流首事件超时");
                UpstreamError::network(af_domain::NetworkFailureKind::ResponseBody)
            })?
            .ok_or(UpstreamError::ProtocolError)??;
        self.prefetched.push_front(first);
        Ok(self)
    }

    /// 等待下一段已经完成身份清理和协议重编码的 SSE 字节。
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, UpstreamError>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }

    fn transform(&mut self, bytes: &[u8]) -> Result<Option<Bytes>, UpstreamError> {
        let events = self.decoder.push(bytes).map_err(|error| {
            // 错误和 decoder Debug 仅含固定分类及生命周期，不记录帧、模型或原生状态。
            tracing::warn!(
                stage = "decode",
                error_kind = ?error,
                decoder_state = ?self.decoder,
                "Anthropic 上游流解码失败"
            );
            UpstreamError::ProtocolError
        })?;
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
                .map_err(|error| {
                    tracing::warn!(
                        stage = "encoder_init",
                        error_kind = ?error,
                        "Anthropic 下游流编码器初始化失败"
                    );
                    UpstreamError::ProtocolError
                })?,
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
                    let usage = self
                        .decoder
                        .billing_usage()
                        .or(self.final_usage)
                        .ok_or(UpstreamError::ProtocolError)?;
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
        let event_kind = event_kind(&event);
        self.encoder
            .as_mut()
            .expect("已取得 Anthropic 初始 usage 时必须创建下游编码器")
            .encode(event)
            .map_err(|error| {
                tracing::warn!(
                    stage = "encode",
                    error_kind = ?error,
                    event_kind,
                    "Anthropic 下游流编码失败"
                );
                UpstreamError::ProtocolError
            })
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
                    let error = map_adaptor_error(error);
                    tracing::warn!(
                        stage = "transport",
                        error_kind = ?error,
                        "Anthropic 上游流读取失败"
                    );
                    return self.fail(error, context);
                }
                Poll::Ready(None) => {
                    self.source = None;
                    if let Err(error) = self.decoder.finish() {
                        tracing::warn!(
                            stage = "finish",
                            error_kind = ?error,
                            decoder_state = ?self.decoder,
                            "Anthropic 上游流终态校验失败"
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

/// 只返回静态事件类别，日志不能包含原生压缩、签名或用户内容。
fn event_kind(event: &CanonicalStreamEvent) -> &'static str {
    match event {
        CanonicalStreamEvent::MessageStart { .. } => "message_start",
        CanonicalStreamEvent::ContentDelta { .. } => "content_delta",
        CanonicalStreamEvent::ReasoningDelta { .. } => "reasoning_delta",
        CanonicalStreamEvent::ToolCallStart { .. } => "tool_call_start",
        CanonicalStreamEvent::ToolCallArgsDelta { .. } => "tool_call_args_delta",
        CanonicalStreamEvent::ToolCallSignature { .. } => "tool_call_signature",
        CanonicalStreamEvent::ToolCallEnd { .. } => "tool_call_end",
        CanonicalStreamEvent::AnthropicCompactionStart { .. } => "compaction_start",
        CanonicalStreamEvent::AnthropicCompactionDelta { .. } => "compaction_delta",
        CanonicalStreamEvent::AnthropicCompactionEnd { .. } => "compaction_end",
        CanonicalStreamEvent::AnthropicUsageMetadata { .. } => "usage_metadata",
        CanonicalStreamEvent::Finish { .. } => "finish",
        CanonicalStreamEvent::Usage(_) => "usage",
        CanonicalStreamEvent::Ping => "ping",
        CanonicalStreamEvent::StreamEnd => "stream_end",
        CanonicalStreamEvent::Error(_) => "error",
        _ => "unsupported_event",
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
    use af_adapter::{AdaptorResult, HeaderMap, StatusCode, UpstreamBodyStream, UpstreamResponse};
    use af_domain::Role;
    use af_protocol::{
        CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount, UsageDetails, UsageSemantics,
        UsageSource,
        anthropic::{AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder},
    };
    use serde_json::{Value, json};
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

    struct WireChunks(VecDeque<Bytes>);

    impl Stream for WireChunks {
        type Item = AdaptorResult<Bytes>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front().map(Ok))
        }
    }

    fn compaction_wire(content: Value) -> (Vec<u8>, Value) {
        let expected = json!({
            "type":"compaction", "content":content, "signature":"runtime-signed-summary",
            "encrypted_content":"runtime-encrypted-state",
            "tool_changes":[{"type":"tool_removal", "name":"previous_tool"}],
            "future_extension":{"kept":true, "opaque":["v1", null]}
        });
        let mut events = vec![json!({
            "type":"message_start", "message":{
                "id":"msg_private_compaction_stream", "type":"message", "role":"assistant",
                "model":"private-upstream-model", "content":[], "stop_reason":null,
                "usage":{"input_tokens":120, "output_tokens":0, "service_tier":"standard"}
            }
        })];
        let mut initial = expected.clone();
        if !expected["content"].is_null() {
            initial.as_object_mut().unwrap().remove("content");
            initial["signature"] = json!("");
            initial["encrypted_content"] = json!("");
        }
        events.push(json!({"type":"content_block_start", "index":0, "content_block":initial}));
        if !expected["content"].is_null() {
            events.push(json!({"type":"content_block_delta", "index":0, "delta":{
                "type":"compaction_delta", "content":expected["content"],
                "encrypted_content":expected["encrypted_content"]
            }}));
            events.push(json!({"type":"content_block_delta", "index":0, "delta":{
                "type":"signature_delta", "signature":expected["signature"]
            }}));
        }
        events.extend([
            json!({"type":"content_block_stop", "index":0}),
            json!({"type":"message_delta", "delta":{"stop_reason":"compaction", "stop_sequence":null}, "usage":{
                "input_tokens":100, "output_tokens":4, "cache_creation_input_tokens":0,
                "cache_read_input_tokens":0, "service_tier":"standard", "iterations":[
                    {"type":"message", "input_tokens":100, "output_tokens":4},
                    {"type":"compaction", "input_tokens":12, "output_tokens":3,
                     "cache_creation_input_tokens":5, "cache_read_input_tokens":2,
                     "cache_creation":{"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":3},
                     "future_usage":{"retained":true}}
                ]
            }}),
            json!({"type":"message_stop"}),
        ]);
        let bytes = events
            .into_iter()
            .flat_map(|event| {
                format!(
                    "event: {}\ndata: {event}\n\n",
                    event["type"].as_str().unwrap()
                )
                .into_bytes()
            })
            .collect();
        (bytes, expected)
    }

    #[tokio::test]
    async fn native_compaction_stream_prefetch_preserves_signed_and_null_state_for_all_chunks() {
        for content in [json!("runtime summary 已保留"), Value::Null] {
            let (wire, expected) = compaction_wire(content);
            for chunk_size in [wire.len(), 7] {
                let chunks = wire
                    .chunks(chunk_size)
                    .map(Bytes::copy_from_slice)
                    .collect();
                let body = UpstreamBody::Stream(UpstreamBodyStream::new(WireChunks(chunks)));
                let (stream, usage_handle) =
                    AnthropicMessagesStream::new(body, "msg_public", "public-model");
                let stream = stream
                    .prefetch_first(Duration::from_secs(1))
                    .await
                    .expect("原生压缩流预取应完成解码和重编码");
                let (sender, completed) = oneshot::channel();
                let mut stream = GenerationStream::with_completion_hook(
                    Box::new(stream),
                    Box::new(RecordingHook { sender }),
                );
                let mut output = Vec::new();
                while let Some(chunk) =
                    poll_fn(|context| Pin::new(stream.as_mut()).poll_next(context)).await
                {
                    output.extend(chunk.expect("压缩流必须以逻辑终态完成"));
                }
                let billed = usage_handle.resolve().await.unwrap();
                assert_eq!(billed.input_tokens().get(), 112);
                assert_eq!(billed.output_tokens().get(), 7);
                assert_eq!(billed.details().cache_read().get(), 2);
                assert_eq!(billed.details().cache_creation_5m().get(), 2);
                assert_eq!(billed.details().cache_creation_1h().get(), 3);
                assert_eq!(completed.await.unwrap().unwrap(), billed);

                let mut decoder = AnthropicMessagesStreamDecoder::new();
                let mut events = Vec::new();
                for chunk in output.chunks(7) {
                    events.extend(decoder.push(chunk).unwrap());
                }
                decoder.finish().unwrap();
                assert_eq!(decoder.initial_usage().unwrap().input_tokens().get(), 120);
                assert_eq!(decoder.billing_usage(), Some(billed));
                assert!(events.iter().any(|event| matches!(event, CanonicalStreamEvent::AnthropicCompactionEnd { item, .. } if item.as_value() == &expected)));
                assert!(events.iter().any(|event| matches!(event, CanonicalStreamEvent::Usage(usage) if usage.input_tokens().get() == 100 && usage.output_tokens().get() == 4)));
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, CanonicalStreamEvent::StreamEnd))
                        .count(),
                    1
                );
                assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
                let rendered = String::from_utf8(output).unwrap();
                assert!(!rendered.contains("msg_private_compaction_stream"));
                assert!(!rendered.contains("private-upstream-model"));
            }
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
