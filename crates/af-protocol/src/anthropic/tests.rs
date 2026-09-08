use af_domain::{Operation, Role};
use serde_json::{Value, json};

use super::{MAX_BODY_BYTES, ParseRequestError, parse_request};
use crate::{CacheHint, ContentBlock, MediaSource, ReasoningEffort, ToolChoice};

fn minimal_request() -> Value {
    json!({
        "model": "claude-test",
        "max_tokens": 256,
        "messages": [{"role": "user", "content": "hello"}]
    })
}

fn parse_json(value: Value) -> Result<crate::CanonicalRequest, ParseRequestError> {
    parse_request(&serde_json::to_vec(&value).unwrap())
}

fn assert_json_error(value: Value, expected: ParseRequestError) {
    assert_eq!(parse_json(value).unwrap_err(), expected);
}

fn tool_history_request() -> Value {
    json!({
        "model": "claude-test",
        "max_tokens": 256,
        "messages": [
            {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "lookup",
                    "input": {"query": "status"}
                }]
            },
            {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "content": "ok"
                }]
            }
        ]
    })
}

#[test]
fn parses_full_non_streaming_request_into_canonical() {
    let request = parse_json(json!({
        "model": "claude-sonnet-test",
        "max_tokens": 4096,
        "system": [
            {
                "type": "text",
                "text": "follow policy",
                "cache_control": {"type": "ephemeral", "ttl": "1h"}
            },
            {"type": "text", "text": "answer briefly"}
        ],
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "inspect"},
                    {
                        "type": "image",
                        "source": {"type": "url", "url": "https://example.com/image.png"},
                        "cache_control": {"type": "ephemeral"}
                    }
                ]
            },
            {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "checking"},
                    {
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "lookup",
                        "input": {"query": "health"}
                    }
                ]
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_1",
                        "content": [
                            {"type": "text", "text": "healthy"},
                            {
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": "image/png",
                                    "data": "aGVsbG8="
                                }
                            }
                        ],
                        "is_error": false
                    },
                    {"type": "text", "text": "continue"}
                ]
            }
        ],
        "tools": [{
            "name": "lookup",
            "description": "look up service health",
            "input_schema": {
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }
        }],
        "tool_choice": {"type": "tool", "name": "lookup"},
        "thinking": {"type": "enabled", "budget_tokens": 1024},
        "temperature": 0.2,
        "top_p": 0.9,
        "stop_sequences": ["END"],
        "metadata": {"user_id": "opaque-user"},
        "stream": false
    }))
    .unwrap();

    assert_eq!(request.operation, Operation::Chat);
    assert_eq!(request.model, "claude-sonnet-test");
    assert!(!request.stream);
    assert_eq!(request.messages.len(), 5);
    assert_eq!(request.messages[0].role, Role::System);
    assert!(matches!(
        request.messages[0].content.as_slice(),
        [
            ContentBlock::Text(_),
            ContentBlock::CacheControl(CacheHint::Ephemeral1Hour),
            ContentBlock::Text(_)
        ]
    ));
    assert_eq!(request.messages[1].role, Role::User);
    assert!(matches!(
        request.messages[1].content.as_slice(),
        [
            ContentBlock::Text(_),
            ContentBlock::Image {
                source: MediaSource::Url(_),
                mime_type: None
            },
            ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes)
        ]
    ));
    assert_eq!(request.messages[2].role, Role::Assistant);
    assert!(matches!(
        request.messages[2].content.as_slice(),
        [ContentBlock::Text(_), ContentBlock::ToolUse { .. }]
    ));
    assert_eq!(request.messages[3].role, Role::Tool);
    let [
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content,
            is_error,
        },
    ] = request.messages[3].content.as_slice()
    else {
        panic!("工具结果必须归一为独立工具消息");
    };
    assert_eq!(tool_use_id, "toolu_1");
    assert!(structured_content.is_none());
    assert!(!*is_error);
    assert!(matches!(
        content.as_slice(),
        [
            ContentBlock::Text(_),
            ContentBlock::Image {
                source: MediaSource::Base64(_),
                mime_type: Some(_)
            }
        ]
    ));
    assert_eq!(request.messages[4].role, Role::User);
    assert!(matches!(
        request.messages[4].content.as_slice(),
        [ContentBlock::Text(_)]
    ));

    assert_eq!(request.tools.len(), 1);
    assert_eq!(
        request.tool_choice,
        ToolChoice::Named {
            name: "lookup".to_owned()
        }
    );
    let reasoning = request.reasoning.unwrap();
    assert_eq!(reasoning.effort(), None);
    assert_eq!(reasoning.budget_tokens().unwrap().get(), 1024);
    assert!(reasoning.include_thinking());
    assert_eq!(request.sampling.temperature(), Some(0.2));
    assert_eq!(request.sampling.top_p(), Some(0.9));
    assert_eq!(request.sampling.max_output_tokens().unwrap().get(), 4096);
    assert_eq!(request.sampling.stop_sequences().len(), 1);
    assert_eq!(request.sampling.stop_sequences()[0], "END");
    assert_eq!(request.metadata.user_id(), Some("opaque-user"));
}

