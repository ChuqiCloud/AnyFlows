use af_domain::{Role, UpstreamError};
use serde_json::{Value, json};

use super::{
    CREATED_AT, MODEL, RESPONSE_ID, frame, frame_without_sequence, initial_response, join,
    message_item, output_text, response, usage, usage_value,
};
use crate::openai_responses::{OpenAiResponsesStreamDecoder, ParseStreamError};
use crate::{CanonicalStreamEvent, ContentDelta, FinishReason};

#[test]
fn decodes_text_stream_across_transport_chunks() {
    let item_id = "msg_stream_1";
    let mut terminal_item = message_item(item_id, "completed", "hello");
    terminal_item["phase"] = json!("final_answer");
    let mut lifecycle = initial_response();
    lifecycle["service_tier"] = json!("auto");
    lifecycle["access_programs"] = json!({"cyber": "daybreak_blue"});
    let mut terminal = response(
        "completed",
        Vec::new(),
        Some(usage_value(8, 3, 2, 1)),
        None,
        None,
    );
    terminal["service_tier"] = json!("default");
    terminal["access_programs"] = json!({"cyber": "daybreak_blue"});
    terminal["usage"]["attribution"] = json!({"items": {}, "request_fields": {}});
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": lifecycle.clone()}),
        ),
        frame("response.in_progress", 1, json!({"response": lifecycle})),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": item_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                    "phase": "final_answer",
                }
            }),
        ),
        frame(
            "response.content_part.added",
            3,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text(""),
            }),
        ),
        frame(
            "response.output_text.delta",
            4,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": "hel",
                "logprobs": [],
            }),
        ),
        frame(
            "response.output_text.delta",
            5,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": "lo",
                "logprobs": [],
            }),
        ),
        frame(
            "response.output_text.done",
            6,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "text": "hello",
                "logprobs": [],
            }),
        ),
        frame(
            "response.content_part.done",
            7,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text("hello"),
            }),
        ),
        frame(
            "response.output_item.done",
            8,
            json!({"output_index": 0, "item": terminal_item.clone()}),
        ),
        frame("response.completed", 9, json!({"response": terminal})),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let mut events = Vec::new();
    for chunk in bytes.chunks(3) {
        events.extend(decoder.push(chunk).unwrap());
    }
    decoder.finish().unwrap();

    assert_eq!(
        events,
        vec![
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("hel".to_owned()),
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("lo".to_owned()),
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::Stop,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(usage(8, 3, 2, 1)),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
    let debug = format!("{events:?}");
    assert!(!debug.contains("hello"));
    assert!(!format!("{decoder:?}").contains(RESPONSE_ID));
}

#[test]
fn decodes_compaction_item_lifecycle() {
    let item = json!({
        "type": "compaction",
        "id": "cmp_stream_1",
        "encrypted_content": "opaque-compaction"
    });
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({"output_index": 0, "item": item.clone()}),
        ),
        frame(
            "response.output_item.done",
            3,
            json!({"output_index": 0, "item": item.clone()}),
        ),
        frame(
            "response.completed",
            4,
            json!({
                "response": response(
                    "completed",
                    vec![item],
                    Some(usage_value(8, 3, 2, 1)),
                    None,
                    None,
                )
            }),
        ),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert!(matches!(
        events[1],
        CanonicalStreamEvent::CompactionStart {
            output_index: 0,
            ..
        }
    ));
    assert!(matches!(
        events[2],
        CanonicalStreamEvent::CompactionEnd {
            output_index: 0,
            ..
        }
    ));
    assert!(events.contains(&CanonicalStreamEvent::Finish {
        choice_index: 0,
        reason: FinishReason::Stop,
        stop_sequence: None,
    }));
}

