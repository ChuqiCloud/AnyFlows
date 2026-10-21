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
fn opencode_plain_reasoning_alias_and_usage_trailer_preserve_single_events() {
    let usage = json!({
        "prompt_tokens":3111,"completion_tokens":27,"total_tokens":3138,
        "prompt_tokens_details":{"audio_tokens":0,"cached_tokens":3072,"cache_write_tokens":0},
        "completion_tokens_details":{"audio_tokens":0,"reasoning_tokens":23}
    });
    let stream = [
        b": keep-alive\n\n".to_vec(),
        sse(&chunk(json!([{"index":0,"delta":{
            "role":"assistant","content":"","reasoning":"Think",
            "reasoning_details":[{"type":"reasoning.text","text":"Think","format":"unknown","index":0}]
        }}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{
            "role":"assistant","reasoning_details":[{"type":"reasoning.text","text":" once","index":0}]
        }}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"role":"assistant","content":"SAVED"}}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"role":"assistant","content":"","reasoning":null},"finish_reason":"stop"}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":"stop"}]), Some(usage))),
        b"data: [DONE]\n\n".to_vec(),
    ].concat();
    for size in [1, 13, 256, stream.len()] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        let mut events = Vec::new();
        for bytes in stream.chunks(size) {
            events.extend(decoder.push(bytes).unwrap());
        }
        decoder.finish().unwrap();
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event {
                    CanonicalStreamEvent::ReasoningDelta { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<String>(),
            "Think once"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CanonicalStreamEvent::Finish { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CanonicalStreamEvent::Usage(_)))
                .count(),
            1
        );
        let final_usage = events
            .iter()
            .find_map(|event| match event {
                CanonicalStreamEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .unwrap();
        assert_eq!(final_usage.input_tokens().get(), 3111);
        assert_eq!(final_usage.output_tokens().get(), 27);
        assert!(matches!(
            events.last(),
            Some(CanonicalStreamEvent::StreamEnd)
        ));
    }
}

#[test]
fn reasoning_details_reject_conflicting_or_opaque_vendor_data() {
    for (details, expected) in [
        (
            json!([{"type":"reasoning.text","text":"different","format":"unknown","index":0}]),
            ParseStreamError::InvalidValue,
        ),
        (
            json!([{"type":"reasoning.encrypted","data":"opaque"}]),
            ParseStreamError::UnsupportedFeature,
        ),
        (
            json!([{"type":"reasoning.text","text":"Think","signature":"opaque"}]),
            ParseStreamError::UnsupportedFeature,
        ),
        (
            json!([{"type":"reasoning.text","text":"Think","format":"anthropic-claude-v1","index":0}]),
            ParseStreamError::UnsupportedFeature,
        ),
        (
            json!([{"type":"reasoning.text","text":"Think","index":1}]),
            ParseStreamError::UnsupportedFeature,
        ),
    ] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        let event = chunk(
            json!([{"index":0,"delta":{
                "role":"assistant","reasoning":"Think","reasoning_details":details
            }}]),
            None,
        );
        assert_eq!(decoder.push(&sse(&event)).unwrap_err(), expected);
    }
}

#[test]
fn usage_trailer_rejects_new_content_changed_finish_and_unmarked_duplicates() {
    for (delta, finish, usage) in [
        (
            json!({"content":"late"}),
            "stop",
            Some(json!({"prompt_tokens":1,"completion_tokens":1})),
        ),
        (
            json!({}),
            "length",
            Some(json!({"prompt_tokens":1,"completion_tokens":1})),
        ),
        (json!({}), "stop", None),
    ] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        decoder
            .push(&sse(&chunk(
                json!([{
                    "index":0,"delta":{"role":"assistant","content":"OK"},"finish_reason":"stop"
                }]),
                None,
            )))
            .unwrap();
        let trailer = chunk(
            json!([{"index":0,"delta":delta,"finish_reason":finish}]),
            usage,
        );
        assert_eq!(
            decoder.push(&sse(&trailer)).unwrap_err(),
            ParseStreamError::InvalidSequence
        );
    }
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
fn gateway_placeholder_ids_and_early_usage_keep_one_terminal_and_usage() {
    let mut content = chunk(json!([{"index":0,"delta":{"content":"OK"}}]), None);
    content["id"] = json!("chatcmpl-wb");
    let mut stop = chunk(json!([{"index":0,"delta":{},"finish_reason":"stop"}]), None);
    stop["id"] = json!("chatcmpl-wb");
    let stream = [
        sse(&chunk(
            json!([{"index":0,"delta":{"role":"assistant"}}]),
            None,
        )),
        sse(&content),
        sse(&chunk(
            json!([{"index":0,"delta":{"role":"assistant","content":""}}]),
            Some(json!({
                "prompt_tokens":15,"completion_tokens":1,"total_tokens":16,
                "completion_thinking_tokens":0,"cached_tokens":0,
                "cache_read_input_tokens":0,"cache_creation_input_tokens":0,
                "prompt_cache_write_tokens":0,"credit":0
            })),
        )),
        sse(&stop),
        b"data: [DONE]\n\n".to_vec(),
    ]
    .concat();
    for size in [1, 7, stream.len()] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        let mut events = Vec::new();
        for bytes in stream.chunks(size) {
            events.extend(decoder.push(bytes).unwrap());
        }
        decoder.finish().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, CanonicalStreamEvent::MessageStart { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, CanonicalStreamEvent::Finish { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, CanonicalStreamEvent::Usage(_)))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, CanonicalStreamEvent::StreamEnd))
                .count(),
            1
        );
        assert!(events.iter().any(|e| matches!(e, CanonicalStreamEvent::ContentDelta { delta: ContentDelta::Text(text), .. } if text == "OK")));
    }
}