#[test]
fn parses_streaming_request_into_canonical() {
    let mut streaming = minimal_request();
    streaming["stream"] = json!(true);

    let request = parse_json(streaming).unwrap();

    assert!(request.stream);
}

#[test]
fn maps_adaptive_and_disabled_thinking_without_guessing_budget() {
    let mut adaptive = minimal_request();
    adaptive["thinking"] = json!({"type": "adaptive", "display": "omitted"});
    let adaptive = parse_json(adaptive).unwrap().reasoning.unwrap();
    assert_eq!(adaptive.effort(), None);
    assert_eq!(adaptive.budget_tokens(), None);
    assert!(!adaptive.include_thinking());

    let mut disabled = minimal_request();
    disabled["thinking"] = json!({"type": "disabled"});
    let disabled = parse_json(disabled).unwrap().reasoning.unwrap();
    assert_eq!(disabled.effort(), Some(ReasoningEffort::None));
    assert_eq!(disabled.budget_tokens(), None);
    assert!(!disabled.include_thinking());
}

#[test]
fn maps_output_effort_and_rejects_disabled_conflicts() {
    for (wire, expected) in [
        ("low", ReasoningEffort::Low),
        ("medium", ReasoningEffort::Medium),
        ("high", ReasoningEffort::High),
        ("xhigh", ReasoningEffort::ExtraHigh),
        ("max", ReasoningEffort::Max),
    ] {
        let mut request = minimal_request();
        request["output_config"] = json!({"effort": wire});
        let reasoning = parse_json(request).unwrap().reasoning.unwrap();
        assert_eq!(reasoning.effort(), Some(expected));
        assert_eq!(reasoning.budget_tokens(), None);
        assert!(!reasoning.include_thinking());
    }

    let mut combined = minimal_request();
    combined["max_tokens"] = json!(4096);
    combined["thinking"] = json!({"type": "enabled", "budget_tokens": 1024});
    combined["output_config"] = json!({"effort": "high"});
    let reasoning = parse_json(combined).unwrap().reasoning.unwrap();
    assert_eq!(reasoning.effort(), Some(ReasoningEffort::High));
    assert_eq!(reasoning.budget_tokens().unwrap().get(), 1024);

    let mut conflict = minimal_request();
    conflict["thinking"] = json!({"type": "disabled"});
    conflict["output_config"] = json!({"effort": "high"});
    assert_json_error(conflict, ParseRequestError::InvalidValue);
}