#[test]
fn terminal_snapshot_closes_message_when_done_events_are_omitted() {
    let item_id = "msg_terminal_close";
    let bytes = stream_with_terminal_message_only(item_id, "hello", "hello");
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();

    assert_eq!(
        events,
        vec![
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("hello".to_owned()),
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::Stop,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(usage(8, 3, 2, 1)),
            CanonicalStreamEvent::StreamEnd,
        ]
    );

    let mismatch = stream_with_terminal_message_only(item_id, "left", "right");
    let mut mismatch_decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        mismatch_decoder.push(&mismatch),
        Err(ParseStreamError::InvalidSequence)
    );
}

/// 构造省略文本、Part 与 Item 三层完成事件的供应商兼容流。
fn stream_with_terminal_message_only(item_id: &str, delta: &str, terminal_text: &str) -> Vec<u8> {
    join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": item_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                }
            }),
        ),
        frame(
            "response.content_part.added",
            3,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text(""),
            }),
        ),
        frame(
            "response.output_text.delta",
            4,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": delta,
                "logprobs": [],
            }),
        ),
        frame(
            "response.completed",
            5,
            json!({
                "response": response(
                    "completed",
                    vec![message_item(item_id, "completed", terminal_text)],
                    Some(usage_value(8, 3, 2, 1)),
                    None,
                    None,
                )
            }),
        ),
    ])
}

#[test]
fn rejects_commentary_message_phase() {
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": "msg_commentary",
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                    "phase": "commentary",
                }
            }),
        ),
    ]);
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        decoder.push(&bytes),
        Err(ParseStreamError::UnsupportedFeature)
    );
}

