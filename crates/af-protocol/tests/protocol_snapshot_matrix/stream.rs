use af_protocol::{
    CanonicalStreamEvent, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource, anthropic,
    gemini, openai_chat, openai_responses,
};

use super::fixtures::{CREATED_AT, MODEL, ProtocolKind, stream};

#[test]
fn snapshots_stream_conversion_matrix() {
    let mut rows = Vec::new();
    for source in ProtocolKind::ALL {
        let canonical = decode_source(source);
        for target in ProtocolKind::ALL {
            let outcome = match encode_target(target, &canonical) {
                Ok(bytes) => {
                    let round_trip = decode_bytes(target, &bytes);
                    assert_stream_semantics(&canonical, &round_trip);
                    format!("ok [{}]", wire_event_names(&bytes).join(" > "))
                }
                Err(error) => format!("error {error}"),
            };
            rows.push(format!("{} -> {}: {outcome}", source.name(), target.name()));
        }
    }

    let report = rows.join("\n");
    insta::assert_snapshot!(report.as_str(), @r"
    openai_chat -> openai_chat: ok [chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > [DONE]]
    openai_chat -> anthropic: ok [message_start > content_block_start > content_block_delta > content_block_stop > message_delta > message_stop]
    openai_chat -> gemini: ok [gemini.chunk > gemini.chunk > gemini.chunk]
    openai_chat -> openai_responses: ok [response.created > response.in_progress > response.output_item.added > response.content_part.added > response.output_text.delta > response.output_text.done > response.content_part.done > response.output_item.done > response.completed]
    anthropic -> openai_chat: ok [chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > [DONE]]
    anthropic -> anthropic: ok [message_start > content_block_start > content_block_delta > content_block_stop > message_delta > message_stop]
    anthropic -> gemini: ok [gemini.chunk > gemini.chunk > gemini.chunk]
    anthropic -> openai_responses: ok [response.created > response.in_progress > response.output_item.added > response.content_part.added > response.output_text.delta > response.output_text.done > response.content_part.done > response.output_item.done > response.completed]
    gemini -> openai_chat: ok [chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > [DONE]]
    gemini -> anthropic: ok [message_start > content_block_start > content_block_delta > content_block_stop > message_delta > message_stop]
    gemini -> gemini: ok [gemini.chunk > gemini.chunk > gemini.chunk]
    gemini -> openai_responses: ok [response.created > response.in_progress > response.output_item.added > response.content_part.added > response.output_text.delta > response.output_text.done > response.content_part.done > response.output_item.done > response.completed]
    openai_responses -> openai_chat: ok [chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > chat.completion.chunk > [DONE]]
    openai_responses -> anthropic: ok [message_start > content_block_start > content_block_delta > content_block_stop > message_delta > message_stop]
    openai_responses -> gemini: ok [gemini.chunk > gemini.chunk > gemini.chunk]
    openai_responses -> openai_responses: ok [response.created > response.in_progress > response.output_item.added > response.content_part.added > response.output_text.delta > response.output_text.done > response.content_part.done > response.output_item.done > response.completed]
    ");
}

fn decode_source(protocol: ProtocolKind) -> Vec<CanonicalStreamEvent> {
    decode_bytes(protocol, &stream(protocol))
}

fn decode_bytes(protocol: ProtocolKind, bytes: &[u8]) -> Vec<CanonicalStreamEvent> {
    let result = match protocol {
        ProtocolKind::OpenAiChat => {
            let mut decoder = openai_chat::OpenAiChatStreamDecoder::new();
            let events = decoder.push(bytes).map_err(debug_error);
            let finish = decoder.finish().map_err(debug_error);
            finish.and(events)
        }
        ProtocolKind::Anthropic => {
            let mut decoder = anthropic::AnthropicMessagesStreamDecoder::new();
            let events = decoder.push(bytes).map_err(debug_error);
            let finish = decoder.finish().map_err(debug_error);
            finish.and(events)
        }
        ProtocolKind::Gemini => {
            let mut decoder = gemini::GeminiGenerateContentStreamDecoder::new();
            decoder
                .push(bytes)
                .map_err(debug_error)
                .and_then(|mut events| {
                    decoder.finish().map_err(debug_error).map(|tail| {
                        events.extend(tail);
                        events
                    })
                })
        }
        ProtocolKind::OpenAiResponses => {
            let mut decoder = openai_responses::OpenAiResponsesStreamDecoder::new();
            let events = decoder.push(bytes).map_err(debug_error);
            let finish = decoder.finish().map_err(debug_error);
            finish.and(events)
        }
    };
    result.unwrap_or_else(|error| panic!("{} 流样本解析失败：{error}", protocol.name()))
}

fn encode_target(
    protocol: ProtocolKind,
    events: &[CanonicalStreamEvent],
) -> Result<Vec<u8>, String> {
    match protocol {
        ProtocolKind::OpenAiChat => {
            let encoder = openai_chat::OpenAiChatStreamEncoder::new(
                "chatcmpl-matrix-target",
                MODEL,
                CREATED_AT,
            )
            .map_err(debug_error)?;
            encode_events(encoder, events, |encoder, event| encoder.encode(event))
        }
        ProtocolKind::Anthropic => {
            let encoder = anthropic::AnthropicMessagesStreamEncoder::new(
                "msg-matrix-target",
                MODEL,
                initial_usage(),
            )
            .map_err(debug_error)?;
            encode_events(encoder, events, |encoder, event| encoder.encode(event))
        }
        ProtocolKind::Gemini => {
            let encoder =
                gemini::GeminiGenerateContentStreamEncoder::new("gemini-matrix-target", MODEL)
                    .map_err(debug_error)?;
            encode_events(encoder, events, |encoder, event| encoder.encode(event))
        }
        ProtocolKind::OpenAiResponses => {
            let encoder = openai_responses::OpenAiResponsesStreamEncoder::new(
                "resp-matrix-target",
                MODEL,
                CREATED_AT,
            )
            .map_err(debug_error)?;
            encode_events(encoder, events, |encoder, event| encoder.encode(event))
        }
    }
}

fn encode_events<T, E>(
    mut encoder: T,
    events: &[CanonicalStreamEvent],
    mut encode: impl FnMut(&mut T, CanonicalStreamEvent) -> Result<Vec<u8>, E>,
) -> Result<Vec<u8>, String>
where
    E: std::fmt::Debug,
{
    let mut output = Vec::new();
    for event in events {
        output.extend(encode(&mut encoder, event.clone()).map_err(debug_error)?);
    }
    Ok(output)
}

fn initial_usage() -> Usage {
    Usage::new(
        TokenCount::new(8).unwrap(),
        TokenCount::ZERO,
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

fn assert_stream_semantics(expected: &[CanonicalStreamEvent], actual: &[CanonicalStreamEvent]) {
    assert_eq!(actual.len(), expected.len());
    for (expected, actual) in expected.iter().zip(actual) {
        match (expected, actual) {
            (CanonicalStreamEvent::Usage(expected), CanonicalStreamEvent::Usage(actual)) => {
                assert_eq!(
                    actual.checked_input_tokens().unwrap(),
                    expected.checked_input_tokens().unwrap()
                );
                assert_eq!(actual.output_tokens(), expected.output_tokens());
                assert_eq!(actual.details(), expected.details());
            }
            _ => assert_eq!(actual, expected),
        }
    }
}

fn wire_event_names(bytes: &[u8]) -> Vec<String> {
    let mut parser = openai_chat::SseParser::new();
    let events = parser.push(bytes).expect("编码结果必须是合法 SSE");
    parser.finish().expect("编码结果必须位于事件边界");
    events
        .into_iter()
        .map(|event| {
            if event.is_comment() {
                return "comment".to_owned();
            }
            if event.data() == b"[DONE]" {
                return "[DONE]".to_owned();
            }
            if let Some(name) = event.event_name() {
                return name.to_owned();
            }
            let value: serde_json::Value =
                serde_json::from_slice(event.data()).expect("SSE data 必须是 JSON");
            if value.get("responseId").is_some() {
                return "gemini.chunk".to_owned();
            }
            value
                .get("object")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("json")
                .to_owned()
        })
        .collect()
}

fn debug_error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
