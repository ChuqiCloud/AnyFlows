use af_domain::{Protocol, Role, UpstreamError};
use serde_json::{Value, json};

use super::*;
use crate::{
    CacheHint, CanonicalStreamEvent, ContentDelta, FinishReason, ProtocolCapability,
    StreamCapability, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
};

fn chunk(choices: Value, usage: Option<Value>) -> Value {
    let mut chunk = json!({
        "id": "chatcmpl-stream-test",
        "object": "chat.completion.chunk",
        "created": 1_700_000_000,
        "model": "gpt-stream-test",
        "choices": choices,
    });
    if let Some(usage) = usage {
        chunk["usage"] = usage;
    }
    chunk
}

fn sse(value: &Value) -> Vec<u8> {
    let data = serde_json::to_vec(value).unwrap();
    let mut frame = Vec::with_capacity(data.len() + 8);
    frame.extend_from_slice(b"data: ");
    frame.extend_from_slice(&data);
    frame.extend_from_slice(b"\n\n");
    frame
}

fn canonical_usage() -> Usage {
    Usage::new(
        TokenCount::new(5).unwrap(),
        TokenCount::new(3).unwrap(),
        UsageDetails::new(
            TokenCount::new(2).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::new(1).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

#[test]
fn sse_parser_handles_every_byte_boundary_line_ending_and_comment() {
    let input = b": keepalive\r\n\r\nevent: message\rdata: {\"a\":\rdata: 1}\r\r";
    let mut parser = SseParser::new();
    let mut events = Vec::new();
    for byte in input {
        events.extend(parser.push(std::slice::from_ref(byte)).unwrap());
    }
    parser.finish().unwrap();

    assert_eq!(events.len(), 2);
    assert!(events[0].is_comment());
    assert_eq!(events[1].event_name(), Some("message"));
    assert_eq!(events[1].data(), b"{\"a\":\n1}");
}

#[test]
fn sse_parser_enforces_limit_truncation_and_terminal_errors() {
    assert_eq!(
        SseParser::with_max_event_bytes(0).unwrap_err(),
        SseParseError::InvalidLimit
    );
    assert_eq!(
        SseParser::with_max_event_bytes(MAX_SSE_EVENT_BYTES + 1).unwrap_err(),
        SseParseError::InvalidLimit
    );

    let mut oversized = SseParser::with_max_event_bytes(8).unwrap();
    assert_eq!(
        oversized.push(b"data: 1\n\n").unwrap_err(),
        SseParseError::EventTooLarge
    );
    assert_eq!(
        oversized.push(b"").unwrap_err(),
        SseParseError::ParserFailed
    );

    let mut truncated = SseParser::new();
    assert!(truncated.push(b"data: partial\n").unwrap().is_empty());
    assert_eq!(
        truncated.finish().unwrap_err(),
        SseParseError::TruncatedEvent
    );
    assert_eq!(truncated.finish().unwrap_err(), SseParseError::ParserFailed);
}

#[test]
fn decoder_handles_multi_choice_tools_usage_and_done_across_every_byte() {
    let frames = [
        b": keepalive\n\n".to_vec(),
        sse(&chunk(
            json!([
                {"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": null},
                {"index": 1, "delta": {"role": "assistant"}, "finish_reason": null}
            ]),
            None,
        )),
        sse(&chunk(
            json!([
                {"index": 0, "delta": {"reasoning_content": "think", "content": "answer"}, "finish_reason": null},
                {"index": 1, "delta": {"content": "other"}, "finish_reason": null}
            ]),
            None,
        )),
        sse(&chunk(
            json!([{
                "index": 0,
                "delta": {"tool_calls": [{
                    "index": 3,
                    "id": "call_stream_test",
                    "type": "function",
                    "function": {"name": "lookup_weather", "arguments": "{\"city\":"}
                }]},
                "finish_reason": null
            }]),
            None,
        )),
        sse(&chunk(
            json!([{
                "index": 0,
                "delta": {"tool_calls": [{
                    "index": 3,
                    "function": {"arguments": "\"Paris\"}"}
                }]},
                "finish_reason": null
            }]),
            None,
        )),
        sse(&chunk(
            json!([
                {"index": 0, "delta": {}, "finish_reason": "tool_calls"},
                {"index": 1, "delta": {}, "finish_reason": "stop"}
            ]),
            None,
        )),
        sse(&chunk(
            json!([]),
            Some(json!({
                "prompt_tokens": 5,
                "completion_tokens": 3,
                "total_tokens": 8,
                "prompt_tokens_details": {"cached_tokens": 2},
                "completion_tokens_details": {"reasoning_tokens": 1},
                "prompt_cache_hit_tokens": 2,
                "prompt_cache_miss_tokens": 3
            })),
        )),
        b"data: [DONE]\n\n".to_vec(),
    ]
    .concat();

    let mut decoder = OpenAiChatStreamDecoder::new();
    let mut events = Vec::new();
    for byte in &frames {
        events.extend(decoder.push(std::slice::from_ref(byte)).unwrap());
    }
    decoder.finish().unwrap();

    assert_eq!(
        events,
        vec![
            CanonicalStreamEvent::Ping,
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::MessageStart {
                choice_index: 1,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 0,
                text: "think".to_owned(),
                signature: None,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 1,
                delta: ContentDelta::Text("answer".to_owned()),
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 1,
                content_index: 0,
                delta: ContentDelta::Text("other".to_owned()),
            },
            CanonicalStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 3,
                id: "call_stream_test".to_owned(),
                name: "lookup_weather".to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 3,
                partial_json: "{\"city\":".to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 3,
                partial_json: "\"Paris\"}".to_owned(),
            },
            CanonicalStreamEvent::ToolCallEnd {
                choice_index: 0,
                tool_index: 3,
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::ToolCalls,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Finish {
                choice_index: 1,
                reason: FinishReason::Stop,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(canonical_usage()),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
}

#[test]
fn decoder_accepts_newapi_request_id_on_stream_chunks() {
    let mut decoder = OpenAiChatStreamDecoder::new();
    let mut role = chunk(
        json!([{ "index": 0, "delta": { "role": "assistant" }, "finish_reason": null }]),
        None,
    );
    role["request_id"] = json!("newapi-request-id");
    decoder.push(&sse(&role)).unwrap();

    let mut reasoning = chunk(
        json!([{ "index": 0, "delta": { "reasoning_content": "think" }, "finish_reason": null }]),
        None,
    );
    reasoning["request_id"] = json!("newapi-request-id");
    let events = decoder.push(&sse(&reasoning)).unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::ReasoningDelta { text, .. } if text == "think"
    )));

    let mut finish = chunk(
        json!([{ "index": 0, "delta": {}, "finish_reason": "stop" }]),
        None,
    );
    finish["request_id"] = json!("newapi-request-id");
    decoder.push(&sse(&finish)).unwrap();
    decoder.push(b"data: [DONE]\n\n").unwrap();
    decoder.finish().unwrap();
}

#[test]
fn decoder_rejects_duplicate_json_and_becomes_terminal() {
    let mut decoder = OpenAiChatStreamDecoder::new();
    let error = decoder
        .push(b"data: {\"id\":\"private-first\",\"id\":\"private-second\"}\n\n")
        .unwrap_err();
    assert_eq!(error, ParseStreamError::DuplicateKey);
    assert_eq!(
        decoder.push(b"").unwrap_err(),
        ParseStreamError::DecoderFailed
    );
    let rendered = format!("{decoder:?}\n{error:?}\n{error}");
    assert!(!rendered.contains("private-first"));
    assert!(!rendered.contains("private-second"));
}

#[test]
fn decoder_rejects_metadata_drift_and_delta_before_role() {
    let mut decoder = OpenAiChatStreamDecoder::new();
    decoder
        .push(&sse(&chunk(
            json!([{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]),
            None,
        )))
        .unwrap();
    let mut changed = chunk(
        json!([{"index": 0, "delta": {"content": "secret"}, "finish_reason": null}]),
        None,
    );
    changed["model"] = json!("different-private-model");
    assert_eq!(
        decoder.push(&sse(&changed)).unwrap_err(),
        ParseStreamError::InvalidSequence
    );

    let mut before_role = OpenAiChatStreamDecoder::new();
    assert_eq!(
        before_role
            .push(&sse(&chunk(
                json!([{"index": 0, "delta": {"content": "early"}, "finish_reason": null}]),
                None,
            )))
            .unwrap_err(),
        ParseStreamError::InvalidSequence
    );
}

#[test]
fn decoder_rejects_known_unsupported_fields_and_invalid_tool_completion() {
    let mut unsupported = OpenAiChatStreamDecoder::new();
    assert_eq!(
        unsupported
            .push(&sse(&chunk(
                json!([{
                    "index": 0,
                    "delta": {"role": "assistant", "refusal": "private-refusal"},
                    "finish_reason": null
                }]),
                None,
            )))
            .unwrap_err(),
        ParseStreamError::UnsupportedFeature
    );

    let mut invalid_tool = OpenAiChatStreamDecoder::new();
    invalid_tool
        .push(&sse(&chunk(
            json!([{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_invalid_json",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{"}
                    }]
                },
                "finish_reason": null
            }]),
            None,
        )))
        .unwrap();
    assert_eq!(
        invalid_tool
            .push(&sse(&chunk(
                json!([{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]),
                None,
            )))
            .unwrap_err(),
        ParseStreamError::InvalidValue
    );
}

#[test]
fn decoder_requires_done_and_rejects_data_after_done() {
    let mut truncated = OpenAiChatStreamDecoder::new();
    truncated
        .push(&sse(&chunk(
            json!([{"index": 0, "delta": {"role": "assistant"}, "finish_reason": "stop"}]),
            None,
        )))
        .unwrap();
    assert_eq!(
        truncated.finish().unwrap_err(),
        ParseStreamError::UnexpectedEof
    );

    let stream = [
        sse(&chunk(
            json!([{"index": 0, "delta": {"role": "assistant"}, "finish_reason": "stop"}]),
            None,
        )),
        b"data: [DONE]\n\n".to_vec(),
    ]
    .concat();
    let mut completed = OpenAiChatStreamDecoder::new();
    completed.push(&stream).unwrap();
    assert_eq!(
        completed.push(b": late\n\n").unwrap_err(),
        ParseStreamError::DataAfterDone
    );
}

#[test]
fn decoder_propagates_configured_sse_limit_without_payloads() {
    let mut decoder = OpenAiChatStreamDecoder::with_max_event_bytes(32).unwrap();
    assert_eq!(
        decoder
            .push(b"data: {\"private\":\"oversized-canary\"}\n\n")
            .unwrap_err(),
        ParseStreamError::Sse(SseParseError::EventTooLarge)
    );
}

#[test]
fn encoder_round_trips_supported_canonical_events() {
    let expected = vec![
        CanonicalStreamEvent::Ping,
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: "think".to_owned(),
            signature: None,
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 1,
            delta: ContentDelta::Text("answer".to_owned()),
        },
        CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: 2,
            id: "call_round_trip".to_owned(),
            name: "lookup".to_owned(),
        },
        CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: 0,
            tool_index: 2,
            partial_json: "{\"city\":".to_owned(),
        },
        CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: 0,
            tool_index: 2,
            partial_json: "\"Paris\"}".to_owned(),
        },
        CanonicalStreamEvent::ToolCallEnd {
            choice_index: 0,
            tool_index: 2,
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::ToolCalls,
            stop_sequence: None,
        },
        CanonicalStreamEvent::Usage(canonical_usage()),
        CanonicalStreamEvent::StreamEnd,
    ];
    let mut encoder =
        OpenAiChatStreamEncoder::new("chatcmpl-stream-test", "gpt-stream-test", 1_700_000_000)
            .unwrap();
    let mut wire = Vec::new();
    for event in &expected {
        wire.extend(encoder.encode(event.clone()).unwrap());
    }

    let mut decoder = OpenAiChatStreamDecoder::new();
    let mut actual = Vec::new();
    for chunk in wire.chunks(7) {
        actual.extend(decoder.push(chunk).unwrap());
    }
    decoder.finish().unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn encoder_emits_openai_shape_and_exact_done_marker() {
    let mut encoder = OpenAiChatStreamEncoder::new("chatcmpl-shape", "gpt-shape", 42).unwrap();
    let role = encoder
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 7,
            role: Role::Assistant,
        })
        .unwrap();
    let data = role
        .strip_prefix(b"data: ")
        .and_then(|data| data.strip_suffix(b"\n\n"))
        .unwrap();
    let value: Value = serde_json::from_slice(data).unwrap();
    assert_eq!(value["id"], "chatcmpl-shape");
    assert_eq!(value["object"], "chat.completion.chunk");
    assert_eq!(value["choices"][0]["index"], 7);
    assert_eq!(value["choices"][0]["delta"]["role"], "assistant");
    assert!(value["choices"][0]["finish_reason"].is_null());

    encoder
        .encode(CanonicalStreamEvent::Finish {
            choice_index: 7,
            reason: FinishReason::Stop,
            stop_sequence: None,
        })
        .unwrap();
    assert_eq!(
        encoder.encode(CanonicalStreamEvent::StreamEnd).unwrap(),
        b"data: [DONE]\n\n"
    );
}

