use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use serde_json::{Value, json};

use super::{MAX_BODY_BYTES, ParseResponseError, parse_response};
use crate::{ContentBlock, FinishReason, UsageSemantics, UsageSource};

fn parse_json(value: Value) -> Result<crate::CanonicalResponse, ParseResponseError> {
    parse_response(&serde_json::to_vec(&value).unwrap())
}

fn response_with_choices(choices: Vec<Value>) -> Value {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "gpt-test",
        "choices": choices
    })
}

fn text_choice(index: u32, text: &str) -> Value {
    json!({
        "index": index,
        "message": { "role": "assistant", "content": text },
        "finish_reason": "stop"
    })
}

#[test]
fn parses_minimal_non_streaming_response() {
    let response = parse_json(response_with_choices(vec![text_choice(0, "hello")])).unwrap();

    assert_eq!(response.operation, Operation::Chat);
    assert_eq!(response.id, "chatcmpl-test");
    assert_eq!(response.model, "gpt-test");
    assert_eq!(response.created_at, Some(1_700_000_000));
    assert_eq!(response.choices.len(), 1);
    assert_eq!(response.choices[0].index, 0);
    assert_eq!(response.choices[0].message.role, Role::Assistant);
    assert_eq!(response.choices[0].finish_reason, FinishReason::Stop);
    assert!(matches!(
        response.choices[0].message.content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
    assert!(response.usage.is_none());
    assert!(response.raw_passthrough().is_none());
}

#[test]
fn parses_deepseek_cache_breakdown_fields() {
    let mut value = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "",
            "reasoning_content": "We"
        },
        "logprobs": null,
        "finish_reason": "length"
    })]);
    value["id"] = json!("b237448a-7b22-4622-880a-3cf0ea79275c");
    value["model"] = json!("deepseek-flash");
    value["system_fingerprint"] = json!("aeb56401ca74e127821c4f9126dcb669");
    value["usage"] = json!({
        "prompt_tokens": 31,
        "completion_tokens": 1,
        "total_tokens": 32,
        "prompt_tokens_details": { "cached_tokens": 0 },
        "completion_tokens_details": { "reasoning_tokens": 1 },
        "prompt_cache_hit_tokens": 0,
        "prompt_cache_miss_tokens": 31
    });

    let parsed = parse_json(value).unwrap();
    let usage = parsed.usage.unwrap();
    assert_eq!(usage.input_tokens().get(), 31);
    assert_eq!(usage.details().cache_read().get(), 0);
    assert_eq!(usage.details().reasoning().get(), 1);
}

#[test]
fn parses_newapi_non_streaming_metadata_and_reasoning_fields() {
    let mut response = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "answer",
            "reasoning_content": "think",
            "reasoning": null
        },
        "finish_reason": "stop"
    })]);
    response["request_id"] = json!("newapi-request-id");

    let parsed = parse_json(response).expect("NewAPI 扩展字段不应破坏非流式响应解析");
    assert!(matches!(
        parsed.choices[0].message.content.as_slice(),
        [ContentBlock::Text(text)] if text == "answer"
    ));
}

#[test]
fn preserves_sparse_choice_order_tool_calls_and_content_filter() {
    let response = parse_json(response_with_choices(vec![
        json!({
            "index": u32::MAX,
            "message": {
                "role": "assistant",
                "content": null,
                "refusal": null,
                "annotations": []
            },
            "finish_reason": "content_filter",
            "logprobs": null
        }),
        json!({
            "index": 2,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": { "name": "lookup", "arguments": "{\"id\":7}" }
                }]
            },
            "finish_reason": "tool_calls"
        }),
    ]))
    .unwrap();

    assert_eq!(response.choices[0].index, u32::MAX);
    assert!(response.choices[0].message.content.is_empty());
    assert_eq!(
        response.choices[0].finish_reason,
        FinishReason::ContentFilter
    );
    assert_eq!(response.choices[1].index, 2);
    assert_eq!(response.choices[1].finish_reason, FinishReason::ToolCalls);
    match &response.choices[1].message.content[0] {
        ContentBlock::ToolUse {
            id,
            name,
            input,
            signature,
        } => {
            assert_eq!(id, "call-1");
            assert_eq!(name, "lookup");
            assert_eq!(input, &json!({ "id": 7 }));
            assert!(signature.is_none());
        }
        _ => panic!("第二个候选应包含工具调用"),
    }
}

