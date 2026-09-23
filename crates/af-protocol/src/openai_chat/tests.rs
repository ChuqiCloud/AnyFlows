use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use serde_json::{Value, json};

use super::{MAX_BODY_BYTES, ParseRequestError, parse_request};
use crate::bounded_json::{BoundedJsonError, JsonLimits, parse_value};
use crate::{
    CanonicalRequest, ContentBlock, MediaSource, RawPassthroughError, ReasoningEffort, ToolChoice,
};

fn parse_json(value: Value) -> Result<CanonicalRequest, ParseRequestError> {
    let body = serde_json::to_vec(&value).unwrap();
    parse_request(&body)
}

fn request_with_messages(messages: Vec<Value>) -> Value {
    json!({
        "model": "gpt-test",
        "messages": messages,
    })
}

fn user_parts_request(parts: Vec<Value>) -> Value {
    request_with_messages(vec![json!({
        "role": "user",
        "content": parts,
    })])
}

fn assistant_arguments_request(arguments: String) -> Value {
    request_with_messages(vec![
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "lookup",
                    "arguments": arguments,
                },
            }],
        }),
        json!({ "role": "tool", "tool_call_id": "call-1", "content": "done" }),
    ])
}

fn nested_object(depth: usize) -> Value {
    let mut value = json!(0);
    for _ in 0..depth {
        value = json!({ "nested": value });
    }
    value
}

#[test]
fn parses_minimal_non_streaming_request() {
    let request =
        parse_request(br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}]}"#)
            .unwrap();

    assert_eq!(request.operation, Operation::Chat);
    assert_eq!(request.model, "gpt-test");
    assert!(!request.stream);
    assert_eq!(request.tool_choice, ToolChoice::None);
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, Role::User);
    assert!(matches!(
        request.messages[0].content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
}

#[test]
fn preserves_all_five_message_roles_and_order() {
    let request = parse_json(request_with_messages(vec![
        json!({ "role": "system", "content": "system" }),
        json!({ "role": "developer", "content": "developer" }),
        json!({ "role": "user", "content": "user" }),
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": { "name": "lookup", "arguments": "{}" },
            }],
        }),
        json!({ "role": "tool", "tool_call_id": "call-1", "content": "result" }),
    ]))
    .unwrap();

    let roles = request
        .messages
        .iter()
        .map(|message| message.role)
        .collect::<Vec<_>>();
    assert_eq!(
        roles,
        [
            Role::System,
            Role::Developer,
            Role::User,
            Role::Assistant,
            Role::Tool,
        ]
    );
}

#[test]
fn preserves_text_part_array_order() {
    let request = parse_json(user_parts_request(vec![
        json!({ "type": "text", "text": "first" }),
        json!({ "type": "text", "text": "second" }),
        json!({ "type": "text", "text": "third" }),
    ]))
    .unwrap();

    let texts = request.messages[0]
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(text) => text.as_str(),
            _ => panic!("应只包含文本内容块"),
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, ["first", "second", "third"]);
}

#[test]
fn maps_function_tools_choice_reasoning_sampling_and_metadata() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{
            "type": "function",
            "function": {
                "name": "lookup",
                "description": "Lookup a record",
                "parameters": {
                    "type": "object",
                    "properties": { "id": { "type": "string" } },
                    "required": ["id"]
                }
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": { "name": "lookup" }
        },
        "reasoning_effort": "high",
        "temperature": 0.5,
        "top_p": 0.75,
        "max_completion_tokens": 123,
        "stop": ["STOP", "END"],
        "user": "user-123"
    }))
    .unwrap();

    assert_eq!(request.tools.len(), 1);
    assert_eq!(request.tools[0].name, "lookup");
    assert_eq!(
        request.tools[0].description.as_deref(),
        Some("Lookup a record")
    );
    assert_eq!(request.tools[0].input_schema["type"], "object");
    assert!(matches!(
        &request.tool_choice,
        ToolChoice::Named { name } if name == "lookup"
    ));
    assert_eq!(
        request
            .reasoning
            .as_ref()
            .and_then(|config| config.effort()),
        Some(ReasoningEffort::High)
    );
    assert_eq!(request.sampling.temperature(), Some(0.5));
    assert_eq!(request.sampling.top_p(), Some(0.75));
    assert_eq!(
        request
            .sampling
            .max_output_tokens()
            .map(|tokens| tokens.get()),
        Some(123)
    );
    assert_eq!(request.sampling.stop_sequences(), ["STOP", "END"]);
    assert_eq!(request.metadata.user_id(), Some("user-123"));
    assert_eq!(request.metadata.session_id(), None);
}

