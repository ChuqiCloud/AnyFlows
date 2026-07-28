use af_domain::{Role, UpstreamError};
use serde_json::json;

use super::{
    CREATED_AT, MODEL, RESPONSE_ID, frame, initial_response, join, message_item, output_text,
    response, usage, usage_value,
};
use crate::openai_responses::{OpenAiResponsesStreamDecoder, ParseStreamError};
use crate::{CanonicalStreamEvent, ContentDelta, FinishReason};

#[test]
fn decodes_text_stream_across_transport_chunks() {
    let item_id = "msg_stream_1";
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
        frame(
            "response.completed",
            9,
            json!({
                "response": response(
                    "completed",
                    vec![terminal_item],
                    Some(usage_value(8, 3, 2, 1)),
                    None,
                    None,
                )
            }),
        ),
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
            CanonicalStreamEvent::Error(UpstreamError::RateLimited),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
    assert!(!format!("{events:?}").contains("secret upstream detail"));
}

#[test]
fn fails_closed_on_sequence_schema_and_terminal_mismatch() {
    let mut gap = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        gap.push(&frame(
            "response.created",
            1,
            json!({"response": initial_response()})
        )),
        Err(ParseStreamError::InvalidSequence)
    );
    assert_eq!(gap.push(b""), Err(ParseStreamError::DecoderFailed));

    let mut unsupported = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        unsupported.push(&frame(
            "response.refusal.delta",
            0,
            json!({"delta": "hidden"})
        )),
        Err(ParseStreamError::UnsupportedFeature)
    );

    let duplicate = format!(
        "event: response.created\ndata: {{\"type\":\"response.created\",\"type\":\"response.created\",\"sequence_number\":0,\"response\":{{}}}}\n\n"
    );
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
    assert_eq!(MODEL, "gpt-5.5");
    assert_eq!(CREATED_AT, 1_700_000_000);
}