#[test]
fn decodes_compatible_responses_stream_placeholders_and_usage_alias() {
    let reasoning_id = "rs_stream_current";
    let message_id = "msg_stream_current";
    let reasoning_done = json!({
        "id": reasoning_id,
        "type": "reasoning",
        "status": "completed",
        "summary": [],
        "content": [],
        "encrypted_content": "sig-current",
    });
    let terminal_item = message_item(message_id, "completed", "ok");
    let mut terminal_usage = usage_value(5, 2, 3, 1);
    terminal_usage["prompt_tokens_details"] = json!({"cached_tokens": 3});
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": current_echo_response("in_progress", Vec::new(), None)}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": current_echo_response("in_progress", Vec::new(), None)}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": reasoning_id,
                    "type": "reasoning",
                    "summary": [],
                    "content": [],
                    "encrypted_content": "",
                }
            }),
        ),
        frame(
            "response.output_item.done",
            3,
            json!({"output_index": 0, "item": reasoning_done.clone()}),
        ),
        frame(
            "response.output_item.added",
            4,
            json!({
                "output_index": 1,
                "item": {
                    "id": message_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                }
            }),
        ),
        frame(
            "response.content_part.added",
            5,
            json!({
                "item_id": message_id,
                "output_index": 1,
                "content_index": 0,
                "part": output_text(""),
            }),
        ),
        frame(
            "response.output_text.delta",
            6,
            json!({
                "item_id": message_id,
                "output_index": 1,
                "content_index": 0,
                "delta": "ok",
                "logprobs": [],
            }),
        ),
        frame(
            "response.output_text.done",
            7,
            json!({
                "item_id": message_id,
                "output_index": 1,
                "content_index": 0,
                "text": "ok",
                "logprobs": [],
            }),
        ),
        frame(
            "response.content_part.done",
            8,
            json!({
                "item_id": message_id,
                "output_index": 1,
                "content_index": 0,
                "part": output_text("ok"),
            }),
        ),
        frame(
            "response.output_item.done",
            9,
            json!({"output_index": 1, "item": terminal_item.clone()}),
        ),
        frame(
            "response.completed",
            10,
            json!({
                "response": current_echo_response(
                    "completed",
                    vec![reasoning_done, terminal_item],
                    Some(terminal_usage),
                )
            }),
        ),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();

    assert_eq!(
        events,
        vec![
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 0,
                text: String::new(),
                signature: Some("sig-current".to_owned()),
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 1,
                delta: ContentDelta::Text("ok".to_owned()),
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::Stop,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(usage(5, 2, 3, 1)),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
}

#[test]
fn accepts_terminal_reasoning_signature_rotation() {
    let reasoning_id = "rs_rotated_signature";
    let reasoning_done = json!({
        "id": reasoning_id,
        "type": "reasoning",
        "summary": [],
        "encrypted_content": "done-signature",
    });
    let reasoning_terminal = json!({
        "id": reasoning_id,
        "type": "reasoning",
        "summary": [],
        "encrypted_content": "terminal-signature",
    });
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": reasoning_id,
                    "type": "reasoning",
                    "status": "in_progress",
                    "summary": [],
                    "encrypted_content": "initial-signature",
                }
            }),
        ),
        frame(
            "response.output_item.done",
            3,
            json!({"output_index": 0, "item": reasoning_done.clone()}),
        ),
        frame(
            "response.completed",
            4,
            json!({
                "response": response(
                    "completed",
                    vec![reasoning_terminal],
                    Some(usage_value(2, 1, 0, 1)),
                    None,
                    None,
                )
            }),
        ),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert!(events.contains(&CanonicalStreamEvent::ReasoningDelta {
        choice_index: 0,
        content_index: 0,
        text: String::new(),
        signature: Some("terminal-signature".to_owned()),
    }));
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn decodes_reasoning_signature_and_function_call() {
    let reasoning_id = "rs_stream_1";
    let function_id = "fc_stream_1";
    let arguments = r#"{"city":"Paris"}"#;
    let reasoning_done = json!({
        "id": reasoning_id,
        "type": "reasoning",
        "status": "completed",
        "summary": [{"type": "summary_text", "text": "brief"}],
        "encrypted_content": "sig-1",
    });
    let function_done = json!({
        "id": function_id,
        "type": "function_call",
        "status": "completed",
        "call_id": "call_1",
        "name": "weather",
        "arguments": arguments,
    });
    let frames = [
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": reasoning_id,
                    "type": "reasoning",
                    "status": "in_progress",
                    "summary": [],
                }
            }),
        ),
        frame(
            "response.reasoning_summary_part.added",
            3,
            json!({
                "item_id": reasoning_id,
                "output_index": 0,
                "summary_index": 0,
                "part": {"type": "summary_text", "text": ""},
            }),
        ),
        frame(
            "response.reasoning_summary_text.delta",
            4,
            json!({
                "item_id": reasoning_id,
                "output_index": 0,
                "summary_index": 0,
                "delta": "brief",
            }),
        ),
        frame(
            "response.reasoning_summary_text.done",
            5,
            json!({
                "item_id": reasoning_id,
                "output_index": 0,
                "summary_index": 0,
                "text": "brief",
            }),
        ),
        frame(
            "response.reasoning_summary_part.done",
            6,
            json!({
                "item_id": reasoning_id,
                "output_index": 0,
                "summary_index": 0,
                "part": {"type": "summary_text", "text": "brief"},
            }),
        ),
        frame(
            "response.output_item.done",
            7,
            json!({"output_index": 0, "item": reasoning_done.clone()}),
        ),
        frame(
            "response.output_item.added",
            8,
            json!({
                "output_index": 1,
                "item": {
                    "id": function_id,
                    "type": "function_call",
                    "status": "in_progress",
                    "call_id": "call_1",
                    "name": "weather",
                    "arguments": "",
                }
            }),
        ),
        frame(
            "response.function_call_arguments.delta",
            9,
            json!({
                "item_id": function_id,
                "output_index": 1,
                "delta": arguments,
            }),
        ),
        frame(
            "response.function_call_arguments.done",
            10,
            json!({
                "item_id": function_id,
                "output_index": 1,
                "arguments": arguments,
                "name": "weather",
            }),
        ),
        frame(
            "response.output_item.done",
            11,
            json!({"output_index": 1, "item": function_done.clone()}),
        ),
        frame(
            "response.completed",
            12,
            json!({
                "response": response(
                    "completed",
                    vec![reasoning_done, function_done],
                    None,
                    None,
                    None,
                )
            }),
        ),
    ];

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&join(frames)).unwrap();
    decoder.finish().unwrap();

    assert!(events.contains(&CanonicalStreamEvent::ReasoningDelta {
        choice_index: 0,
        content_index: 0,
        text: "brief".to_owned(),
        signature: None,
    }));
    assert!(events.contains(&CanonicalStreamEvent::ReasoningDelta {
        choice_index: 0,
        content_index: 0,
        text: String::new(),
        signature: Some("sig-1".to_owned()),
    }));
    assert!(events.contains(&CanonicalStreamEvent::ToolCallStart {
        choice_index: 0,
        tool_index: 0,
        id: "call_1".to_owned(),
        name: "weather".to_owned(),
    }));
    assert!(events.contains(&CanonicalStreamEvent::ToolCallArgsDelta {
        choice_index: 0,
        tool_index: 0,
        partial_json: arguments.to_owned(),
    }));
    assert!(events.contains(&CanonicalStreamEvent::ToolCallEnd {
        choice_index: 0,
        tool_index: 0,
    }));
    assert_eq!(
        events[events.len() - 2],
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::ToolCalls,
            stop_sequence: None,
        }
    );
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn accepts_optional_done_only_after_terminal_event() {
    let terminal = response("completed", Vec::new(), None, None, None);
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame("response.completed", 2, json!({"response": terminal})),
        b"data: [DONE]\n\n".to_vec(),
    ]);
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn ignores_rate_limit_metadata_and_missing_sequence_numbers() {
    let mut lifecycle = initial_response();
    lifecycle["frequency_penalty"] = json!(0);
    lifecycle["presence_penalty"] = json!(0);
    lifecycle["service_tier"] = json!("auto");
    let mut terminal = response("completed", Vec::new(), None, None, None);
    terminal["_sub2api_display_scaled"] = json!(true);
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": lifecycle.clone()}),
        ),
        b"event: codex.rate_limits\ndata: {\"rate_limits\":{\"primary\":{\"remaining\":1}}}\n\n"
            .to_vec(),
        frame_without_sequence("response.in_progress", json!({"response": lifecycle})),
        frame("response.completed", 8, json!({"response": terminal})),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert!(events.contains(&CanonicalStreamEvent::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    }));
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn maps_structured_error_without_preserving_message() {
    let bytes = frame(
        "error",
        0,
        json!({
            "code": "rate_limit_exceeded",
            "message": "secret upstream detail",
            "param": null,
        }),
    );
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
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
    assert!(!format!("{events:?}").contains("secret upstream detail"));
}