#[test]
fn maps_tool_call_arguments_and_associated_tool_result() {
    let request = parse_json(request_with_messages(vec![
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "lookup",
                    "arguments": "{\"id\":7,\"tags\":[\"a\",\"b\"]}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call-1",
            "content": [
                { "type": "text", "text": "first" },
                { "type": "text", "text": "second" }
            ]
        }),
    ]))
    .unwrap();

    match &request.messages[0].content[0] {
        ContentBlock::ToolUse {
            id,
            name,
            input,
            signature,
        } => {
            assert_eq!(id, "call-1");
            assert_eq!(name, "lookup");
            assert_eq!(input, &json!({ "id": 7, "tags": ["a", "b"] }));
            assert!(signature.is_none());
        }
        _ => panic!("助手消息应映射为工具调用"),
    }
    match &request.messages[1].content[0] {
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content,
            is_error,
        } => {
            assert_eq!(tool_use_id, "call-1");
            assert!(structured_content.is_none());
            assert!(!is_error);
            assert!(matches!(
                content.as_slice(),
                [ContentBlock::Text(first), ContentBlock::Text(second)]
                    if first == "first" && second == "second"
            ));
        }
        _ => panic!("工具消息应映射为关联的工具结果"),
    }
}

#[test]
fn rejects_incomplete_or_interleaved_tool_results() {
    let assistant = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "call-1",
            "type": "function",
            "function": { "name": "lookup", "arguments": "{}" }
        }]
    });
    let dangling = request_with_messages(vec![assistant.clone()]);
    let interleaved = request_with_messages(vec![
        assistant,
        json!({ "role": "user", "content": "interrupt" }),
        json!({ "role": "tool", "tool_call_id": "call-1", "content": "done" }),
    ]);

    assert_eq!(parse_json(dangling), Err(ParseRequestError::InvalidValue));
    assert_eq!(
        parse_json(interleaved),
        Err(ParseRequestError::InvalidValue)
    );
}

#[test]
fn accepts_standard_assistant_history_for_continuation() {
    let baseline = request_with_messages(vec![
        json!({ "role": "user", "content": "first" }),
        json!({ "role": "assistant", "content": "answer" }),
        json!({ "role": "user", "content": "follow up" }),
    ]);

    for stream in [false, true] {
        let mut baseline = baseline.clone();
        baseline["stream"] = json!(stream);
        let expected = parse_json(baseline.clone()).unwrap();
        for metadata in [
            json!({ "refusal": null }),
            json!({ "annotations": [] }),
            json!({ "annotations": null }),
            json!({ "tool_calls": null }),
            json!({ "tool_calls": [] }),
            json!({ "audio": null }),
            json!({ "function_call": null }),
            json!({
                "refusal": null,
                "annotations": [],
                "tool_calls": null,
                "audio": null,
                "function_call": null
            }),
        ] {
            let mut request = baseline.clone();
            request["messages"][1]
                .as_object_mut()
                .unwrap()
                .extend(metadata.as_object().unwrap().clone());
            assert_eq!(parse_json(request), Ok(expected.clone()), "{metadata}");
        }
    }
}

