use std::{
    collections::VecDeque,
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_adapter::{AdaptorResult, Bytes, ResponseMode, UpstreamResponse};
use af_domain::{NetworkFailureKind, Protocol, UpstreamError};
use af_protocol::{CanonicalStreamEvent, ContentDelta};
use futures_core::Stream;

use crate::{
    conversation_decoder::ConversationDecoder, error::map_adaptor_error,
    generation_stream::StreamSource,
};

const MAX_PREFETCH_BYTES: usize = 1024 * 1024;

/// Validate before a race winner is committed, replaying the original bytes
/// through the normal protocol and billing pipeline exactly once.
pub(crate) async fn validate(
    response: UpstreamResponse,
    protocol: Protocol,
    mode: ResponseMode,
    first_output_timeout: Duration,
) -> Result<UpstreamResponse, UpstreamError> {
    let status = response.status();
    let headers = response.headers().clone();
    if mode != ResponseMode::Stream {
        let bytes = tokio::time::timeout(first_output_timeout, response.into_body().into_bytes())
            .await
            .map_err(|_| UpstreamError::network(NetworkFailureKind::ResponseBody))?
            .map_err(map_adaptor_error)?;
        let is_sse = headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if is_sse {
            let mut decoder = ConversationDecoder::new(protocol)?;
            let mut events = decoder.push(&bytes)?;
            events.extend(decoder.finish()?);
            if let Some(error) = events.iter().find_map(|event| match event {
                CanonicalStreamEvent::Error(error) => Some(*error),
                _ => None,
            }) {
                return Err(error);
            }
            if !events.iter().any(is_usable) {
                return Err(UpstreamError::ProtocolError);
            }
        } else {
            crate::conversation::parse_response(protocol, &bytes)?;
        }
        return UpstreamResponse::full(status, headers, bytes).map_err(map_adaptor_error);
    }
    let mut source = StreamSource::from(response.into_body());
    let mut decoder = ConversationDecoder::new(protocol)?;
    let mut prefetched = VecDeque::new();
    let ready = async {
        let mut size = 0_usize;
        loop {
            let bytes = poll_fn(|context| source.poll_next(context))
                .await
                .ok_or(UpstreamError::ProtocolError)?
                .map_err(map_adaptor_error)?;
            size = size
                .checked_add(bytes.len())
                .filter(|size| *size <= MAX_PREFETCH_BYTES)
                .ok_or(UpstreamError::ProtocolError)?;
            let events = decoder.push(&bytes)?;
            if let Some(error) = events.iter().find_map(|event| match event {
                CanonicalStreamEvent::Error(error) => Some(*error),
                _ => None,
            }) {
                return Err(error);
            }
            let usable = events.iter().any(is_usable);
            prefetched.push_back(bytes);
            if usable {
                return Ok(());
            }
        }
    };
    tokio::time::timeout(first_output_timeout, ready)
        .await
        .map_err(|_| UpstreamError::network(NetworkFailureKind::ResponseBody))??;
    UpstreamResponse::stream(status, headers, ReplayStream { prefetched, source })
        .map_err(map_adaptor_error)
}

fn is_usable(event: &CanonicalStreamEvent) -> bool {
    match event {
        CanonicalStreamEvent::ContentDelta { delta, .. } => match delta {
            ContentDelta::Text(text) => !text.is_empty(),
            ContentDelta::Image { data, .. } | ContentDelta::Audio { data, .. } => !data.is_empty(),
        },
        CanonicalStreamEvent::ReasoningDelta { text, .. } => !text.is_empty(),
        CanonicalStreamEvent::ToolCallStart { name, .. } => !name.is_empty(),
        CanonicalStreamEvent::ToolCallArgsDelta { partial_json, .. } => !partial_json.is_empty(),
        _ => false,
    }
}

struct ReplayStream {
    prefetched: VecDeque<Bytes>,
    source: StreamSource,
}

impl Stream for ReplayStream {
    type Item = AdaptorResult<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(bytes) = self.prefetched.pop_front() {
            return Poll::Ready(Some(Ok(bytes)));
        }
        self.source.poll_next(context)
    }
}
