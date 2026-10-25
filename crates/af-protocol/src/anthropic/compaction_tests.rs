use serde_json::{Value, json};

use super::{
    AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder, ParseRequestError,
    ParseStreamError, billing_usage, build_request, build_response, parse_request, parse_response,
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

fn compaction_stream_with_usage(initial_usage: Value, final_usage: Value) -> Vec<u8> {
    [
        json!({"type":"message_start", "message":{"id":"msg_compaction", "type":"message", "role":"assistant", "content":[], "model":"claude-test", "stop_reason":null, "usage":initial_usage}}),
        json!({"type":"content_block_start", "index":0, "content_block":native_block(json!("summary"))}),
        json!({"type":"content_block_stop", "index":0}),
        json!({"type":"message_delta", "delta":{"stop_reason":"compaction", "stop_sequence":null}, "usage":final_usage}),
        json!({"type":"message_stop"}),
    ]
    .into_iter()
    .flat_map(frame)
    .collect()
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
            stream.extend_from_slice(b": keep-alive\n\n");
            let mut decoder = AnthropicMessagesStreamDecoder::new();
            let mut events = Vec::new();
            for chunk in stream.chunks(7) {
                events.extend(decoder.push(chunk).unwrap());
            }
            decoder.finish().unwrap();
            assert!(matches!(events.last(), Some(Event::StreamEnd)));
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

#[test]
fn terminal_sse_accepts_only_comment_heartbeats_without_appending_canonical_events() {
    let mut initial_usage = usage();
    initial_usage["output_tokens"] = json!(0);
    let mut final_usage = usage();
    final_usage["iterations"] = iterations();
    for terminal_stream in [
        compaction_stream_with_usage(initial_usage, final_usage),
        frame(json!({"type":"error", "error":{"type":"overloaded_error","message":"unavailable"}})),
    ] {
        let mut baseline = AnthropicMessagesStreamDecoder::new();
        let expected = baseline.push(&terminal_stream).unwrap();
        baseline.finish().unwrap();
        let mut wire = terminal_stream;
        // 包含空行、各种 SSE 换行及跨字节分片的 UTF-8 注释。
        wire.extend_from_slice("\n\r\n: keep-alive\n\n: 心跳\r\n\r\n\r\r".as_bytes());
        for chunk_size in [1, 7, wire.len()] {
            let mut decoder = AnthropicMessagesStreamDecoder::new();
            let mut events = Vec::new();
            for chunk in wire.chunks(chunk_size) {
                events.extend(decoder.push(chunk).unwrap());
            }
            decoder.finish().unwrap();
            assert_eq!(events, expected);
            assert_eq!(decoder.initial_usage(), baseline.initial_usage());
            assert_eq!(decoder.billing_usage(), baseline.billing_usage());
        }
    }

    let mut decoder = AnthropicMessagesStreamDecoder::new();
    assert_eq!(decoder.push(b": keep-alive\n\n").unwrap(), [Event::Ping]);
}

#[test]
fn terminal_sse_rejects_data_even_when_mixed_with_comment_heartbeats() {
    let mut initial_usage = usage();
    initial_usage["output_tokens"] = json!(0);
    let terminal_stream = compaction_stream_with_usage(initial_usage, usage());
    for trailing in [
        frame(json!({"type":"ping"})),
        frame(json!({"type":"message_stop"})),
        b"data:\n\n".to_vec(),
        b": keep-alive\nevent: ping\ndata: {\"type\":\"ping\"}\n\n".to_vec(),
        b": keep-alive\n\nevent: ping\ndata: {\"type\":\"ping\"}\n\n".to_vec(),
    ] {
        let wire = [terminal_stream.as_slice(), trailing.as_slice()].concat();
        for chunk_size in [1, 7, wire.len()] {
            let mut decoder = AnthropicMessagesStreamDecoder::new();
            let error = wire
                .chunks(chunk_size)
                .find_map(|chunk| decoder.push(chunk).err());
            assert_eq!(error, Some(ParseStreamError::DataAfterDone));
            assert_eq!(
                decoder.push(b": keep-alive\n\n"),
                Err(ParseStreamError::DecoderFailed)
            );
            assert_eq!(decoder.finish(), Err(ParseStreamError::DecoderFailed));
        }
    }
}

#[test]
fn compaction_sse_replays_initial_and_final_geo_with_other_usage_metadata() {
    for (initial_geo, final_geo, expected_geo) in [
        (Some("us"), None, "us"),
        (None, Some("eu"), "eu"),
        (Some("us"), Some("eu"), "eu"),
    ] {
        let mut initial_usage = usage();
        initial_usage["output_tokens"] = json!(0);
        initial_usage["service_tier"] = json!("standard");
        if let Some(geo) = initial_geo {
            initial_usage["inference_geo"] = json!(geo);
        }
        let mut final_usage = usage();
        final_usage["iterations"] = iterations();
        if let Some(geo) = final_geo {
            final_usage["inference_geo"] = json!(geo);
        }
        let stream = compaction_stream_with_usage(initial_usage, final_usage);
        let mut decoder = AnthropicMessagesStreamDecoder::new();
        let events = decoder.push(&stream).unwrap();
        decoder.finish().unwrap();
        let expected_metadata = json!({"inference_geo":expected_geo, "service_tier":"standard", "iterations":iterations()});
        assert!(events.iter().any(|event| matches!(event, Event::AnthropicUsageMetadata {raw} if raw == &expected_metadata)));
        assert_eq!(decoder.billing_usage().unwrap().input_tokens().get(), 112);
        assert_eq!(decoder.billing_usage().unwrap().output_tokens().get(), 7);

        let mut encoder = AnthropicMessagesStreamEncoder::new(
            "msg_replay",
            "claude-test",
            decoder.initial_usage().unwrap(),
        )
        .unwrap();
        let mut encoded = Vec::new();
        for event in events {
            encoded.extend(encoder.encode(event).unwrap());
        }
        let mut replay = AnthropicMessagesStreamDecoder::new();
        let mut replay_events = Vec::new();
        for chunk in encoded.chunks(11) {
            replay_events.extend(replay.push(chunk).unwrap());
        }
        replay.finish().unwrap();
        assert!(replay_events.iter().any(|event| matches!(event, Event::AnthropicUsageMetadata {raw} if raw == &expected_metadata)));
        assert!(replay_events.iter().any(|event| matches!(event, Event::Usage(usage) if usage.input_tokens().get() == 100 && usage.output_tokens().get() == 4)));
        assert_eq!(replay.billing_usage(), decoder.billing_usage());
    }
}

#[test]
fn compaction_sse_validates_initial_and_final_geo_with_the_same_byte_limit() {
    for final_geo in [false, true] {
        for (geo, accepted) in [
            ("us".to_owned(), true),
            ("x".repeat(64), true),
            (format!("{}a", "中".repeat(21)), true),
            (String::new(), false),
            (" us".to_owned(), false),
            ("us ".to_owned(), false),
            ("u\ns".to_owned(), false),
            ("x".repeat(65), false),
            ("中".repeat(22), false),
        ] {
            let mut initial_usage = usage();
            initial_usage["output_tokens"] = json!(0);
            let mut final_usage = usage();
            if final_geo {
                final_usage["inference_geo"] = json!(geo);
            } else {
                initial_usage["inference_geo"] = json!(geo);
            }
            let mut decoder = AnthropicMessagesStreamDecoder::new();
            let result = decoder.push(&compaction_stream_with_usage(initial_usage, final_usage));
            if accepted {
                let events = result.unwrap();
                decoder.finish().unwrap();
                assert!(events.iter().any(|event| matches!(event, Event::AnthropicUsageMetadata {raw} if raw["inference_geo"] == geo)));
            } else {
                assert!(matches!(result, Err(ParseStreamError::InvalidValue)));
            }
        }
    }
}