#[test]
fn accepts_empty_response_metadata_in_tool_call_history() {
    let baseline = assistant_arguments_request("{}".to_owned());
    let expected = parse_json(baseline.clone()).unwrap();
    let mut request = baseline;
    request["messages"][0]["refusal"] = Value::Null;
    request["messages"][0]["annotations"] = json!([]);
    request["messages"][0]["audio"] = Value::Null;
    request["messages"][0]["function_call"] = Value::Null;

    assert_eq!(parse_json(request), Ok(expected));
}

#[test]
fn accepts_reasoning_metadata_in_assistant_history() {
    let baseline = request_with_messages(vec![
        json!({ "role": "user", "content": "first" }),
        json!({ "role": "assistant", "content": "answer" }),
        json!({ "role": "user", "content": "follow up" }),
    ]);

    for metadata in [
        json!({ "reasoning_content": "internal reasoning" }),
        json!({ "reasoning_content": null }),
        json!({ "reasoning": "internal reasoning" }),
        json!({ "reasoning": null }),
        json!({
            "reasoning_content": "internal reasoning",
            "reasoning": null
        }),
    ] {
        let mut request = baseline.clone();
        request["messages"][1]
            .as_object_mut()
            .unwrap()
            .extend(metadata.as_object().unwrap().clone());
        assert_eq!(
            parse_json(request),
            parse_json(baseline.clone()),
            "{metadata}"
        );
    }
}

#[test]
fn rejects_non_empty_unsupported_assistant_response_metadata() {
    for metadata in [
        json!({ "refusal": "Request refused" }),
        json!({ "audio": { "id": "audio-1" } }),
        json!({ "function_call": { "name": "lookup", "arguments": "{}" } }),
        json!({ "annotations": [{ "type": "url_citation" }] }),
    ] {
        let mut assistant = json!({ "role": "assistant", "content": "answer" });
        assistant
            .as_object_mut()
            .unwrap()
            .extend(metadata.as_object().unwrap().clone());
        assert_eq!(
            parse_json(request_with_messages(vec![assistant])),
            Err(ParseRequestError::UnsupportedFeature),
            "{metadata}"
        );
    }
}

#[test]
fn rejects_invalid_assistant_metadata_and_empty_history() {
    for assistant in [
        json!({ "role": "assistant", "content": "answer", "annotations": {} }),
        json!({ "role": "assistant", "content": "answer", "tool_calls": {} }),
        json!({ "role": "assistant", "content": "answer", "unknown": null }),
        json!({ "role": "assistant", "content": null, "tool_calls": null }),
        json!({ "role": "assistant", "refusal": null, "annotations": [] }),
    ] {
        assert_eq!(
            parse_json(request_with_messages(vec![assistant.clone()])),
            Err(ParseRequestError::InvalidValue),
            "{assistant}"
        );
    }
}

#[test]
fn rejects_conflicting_max_token_fields() {
    let error = parse_json(json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "max_tokens": 10,
        "max_completion_tokens": 20
    }))
    .unwrap_err();

    assert_eq!(error, ParseRequestError::ConflictingParameters);
}

#[test]
fn rejects_explicit_null_for_non_nullable_top_level_fields() {
    for request in [
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "temperature": null
        }),
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "tools": null
        }),
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": null
        }),
    ] {
        assert_eq!(parse_json(request), Err(ParseRequestError::InvalidValue));
    }
}

#[test]
fn rejects_unknown_top_level_and_nested_fields() {
    let unknown_top_level = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "future_option": true
    });
    let known_unsupported_nested = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello", "name": "caller" }]
    });
    let unknown_nested = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello", "future_option": true }]
    });

    assert_eq!(
        parse_json(unknown_top_level),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(known_unsupported_nested),
        Err(ParseRequestError::UnsupportedFeature)
    );
    assert_eq!(
        parse_json(unknown_nested),
        Err(ParseRequestError::InvalidValue)
    );
}

