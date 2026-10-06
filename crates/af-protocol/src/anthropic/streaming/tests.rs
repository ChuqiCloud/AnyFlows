use af_domain::{Protocol, Role, UpstreamError};
use serde_json::{Value, json};

use super::{
    AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder, EncodeStreamError,
    ParseStreamError,
};
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason, ProtocolCapability, StreamCapability,
    TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
    openai_chat::OpenAiChatStreamEncoder,
};

fn usage(input: i64, output: i64, reasoning: i64) -> Usage {
    Usage::new(
        TokenCount::new(input).unwrap(),
        TokenCount::new(output).unwrap(),
        UsageDetails::new(
            TokenCount::new(12).unwrap(),
            TokenCount::new(5).unwrap(),
            TokenCount::new(3).unwrap(),
            TokenCount::new(reasoning).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::CacheSeparated,
    )
    .unwrap()
}

fn initial_usage_wire(output: i64, reasoning: i64) -> Value {
    json!({
        "input_tokens": 100,
        "cache_creation_input_tokens": 8,
        "cache_read_input_tokens": 12,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 5,
            "ephemeral_1h_input_tokens": 3
        },
        "output_tokens": output,
        "output_tokens_details": { "thinking_tokens": reasoning },
        "inference_geo": null,
        "server_tool_use": null,
        "service_tier": null
    })
}

fn final_usage_wire(output: i64, reasoning: i64) -> Value {
    json!({
        "input_tokens": 100,
        "cache_creation_input_tokens": 8,
        "cache_read_input_tokens": 12,
        "output_tokens": output,
        "output_tokens_details": { "thinking_tokens": reasoning },
        "server_tool_use": null
    })
}

fn event(name: &str, payload: Value) -> Vec<u8> {
    format!(
        "event: {name}\ndata: {}\n\n",
        serde_json::to_string(&payload).unwrap()
    )
    .into_bytes()
}

fn event_payload(bytes: &[u8]) -> Value {
    serde_json::from_slice(
        &bytes
            .split(|byte| *byte == b'\n')
            .find(|line| line.starts_with(b"data: "))
            .unwrap()[6..],
    )
    .unwrap()
}

fn message_start(output: i64, reasoning: i64) -> Vec<u8> {
    event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_stream_test",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-stream-test",
                "container": null,
                "stop_reason": null,
                "stop_sequence": null,
                "stop_details": null,
                "usage": initial_usage_wire(output, reasoning)
            }
        }),
    )
}

fn message_delta(reason: &str, stop_sequence: Option<&str>, output: i64, thinking: i64) -> Vec<u8> {
    event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": {
                "container": null,
                "stop_details": null,
                "stop_reason": reason,
                "stop_sequence": stop_sequence
            },
            "usage": final_usage_wire(output, thinking)
        }),
    )
}

fn full_anthropic_stream() -> Vec<u8> {
    [
        message_start(1, 1),
        event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "thinking", "thinking": "", "signature": "" }
            }),
        ),
        event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "thinking_delta", "thinking": "plan" }
            }),
        ),
        event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "signature_delta", "signature": "sig" }
            }),
        ),
        event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 }),
        ),
        event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 1,
                "content_block": { "type": "text", "text": "", "citations": null }
            }),
        ),
        event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": { "type": "text_delta", "text": "answer" }
            }),
        ),
        event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 1 }),
        ),
        event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 2,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_stream_test",
                    "name": "lookup",
                    "input": {},
                    "caller": { "type": "direct" }
                }
            }),
        ),
        event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 2,
                "delta": { "type": "input_json_delta", "partial_json": "{\"city\":" }
            }),
        ),
        event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 2,
                "delta": { "type": "input_json_delta", "partial_json": "\"Paris\"}" }
            }),
        ),
        event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 2 }),
        ),
        message_delta("tool_use", None, 20, 5),
        event("message_stop", json!({ "type": "message_stop" })),
    ]
    .concat()
}

