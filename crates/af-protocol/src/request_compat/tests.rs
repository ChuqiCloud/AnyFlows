use super::*;
use crate::{
    CanonicalRequestEnvelope, SameProtocolDecision, anthropic, gemini, openai_chat,
    openai_responses,
};
use bytes::Bytes;

fn parse(protocol: Protocol, value: Value) -> CanonicalRequestEnvelope {
    let body = Bytes::from(serde_json::to_vec(&value).unwrap());
    match protocol {
        Protocol::OpenAiChat => openai_chat::parse_request_envelope(body).unwrap(),
        Protocol::OpenAiResponses => openai_responses::parse_request_envelope(body).unwrap(),
        Protocol::Anthropic => anthropic::parse_request_envelope(body).unwrap(),
        Protocol::Gemini => {
            gemini::parse_stream_request_envelope("models/test-model", body).unwrap()
        }
        _ => unreachable!(),
    }
}

fn source(envelope: CanonicalRequestEnvelope, protocol: Protocol) -> Value {
    let SameProtocolDecision::Passthrough(source) = envelope.into_same_protocol(protocol) else {
        panic!("native source must be preserved")
    };
    serde_json::from_slice(source.body()).unwrap()
}

#[test]
fn scalar_null_role_content_and_extensions_normalize_at_all_ingresses() {
    for (protocol, value) in [
        (
            Protocol::OpenAiChat,
            json!({"model":"test-model","stream":"true","max_tokens":"256","temperature":null,"vendor_metadata":{"key":7},"messages":[{"role":"human","content":{"text":"hello"},"client_id":"one"}]}),
        ),
        (
            Protocol::OpenAiResponses,
            json!({"model":"test-model","stream":"true","max_output_tokens":"256","temperature":null,"vendor_metadata":{"key":7},"input":[{"role":"human","content":{"text":"hello"},"client_id":"one"}]}),
        ),
        (
            Protocol::Anthropic,
            json!({"model":"test-model","stream":"true","max_tokens":"256","temperature":null,"vendor_metadata":{"key":7},"messages":[{"role":"human","content":{"text":"hello"},"client_id":"one"}]}),
        ),
        (
            Protocol::Gemini,
            json!({"vendor_metadata":{"key":7},"generation_config":{"max_output_tokens":"256","thinking_config":null},"contents":[{"role":"human","parts":"hello","client_id":"one"}]}),
        ),
    ] {
        let envelope = parse(protocol, value);
        assert!(envelope.canonical().stream);
        assert_eq!(
            envelope
                .canonical()
                .sampling
                .max_output_tokens()
                .unwrap()
                .get(),
            256
        );
        let body = source(envelope, protocol);
        assert_eq!(body["vendor_metadata"], json!({"key":7}));
        assert!(body.get("temperature").is_none());
    }
}

#[test]
fn empty_stop_defaults_are_omitted_at_compatible_ingresses() {
    for protocol in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
        for empty in [json!([]), json!("")] {
            let (mut value, key) = match protocol {
                Protocol::OpenAiChat => (
                    json!({"model":"test-model","messages":[{"role":"user","content":"hello"}]}),
                    "stop",
                ),
                Protocol::Anthropic => (
                    json!({"model":"test-model","max_tokens":256,"messages":[{"role":"user","content":"hello"}]}),
                    "stop_sequences",
                ),
                Protocol::Gemini => (
                    json!({"contents":[{"role":"user","parts":[{"text":"hello"}]}],"generationConfig":{}}),
                    "stopSequences",
                ),
                _ => unreachable!(),
            };
            if protocol == Protocol::Gemini {
                value["generationConfig"][key] = empty;
            } else {
                value[key] = empty;
            }
            let envelope = parse(protocol, value);
            for target in [
                Protocol::OpenAiChat,
                Protocol::OpenAiResponses,
                Protocol::Anthropic,
                Protocol::Gemini,
            ] {
                assert!(
                    envelope.clone().plan_conversation(target).is_some(),
                    "{protocol:?}->{target:?}"
                );
            }
            let body = source(envelope, protocol);
            let config = if protocol == Protocol::Gemini {
                &body["generationConfig"]
            } else {
                &body
            };
            assert!(config.get(key).is_none());
        }
    }
}