#[test]
fn maps_usage_details_and_same_protocol_response_metadata() {
    let mut value = response_with_choices(vec![text_choice(0, "hello")]);
    value["service_tier"] = json!("priority");
    value["system_fingerprint"] = json!("fp-test");
    value["usage"] = json!({
        "prompt_tokens": 12,
        "completion_tokens": 8,
        "total_tokens": 20,
        "prompt_tokens_details": {
            "cached_tokens": 4,
            "audio_tokens": 2,
            "cache_write_tokens": 0
        },
        "completion_tokens_details": {
            "reasoning_tokens": 3,
            "audio_tokens": 1,
            "accepted_prediction_tokens": 0,
            "rejected_prediction_tokens": 0
        }
    });

    let response = parse_json(value).unwrap();
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens().get(), 12);
    assert_eq!(usage.output_tokens().get(), 8);
    assert_eq!(usage.details().cache_read().get(), 4);
    assert_eq!(usage.details().audio_input().get(), 2);
    assert_eq!(usage.details().reasoning().get(), 3);
    assert_eq!(usage.details().audio_output().get(), 1);
    assert_eq!(usage.source(), UsageSource::Upstream);
    assert_eq!(usage.semantics(), UsageSemantics::Inclusive);
    assert_eq!(usage.checked_total_tokens().unwrap().get(), 20);

    let raw = response.raw_passthrough().unwrap();
    assert_eq!(raw.source_protocol(), Protocol::OpenAiChat);
    let fields = raw.fields_for_protocol(Protocol::OpenAiChat).unwrap();
    assert_eq!(fields["service_tier"], "priority");
    assert_eq!(fields["system_fingerprint"], "fp-test");
}

#[test]
fn distinguishes_missing_usage_from_explicit_zero_usage() {
    let missing = parse_json(response_with_choices(vec![text_choice(0, "hello")])).unwrap();
    assert!(missing.usage.is_none());

    let mut zero = response_with_choices(vec![text_choice(0, "hello")]);
    zero["usage"] = json!({
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "total_tokens": 0
    });
    let zero = parse_json(zero).unwrap();
    assert_eq!(zero.usage.unwrap().checked_total_tokens().unwrap().get(), 0);
}

#[test]
fn rejects_duplicate_keys_in_response_and_tool_arguments() {
    let top_level = br#"{
        "id":"first","id":"second","object":"chat.completion","created":1,
        "model":"gpt-test","choices":[]
    }"#;
    assert_eq!(
        parse_response(top_level),
        Err(ParseResponseError::DuplicateKey)
    );

    let nested = br#"{
        "id":"chatcmpl-test","object":"chat.completion","created":1,
        "model":"gpt-test","choices":[{
            "index":0,"index":1,"message":{"role":"assistant","content":"hello"},
            "finish_reason":"stop"
        }]
    }"#;
    assert_eq!(
        parse_response(nested),
        Err(ParseResponseError::DuplicateKey)
    );

    let arguments = br#"{
        "id":"chatcmpl-test","object":"chat.completion","created":1,
        "model":"gpt-test","choices":[{
            "index":0,
            "message":{"role":"assistant","content":null,"tool_calls":[{
                "id":"call-1","type":"function",
                "function":{"name":"lookup","arguments":"{\"id\":1,\"id\":2}"}
            }]},
            "finish_reason":"tool_calls"
        }]
    }"#;
    assert_eq!(
        parse_response(arguments),
        Err(ParseResponseError::DuplicateKey)
    );
}

