use super::*;

fn fixtures(protocol: Protocol, mixed: bool, split: bool) -> Value {
    let (key, block_key, assistant, call_a, call_b, result_a, result_b, text) = if protocol
        == Protocol::Anthropic
    {
        (
            "messages",
            "content",
            "assistant",
            json!({"type":"tool_use","id":"call_a","name":"lookup","input":{"q":"a"}}),
            json!({"type":"tool_use","id":"call_b","name":"lookup","input":{"q":"b"}}),
            json!({"type":"tool_result","tool_use_id":"call_a","content":"result-a","x_block":{"which":"a"}}),
            json!({"type":"tool_result","tool_use_id":"call_b","content":"result-b","x_block":{"which":"b"}}),
            json!({"type":"text","text":"keep-followup"}),
        )
    } else {
        (
            "contents",
            "parts",
            "model",
            json!({"functionCall":{"id":"call_a","name":"lookup","args":{"q":"a"}}}),
            json!({"functionCall":{"id":"call_b","name":"lookup","args":{"q":"b"}}}),
            json!({"functionResponse":{"id":"call_a","name":"lookup","response":{"result":"result-a"}},"x_block":{"which":"a"}}),
            json!({"functionResponse":{"id":"call_b","name":"lookup","response":{"result":"result-b"}},"x_block":{"which":"b"}}),
            json!({"text":"keep-followup"}),
        )
    };
    let mut messages = vec![json!({"role":assistant, block_key:[call_a,call_b]})];
    if split {
        messages.push(
            json!({"role":"user","x_vendor":{"source":"b"},"cacheTag":null,block_key:[result_b]}),
        );
        messages.push(
            json!({"role":"user","x_vendor":{"source":"a"},"cacheTag":null,block_key:[result_a]}),
        );
    } else {
        let mut parts = vec![result_b, result_a];
        if mixed {
            parts.push(text.clone());
        }
        messages.push(
            json!({"role":"user","x_vendor":{"source":"both"},"cacheTag":null,block_key:parts}),
        );
    }
    messages.push(json!({"role":"user",block_key:[text]}));
    json!({key:messages})
}

#[test]
fn result_reorder_preserves_original_message_wrappers_and_block_extensions() {
    for protocol in [Protocol::Anthropic, Protocol::Gemini] {
        for projection in [false, true] {
            for split in [false, true] {
                for mixed in [false, true] {
                    let mut body = fixtures(protocol, mixed, split);
                    normalize_tool_history(protocol, &mut body, projection).unwrap();
                    let key = if protocol == Protocol::Gemini {
                        "contents"
                    } else {
                        "messages"
                    };
                    let block_key = result_content_key(protocol).unwrap();
                    let results: Vec<_> = body[key]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|message| !result_blocks(protocol, message).is_empty())
                        .collect();
                    assert_eq!(
                        results.len(),
                        if split { 2 } else { 1 },
                        "{protocol:?},split={split},mixed={mixed},projection={projection}"
                    );
                    let mut ordered = Vec::new();
                    for message in results {
                        assert!(message.as_object().unwrap().contains_key("cacheTag"));
                        assert!(message["cacheTag"].is_null());
                        for result in result_blocks(protocol, message) {
                            let id = if protocol == Protocol::Gemini {
                                &result["functionResponse"]["id"]
                            } else {
                                &result["tool_use_id"]
                            };
                            let which = if id == "call_a" { "a" } else { "b" };
                            assert_eq!(result["x_block"]["which"], which);
                            assert_eq!(
                                message["x_vendor"]["source"],
                                if split { which } else { "both" }
                            );
                            ordered.push(id.as_str().unwrap().to_owned());
                        }
                    }
                    assert_eq!(ordered, vec!["call_a", "call_b"]);
                    if mixed && !split {
                        assert!(
                            body[key].as_array().unwrap().iter().any(|message| {
                                message["x_vendor"]["source"] == "both"
                                    && message[block_key].as_array().is_some_and(|parts| {
                                        parts.iter().any(|part| {
                                            part.get("text").and_then(Value::as_str)
                                                == Some("keep-followup")
                                        })
                                    })
                            }),
                            "mixed result wrapper lost its remaining content"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn orphan_result_repair_keeps_wrapper_extensions() {
    for protocol in [Protocol::Anthropic, Protocol::Gemini] {
        for projection in [false, true] {
            let mut body = fixtures(protocol, false, false);
            let key = if protocol == Protocol::Gemini {
                "contents"
            } else {
                "messages"
            };
            body[key].as_array_mut().unwrap().remove(0);
            normalize_tool_history(protocol, &mut body, projection).unwrap();
            let wrappers: Vec<_> = body[key]
                .as_array()
                .unwrap()
                .iter()
                .filter(|message| message["x_vendor"]["source"] == "both")
                .collect();
            assert_eq!(
                wrappers.len(),
                2,
                "orphan result wrapper extensions must follow each repaired result"
            );
            assert!(
                wrappers
                    .iter()
                    .all(|message| message.as_object().unwrap().contains_key("cacheTag"))
            );
        }
    }
}

#[test]
fn native_request_envelopes_keep_result_message_extensions_after_repair() {
    for protocol in [Protocol::Anthropic, Protocol::Gemini] {
        let mut wire = fixtures(protocol, false, true);
        if protocol == Protocol::Anthropic {
            wire["model"] = json!("history-test");
            wire["max_tokens"] = json!(4096);
        }
        let body = bytes::Bytes::from(serde_json::to_vec(&wire).unwrap());
        let request = if protocol == Protocol::Anthropic {
            crate::anthropic::parse_request_envelope(body).unwrap()
        } else {
            crate::gemini::parse_request_envelope("models/history-test", body).unwrap()
        };
        let crate::SameProtocolDecision::Passthrough(native) = request.into_same_protocol(protocol)
        else {
            panic!("native tool history must retain its source envelope");
        };
        let value: Value = serde_json::from_slice(native.body()).unwrap();
        let key = if protocol == Protocol::Anthropic {
            "messages"
        } else {
            "contents"
        };
        let results: Vec<_> = value[key]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| !result_blocks(protocol, message).is_empty())
            .collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["x_vendor"]["source"], "a");
        assert_eq!(results[1]["x_vendor"]["source"], "b");
        assert!(
            results
                .iter()
                .all(|message| message.as_object().unwrap().contains_key("cacheTag"))
        );
    }
}