#[test]
fn encoder_can_emit_null_usage_on_regular_chunks() {
    let mut encoder = OpenAiChatStreamEncoder::new("chatcmpl-usage-null", "gpt-usage", 42)
        .unwrap()
        .with_usage_null_fields(true);
    let role = encoder
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    let data = role
        .strip_prefix(b"data: ")
        .and_then(|data| data.strip_suffix(b"\n\n"))
        .unwrap();
    let value: Value = serde_json::from_slice(data).unwrap();

    assert!(value.get("usage").is_some());
    assert!(value["usage"].is_null());
}

#[test]
fn encoder_rejects_unsupported_events_invalid_sequences_and_reuse() {
    assert_eq!(
        OpenAiChatStreamEncoder::new("", "private-model", 0).unwrap_err(),
        EncodeStreamError::InvalidMetadata
    );

    let mut sequence =
        OpenAiChatStreamEncoder::new("chatcmpl-sequence", "gpt-sequence", 0).unwrap();
    assert_eq!(
        sequence
            .encode(CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("early".to_owned()),
            })
            .unwrap_err(),
        EncodeStreamError::InvalidSequence
    );
    assert_eq!(
        sequence.encode(CanonicalStreamEvent::Ping).unwrap_err(),
        EncodeStreamError::EncoderFailed
    );

    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::User,
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Image {
                data: "private-image".to_owned(),
                mime_type: "image/png".to_owned(),
            },
        },
        CanonicalStreamEvent::Error(UpstreamError::network(
            af_domain::NetworkFailureKind::ResponseBody,
        )),
    ] {
        let mut encoder =
            OpenAiChatStreamEncoder::new("chatcmpl-unsupported", "gpt-unsupported", 0).unwrap();
        if !matches!(event, CanonicalStreamEvent::MessageStart { .. }) {
            encoder
                .encode(CanonicalStreamEvent::MessageStart {
                    choice_index: 0,
                    role: Role::Assistant,
                })
                .unwrap();
        }
        assert!(encoder.encode(event).is_err());
    }
}