#[test]
fn rejects_malformed_wrong_object_and_unknown_fields() {
    for body in [b"".as_slice(), b"{".as_slice(), b"{}{}".as_slice(), &[0xff]] {
        assert_eq!(parse_response(body), Err(ParseResponseError::InvalidJson));
    }
    assert_eq!(
        parse_response(br#"["not","an","object"]"#),
        Err(ParseResponseError::InvalidValue)
    );
    assert_eq!(
        parse_response(&vec![b' '; MAX_BODY_BYTES + 1]),
        Err(ParseResponseError::BodyTooLarge)
    );

    let mut wrong_object = response_with_choices(vec![text_choice(0, "hello")]);
    wrong_object["object"] = json!("chat.completion.chunk");
    assert_eq!(
        parse_json(wrong_object),
        Err(ParseResponseError::InvalidValue)
    );

    let mut unknown = response_with_choices(vec![text_choice(0, "hello")]);
    unknown["future_field"] = json!(true);
    assert_eq!(parse_json(unknown), Err(ParseResponseError::InvalidValue));
}

#[test]
fn rejects_known_response_features_without_canonical_mapping() {
    let mut moderation = response_with_choices(vec![text_choice(0, "hello")]);
    moderation["moderation"] = json!({ "input": {} });

    let logprobs = response_with_choices(vec![json!({
        "index": 0,
        "message": { "role": "assistant", "content": "hello" },
        "finish_reason": "stop",
        "logprobs": { "content": [] }
    })]);
    let refusal = response_with_choices(vec![json!({
        "index": 0,
        "message": { "role": "assistant", "content": null, "refusal": "blocked" },
        "finish_reason": "content_filter"
    })]);
    let annotations = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "source",
            "annotations": [{ "type": "url_citation", "url_citation": {} }]
        },
        "finish_reason": "stop"
    })]);
    let legacy_finish = response_with_choices(vec![json!({
        "index": 0,
        "message": { "role": "assistant", "content": null, "function_call": {} },
        "finish_reason": "function_call"
    })]);
    let custom_tool = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1",
                "type": "custom",
                "custom": { "name": "grammar", "input": "text" }
            }]
        },
        "finish_reason": "tool_calls"
    })]);

    for value in [
        moderation,
        logprobs,
        refusal,
        annotations,
        legacy_finish,
        custom_tool,
    ] {
        assert_eq!(
            parse_json(value),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }
}

#[test]
fn validates_choice_indexes_content_and_tool_call_relationships() {
    let duplicate_indexes =
        response_with_choices(vec![text_choice(7, "first"), text_choice(7, "second")]);
    assert_eq!(
        parse_json(duplicate_indexes),
        Err(ParseResponseError::InvalidValue)
    );

    let missing_tool_call = response_with_choices(vec![json!({
        "index": 0,
        "message": { "role": "assistant", "content": "hello" },
        "finish_reason": "tool_calls"
    })]);
    assert_eq!(
        parse_json(missing_tool_call),
        Err(ParseResponseError::InvalidValue)
    );

    let empty_stop = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": null,
            "refusal": null,
            "tool_calls": null
        },
        "finish_reason": "stop"
    })]);
    assert!(
        parse_json(empty_stop).unwrap().choices[0]
            .message
            .content
            .is_empty()
    );

    let duplicate_ids = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {
                    "id": "call-1", "type": "function",
                    "function": { "name": "lookup", "arguments": "{}" }
                },
                {
                    "id": "call-1", "type": "function",
                    "function": { "name": "lookup", "arguments": "{}" }
                }
            ]
        },
        "finish_reason": "tool_calls"
    })]);
    assert_eq!(
        parse_json(duplicate_ids),
        Err(ParseResponseError::InvalidValue)
    );

    for arguments in ["[]", "not-json"] {
        let invalid_arguments = response_with_choices(vec![json!({
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-1", "type": "function",
                    "function": { "name": "lookup", "arguments": arguments }
                }]
            },
            "finish_reason": "tool_calls"
        })]);
        assert_eq!(
            parse_json(invalid_arguments),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }
}