#[test]
fn legacy_functions_and_object_results_become_paired_standard_history() {
    let value = json!({"model":"test-model","functions":[{"name":"lookup"}],"function_call":"auto","messages":[
        {"role":"user","content":"check"},
        {"role":"assistant","content":null,"function_call":{"name":"lookup","arguments":{"query":"status"}}},
        {"role":"function","name":"lookup","content":{"ready":true}},
        {"role":"user","content":"continue"}
    ]});
    let envelope = parse(Protocol::OpenAiChat, value.clone());
    assert!(openai_chat::parse_request(&serde_json::to_vec(&value).unwrap()).is_err());
    let body = source(envelope, Protocol::OpenAiChat);
    assert!(body.get("functions").is_none());
    assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    assert_eq!(
        body["messages"][1]["tool_calls"][0]["id"],
        body["messages"][2]["tool_call_id"]
    );
    assert_eq!(body["messages"][2]["content"], "{\"ready\":true}");
}

#[test]
fn tool_results_are_reordered_by_real_call_ids_without_fabrication() {
    let value = json!({"model":"test-model","tools":[{"type":"function","name":"one"},{"type":"function","name":"two"}],"input":[
        {"role":"user","content":"check"},
        {"type":"function_call","call_id":"a","name":"one","arguments":{}},
        {"type":"function_call","call_id":"b","name":"two","arguments":{}},
        {"type":"function_call_output","call_id":"b","output":{"two":2}},
        {"type":"function_call_output","call_id":"a","output":{"one":1}},
        {"role":"user","content":"continue"}
    ]});
    let body = source(
        parse(Protocol::OpenAiResponses, value),
        Protocol::OpenAiResponses,
    );
    assert_eq!(body["input"][1]["call_id"], "a");
    assert_eq!(body["input"][2]["call_id"], "b");
    assert_eq!(body["input"][3]["call_id"], "a");
    assert_eq!(body["input"][4]["call_id"], "b");
    assert_eq!(body["input"].as_array().unwrap().len(), 6);
}

#[test]
fn unknown_media_keeps_native_payload_and_blocks_cross_protocol_loss() {
    for (protocol, value) in [
        (
            Protocol::OpenAiChat,
            json!({"model":"test-model","messages":[{"role":"user","content":[{"type":"future_video","url":"https://media.example/video"}]}]}),
        ),
        (
            Protocol::OpenAiResponses,
            json!({"model":"test-model","input":[{"role":"user","content":[{"type":"input_file","file_id":"file_owned"}]}]}),
        ),
        (
            Protocol::Anthropic,
            json!({"model":"test-model","max_tokens":256,"messages":[{"role":"user","content":[{"type":"document","source":{"type":"file","file_id":"file_owned"}}]}]}),
        ),
        (
            Protocol::Gemini,
            json!({"contents":[{"role":"user","parts":[{"fileData":{"mimeType":"video/mp4","fileUri":"gs://fixture/video"}}]}]}),
        ),
    ] {
        let envelope = parse(protocol, value.clone());
        let target = if protocol == Protocol::Gemini {
            Protocol::OpenAiChat
        } else {
            Protocol::Gemini
        };
        assert!(envelope.clone().for_conversation_protocol(target).is_none());
        assert_eq!(source(envelope, protocol), value);
    }
}