fn expected_full_events() -> Vec<CanonicalStreamEvent> {
    vec![
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: "plan".to_owned(),
            signature: None,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: String::new(),
            signature: Some("sig".to_owned()),
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 1,
            delta: ContentDelta::Text("answer".to_owned()),
        },
        CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: 2,
            id: "toolu_stream_test".to_owned(),
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
        CanonicalStreamEvent::Usage(usage(100, 20, 5)),
        CanonicalStreamEvent::StreamEnd,
    ]
}

#[test]
fn decoder_handles_every_byte_boundary_and_emits_one_final_usage() {
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    let mut actual = Vec::new();
    for byte in full_anthropic_stream() {
        actual.extend(decoder.push(&[byte]).unwrap());
    }
    decoder.finish().unwrap();
    assert_eq!(actual, expected_full_events());
    assert_eq!(
        actual
            .iter()
            .filter(|event| matches!(event, CanonicalStreamEvent::Usage(_)))
            .count(),
        1
    );
}

#[test]
fn decoder_preserves_the_matched_stop_sequence() {
    let stream = [
        message_start(0, 0),
        event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "done", "citations": null }
            }),
        ),
        event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 }),
        ),
        message_delta("stop_sequence", Some("END"), 4, 0),
        event("message_stop", json!({ "type": "message_stop" })),
    ]
    .concat();
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    let events = decoder.push(&stream).unwrap();
    decoder.finish().unwrap();
    assert!(events.contains(&CanonicalStreamEvent::Finish {
        choice_index: 0,
        reason: FinishReason::Stop,
        stop_sequence: Some("END".to_owned()),
    }));
}

#[test]
fn decoder_maps_stream_errors_without_retaining_the_message() {
    let secret = "upstream-error-secret-canary";
    let bytes = event(
        "error",
        json!({
            "type": "error",
            "error": { "type": "overloaded_error", "message": secret }
        }),
    );
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(&bytes).unwrap(),
        vec![CanonicalStreamEvent::Error(UpstreamError::overloaded())]
    );
    decoder.finish().unwrap();
    assert!(!format!("{decoder:?}").contains(secret));
}

#[test]
fn decoder_distinguishes_recoverable_auth_and_permission_errors() {
    for (kind, expected) in [
        ("authentication_error", UpstreamError::AuthExpired),
        ("permission_error", UpstreamError::ProtocolError),
        (
            "rate_limit_error",
            UpstreamError::rate_limited(af_domain::RateLimitScope::Window),
        ),
    ] {
        let bytes = event(
            "error",
            json!({
                "type": "error",
                "error": { "type": kind, "message": "private" }
            }),
        );
        let mut decoder = AnthropicMessagesStreamDecoder::new();
        assert_eq!(
            decoder.push(&bytes).unwrap(),
            vec![CanonicalStreamEvent::Error(expected)]
        );
        decoder.finish().unwrap();
    }
}

#[test]
fn decoder_rejects_unsupported_and_mismatched_events() {
    let citations = event(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "citations_delta", "citation": {} }
        }),
    );
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(&citations),
        Err(ParseStreamError::UnsupportedFeature)
    );
    assert_eq!(
        decoder.push(&message_start(0, 0)),
        Err(ParseStreamError::DecoderFailed)
    );

    let mismatch = event("message_stop", json!({ "type": "ping" }));
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(&mismatch),
        Err(ParseStreamError::InvalidSequence)
    );
}

#[test]
fn decoder_rejects_duplicate_keys_truncation_and_data_after_stop() {
    let duplicate = b"event: ping\ndata: {\"type\":\"ping\",\"type\":\"ping\"}\n\n";
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(decoder.push(duplicate), Err(ParseStreamError::DuplicateKey));

    let mut truncated = AnthropicMessagesStreamDecoder::new();
    truncated.push(&message_start(0, 0)).unwrap();
    assert_eq!(truncated.finish(), Err(ParseStreamError::UnexpectedEof));

    let mut completed = AnthropicMessagesStreamDecoder::new();
    completed.push(&full_anthropic_stream()).unwrap();
    assert_eq!(
        completed.push(&event("ping", json!({ "type": "ping" }))),
        Err(ParseStreamError::DataAfterDone)
    );
}