#[test]
fn rejects_duplicate_keys_and_non_single_json_documents() {
    assert_eq!(
        parse_request(
            br#"{"model":"m","max_tokens":1,"max_tokens":2,"messages":[{"role":"user","content":"x"}]}"#
        )
        .unwrap_err(),
        ParseRequestError::DuplicateKey
    );
    assert_eq!(
        parse_request(
            br#"{"model":"m","max_tokens":8,"messages":[{"role":"assistant","content":[{"type":"tool_use","id":"x","name":"x","input":{"a":1,"a":2}}]}]}"#
        )
        .unwrap_err(),
        ParseRequestError::DuplicateKey
    );
    assert_eq!(
        parse_request(br#"{} {}"#).unwrap_err(),
        ParseRequestError::InvalidJson
    );
}

#[test]
fn rejects_unknown_null_invalid_and_known_unsupported_fields() {
    let mut unknown = minimal_request();
    unknown["unknown_field"] = json!(true);
    assert_json_error(unknown, ParseRequestError::InvalidValue);

    let mut explicit_null = minimal_request();
    explicit_null["stream"] = Value::Null;
    assert_json_error(explicit_null, ParseRequestError::InvalidValue);

    let mut invalid_role = minimal_request();
    invalid_role["messages"][0]["role"] = json!("developer");
    assert_json_error(invalid_role, ParseRequestError::InvalidValue);

    let mut top_k = minimal_request();
    top_k["top_k"] = json!(20);
    assert_json_error(top_k, ParseRequestError::UnsupportedFeature);

    let mut document = minimal_request();
    document["messages"][0]["content"] = json!([{
        "type": "document",
        "source": {"type": "url", "url": "https://example.com/a.pdf"}
    }]);
    assert_json_error(document, ParseRequestError::UnsupportedFeature);

    let mut thinking_block = minimal_request();
    thinking_block["messages"][0] = json!({
        "role": "assistant",
        "content": [{"type": "thinking", "thinking": "secret", "signature": "sig"}]
    });
    assert_json_error(thinking_block, ParseRequestError::UnsupportedFeature);
}

#[test]
fn rejects_explicit_null_at_nested_optional_boundaries() {
    let mut system = minimal_request();
    system["system"] = Value::Null;
    assert_json_error(system, ParseRequestError::InvalidValue);

    let mut metadata = minimal_request();
    metadata["metadata"] = json!({"user_id": null});
    assert_json_error(metadata, ParseRequestError::InvalidValue);

    let mut cache_control = minimal_request();
    cache_control["messages"][0]["content"] =
        json!([{"type": "text", "text": "hello", "cache_control": null}]);
    assert_json_error(cache_control, ParseRequestError::InvalidValue);

    let mut description = minimal_request();
    description["tools"] = json!([{
        "name": "lookup",
        "description": null,
        "input_schema": {"type": "object"}
    }]);
    assert_json_error(description, ParseRequestError::InvalidValue);
}

#[test]
fn enforces_body_structure_and_business_budgets() {
    assert_eq!(
        parse_request(&vec![b' '; MAX_BODY_BYTES + 1]).unwrap_err(),
        ParseRequestError::BodyTooLarge
    );

    let mut nested = Value::Null;
    for _ in 0..40 {
        nested = json!({"next": nested});
    }
    let mut too_deep = minimal_request();
    too_deep["unknown"] = nested;
    assert_json_error(too_deep, ParseRequestError::StructureLimitExceeded);

    let mut too_many_messages = minimal_request();
    too_many_messages["messages"] = Value::Array(
        (0..257)
            .map(|_| json!({"role": "user", "content": "x"}))
            .collect(),
    );
    assert_json_error(too_many_messages, ParseRequestError::StructureLimitExceeded);

    let mut empty_parts = minimal_request();
    empty_parts["messages"][0]["content"] = json!([]);
    assert_json_error(empty_parts, ParseRequestError::StructureLimitExceeded);

    let mut long_text = minimal_request();
    long_text["messages"][0]["content"] = json!("x".repeat(1024 * 1024 + 1));
    assert_json_error(long_text, ParseRequestError::StructureLimitExceeded);

    let mut too_many_cache_breakpoints = minimal_request();
    too_many_cache_breakpoints["system"] = Value::Array(
        (0..5)
            .map(|index| {
                json!({
                    "type": "text",
                    "text": format!("system-{index}"),
                    "cache_control": {"type": "ephemeral"}
                })
            })
            .collect(),
    );
    assert_json_error(
        too_many_cache_breakpoints,
        ParseRequestError::StructureLimitExceeded,
    );
}

#[test]
fn validates_sampling_metadata_and_thinking_ranges() {
    let mut temperature = minimal_request();
    temperature["temperature"] = json!(1.01);
    assert_json_error(temperature, ParseRequestError::InvalidValue);

    let mut top_p = minimal_request();
    top_p["top_p"] = json!(-0.01);
    assert_json_error(top_p, ParseRequestError::InvalidValue);

    let mut max_tokens = minimal_request();
    max_tokens["max_tokens"] = json!(-1);
    assert_json_error(max_tokens, ParseRequestError::InvalidValue);

    let mut empty_stop = minimal_request();
    empty_stop["stop_sequences"] = json!([""]);
    assert_json_error(empty_stop, ParseRequestError::InvalidValue);

    let mut too_many_stops = minimal_request();
    too_many_stops["stop_sequences"] = json!(["a", "b", "c", "d", "e"]);
    assert_json_error(too_many_stops, ParseRequestError::InvalidValue);

    let mut low_budget = minimal_request();
    low_budget["max_tokens"] = json!(2048);
    low_budget["thinking"] = json!({"type": "enabled", "budget_tokens": 1023});
    assert_json_error(low_budget, ParseRequestError::InvalidValue);

    let mut non_smaller_budget = minimal_request();
    non_smaller_budget["thinking"] = json!({"type": "enabled", "budget_tokens": 256});
    assert_json_error(non_smaller_budget, ParseRequestError::InvalidValue);

    let mut empty_user = minimal_request();
    empty_user["metadata"] = json!({"user_id": ""});
    assert_json_error(empty_user, ParseRequestError::InvalidValue);
}

#[test]
fn validates_media_tools_and_schema_boundaries() {
    let mut http_image = minimal_request();
    http_image["messages"][0]["content"] = json!([{
        "type": "image",
        "source": {"type": "url", "url": "http://example.com/image.png"}
    }]);
    assert_json_error(http_image, ParseRequestError::InvalidValue);

    let mut invalid_base64 = minimal_request();
    invalid_base64["messages"][0]["content"] = json!([{
        "type": "image",
        "source": {"type": "base64", "media_type": "image/png", "data": "***="}
    }]);
    assert_json_error(invalid_base64, ParseRequestError::InvalidValue);

    let mut invalid_mime = minimal_request();
    invalid_mime["messages"][0]["content"] = json!([{
        "type": "image",
        "source": {"type": "base64", "media_type": "image/bmp", "data": "aGVsbG8="}
    }]);
    assert_json_error(invalid_mime, ParseRequestError::InvalidValue);

    let mut invalid_schema = minimal_request();
    invalid_schema["tools"] = json!([{"name": "lookup", "input_schema": []}]);
    assert_json_error(invalid_schema, ParseRequestError::InvalidValue);

    let mut remote_schema = minimal_request();
    remote_schema["tools"] = json!([{
        "name": "lookup",
        "input_schema": {"$ref": "https://example.com/schema.json"}
    }]);
    assert_json_error(remote_schema, ParseRequestError::UnsupportedFeature);

    let mut server_tool_option = minimal_request();
    server_tool_option["tools"] = json!([{
        "name": "lookup",
        "input_schema": {"type": "object"},
        "cache_control": {"type": "ephemeral"}
    }]);
    assert_json_error(server_tool_option, ParseRequestError::UnsupportedFeature);

    let mut non_object_input = tool_history_request();
    non_object_input["messages"][0]["content"][0]["input"] = json!([]);
    assert_json_error(non_object_input, ParseRequestError::InvalidValue);
}

#[test]
fn enforces_tool_choice_and_tool_result_relationships() {
    let mut named_unknown = minimal_request();
    named_unknown["tools"] = json!([{"name": "lookup", "input_schema": {"type": "object"}}]);
    named_unknown["tool_choice"] = json!({"type": "tool", "name": "missing"});
    assert_json_error(named_unknown, ParseRequestError::InvalidValue);

    let mut required_without_tools = minimal_request();
    required_without_tools["tool_choice"] = json!({"type": "any"});
    assert_json_error(required_without_tools, ParseRequestError::InvalidValue);

    let mut disabled_parallel = minimal_request();
    disabled_parallel["tools"] = json!([{"name": "lookup", "input_schema": {"type": "object"}}]);
    disabled_parallel["tool_choice"] = json!({"type": "auto", "disable_parallel_tool_use": true});
    assert_json_error(disabled_parallel, ParseRequestError::UnsupportedFeature);

    let mut unmatched_result = minimal_request();
    unmatched_result["messages"][0]["content"] = json!([{
        "type": "tool_result",
        "tool_use_id": "missing",
        "content": "result"
    }]);
    assert_json_error(unmatched_result, ParseRequestError::InvalidValue);

    let mut unresolved_call = tool_history_request();
    unresolved_call["messages"].as_array_mut().unwrap().pop();
    assert_json_error(unresolved_call, ParseRequestError::InvalidValue);

    let mut result_after_text = tool_history_request();
    result_after_text["messages"][1]["content"] = json!([
        {"type": "text", "text": "continue"},
        {"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}
    ]);
    assert_json_error(result_after_text, ParseRequestError::InvalidValue);

    let mut duplicate_call = tool_history_request();
    duplicate_call["messages"][0]["content"] = json!([
        {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}},
        {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}}
    ]);
    assert_json_error(duplicate_call, ParseRequestError::InvalidValue);
}