#[test]
fn schemas_arguments_and_metadata_values_are_not_recursively_rewritten() {
    let original = json!({"model":"test-model","tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{"stream":{"type":"string","default":"false"}}}}}],"messages":[{"role":"user","content":"hello"}],"vendor":{"max_tokens":"seven","stream":"false"}});
    let body = source(
        parse(Protocol::OpenAiChat, original.clone()),
        Protocol::OpenAiChat,
    );
    assert_eq!(body, original);
}

#[test]
fn duplicate_json_keys_stay_rejected_and_standard_aliases_win() {
    assert!(
        openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"test-model","model":"other","messages":[{"role":"user","content":"x"}]}"#
        ))
        .is_err()
    );
    let compatible = normalize(
        Protocol::Gemini,
        json!({
            "contents":[],
            "generation_config":{"maxOutputTokens":1},
            "generationConfig":{"maxOutputTokens":2}
        }),
    )
    .unwrap();
    assert_eq!(compatible.body["generationConfig"]["maxOutputTokens"], 2);
}

#[test]
fn redundant_legacy_functions_merge_without_overwriting_modern_definitions() {
    let body = source(
        parse(
            Protocol::OpenAiChat,
            json!({
                "model":"test-model",
                "tools":[{"type":"function","function":{"name":"lookup","description":"modern","parameters":{"type":"object"}}}],
                "functions":[{"name":"lookup","description":"legacy"},{"name":"extra"}],
                "tool_choice":"auto",
                "function_call":{"name":"lookup"},
                "messages":[{"role":"user","content":"hello"}]
            }),
        ),
        Protocol::OpenAiChat,
    );
    assert_eq!(body["tools"].as_array().unwrap().len(), 2);
    assert_eq!(body["tools"][0]["function"]["description"], "modern");
    assert_eq!(body["tool_choice"], "auto");
}

#[test]
fn partial_tool_history_repairs_wire_without_losing_paired_results_or_exposing_signatures() {
    for (protocol, value) in [
        (
            Protocol::OpenAiChat,
            json!({
                "model":"test-model","messages":[{"role":"user","content":"check"},{
                    "role":"assistant","content":null,"tool_calls":[
                        {"id":"good","type":"function","function":{"name":"lookup","arguments":"{}"}},
                        {"id":"missing","type":"function","function":{"name":"lookup","arguments":"{}"},"signature":"secret-signature"}
                    ]},
                    {"role":"tool","tool_call_id":"good","content":"actual-result"},
                    {"role":"tool","tool_call_id":"orphan","content":"orphan-result","encrypted_content":"secret-encrypted"}
                ]
            }),
        ),
        (
            Protocol::OpenAiResponses,
            json!({
                "model":"test-model","input":[{"role":"user","content":"check"},
                    {"type":"function_call","call_id":"good","name":"lookup","arguments":"{}"},
                    {"type":"function_call","call_id":"missing","name":"lookup","arguments":"{}","signature":"secret-signature"},
                    {"type":"function_call_output","call_id":"good","output":"actual-result"},
                    {"type":"function_call_output","call_id":"orphan","output":"orphan-result","encrypted_content":"secret-encrypted"}
                ]
            }),
        ),
        (
            Protocol::Anthropic,
            json!({
                "model":"test-model","max_tokens":256,"messages":[{"role":"user","content":"check"},{
                    "role":"assistant","content":[
                        {"type":"tool_use","id":"good","name":"lookup","input":{}},
                        {"type":"tool_use","id":"missing","name":"lookup","input":{},"signature":"secret-signature"}
                    ]},{"role":"user","content":[
                        {"type":"tool_result","tool_use_id":"good","content":"actual-result"},
                        {"type":"tool_result","tool_use_id":"orphan","content":"orphan-result","encrypted_content":"secret-encrypted"}
                    ]}
                ]
            }),
        ),
        (
            Protocol::Gemini,
            json!({
                "contents":[{"role":"user","parts":[{"text":"check"}]},{
                    "role":"model","parts":[
                        {"functionCall":{"id":"good","name":"lookup","args":{}}},
                        {"functionCall":{"id":"missing","name":"lookup","args":{},"signature":"secret-signature"}}
                    ]},{"role":"user","parts":[
                        {"functionResponse":{"id":"good","name":"lookup","response":{"result":"actual-result"}}},
                        {"functionResponse":{"id":"orphan","name":"lookup","response":{"result":"orphan-result"},"encrypted_content":"secret-encrypted"}}
                    ]}
                ]
            }),
        ),
    ] {
        let body = source(parse(protocol, value), protocol);
        let rendered = body.to_string();
        assert!(rendered.contains("actual-result"));
        assert!(rendered.contains("orphan-result"));
        assert!(rendered.contains("preserved native content"));
        assert!(!rendered.contains("secret-signature"));
        assert!(!rendered.contains("secret-encrypted"));
    }
}