#[test]
fn decoder_rejects_non_cumulative_usage_and_unmodelled_cache_breakdown() {
    let wrong_final = [message_start(1, 1), message_delta("end_turn", None, 0, 0)].concat();
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(&wrong_final),
        Err(ParseStreamError::InvalidSequence)
    );

    let mut start = event_payload(&message_start(0, 0));
    start["message"]["usage"]["cache_creation"] = Value::Null;
    let bytes = event("message_start", start);
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(&bytes),
        Err(ParseStreamError::UnsupportedFeature)
    );
}

#[test]
fn decoder_accepts_validated_service_tier_in_initial_usage() {
    let mut start = event_payload(&message_start(0, 0));
    start["message"]["usage"]["service_tier"] = json!("standard");

    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(&event("message_start", start)).unwrap(),
        vec![CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        }]
    );
    assert!(decoder.initial_usage().is_some());
}

#[test]
fn decoder_accepts_consistent_service_tier_in_final_usage() {
    let mut start = event_payload(&message_start(0, 0));
    start["message"]["usage"]["service_tier"] = json!("standard");
    let mut delta = event_payload(&message_delta("end_turn", None, 1, 0));
    delta["usage"]["service_tier"] = json!("standard");

    let mut decoder = AnthropicMessagesStreamDecoder::new();
    let events = decoder
        .push(
            &[
                event("message_start", start),
                event("message_delta", delta),
                event("message_stop", json!({ "type": "message_stop" })),
            ]
            .concat(),
        )
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CanonicalStreamEvent::Usage(_)))
    );
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn decoder_accepts_deepseek_anthropic_stream() {
    let start = event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": "85db1bb8-9369-4436-bcbe-810d4f86a43b",
                "type": "message",
                "role": "assistant",
                "model": "deepseek-flash",
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": 31,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 0,
                    "output_tokens": 0,
                    "service_tier": "standard"
                }
            }
        }),
    );
    let thinking_start = event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "thinking", "thinking": "", "signature": ""}
        }),
    );
    let thinking = event(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "thinking_delta", "thinking": "We"}
        }),
    );
    let signature = event(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "signature_delta", "signature": "85db1bb8-9369-4436-bcbe-810d4f86a43b"}
        }),
    );
    let block_stop = event(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 0}),
    );
    let delta = event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": {"stop_reason": "max_tokens", "stop_sequence": null},
            "usage": {
                "input_tokens": 31,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 1,
                "service_tier": "standard"
            }
        }),
    );
    let stop = event("message_stop", json!({"type": "message_stop"}));

    let mut decoder = AnthropicMessagesStreamDecoder::new();
    let events = decoder
        .push(
            &[
                start,
                thinking_start,
                thinking,
                signature,
                block_stop,
                delta,
                stop,
            ]
            .concat(),
        )
        .unwrap();
    decoder.finish().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::ReasoningDelta { text, .. } if text == "We"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::Finish {
            reason: FinishReason::Length,
            ..
        }
    )));
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn decoder_rejects_conflicting_service_tiers() {
    let mut start = event_payload(&message_start(0, 0));
    start["message"]["usage"]["service_tier"] = json!("standard");
    let mut delta = event_payload(&message_delta("end_turn", None, 1, 0));
    delta["usage"]["service_tier"] = json!("priority");

    let mut decoder = AnthropicMessagesStreamDecoder::new();
    decoder.push(&event("message_start", start)).unwrap();
    assert_eq!(
        decoder.push(&event("message_delta", delta)),
        Err(ParseStreamError::InvalidSequence)
    );
}

