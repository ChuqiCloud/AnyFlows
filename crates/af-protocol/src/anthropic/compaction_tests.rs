use serde_json::{Value, json};

use super::{
    AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder, ParseRequestError,
    billing_usage, build_request, build_response, parse_request, parse_response,
};
use crate::{CanonicalStreamEvent as Event, ContentBlock, FinishReason};

fn native_block(content: Value) -> Value {
    json!({"type":"compaction", "content":content, "signature":"signed-summary", "encrypted_content":"encrypted-state", "tool_changes":[{"type":"tool_removal", "name":"old_tool"}], "future_extension":{"kept":true}})
}

fn usage() -> Value {
    json!({"input_tokens":100, "output_tokens":4, "cache_creation_input_tokens":0, "cache_read_input_tokens":0})
}

fn iterations() -> Value {
    json!([
        {"type":"message", "input_tokens":100, "output_tokens":4, "cache_creation_input_tokens":0, "cache_read_input_tokens":0},
        {"type":"compaction", "input_tokens":12, "output_tokens":3, "cache_creation_input_tokens":5, "cache_read_input_tokens":2, "cache_creation":{"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":3}, "future_usage":{"kept":true}}
    ])
}

fn response(block: Value, stop_reason: &str) -> Value {
    json!({"id":"msg_compaction", "type":"message", "role":"assistant", "model":"claude-test", "content":[block], "stop_reason":stop_reason, "usage":usage()})
}

fn frame(value: Value) -> Vec<u8> {
    format!(
        "event: {}\ndata: {value}\n\n",
        value["type"].as_str().unwrap()
    )
    .into_bytes()
}

#[test]
fn native_compaction_controls_and_signed_history_round_trip() {
    let block = native_block(json!("summary 已保留"));
    for controls in [
        json!({"compaction":{"type":"summarize", "instructions":"keep constraints", "future_option":true}}),
        json!({"context_management":{"edits":[{"type":"compact_20260112", "pause_after_compaction":true, "trigger":{"type":"input_tokens","value":20000}, "instructions":null}], "future_option":true}}),
    ] {
        let mut wire = json!({"model":"claude-test", "max_tokens":128, "messages":[{"role":"assistant", "content":[block.clone()]}, {"role":"user", "content":"continue"}]});
        wire.as_object_mut()
            .unwrap()
            .extend(controls.as_object().unwrap().clone());
        let parsed = parse_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert!(
            matches!(&parsed.messages[0].content[0], ContentBlock::AnthropicCompaction(item) if item.as_value() == &block)
        );
        let built = build_request(&parsed).unwrap();
        assert_eq!(built["messages"][0]["content"][0], block);
        for key in ["compaction", "context_management"] {
            assert_eq!(built.get(key), wire.get(key));
        }
    }
    let invalid = json!({"model":"claude-test", "max_tokens":16, "messages":[{"role":"user","content":"hello"}], "compaction":{"type":"summarize"}, "context_management":{"edits":[]}});
    assert_eq!(
        parse_request(&serde_json::to_vec(&invalid).unwrap()).unwrap_err(),
        ParseRequestError::InvalidValue
    );
}