#[test]
fn placeholder_id_does_not_allow_real_identity_drift_or_missing_finish() {
    for field in ["id", "model", "created"] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        let mut start = chunk(json!([{"index":0,"delta":{"role":"assistant"}}]), None);
        start["id"] = json!("chatcmpl-wb");
        decoder.push(&sse(&start)).unwrap();
        let pinned = chunk(json!([{"index":0,"delta":{"content":"OK"}}]), None);
        decoder.push(&sse(&pinned)).unwrap();
        let mut changed = pinned;
        changed[field] = if field == "created" {
            json!(1_700_000_001)
        } else {
            json!("different")
        };
        assert_eq!(
            decoder.push(&sse(&changed)),
            Err(ParseStreamError::InvalidSequence)
        );
    }
    let mut decoder = OpenAiChatStreamDecoder::new();
    decoder
        .push(&sse(&chunk(
            json!([{"index":0,"delta":{"role":"assistant","content":"OK"}}]),
            Some(json!({"prompt_tokens":15,"completion_tokens":1,"total_tokens":16})),
        )))
        .unwrap();
    assert_eq!(
        decoder.push(b"data: [DONE]\n\n"),
        Err(ParseStreamError::InvalidSequence)
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
        completed.push(&sse(&chunk(json!([]), None))).unwrap_err(),
        ParseStreamError::DataAfterDone
    );
}