#[test]
fn encoder_round_trips_text_thinking_tools_and_usage() {
    let mut encoder = AnthropicMessagesStreamEncoder::new(
        "msg_stream_test",
        "claude-stream-test",
        usage(100, 1, 1),
    )
    .unwrap();
    let source = expected_full_events();
    let mut bytes = Vec::new();
    for event in source.clone() {
        bytes.extend(encoder.encode(event).unwrap());
    }

    let rendered = String::from_utf8(bytes.clone()).unwrap();
    let event_names = rendered
        .lines()
        .filter_map(|line| line.strip_prefix("event: "))
        .collect::<Vec<_>>();
    assert_eq!(
        event_names,
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_stop",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ]
    );

    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(decoder.push(&bytes).unwrap(), source);
    decoder.finish().unwrap();
}

#[test]
fn encoder_preserves_stop_sequence_and_uses_final_usage_snapshot() {
    let mut encoder =
        AnthropicMessagesStreamEncoder::new("msg_stop_test", "claude-stop-test", usage(100, 0, 0))
            .unwrap();
    let events = [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 9,
            delta: ContentDelta::Text("done".to_owned()),
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: Some("END".to_owned()),
        },
        CanonicalStreamEvent::Usage(usage(100, 4, 0)),
        CanonicalStreamEvent::StreamEnd,
    ];
    let bytes = events
        .into_iter()
        .flat_map(|event| encoder.encode(event).unwrap())
        .collect::<Vec<_>>();
    let rendered = String::from_utf8(bytes).unwrap();
    assert!(rendered.contains("\"stop_reason\":\"stop_sequence\""));
    assert!(rendered.contains("\"stop_sequence\":\"END\""));
    assert!(rendered.contains("\"output_tokens\":4"));
}

#[test]
fn encoder_fails_closed_on_missing_signature_or_usage_regression() {
    let mut signature = AnthropicMessagesStreamEncoder::new(
        "msg_signature_test",
        "claude-signature-test",
        usage(100, 0, 0),
    )
    .unwrap();
    signature
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    signature
        .encode(CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: "plan".to_owned(),
            signature: None,
        })
        .unwrap();
    assert_eq!(
        signature.encode(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        }),
        Err(EncodeStreamError::InvalidSequence)
    );
    assert_eq!(
        signature.encode(CanonicalStreamEvent::Ping),
        Err(EncodeStreamError::EncoderFailed)
    );

    let mut usage_regression = AnthropicMessagesStreamEncoder::new(
        "msg_usage_test",
        "claude-usage-test",
        usage(100, 4, 1),
    )
    .unwrap();
    usage_regression
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    usage_regression
        .encode(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        })
        .unwrap();
    assert_eq!(
        usage_regression.encode(CanonicalStreamEvent::Usage(usage(100, 3, 1))),
        Err(EncodeStreamError::InvalidUsage)
    );
}

#[test]
fn openai_encoder_rejects_anthropic_stop_sequence_without_dropping_it() {
    let mut encoder =
        OpenAiChatStreamEncoder::new("chatcmpl-stop-test", "gpt-stop-test", 1).unwrap();
    encoder
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    let error = encoder
        .encode(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: Some("END".to_owned()),
        })
        .expect_err("OpenAI Chat 不得丢弃停止序列");
    let crate::openai_chat::EncodeStreamError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::OpenAiChat);
    assert_eq!(
        error.capability(),
        ProtocolCapability::Stream(StreamCapability::StopSequence)
    );
}

#[test]
fn streaming_debug_output_redacts_identity_and_payloads() {
    let id = "stream-id-secret-canary";
    let model = "stream-model-secret-canary";
    let encoder = AnthropicMessagesStreamEncoder::new(id, model, usage(100, 0, 0)).unwrap();
    let decoder = AnthropicMessagesStreamDecoder::new();
    let rendered = format!("{encoder:?}\n{decoder:?}");
    assert!(!rendered.contains(id));
    assert!(!rendered.contains(model));
    assert!(rendered.contains("<已脱敏>"));
}