#[test]
fn preserves_explicit_streaming_mode() {
    let request = parse_request(
        br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}"#,
    )
    .unwrap();

    assert!(request.stream);
    assert!(request.stream_options.include_usage());
}

#[test]
fn stream_options_require_stream_and_reject_unknown_fields() {
    assert_eq!(
        parse_request(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream_options":{"include_usage":true}}"#,
        ),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_request(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream_options":{"include_usage":false}}"#,
        ),
        Err(ParseRequestError::InvalidValue)
    );
    let disabled = parse_request(
        br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":false}}"#,
    )
    .unwrap();
    assert!(!disabled.stream_options.include_usage());
    assert_eq!(
        parse_request(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_obfuscation":false}}"#,
        ),
        Err(ParseRequestError::UnsupportedFeature)
    );
}

#[test]
fn rejects_malformed_non_object_and_oversized_bodies() {
    for body in [b"".as_slice(), b"{".as_slice(), b"{}{}".as_slice(), &[0xff]] {
        assert_eq!(parse_request(body), Err(ParseRequestError::InvalidJson));
    }
    assert_eq!(
        parse_request(br#"["not","an","object"]"#),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_request(&vec![b' '; MAX_BODY_BYTES + 1]),
        Err(ParseRequestError::BodyTooLarge)
    );
}

#[test]
fn bounded_json_enforces_each_configured_budget() {
    let limits = JsonLimits {
        max_depth: 1,
        max_nodes: 2,
        max_object_entries: 1,
        max_array_items: 1,
        max_string_bytes: 2,
        max_key_bytes: 1,
    };

    assert_eq!(
        parse_value(br#"{"a":"b"}"#, limits),
        Ok(json!({ "a": "b" }))
    );
    for body in [
        br#"{"a":{"b":0}}"#.as_slice(),
        br#"{"a":[0,1]}"#.as_slice(),
        br#"{"a":0,"b":1}"#.as_slice(),
        br#"{"aa":0}"#.as_slice(),
        br#"{"a":"bc"}"#.as_slice(),
    ] {
        assert_eq!(
            parse_value(body, limits),
            Err(BoundedJsonError::LimitExceeded)
        );
    }
    assert_eq!(
        parse_value(
            br#"{"a":0,"a":1}"#,
            JsonLimits {
                max_object_entries: 2,
                ..limits
            },
        ),
        Err(BoundedJsonError::DuplicateKey)
    );
}

#[test]
fn rejects_duplicate_keys_at_top_level() {
    let error = parse_request(
        br#"{"model":"first","model":"second","messages":[{"role":"user","content":"hello"}]}"#,
    )
    .unwrap_err();

    assert_eq!(error, ParseRequestError::DuplicateKey);
}

#[test]
fn rejects_duplicate_keys_in_nested_request_objects() {
    let error = parse_request(
        br#"{"model":"gpt-test","messages":[{"role":"user","content":[{"type":"text","text":"first","text":"second"}]}]}"#,
    )
    .unwrap_err();

    assert_eq!(error, ParseRequestError::DuplicateKey);
}

#[test]
fn rejects_duplicate_keys_inside_tool_arguments() {
    let error = parse_request(
        br#"{"model":"gpt-test","messages":[{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"id\":1,\"id\":2}"}}]}]}"#,
    )
    .unwrap_err();

    assert_eq!(error, ParseRequestError::DuplicateKey);
}

#[test]
fn enforces_representative_depth_array_and_string_boundaries() {
    let accepted_arguments = serde_json::to_string(&nested_object(16)).unwrap();
    assert!(parse_json(assistant_arguments_request(accepted_arguments)).is_ok());

    let excessive_arguments = serde_json::to_string(&nested_object(17)).unwrap();
    assert_eq!(
        parse_json(assistant_arguments_request(excessive_arguments)),
        Err(ParseRequestError::StructureLimitExceeded)
    );

    let accepted_parts = (0..128)
        .map(|_| json!({ "type": "text", "text": "x" }))
        .collect();
    assert!(parse_json(user_parts_request(accepted_parts)).is_ok());

    let excessive_parts = (0..129)
        .map(|_| json!({ "type": "text", "text": "x" }))
        .collect();
    assert_eq!(
        parse_json(user_parts_request(excessive_parts)),
        Err(ParseRequestError::StructureLimitExceeded)
    );

    let mut accepted_model = request_with_messages(vec![json!({
        "role": "user",
        "content": "hello"
    })]);
    accepted_model["model"] = json!("m".repeat(256));
    assert!(parse_json(accepted_model).is_ok());

    let mut excessive_model = request_with_messages(vec![json!({
        "role": "user",
        "content": "hello"
    })]);
    excessive_model["model"] = json!("m".repeat(257));
    assert_eq!(
        parse_json(excessive_model),
        Err(ParseRequestError::InvalidValue)
    );
}

#[test]
fn enforces_request_wide_tool_argument_budget() {
    let calls = (0..128)
        .map(|index| {
            json!({
                "id": format!("call-{index}"),
                "type": "function",
                "function": { "name": "lookup", "arguments": "{}" }
            })
        })
        .collect::<Vec<_>>();
    let mut messages = vec![json!({
        "role": "assistant",
        "content": null,
        "tool_calls": calls,
    })];
    messages.extend((0..128).map(|index| {
        json!({
            "role": "tool",
            "tool_call_id": format!("call-{index}"),
            "content": "done",
        })
    }));
    messages.push(json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "call-over-budget",
            "type": "function",
            "function": { "name": "lookup", "arguments": "{}" }
        }],
    }));

    assert_eq!(
        parse_json(request_with_messages(messages)),
        Err(ParseRequestError::StructureLimitExceeded)
    );
}