#[test]
fn decoder_accepts_delayed_tool_identity_and_empty_continuations() {
    let stream = [
        sse(&chunk(json!([{"index":0,"delta":{"role":"assistant","tool_calls":[]}}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"value\":"}}]}}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_delayed","type":"function","function":{"name":"routing_probe"}}]}}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"tool_calls":[{"index":0,"id":"","type":"","function":{"name":"","arguments":"\"ok\"}"}}]}}]), None)),
        sse(&chunk(json!([{"index":0,"delta":{"tool_calls":[]},"finish_reason":"tool_calls"}]), None)),
        b"data: [DONE]\n\n".to_vec(),
    ].concat();
    for size in [1, 7, stream.len()] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        let mut events = Vec::new();
        for bytes in stream.chunks(size) {
            events.extend(decoder.push(bytes).unwrap());
        }
        decoder.finish().unwrap();
        let calls: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                CanonicalStreamEvent::ToolCallStart { id, name, .. } => {
                    Some((id.as_str(), name.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(calls, vec![("call_delayed", "routing_probe")]);
        let arguments: String = events
            .iter()
            .filter_map(|event| match event {
                CanonicalStreamEvent::ToolCallArgsDelta { partial_json, .. } => {
                    Some(partial_json.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(arguments, "{\"value\":\"ok\"}");
        assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
    }
}

#[test]
fn decoder_accepts_fragmented_tool_names_before_arguments() {
    let frames = [
        json!([{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_fragment","function":{"name":"routing_"}}]}}]),
        json!([{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"probe","arguments":"{}"}}]}}]),
        json!([{"index":0,"delta":{},"finish_reason":"tool_calls"}]),
    ];
    let mut decoder = OpenAiChatStreamDecoder::new();
    let mut events = Vec::new();
    for choices in frames {
        events.extend(decoder.push(&sse(&chunk(choices, None))).unwrap());
    }
    events.extend(decoder.push(b"data: [DONE]\n\n").unwrap());
    decoder.finish().unwrap();
    assert!(events.iter().any(|event| matches!(event,
        CanonicalStreamEvent::ToolCallStart { name, .. } if name == "routing_probe"
    )));
}

#[test]
fn decoder_rejects_unresolved_or_changed_tool_identity() {
    for continuation in [
        json!({"index":0,"id":"different","function":{"arguments":"}"}}),
        json!({"index":0,"function":{"name":"different","arguments":"}"}}),
        json!({"index":0,"type":"unsupported"}),
    ] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        decoder.push(&sse(&chunk(json!([{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_original","type":"function","function":{"name":"lookup","arguments":"{"}}]}}]), None))).unwrap();
        assert!(
            decoder
                .push(&sse(&chunk(
                    json!([{"index":0,"delta":{"tool_calls":[continuation]}}]),
                    None
                )))
                .is_err()
        );
    }
    let mut decoder = OpenAiChatStreamDecoder::new();
    decoder.push(&sse(&chunk(json!([{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]), None))).unwrap();
    assert!(
        decoder
            .push(&sse(&chunk(
                json!([{"index":0,"delta":{},"finish_reason":"tool_calls"}]),
                None
            )))
            .is_err()
    );
}

#[test]
fn decoder_preserves_logprobs_delta_fields() {
    let stream = [
        sse(&chunk(
            json!([{
                "index": 0,
                "delta": {"role": "assistant"},
                "logprobs": {
                    "content": [{"token": "Hello", "logprob": -0.1}],
                    "reasoning_content": [{"token": "Think", "logprob": -0.2}]
                },
                "finish_reason": null
            }]),
            None,
        )),
        sse(&chunk(
            json!([{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": "stop"
            }]),
            None,
        )),
        b"data: [DONE]\n\n".to_vec(),
    ]
    .concat();
    let mut decoder = OpenAiChatStreamDecoder::new();
    let mut events = decoder.push(&stream).unwrap();
    decoder.finish().unwrap();

    assert_eq!(
        events.remove(1),
        CanonicalStreamEvent::LogprobsDelta {
            choice_index: 0,
            logprobs: json!({
                "content": [{"token": "Hello", "logprob": -0.1}],
                "reasoning_content": [{"token": "Think", "logprob": -0.2}]
            })
        }
    );
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::ContentDelta {
            delta: ContentDelta::Text(text),
            ..
        } if text == "Hello"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::Finish {
            reason: FinishReason::Stop,
            ..
        }
    )));
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn decoder_accepts_gateway_transport_metadata_without_affecting_usage() {
    let mut frame = chunk(
        json!([{"index": 0, "delta": {"role": "assistant", "content": "OK"}, "finish_reason": "stop"}]),
        Some(json!({"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6})),
    );
    frame["service_tier"] = json!("priority-v2");
    frame["elapsed_ms"] = json!(245.5);
    frame["first_chunk_at"] = json!("2026-10-21T03:15:00Z");
    let stream = [sse(&frame), b"data: [DONE]\n\n".to_vec()].concat();
    let mut decoder = OpenAiChatStreamDecoder::new();
    let mut events = Vec::new();
    for bytes in stream.chunks(7) {
        events.extend(decoder.push(bytes).unwrap());
    }
    decoder.finish().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::Usage(usage)
            if usage.input_tokens().get() == 5 && usage.output_tokens().get() == 1
    )));
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn compatible_gateway_stream_uses_last_usage_once_at_every_byte_boundary() {
    let provisional = json!({"prompt_tokens":2041,"completion_tokens":1,"total_tokens":2042,"reasoning_tokens":0});
    let final_usage = json!({"prompt_tokens":41,"completion_tokens":1,"total_tokens":42});
    let stream = [
        sse(&chunk(
            json!([{"index":0,"delta":{"role":"assistant","content":""}}]),
            None,
        )),
        sse(&chunk(
            json!([{"index":0,"delta":{"role":"assistant","content":"OK"}}]),
            None,
        )),
        sse(&chunk(
            json!([{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":"stop"}]),
            Some(provisional),
        )),
        sse(&chunk(json!([]), Some(final_usage))),
        b"data: [DONE]\n\ndata: [DONE]\n\n".to_vec(),
    ]
    .concat();
    for size in [1, 7, 64, stream.len()] {
        let mut decoder = OpenAiChatStreamDecoder::new();
        let mut events = Vec::new();
        for bytes in stream.chunks(size) {
            events.extend(decoder.push(bytes).unwrap());
        }
        decoder.finish().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, CanonicalStreamEvent::MessageStart { .. }))
                .count(),
            1
        );
        let usage: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                CanonicalStreamEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .collect();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].input_tokens().get(), 41);
        assert_eq!(usage[0].output_tokens().get(), 1);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, CanonicalStreamEvent::StreamEnd))
                .count(),
            1
        );
        assert!(events.iter().any(|e| matches!(e, CanonicalStreamEvent::ContentDelta { delta: ContentDelta::Text(text), .. } if text == "OK")));
    }
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
        CanonicalStreamEvent::LogprobsDelta {
            choice_index: 0,
            logprobs: json!({
                "content": [{"token": "answer", "logprob": -0.1}],
                "reasoning_content": [{"token": "think", "logprob": -0.2}]
            }),
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
    let logprobs = CanonicalStreamEvent::LogprobsDelta {
        choice_index: 0,
        logprobs: json!({"content": [{"token": "logprobs-secret-canary"}]}),
    };
    assert!(!format!("{logprobs:?}").contains("logprobs-secret-canary"));
}