#[test]
fn encoder_rejects_incomplete_tool_json_and_unrepresentable_usage() {
    let mut tool = OpenAiChatStreamEncoder::new("chatcmpl-tool", "gpt-tool", 0).unwrap();
    tool.encode(CanonicalStreamEvent::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    })
    .unwrap();
    tool.encode(CanonicalStreamEvent::ToolCallStart {
        choice_index: 0,
        tool_index: 0,
        id: "call_invalid".to_owned(),
        name: "lookup".to_owned(),
    })
    .unwrap();
    tool.encode(CanonicalStreamEvent::ToolCallArgsDelta {
        choice_index: 0,
        tool_index: 0,
        partial_json: "{".to_owned(),
    })
    .unwrap();
    assert_eq!(
        tool.encode(CanonicalStreamEvent::ToolCallEnd {
            choice_index: 0,
            tool_index: 0,
        })
        .unwrap_err(),
        EncodeStreamError::InvalidSequence
    );

    let usage = Usage::new(
        TokenCount::new(1).unwrap(),
        TokenCount::ZERO,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::new(1).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::CacheSeparated,
    )
    .unwrap();
    let mut encoder = OpenAiChatStreamEncoder::new("chatcmpl-usage", "gpt-usage", 0).unwrap();
    encoder
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    encoder
        .encode(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        })
        .unwrap();
    let error = encoder
        .encode(CanonicalStreamEvent::Usage(usage))
        .expect_err("OpenAI Chat 不支持缓存写入 usage");
    let EncodeStreamError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::OpenAiChat);
    assert_eq!(
        error.capability(),
        ProtocolCapability::Stream(StreamCapability::UsageCacheCreation(
            CacheHint::Ephemeral5Minutes
        ))
    );
}

#[test]
fn streaming_debug_output_never_contains_metadata_or_payloads() {
    let parser = SseParser::new();
    let decoder = OpenAiChatStreamDecoder::new();
    let encoder =
        OpenAiChatStreamEncoder::new("private-response-id", "private-model-name", 1_700_000_000)
            .unwrap();
    let mut event_parser = SseParser::new();
    let event = event_parser
        .push(b"event: private-event\ndata: private-payload\n\n")
        .unwrap()
        .pop()
        .unwrap();
    let rendered = format!("{parser:?}\n{decoder:?}\n{encoder:?}\n{event:?}");
    for private in [
        "private-response-id",
        "private-model-name",
        "private-event",
        "private-payload",
    ] {
        assert!(!rendered.contains(private));
    }
}