#[test]
fn enforces_request_wide_argument_byte_and_node_budgets() {
    let request_for_arguments = |arguments: Vec<String>| {
        let mut messages = Vec::with_capacity(arguments.len() * 2);
        for (index, arguments) in arguments.into_iter().enumerate() {
            messages.push(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": format!("call-{index}"),
                    "type": "function",
                    "function": { "name": "lookup", "arguments": arguments }
                }]
            }));
            messages.push(json!({
                "role": "tool",
                "tool_call_id": format!("call-{index}"),
                "content": "done",
            }));
        }
        request_with_messages(messages)
    };

    let large_argument = serde_json::to_string(&json!({
        "data": "x".repeat(240 * 1024)
    }))
    .unwrap();
    assert_eq!(
        parse_json(request_for_arguments(vec![large_argument; 9])),
        Err(ParseRequestError::StructureLimitExceeded)
    );

    let dense_argument = serde_json::to_string(&json!({
        "items": vec![0; 4_090]
    }))
    .unwrap();
    assert_eq!(
        parse_json(request_for_arguments(vec![dense_argument; 9])),
        Err(ParseRequestError::StructureLimitExceeded)
    );
}

#[test]
fn preserves_allowlisted_raw_fields_for_same_protocol_only() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "frequency_penalty": -1.5,
        "logit_bias": { "42": -5 },
        "logprobs": true,
        "n": 2,
        "seed": 7,
        "store": false,
        "top_logprobs": 2,
        "metadata": { "trace": "trace-1" }
    }))
    .unwrap();

    let raw = request.raw_passthrough().unwrap();
    assert_eq!(raw.source_protocol(), Protocol::OpenAiChat);
    let fields = raw.fields_for_protocol(Protocol::OpenAiChat).unwrap();
    assert_eq!(fields.get("seed"), Some(&json!(7)));
    assert_eq!(fields.get("store"), Some(&json!(false)));
    assert_eq!(fields.get("top_logprobs"), Some(&json!(2)));
    assert_eq!(fields["metadata"]["trace"], "trace-1");
    assert_eq!(
        raw.fields_for_protocol(Protocol::Anthropic),
        Err(RawPassthroughError::ProtocolMismatch)
    );
}