#[test]
fn accepts_error_terminal_reusing_previous_sequence() {
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "error",
            1,
            json!({
                "code": "server_error",
                "message": "secret upstream detail",
                "param": null,
            }),
        ),
    ]);
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();

    assert!(matches!(
        events.as_slice(),
        [
            CanonicalStreamEvent::MessageStart { .. },
            CanonicalStreamEvent::Error(UpstreamError::ServerError { .. }),
            CanonicalStreamEvent::StreamEnd,
        ]
    ));
    assert!(!format!("{events:?}").contains("secret upstream detail"));
}

#[test]
fn maps_only_deterministic_auth_and_account_death_as_permanent() {
    for (code, expected) in [
        ("invalid_api_key", UpstreamError::AuthRevoked),
        ("permission_error", UpstreamError::ProtocolError),
        ("deactivated_workspace", UpstreamError::AccountDisabled),
    ] {
        let bytes = frame(
            "error",
            0,
            json!({"code": code, "message": "private", "param": null}),
        );
        let mut decoder = OpenAiResponsesStreamDecoder::new();
        assert_eq!(
            decoder.push(&bytes).unwrap(),
            vec![
                CanonicalStreamEvent::Error(expected),
                CanonicalStreamEvent::StreamEnd,
            ]
        );
        decoder.finish().unwrap();
    }
}