#[test]
fn debug_and_errors_do_not_expose_external_values() {
    let request = parse_json(json!({
        "model": "secret-model-z9",
        "max_tokens": 2048,
        "system": "secret-system-z9",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "image",
                "source": {"type": "url", "url": "https://secret-host-z9.example/image"}
            }, {"type": "text", "text": "secret-message-z9"}]
        }],
        "tools": [{
            "name": "secret_tool_z9",
            "description": "secret-description-z9",
            "input_schema": {"type": "object", "title": "secret-schema-z9"}
        }],
        "tool_choice": {"type": "tool", "name": "secret_tool_z9"},
        "metadata": {"user_id": "secret-user-z9"}
    }))
    .unwrap();

    let debug = format!("{request:?}");
    for secret in [
        "secret-model-z9",
        "secret-system-z9",
        "secret-host-z9",
        "secret-message-z9",
        "secret_tool_z9",
        "secret-description-z9",
        "secret-schema-z9",
        "secret-user-z9",
    ] {
        assert!(!debug.contains(secret));
    }

    let mut invalid = minimal_request();
    invalid["secret-field-z9"] = json!("secret-value-z9");
    let error = parse_json(invalid).unwrap_err();
    assert_eq!(format!("{error:?}"), "InvalidValue");
    assert_eq!(error.to_string(), "请求字段值无效");
}