#[test]
fn rejects_unknown_unsupported_and_forbidden_raw_fields() {
    let unknown = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "unknown_raw": { "safe": true }
    });
    let unsupported = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "response_format": { "type": "json_object" }
    });
    let forbidden = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "metadata": { "authorization": "secret" }
    });

    assert_eq!(parse_json(unknown), Err(ParseRequestError::InvalidValue));
    assert_eq!(
        parse_json(unsupported),
        Err(ParseRequestError::UnsupportedFeature)
    );
    assert_eq!(parse_json(forbidden), Err(ParseRequestError::InvalidValue));
}

#[test]
fn rejects_invalid_allowlisted_raw_semantics() {
    for request in [
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "frequency_penalty": 3
        }),
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "n": "many"
        }),
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "logprobs": {},
        }),
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "metadata": { "trace": { "id": "nested" } }
        }),
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "top_logprobs": 2
        }),
    ] {
        assert_eq!(parse_json(request), Err(ParseRequestError::InvalidValue));
    }
}

#[test]
fn maps_https_data_image_and_audio_content() {
    let request = parse_json(user_parts_request(vec![
        json!({
            "type": "image_url",
            "image_url": {
                "url": "https://media.example.test/image.png?version=1",
                "detail": "auto"
            }
        }),
        json!({
            "type": "image_url",
            "image_url": { "url": "data:image/png;base64,aGk=" }
        }),
        json!({
            "type": "input_audio",
            "input_audio": { "data": "aGk=", "format": "mp3" }
        }),
    ]))
    .unwrap();

    match &request.messages[0].content[0] {
        ContentBlock::Image {
            source: MediaSource::Url(url),
            mime_type,
        } => {
            assert_eq!(url, "https://media.example.test/image.png?version=1");
            assert_eq!(mime_type, &None);
        }
        _ => panic!("首个内容块应为 HTTPS 图片"),
    }
    match &request.messages[0].content[1] {
        ContentBlock::Image {
            source: MediaSource::Base64(data),
            mime_type,
        } => {
            assert_eq!(data, "aGk=");
            assert_eq!(mime_type.as_deref(), Some("image/png"));
        }
        _ => panic!("第二个内容块应为 data URL 图片"),
    }
    match &request.messages[0].content[2] {
        ContentBlock::Audio {
            source: MediaSource::Base64(data),
            mime_type,
        } => {
            assert_eq!(data, "aGk=");
            assert_eq!(mime_type, "audio/mpeg");
        }
        _ => panic!("第三个内容块应为音频"),
    }
}

#[test]
fn rejects_invalid_remote_image_urls() {
    for url in [
        "http://media.example.test/image.png",
        "https://user@media.example.test/image.png",
        "https://media.example.test/image.png#fragment",
        "https://media.example.test\\@127.0.0.1/image.png",
        " https://media.example.test/image.png",
        "not-a-url",
    ] {
        let request = user_parts_request(vec![json!({
            "type": "image_url",
            "image_url": { "url": url }
        })]);
        assert_eq!(parse_json(request), Err(ParseRequestError::InvalidValue));
    }
}

#[test]
fn rejects_invalid_base64_media_format_and_image_detail() {
    let invalid_base64 = user_parts_request(vec![json!({
        "type": "image_url",
        "image_url": { "url": "data:image/png;base64,***=" }
    })]);
    let unsupported_mime = user_parts_request(vec![json!({
        "type": "image_url",
        "image_url": { "url": "data:image/svg+xml;base64,aGk=" }
    })]);
    let unsupported_detail = user_parts_request(vec![json!({
        "type": "image_url",
        "image_url": {
            "url": "https://media.example.test/image.png",
            "detail": "low"
        }
    })]);
    let invalid_detail = user_parts_request(vec![json!({
        "type": "image_url",
        "image_url": {
            "url": "https://media.example.test/image.png",
            "detail": "original"
        }
    })]);
    let invalid_audio_format = user_parts_request(vec![json!({
        "type": "input_audio",
        "input_audio": { "data": "aGk=", "format": "flac" }
    })]);

    assert_eq!(
        parse_json(invalid_base64),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(unsupported_mime),
        Err(ParseRequestError::UnsupportedFeature)
    );
    assert_eq!(
        parse_json(unsupported_detail),
        Err(ParseRequestError::UnsupportedFeature)
    );
    assert_eq!(
        parse_json(invalid_detail),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(invalid_audio_format),
        Err(ParseRequestError::InvalidValue)
    );
}