#[test]
fn duplicate_call_ids_are_repaired_with_real_results_in_stable_order() {
    let body = source(
        parse(
            Protocol::OpenAiChat,
            json!({
                "model":"test-model","messages":[{"role":"user","content":"check"},
                    {"role":"assistant","tool_calls":[
                        {"id":"same","type":"function","function":{"name":"one","arguments":"{}"}},
                        {"id":"same","type":"function","function":{"name":"two","arguments":"{}"}}
                    ]},
                    {"role":"tool","tool_call_id":"same","content":"first"},
                    {"role":"tool","tool_call_id":"same","content":"second"}
                ]
            }),
        ),
        Protocol::OpenAiChat,
    );
    let calls = body["messages"][1]["tool_calls"].as_array().unwrap();
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    assert_eq!(body["messages"][2]["tool_call_id"], calls[0]["id"]);
    assert_eq!(body["messages"][3]["tool_call_id"], calls[1]["id"]);
    assert_eq!(body["messages"][2]["content"], "first");
    assert_eq!(body["messages"][3]["content"], "second");
}

#[test]
fn optional_scalar_limits_and_unknown_tool_choice_are_normalized() {
    for (protocol, value) in [
        (
            Protocol::OpenAiChat,
            json!({"model":"test-model","temperature":9,"top_p":-2,"max_tokens":0,"tool_choice":{},"messages":[{"role":"user","content":"hello"}]}),
        ),
        (
            Protocol::OpenAiResponses,
            json!({"model":"test-model","temperature":9,"top_p":-2,"max_output_tokens":0,"tool_choice":"sdk_future","input":"hello"}),
        ),
        (
            Protocol::Anthropic,
            json!({"model":"test-model","temperature":9,"top_p":-2,"max_tokens":0,"tool_choice":{},"messages":[{"role":"user","content":"hello"}]}),
        ),
        (
            Protocol::Gemini,
            json!({"generationConfig":{"temperature":9,"topP":-2,"maxOutputTokens":0},"contents":[{"role":"user","parts":[{"text":"hello"}]}]}),
        ),
    ] {
        let envelope = parse(protocol, value);
        assert_eq!(
            envelope.canonical().sampling.temperature(),
            Some(if protocol == Protocol::Anthropic {
                1.0
            } else {
                2.0
            })
        );
        assert_eq!(envelope.canonical().sampling.top_p(), Some(0.0));
        assert!(envelope.canonical().sampling.max_output_tokens().is_none());
        let envelope =
            envelope.with_default_max_output_tokens(crate::TokenCount::new(8192).unwrap());
        let body: Value =
            serde_json::from_slice(envelope.plan_conversation(protocol).unwrap().body()).unwrap();
        let key = match protocol {
            Protocol::OpenAiChat => "max_completion_tokens",
            Protocol::OpenAiResponses => "max_output_tokens",
            Protocol::Anthropic => "max_tokens",
            _ => "maxOutputTokens",
        };
        let config = if protocol == Protocol::Gemini {
            &body["generationConfig"]
        } else {
            &body
        };
        assert_eq!(config[key], 8192);
    }
}