#[test]
fn json_preserves_failed_compaction_and_bills_only_additional_iterations() {
    for content in [json!("summary"), json!(""), Value::Null] {
        let block = native_block(content);
        let mut wire = response(block.clone(), "compaction");
        wire["usage"]["iterations"] = iterations();
        let parsed = parse_response(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(parsed.choices[0].finish_reason, FinishReason::Compaction);
        assert_eq!(parsed.usage.unwrap().input_tokens().get(), 100);
        let billed = billing_usage(&parsed).unwrap().unwrap();
        assert_eq!(billed.input_tokens().get(), 112);
        assert_eq!(billed.output_tokens().get(), 7);
        assert_eq!(billed.details().cache_read().get(), 2);
        assert_eq!(billed.details().cache_creation_1h().get(), 3);
        let built = build_response(&parsed).unwrap();
        assert_eq!(built["content"][0], block);
        assert_eq!(built["usage"]["input_tokens"], 100);
        assert_eq!(built["usage"]["output_tokens"], 4);
        assert_eq!(built["usage"]["iterations"], iterations());
        assert_eq!(built["stop_reason"], "compaction");
    }
    let mut no_block = response(native_block(Value::Null), "end_turn");
    no_block["content"] = json!([]);
    no_block["usage"]["iterations"] = iterations();
    let parsed = parse_response(&serde_json::to_vec(&no_block).unwrap()).unwrap();
    assert_eq!(
        billing_usage(&parsed)
            .unwrap()
            .unwrap()
            .output_tokens()
            .get(),
        7
    );
}

#[test]
fn both_compaction_sse_forms_preserve_native_state_and_usage() {
    for streamed in [false, true] {
        for null_content in [false, true] {
            let expected_content = if null_content {
                Value::Null
            } else {
                json!("summary")
            };
            let block = native_block(expected_content.clone());
            let mut final_usage = usage();
            final_usage["iterations"] = iterations();
            // Compaction may replace the input snapshot with the reduced context.
            final_usage["input_tokens"] = json!(80);
            let mut stream = Vec::new();
            stream.extend(frame(json!({"type":"message_start", "message":{"id":"msg_compaction", "type":"message", "role":"assistant", "content":[], "model":"claude-test", "stop_reason":null, "usage":{"input_tokens":100,"output_tokens":0}}})));
            let mut start_block = block.clone();
            if streamed && !null_content {
                start_block.as_object_mut().unwrap().remove("content");
                start_block["encrypted_content"] = json!("");
                start_block["signature"] = json!("");
            }
            stream.extend(frame(
                json!({"type":"content_block_start","index":0,"content_block":start_block}),
            ));
            if streamed {
                let delta = if null_content {
                    json!({"type":"compaction_delta", "content":null})
                } else {
                    json!({"type":"compaction_delta", "content":"sum", "encrypted_content":"encrypted-"})
                };
                stream.extend(frame(
                    json!({"type":"content_block_delta","index":0,"delta":delta}),
                ));
                if !null_content {
                    for delta in [
                        json!({"type":"compaction_delta", "content":"mary", "encrypted_content":"state"}),
                        json!({"type":"signature_delta", "signature":"signed-"}),
                        json!({"type":"signature_delta", "signature":"summary"}),
                    ] {
                        stream.extend(frame(
                            json!({"type":"content_block_delta","index":0,"delta":delta}),
                        ));
                    }
                }
            }
            stream.extend(frame(json!({"type":"content_block_stop","index":0})));
            stream.extend(frame(json!({"type":"message_delta", "delta":{"stop_reason":"compaction","stop_sequence":null},"usage":final_usage})));
            stream.extend(frame(json!({"type":"message_stop"})));
            let mut decoder = AnthropicMessagesStreamDecoder::new();
            let mut events = Vec::new();
            for chunk in stream.chunks(7) {
                events.extend(decoder.push(chunk).unwrap());
            }
            decoder.finish().unwrap();
            assert_eq!(decoder.billing_usage().unwrap().input_tokens().get(), 92);
            assert_eq!(decoder.billing_usage().unwrap().output_tokens().get(), 7);
            assert!(events.iter().any(|event| matches!(event, Event::AnthropicCompactionEnd{item,..} if item.as_value() == &block)));
            let mut encoder = AnthropicMessagesStreamEncoder::new(
                "msg_public",
                "public-model",
                decoder.initial_usage().unwrap(),
            )
            .unwrap();
            let mut encoded = Vec::new();
            for event in events {
                encoded.extend(encoder.encode(event).unwrap());
            }
            let mut replay = AnthropicMessagesStreamDecoder::new();
            let events = replay.push(&encoded).unwrap();
            replay.finish().unwrap();
            assert!(events.iter().any(|event| matches!(event, Event::AnthropicCompactionEnd{item,..} if item.as_value() == &block)));
            assert!(events.iter().any(|event| matches!(event, Event::AnthropicUsageMetadata{raw} if raw["iterations"] == iterations())));
            assert_eq!(replay.billing_usage(), decoder.billing_usage());
        }
    }
}
