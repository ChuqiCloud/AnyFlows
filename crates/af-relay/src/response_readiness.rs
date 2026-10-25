use std::{
    collections::VecDeque,
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use af_adapter::{AdaptorResult, Bytes, ResponseMode, UpstreamResponse};
use af_domain::{NetworkFailureKind, Protocol, UpstreamError};
use af_protocol::{CanonicalStreamEvent, ContentDelta, FinishReason};
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
        // A completed native compaction block is meaningful output even if its readable
        // summary is null. Do not require text or commit a still incomplete block.
        CanonicalStreamEvent::CompactionEnd { .. }
        | CanonicalStreamEvent::AnthropicCompactionEnd { .. }
        | CanonicalStreamEvent::Finish {
            reason: FinishReason::Compaction,
            ..
        } => true,
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

#[cfg(test)]
mod tests {
    use af_adapter::{HeaderMap, HeaderValue, StatusCode, UpstreamBodyStream};
    use serde_json::{Value, json};

    use super::*;

    struct Chunks(VecDeque<AdaptorResult<Bytes>>);

    impl Stream for Chunks {
        type Item = AdaptorResult<Bytes>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    fn frame(value: Value) -> Vec<u8> {
        format!(
            "event: {}\ndata: {value}\n\n",
            value["type"].as_str().unwrap()
        )
        .into_bytes()
    }

    fn anthropic_compaction(content: Value, complete: bool) -> Vec<u8> {
        let mut frames = vec![
            json!({"type":"message_start","message":{
                "id":"msg_compaction_ready","type":"message","role":"assistant",
                "content":[],"model":"claude-test","stop_reason":null,
                "usage":{"input_tokens":100,"output_tokens":0}
            }}),
            json!({"type":"content_block_start","index":0,"content_block":{
                "type":"compaction","content":content,"signature":"signed-summary",
                "encrypted_content":"opaque-state"
            }}),
        ];
        if complete {
            frames.extend([
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"compaction"},
                    "usage":{"input_tokens":80,"output_tokens":3}}),
                json!({"type":"message_stop"}),
            ]);
        }
        frames.into_iter().flat_map(frame).collect()
    }

    fn responses_compaction(complete: bool) -> Vec<u8> {
        let response = json!({
            "id":"resp_compaction_ready","object":"response","created_at":1_700_000_000,
            "status":"in_progress","error":null,"incomplete_details":null,
            "model":"gpt-test","output":[],"usage":null
        });
        let item = json!({"type":"compaction","id":"cmp_ready","encrypted_content":"opaque-state"});
        let mut frames = vec![
            json!({"type":"response.created","response":response.clone()}),
            json!({"type":"response.in_progress","response":response.clone()}),
            json!({"type":"response.output_item.added","output_index":0,"item":item.clone()}),
        ];
        if complete {
            let mut terminal = response;
            terminal["status"] = json!("completed");
            terminal["output"] = json!([item.clone()]);
            terminal["usage"] = json!({
                "input_tokens":8,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
                "output_tokens":3,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":11
            });
            frames.extend([
                json!({"type":"response.output_item.done","output_index":0,"item":item}),
                json!({"type":"response.completed","response":terminal}),
            ]);
        }
        frames
            .into_iter()
            .enumerate()
            .flat_map(|(index, mut value)| {
                value["sequence_number"] = json!(index);
                frame(value)
            })
            .collect()
    }

    fn response(body: &[u8], chunked: bool) -> UpstreamResponse {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("text/event-stream"),
        );
        if chunked {
            let chunks = body
                .chunks(7)
                .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
                .collect();
            UpstreamResponse::stream(
                StatusCode::OK,
                headers,
                UpstreamBodyStream::new(Chunks(chunks)),
            )
            .unwrap()
        } else {
            UpstreamResponse::full(StatusCode::OK, headers, Bytes::copy_from_slice(body)).unwrap()
        }
    }

    #[tokio::test]
    async fn compaction_only_responses_pass_readiness_and_replay_original_bytes() {
        for (protocol, body) in [
            (
                Protocol::Anthropic,
                anthropic_compaction(json!("summary 已保留"), true),
            ),
            (Protocol::Anthropic, anthropic_compaction(Value::Null, true)),
            (Protocol::OpenAiResponses, responses_compaction(true)),
        ] {
            for mode in [ResponseMode::Full, ResponseMode::Stream] {
                for chunked in [false, true] {
                    let ready = validate(
                        response(&body, chunked),
                        protocol,
                        mode,
                        Duration::from_secs(1),
                    )
                    .await
                    .unwrap();
                    assert_eq!(ready.into_body().into_bytes().await.unwrap().as_ref(), body);
                }
            }
        }
    }

    #[tokio::test]
    async fn compaction_starts_and_incomplete_states_are_not_ready_output() {
        for (protocol, body) in [
            (
                Protocol::Anthropic,
                anthropic_compaction(Value::Null, false),
            ),
            (Protocol::OpenAiResponses, responses_compaction(false)),
        ] {
            for mode in [ResponseMode::Full, ResponseMode::Stream] {
                let error = validate(
                    response(&body, true),
                    protocol,
                    mode,
                    Duration::from_secs(1),
                )
                .await
                .unwrap_err();
                assert_eq!(error, UpstreamError::ProtocolError);
            }
        }
    }
}