#[test]
fn rejects_invalid_tool_names_choices_and_remote_schema_references() {
    let duplicate_names = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [
            { "type": "function", "function": { "name": "lookup" } },
            { "type": "function", "function": { "name": "lookup" } }
        ]
    });
    let invalid_name = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{ "type": "function", "function": { "name": "bad name" } }]
    });
    let undeclared_named_choice = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{ "type": "function", "function": { "name": "lookup" } }],
        "tool_choice": {
            "type": "function",
            "function": { "name": "missing" }
        }
    });
    let remote_reference = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{
            "type": "function",
            "function": {
                "name": "lookup",
                "parameters": { "$ref": "https://schema.example.test/tool.json" }
            }
        }]
    });
    let explicit_non_strict_tool = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{
            "type": "function",
            "function": { "name": "lookup", "strict": false }
        }]
    });
    let strict_tool = json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{
            "type": "function",
            "function": { "name": "lookup", "strict": true }
        }]
    });

    assert_eq!(
        parse_json(duplicate_names),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(invalid_name),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(undeclared_named_choice),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(remote_reference),
        Err(ParseRequestError::UnsupportedFeature)
    );
    let explicit_non_strict_tool = parse_json(explicit_non_strict_tool).unwrap();
    assert_eq!(explicit_non_strict_tool.tools[0].strict, Some(false));
    let strict_tool = parse_json(strict_tool).unwrap();
    assert_eq!(strict_tool.tools[0].strict, Some(true));
}

#[test]
fn redacts_success_and_error_debug_output_and_error_source_chain() {
    let request = parse_json(json!({
        "model": "model-canary-9f4a",
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "text-canary-9f4a" },
                {
                    "type": "image_url",
                    "image_url": { "url": "https://media-canary-9f4a.example.test/image.png" }
                }
            ]
        }],
        "tools": [{
            "type": "function",
            "function": {
                "name": "tool_canary_9f4a",
                "description": "description-canary-9f4a",
                "parameters": {
                    "type": "object",
                    "properties": { "schema_canary_9f4a": { "type": "string" } }
                }
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": { "name": "tool_canary_9f4a" }
        },
        "user": "user-canary-9f4a",
        "metadata": { "trace": "raw-canary-9f4a" }
    }))
    .unwrap();

    let success_debug = format!(
        "{request:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        request.messages[0],
        request.messages[0].content[0],
        request.messages[0].content[1],
        request.tools[0],
        request.raw_passthrough().unwrap()
    );
    for canary in [
        "model-canary-9f4a",
        "text-canary-9f4a",
        "media-canary-9f4a",
        "tool_canary_9f4a",
        "description-canary-9f4a",
        "schema_canary_9f4a",
        "user-canary-9f4a",
        "raw-canary-9f4a",
    ] {
        assert!(!success_debug.contains(canary));
    }
    assert!(success_debug.contains("<已脱敏>"));

    let error = parse_request(
        br#"{"model":"error-canary-9f4a","messages":[{"role":"user","content":"hello"}],"unknown-canary-9f4a":true}"#,
    )
    .unwrap_err();
    let error_output = format!("{error:?}|{error}");
    assert!(!error_output.contains("canary-9f4a"));
    assert!(error.source().is_none());
}