#[test]
fn accepts_gapped_sequence_but_rejects_rewind_schema_and_terminal_mismatch() {
    let terminal = response("completed", Vec::new(), None, None, None);
    let mut gapped = OpenAiResponsesStreamDecoder::new();
    let gapped_bytes = join([
        frame(
            "response.created",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            3,
            json!({"response": initial_response()}),
        ),
        frame("response.completed", 9, json!({"response": terminal})),
    ]);
    gapped.push(&gapped_bytes).unwrap();
    gapped.finish().unwrap();

    let mut rewind = OpenAiResponsesStreamDecoder::new();
    rewind
        .push(&frame(
            "response.created",
            1,
            json!({"response": initial_response()}),
        ))
        .unwrap();
    assert_eq!(
        rewind.push(&frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        )),
        Err(ParseStreamError::InvalidSequence)
    );
    assert_eq!(rewind.push(b""), Err(ParseStreamError::DecoderFailed));

    let mut unsupported = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        unsupported.push(&frame(
            "response.refusal.delta",
            0,
            json!({"delta": "hidden"})
        )),
        Err(ParseStreamError::UnsupportedFeature)
    );

    let mut unknown_metadata = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        unknown_metadata.push(&frame_without_sequence(
            "codex.unknown",
            json!({"private": true}),
        )),
        Err(ParseStreamError::InvalidValue)
    );

    let duplicate = "event: response.created\ndata: {\"type\":\"response.created\",\"type\":\"response.created\",\"sequence_number\":0,\"response\":{}}\n\n";
    let mut duplicate_decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        duplicate_decoder.push(duplicate.as_bytes()),
        Err(ParseStreamError::DuplicateKey)
    );

    let item_id = "msg_stream_mismatch";
    let streamed_item = message_item(item_id, "completed", "left");
    let terminal_item = message_item(item_id, "completed", "right");
    let mismatch = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": item_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                }
            }),
        ),
        frame(
            "response.content_part.added",
            3,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text(""),
            }),
        ),
        frame(
            "response.output_text.delta",
            4,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": "left",
                "logprobs": [],
            }),
        ),
        frame(
            "response.output_text.done",
            5,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "text": "left",
                "logprobs": [],
            }),
        ),
        frame(
            "response.content_part.done",
            6,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text("left"),
            }),
        ),
        frame(
            "response.output_item.done",
            7,
            json!({"output_index": 0, "item": streamed_item}),
        ),
        frame(
            "response.completed",
            8,
            json!({
                "response": response(
                    "completed",
                    vec![terminal_item],
                    None,
                    None,
                    None,
                )
            }),
        ),
    ]);
    let mut mismatch_decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        mismatch_decoder.push(&mismatch),
        Err(ParseStreamError::InvalidSequence)
    );
}

#[test]
fn rejects_eof_before_terminal() {
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    decoder
        .push(&frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ))
        .unwrap();
    assert_eq!(decoder.finish(), Err(ParseStreamError::UnexpectedEof));
}