#[test]
fn output_format_preferences_are_convertible_but_media_modes_stay_native() {
    for (protocol, value) in [
        (
            Protocol::OpenAiChat,
            json!({"response_format":{"type":"json_schema","json_schema":{"name":"x","schema":{"type":"object"}}}}),
        ),
        (
            Protocol::OpenAiResponses,
            json!({"text":{"format":{"type":"json_schema","name":"x","schema":{"type":"object"}}}}),
        ),
        (
            Protocol::Anthropic,
            json!({"output_config":{"format":{"type":"json_schema","schema":{"type":"object"}}}}),
        ),
        (
            Protocol::Gemini,
            json!({"generationConfig":{"responseMimeType":"application/json","responseJsonSchema":{"type":"object"}}}),
        ),
    ] {
        assert!(!has_opaque_semantics(protocol, &value));
    }
    assert!(has_opaque_semantics(
        Protocol::Gemini,
        &json!({"generationConfig":{"responseModalities":["AUDIO"]}})
    ));
}

#[test]
fn caller_owned_tool_extensions_preserve_native_fields_and_convert_input_schema() {
    let schema = json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false});
    let targets = [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
    ];
    for (protocol, value) in [
        (
            Protocol::Anthropic,
            json!({
                "model":"test-model","max_tokens":4096,"messages":[{"role":"user","content":"check"}],
                "tools":[{"type":"custom","name":"lookup","description":"Find a record","strict":true,"input_schema":schema}]
            }),
        ),
        (
            Protocol::OpenAiResponses,
            json!({
                "model":"test-model","max_output_tokens":4096,"input":"check",
                "tools":[{"type":"function","name":"lookup","description":"Find a record","parameters":schema,
                    "strict":true,"allowed_callers":["direct"],"defer_loading":true,"output_schema":{"type":"object","properties":{"found":{"type":"boolean"}}}}]
            }),
        ),
    ] {
        assert!(!has_opaque_semantics(protocol, &value));
        let envelope = parse(protocol, value.clone());
        let native = source(envelope.clone(), protocol);
        assert_eq!(
            native["tools"], value["tools"],
            "native extensions must remain intact"
        );
        for target in targets {
            let plan = envelope
                .clone()
                .plan_conversation(target)
                .unwrap_or_else(|| {
                    panic!("{protocol:?}->{target:?}: caller-owned tool was blocked")
                });
            let converted = parse(target, serde_json::from_slice(plan.body()).unwrap());
            let [tool] = converted.canonical().tools.as_slice() else {
                panic!("{protocol:?}->{target:?}: caller-owned tool definition lost")
            };
            assert_eq!(tool.name, "lookup");
            assert_eq!(tool.description.as_deref(), Some("Find a record"));
            assert_eq!(
                tool.input_schema, schema,
                "{protocol:?}->{target:?}: input schema changed"
            );
        }
    }
}

#[test]
fn provider_executed_tool_types_remain_native_only() {
    for (protocol, value) in [
        (
            Protocol::Anthropic,
            json!({"tools":[{"type":"web_search_20250305","name":"web_search"}]}),
        ),
        (
            Protocol::Anthropic,
            json!({"tools":[{"type":"computer_20250124","name":"computer"}]}),
        ),
        (
            Protocol::OpenAiResponses,
            json!({"tools":[{"type":"web_search_preview"}]}),
        ),
        (
            Protocol::OpenAiResponses,
            json!({"tools":[{"type":"code_interpreter","container":{"type":"auto"}}]}),
        ),
    ] {
        assert!(
            has_opaque_semantics(protocol, &value),
            "{protocol:?}: provider execution state cannot be converted into a caller-owned function"
        );
    }
    for kind in [Value::Null, json!("custom"), json!("function")] {
        assert!(!has_opaque_semantics(
            Protocol::Anthropic,
            &json!({"tools":[{"type":kind,"name":"lookup","strict":true,"input_schema":{"type":"object"}}]})
        ));
    }
}