#[test]
fn treats_explicit_empty_tool_calls_as_no_tool_call() {
    let response = parse_json(response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "hello",
            "tool_calls": []
        },
        "finish_reason": "stop"
    })]))
    .unwrap();
    assert!(matches!(
        response.choices[0].message.content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
}

#[test]
fn validates_usage_totals_details_and_unsupported_dimensions() {
    let usage_response = |usage: Value| {
        let mut value = response_with_choices(vec![text_choice(0, "hello")]);
        value["usage"] = usage;
        value
    };

    for usage in [
        json!({ "prompt_tokens": -1, "completion_tokens": 1, "total_tokens": 0 }),
        json!({ "prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 6 }),
        json!({
            "prompt_tokens": 2,
            "completion_tokens": 1,
            "total_tokens": 3,
            "prompt_tokens_details": { "cached_tokens": 3 }
        }),
        json!({
            "prompt_tokens": 1,
            "completion_tokens": 2,
            "total_tokens": 3,
            "completion_tokens_details": { "reasoning_tokens": 3 }
        }),
    ] {
        assert_eq!(
            parse_json(usage_response(usage)),
            Err(ParseResponseError::InvalidValue)
        );
    }

    for usage in [
        json!({
            "prompt_tokens": 2,
            "completion_tokens": 1,
            "total_tokens": 3,
            "prompt_tokens_details": { "cache_write_tokens": 1 }
        }),
        json!({
            "prompt_tokens": 1,
            "completion_tokens": 2,
            "total_tokens": 3,
            "completion_tokens_details": { "accepted_prediction_tokens": 1 }
        }),
    ] {
        assert_eq!(
            parse_json(usage_response(usage)),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }

    let unknown_detail = usage_response(json!({
        "prompt_tokens": 1,
        "completion_tokens": 1,
        "total_tokens": 2,
        "prompt_tokens_details": { "future_tokens": 0 }
    }));
    assert_eq!(
        parse_json(unknown_detail),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn enforces_response_choice_text_and_argument_budgets() {
    let choices = (0..129)
        .map(|index| text_choice(index, "x"))
        .collect::<Vec<_>>();
    assert_eq!(
        parse_json(response_with_choices(choices)),
        Err(ParseResponseError::StructureLimitExceeded)
    );

    let large_text = response_with_choices(vec![text_choice(0, &"x".repeat(1024 * 1024 + 1))]);
    assert_eq!(
        parse_json(large_text),
        Err(ParseResponseError::StructureLimitExceeded)
    );

    let large_arguments = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "lookup",
                    "arguments": serde_json::to_string(&json!({
                        "data": "x".repeat(256 * 1024)
                    })).unwrap()
                }
            }]
        },
        "finish_reason": "tool_calls"
    })]);
    assert_eq!(
        parse_json(large_arguments),
        Err(ParseResponseError::StructureLimitExceeded)
    );
}

#[test]
fn redacts_success_and_error_debug_output() {
    let mut value = response_with_choices(vec![json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "text-canary-41aa",
            "tool_calls": [{
                "id": "call-canary-41aa",
                "type": "function",
                "function": {
                    "name": "tool_canary_41aa",
                    "arguments": "{\"secret-canary-41aa\":true}"
                }
            }]
        },
        "finish_reason": "tool_calls"
    })]);
    value["id"] = json!("id-canary-41aa");
    value["model"] = json!("model-canary-41aa");
    value["system_fingerprint"] = json!("fingerprint-canary-41aa");
    let response = parse_json(value).unwrap();

    let success = format!(
        "{response:?}|{:?}|{:?}|{:?}",
        response.choices[0], response.choices[0].message, response.choices[0].message.content[1]
    );
    for canary in [
        "id-canary-41aa",
        "model-canary-41aa",
        "text-canary-41aa",
        "call-canary-41aa",
        "tool_canary_41aa",
        "secret-canary-41aa",
        "fingerprint-canary-41aa",
    ] {
        assert!(!success.contains(canary));
    }
    assert!(success.contains("<已脱敏>"));

    let error =
        parse_response(br#"{"id":"error-canary-41aa","future-canary-41aa":true}"#).unwrap_err();
    let rendered = format!("{error:?}|{error}");
    assert!(!rendered.contains("canary-41aa"));
    assert!(error.source().is_none());
}
