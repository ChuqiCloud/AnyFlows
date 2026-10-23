use af_domain::{Operation, Protocol, Role};
use bytes::Bytes;
use serde_json::{Value, json};

use super::{
    OpenAiResponsesStreamEncoder, ParseRequestError, build_response, parse_request,
    parse_request_envelope,
};
use crate::{
    CanonicalResponse, CanonicalStreamEvent, ContentBlock, ContentDelta, FinishReason, Message,
    ResponseChoice, SameProtocolDecision, TokenCount, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};

fn assert_replay(input: Vec<Value>, expected: &[ContentBlock]) {
    let mut items = vec![json!({
        "role": "user", "content": [{"type": "input_text", "text": "Check status"}]
    })];
    items.extend(input);
    items.push(json!({"role": "user", "content": "Continue"}));
    let value = json!({
        "model": "gpt-test",
        "input": items,
        "max_output_tokens": 128,
        "tools": [{
            "type": "function",
            "name": "lookup",
            "parameters": {"type": "object", "properties": {"query": {"type": "string"}}},
            "strict": false
        }]
    });
    let envelope = parse_request_envelope(Bytes::from(serde_json::to_vec(&value).unwrap()))
        .expect("complete standard output items must be replayable");
    let replayed: Vec<_> = envelope
        .canonical()
        .messages
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .flat_map(|message| message.content.iter().cloned())
        .collect();
    assert_eq!(replayed, expected);
    let SameProtocolDecision::Passthrough(native) = envelope
        .clone()
        .into_same_protocol(Protocol::OpenAiResponses)
    else {
        panic!("validated native history must keep the complete source body");
    };
    assert_eq!(
        serde_json::from_slice::<Value>(native.body()).unwrap(),
        value
    );
    for protocol in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
        let converted = envelope
            .clone()
            .for_conversation_protocol(protocol)
            .unwrap();
        let built = match protocol {
            Protocol::OpenAiChat => {
                crate::openai_chat::build_request(converted.canonical()).is_ok()
            }
            Protocol::Anthropic => crate::anthropic::build_request(converted.canonical()).is_ok(),
            Protocol::Gemini => crate::gemini::build_request(converted.canonical()).is_ok(),
            _ => unreachable!(),
        };
        assert!(built, "history must build for {protocol:?}");
    }
}

#[test]
fn replays_pi_captured_assistant_message_identity_lifecycle_and_phase() {
    // Sanitized item from the second request in the 2026-10-23 Pi live capture.
    assert_replay(
        vec![json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "ACK_731", "annotations": []}],
            "status": "completed",
            "id": "msg_0f212d24-1ccb-4142-bc79-93bf4f3c766c_0",
            "phase": "final_answer"
        })],
        &[ContentBlock::Text("ACK_731".to_owned())],
    );
}

#[test]
fn known_message_and_function_item_states_replay_supplied_content() {
    for status in ["in_progress", "completed", "incomplete"] {
        for phase in [Value::Null, json!("commentary"), json!("final_answer")] {
            assert_replay(
                vec![json!({
                    "type": "message", "id": "msg_history", "role": "assistant",
                    "status": status, "phase": phase,
                    "content": [{"type": "output_text", "text": "Partial answer", "annotations": [], "logprobs": []}]
                })],
                &[ContentBlock::Text("Partial answer".to_owned())],
            );
        }
        assert_replay(
            vec![
                json!({
                    "type": "function_call", "id": "fc_history", "status": status,
                    "call_id": "call_history", "name": "lookup", "arguments": "{\"query\":\"status\"}"
                }),
                json!({
                    "type": "function_call_output", "id": "fco_history", "status": status,
                    "call_id": "call_history", "output": "ready"
                }),
            ],
            &[ContentBlock::ToolUse {
                id: "call_history".to_owned(),
                name: "lookup".to_owned(),
                input: json!({"query": "status"}),
                signature: None,
            }],
        );
    }
}

#[test]
fn replay_metadata_still_rejects_unknown_values_and_fields() {
    let message = json!({
        "type": "message", "id": "msg_history", "role": "assistant",
        "status": "completed", "phase": "final_answer",
        "content": [{"type": "output_text", "text": "answer", "annotations": []}]
    });
    for (key, value) in [
        ("id", json!("")),
        ("id", json!("bad\nid")),
        ("status", json!("future_state")),
        ("phase", json!("future_phase")),
        ("future_metadata", json!(true)),
        ("role", json!("user")),
    ] {
        let mut item = message.clone();
        item[key] = value;
        assert_eq!(
            parse_request(
                &serde_json::to_vec(&json!({"model": "gpt-test", "input": [item]})).unwrap()
            ),
            Err(ParseRequestError::InvalidValue),
            "invalid {key} must not bypass input validation"
        );
    }
}

fn response_usage() -> Usage {
    Usage::new(
        TokenCount::new(8).unwrap(),
        TokenCount::new(3).unwrap(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

#[test]
fn generated_json_and_sse_output_items_replay_without_stripping_metadata() {
    for tool in [false, true] {
        let mut blocks = vec![ContentBlock::Text("The status is ready.".to_owned())];
        let mut events = vec![
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("The status is ready.".to_owned()),
            },
        ];
        if tool {
            blocks.push(ContentBlock::ToolUse {
                id: "call_history".to_owned(),
                name: "lookup".to_owned(),
                input: json!({"query": "status"}),
                signature: None,
            });
            events.extend([
                CanonicalStreamEvent::ToolCallStart {
                    choice_index: 0,
                    tool_index: 0,
                    id: "call_history".to_owned(),
                    name: "lookup".to_owned(),
                },
                CanonicalStreamEvent::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index: 0,
                    partial_json: "{\"query\":\"status\"}".to_owned(),
                },
                CanonicalStreamEvent::ToolCallEnd {
                    choice_index: 0,
                    tool_index: 0,
                },
            ]);
        }
        let reason = if tool {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        };
        let response = CanonicalResponse::new(
            Operation::Responses,
            "resp_replay".to_owned(),
            "gpt-test".to_owned(),
            Some(1_700_000_000),
            vec![ResponseChoice::new(
                0,
                Message::new(Role::Assistant, blocks.clone()),
                reason,
            )],
            Some(response_usage()),
        );
        let json_output = build_response(&response).unwrap()["output"]
            .as_array()
            .unwrap()
            .clone();
        let mut encoder =
            OpenAiResponsesStreamEncoder::new("resp_replay", "gpt-test", 1_700_000_000).unwrap();
        events.extend([
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(response_usage()),
            CanonicalStreamEvent::StreamEnd,
        ]);
        let mut sse = Vec::new();
        for event in events {
            sse.extend(encoder.encode(event).unwrap());
        }
        let frames: Vec<Value> = std::str::from_utf8(&sse)
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|data| serde_json::from_str(data).unwrap())
            .collect();
        let done_output = frames
            .iter()
            .filter(|event| event["type"] == "response.output_item.done")
            .map(|event| event["item"].clone())
            .collect::<Vec<_>>();
        let terminal_output = frames
            .iter()
            .find(|event| event["type"] == "response.completed")
            .unwrap()["response"]["output"]
            .as_array()
            .unwrap()
            .clone();
        for mut output in [json_output, done_output, terminal_output] {
            assert_eq!(output[0]["content"][0]["annotations"], json!([]));
            assert_eq!(output[0]["content"][0]["logprobs"], json!([]));
            if tool {
                output.push(json!({"type": "function_call_output", "call_id": "call_history", "output": "ready"}));
            }
            assert_replay(output, &blocks);
        }
    }
}
