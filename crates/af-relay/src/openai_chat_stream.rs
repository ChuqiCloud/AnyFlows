use std::{
    collections::VecDeque,
    fmt,
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_adapter::{AdaptorResult, Bytes, UpstreamBody, UpstreamBodyStream};
use af_domain::UpstreamError;
use af_protocol::{
    CanonicalStreamEvent,
    openai_chat::{OpenAiChatStreamDecoder, OpenAiChatStreamEncoder},
};
use futures_core::Stream;

use crate::error::map_adaptor_error;

/// 已完成 OpenAI Chat 协议转换的下游 SSE 字节流。
///
/// 每段上游字节都会先解码为 Canonical 事件再重新编码；任一错误都会终止实例，
/// 且只向调用方暴露闭合的脱敏错误分类。
pub struct OpenAiChatStream {
    source: Option<StreamSource>,
    decoder: OpenAiChatStreamDecoder,
    encoder: OpenAiChatStreamEncoder,
    prefetched: VecDeque<Bytes>,
    terminated: bool,
}

impl OpenAiChatStream {
    pub(crate) fn new(source: UpstreamBody, encoder: OpenAiChatStreamEncoder) -> Self {
        Self {
            source: Some(StreamSource::from(source)),
            decoder: OpenAiChatStreamDecoder::new(),
            encoder,
            prefetched: VecDeque::new(),
            terminated: false,
        }
    }

    /// 在 HTTP 成功响应提交前读取并验证首个可交付 SSE 事件。
    pub(crate) async fn prefetch_first(
        mut self,
        first_event_timeout: Duration,
    ) -> Result<Self, UpstreamError> {
        let first = tokio::time::timeout(first_event_timeout, self.next_chunk())
            .await
            .map_err(|_| UpstreamError::Network)?
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
        for event in events {
            completed |= matches!(event, CanonicalStreamEvent::StreamEnd);
            output.extend(
                self.encoder
                    .encode(event)
                    .map_err(|_| UpstreamError::ProtocolError)?,
            );
        }
        if completed {
            // `[DONE]` 已经定义逻辑终点，立即丢弃底层响应体以传播取消。
            self.source = None;
            self.terminated = true;
        }
        Ok((!output.is_empty()).then(|| Bytes::from(output)))
    }

    fn fail(&mut self, error: UpstreamError) -> Poll<Option<Result<Bytes, UpstreamError>>> {
        self.source = None;
        self.terminated = true;
        Poll::Ready(Some(Err(error)))
    }
}

impl Stream for OpenAiChatStream {
    type Item = Result<Bytes, UpstreamError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(bytes) = self.prefetched.pop_front() {
            return Poll::Ready(Some(Ok(bytes)));
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
                    Err(error) => return self.fail(error),
                },
                Poll::Ready(Some(Err(error))) => {
                    return self.fail(map_adaptor_error(error));
                }
                Poll::Ready(None) => {
                    self.source = None;
                    self.terminated = true;
                    return match self.decoder.finish() {
                        Ok(()) => Poll::Ready(None),
                        Err(_) => Poll::Ready(Some(Err(UpstreamError::ProtocolError))),
                    };
                }
            }
        }
    }
}

impl fmt::Debug for OpenAiChatStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatStream")
            .field("prefetched_chunks", &self.prefetched.len())
            .field("terminated", &self.terminated)
            .finish()
    }
}

enum StreamSource {
    Full(Option<Bytes>),
    Stream(UpstreamBodyStream),
}

impl From<UpstreamBody> for StreamSource {
    fn from(body: UpstreamBody) -> Self {
        match body {
            UpstreamBody::Full(bytes) => Self::Full(Some(bytes)),
            UpstreamBody::Stream(stream) => Self::Stream(stream),
        }
    }
}

impl StreamSource {
    fn poll_next(&mut self, context: &mut Context<'_>) -> Poll<Option<AdaptorResult<Bytes>>> {
        match self {
            Self::Full(bytes) => Poll::Ready(bytes.take().map(Ok)),
            Self::Stream(stream) => Pin::new(stream).poll_next(context),
        }
    }
}

#[cfg(test)]
mod tests {
    use af_adapter::{HeaderMap, StatusCode, UpstreamResponse};

    use super::*;

    fn encoder() -> OpenAiChatStreamEncoder {
        OpenAiChatStreamEncoder::new("chatcmpl-relay-test", "gpt-relay-test", 1_700_000_000)
            .unwrap()
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
        let mut stream = OpenAiChatStream::new(response.into_body(), encoder())
            .prefetch_first(Duration::from_secs(1))
            .await
            .unwrap();

        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("chatcmpl-relay-test"));
        assert!(output.contains("gpt-relay-test"));
        assert!(output.contains("answer"));
        assert!(output.ends_with("data: [DONE]\n\n"));
        assert!(!output.contains("upstream-private-id"));
        assert!(!output.contains("upstream-private-model"));
    }

    #[tokio::test]
    async fn invalid_first_event_fails_before_stream_is_returned() {
        let response = UpstreamResponse::full(
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"data: {\"private\":\"payload\"}\n\n"),
        )
        .unwrap();
        let error = OpenAiChatStream::new(response.into_body(), encoder())
            .prefetch_first(Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(error, UpstreamError::ProtocolError);
    }

    #[tokio::test]
    async fn first_event_timeout_cancels_pending_upstream() {
        let response =
            UpstreamResponse::stream(StatusCode::OK, HeaderMap::new(), TestStream::pending())
                .unwrap();
        let error = OpenAiChatStream::new(response.into_body(), encoder())
            .prefetch_first(Duration::from_millis(20))
            .await
            .unwrap_err();
        assert_eq!(error, UpstreamError::Network);
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