#[test]
fn accepts_shared_sequence_for_adjacent_text_delta_and_done() {
    let item_id = "msg_shared_sequence";
    let terminal_item = message_item(item_id, "completed", "hello");
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": item_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                }
            }),
        ),
        frame(
            "response.content_part.added",
            3,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text(""),
            }),
        ),
        frame(
            "response.output_text.delta",
            4,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": "hello",
                "logprobs": [],
            }),
        ),
        frame(
            "response.output_text.done",
            4,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "text": "hello",
                "logprobs": [],
            }),
        ),
        frame(
            "response.content_part.done",
            5,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text("hello"),
            }),
        ),
        frame(
            "response.output_item.done",
            6,
            json!({"output_index": 0, "item": terminal_item.clone()}),
        ),
        frame(
            "response.completed",
            7,
            json!({
                "response": response(
                    "completed",
                    vec![terminal_item],
                    None,
                    None,
                    None,
                )
            }),
        ),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert!(events.contains(&CanonicalStreamEvent::ContentDelta {
        choice_index: 0,
        content_index: 0,
        delta: ContentDelta::Text("hello".to_owned()),
    }));
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn fills_terminal_message_identity_omitted_by_compatible_provider() {
    let item_id = "msg_omitted_terminal_identity";
    let mut terminal_item = message_item(item_id, "completed", "hello");
    terminal_item["phase"] = json!("final_answer");
    let mut terminal_without_identity = terminal_item.clone();
    terminal_without_identity
        .as_object_mut()
        .expect("消息必须是对象")
        .remove("id");
    terminal_without_identity
        .as_object_mut()
        .expect("消息必须是对象")
        .remove("status");
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": item_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                    "phase": "final_answer",
                }
            }),
        ),
        frame(
            "response.content_part.added",
            3,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text(""),
            }),
        ),
        frame(
            "response.output_text.delta",
            4,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "delta": "hello",
                "logprobs": [],
            }),
        ),
        frame(
            "response.output_text.done",
            5,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "text": "hello",
                "logprobs": [],
            }),
        ),
        frame(
            "response.content_part.done",
            6,
            json!({
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text("hello"),
            }),
        ),
        frame(
            "response.output_item.done",
            7,
            json!({"output_index": 0, "item": terminal_item}),
        ),
        frame(
            "response.completed",
            8,
            json!({
                "response": response(
                    "completed",
                    vec![terminal_without_identity],
                    Some(usage_value(2, 1, 0, 0)),
                    None,
                    None,
                )
            }),
        ),
    ]);

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn lifecycle_identity_is_stable() {
    let mut changed = initial_response();
    changed["model"] = json!("other-model");
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame("response.in_progress", 1, json!({"response": changed})),
    ]);
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(decoder.push(&bytes), Err(ParseStreamError::InvalidSequence));

    let mut initial = initial_response();
    initial["service_tier"] = json!("default");
    let mut terminal = response("completed", Vec::new(), None, None, None);
    terminal["service_tier"] = json!("flex");
    let bytes = join([
        frame("response.created", 0, json!({"response": initial.clone()})),
        frame("response.in_progress", 1, json!({"response": initial})),
        frame("response.completed", 2, json!({"response": terminal})),
    ]);
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(decoder.push(&bytes), Err(ParseStreamError::InvalidSequence));
    assert_eq!(MODEL, "gpt-5.5");
    assert_eq!(CREATED_AT, 1_700_000_000);
}

#[test]
fn accepts_provider_builtin_tool_echo_on_terminal_frame() {
    let mut terminal = current_echo_response("completed", Vec::new(), None);
    terminal["tools"] = json!([{
        "type": "image_generation",
        "background": "auto",
        "model": "gpt-image-2-codex",
        "moderation": "auto",
        "n": 1,
        "output_compression": 100,
        "output_format": "png",
        "quality": "auto",
        "size": "auto"
    }]);
    let bytes = join([
        frame(
            "response.created",
            0,
            json!({"response": initial_response()}),
        ),
        frame(
            "response.in_progress",
            1,
            json!({"response": initial_response()}),
        ),
        frame("response.completed", 2, json!({"response": terminal})),
    ]);
    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

fn current_echo_response(status: &str, output: Vec<Value>, usage: Option<Value>) -> Value {
    let mut value = response(status, output, usage, None, None);
    let object = value.as_object_mut().expect("测试响应必须是对象");
    object.insert("frequency_penalty".to_owned(), json!(0));
    object.insert("presence_penalty".to_owned(), json!(0));
    object.insert(
        "tool_usage".to_owned(),
        json!({
            "image_gen": {
                "input_tokens": 0,
                "input_tokens_details": {"image_tokens": 0, "text_tokens": 0},
                "output_tokens": 0,
                "output_tokens_details": {"image_tokens": 0, "text_tokens": 0},
                "total_tokens": 0,
            },
            "web_search": {"num_requests": 0},
        }),
    );
    if status == "completed" {
        object.insert("completed_at".to_owned(), json!(CREATED_AT + 1));
    }
    value
}
