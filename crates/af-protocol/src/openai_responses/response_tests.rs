use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value, json};

use super::{
    BuildResponseError, MAX_BODY_BYTES, ParseResponseError, build_response, parse_response,
};
use crate::{
    CacheHint, CanonicalResponse, ContentBlock, FinishReason, Message, ProtocolCapability,
    ResponseCapability, ResponseChoice, TokenCount, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};

fn assert_unsupported_capability(response: &CanonicalResponse, expected: ResponseCapability) {
    let error = build_response(response).expect_err("响应能力必须被明确拒绝");
    let BuildResponseError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::OpenAiResponses);
    assert_eq!(error.capability(), ProtocolCapability::Response(expected));
}

fn parse_json(value: Value) -> Result<CanonicalResponse, ParseResponseError> {
    parse_response(&serde_json::to_vec(&value).unwrap())
}

fn response(output: Vec<Value>, status: &str) -> Value {
    json!({
        "id": "resp_test_1",
        "object": "response",
        "created_at": 1_700_000_000,
        "status": status,
        "error": null,
        "incomplete_details": null,
        "model": "gpt-test",
        "output": output,
        "usage": {
            "input_tokens": 10,
            "input_tokens_details": {
                "cache_write_tokens": 0,
                "cached_tokens": 0
            },
            "output_tokens": 4,
            "output_tokens_details": { "reasoning_tokens": 0 },
            "total_tokens": 14
        }
    })
}

fn message(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "annotations": [],
            "logprobs": []
        }]
    })
}

fn count(value: i64) -> TokenCount {
    TokenCount::new(value).unwrap()
}

fn usage(
    input: i64,
    output: i64,
    cache_read: i64,
    cache_5m: i64,
    cache_1h: i64,
    reasoning: i64,
    semantics: UsageSemantics,
) -> Usage {
    Usage::new(
        count(input),
        count(output),
        UsageDetails::new(
            count(cache_read),
            count(cache_5m),
            count(cache_1h),
            count(reasoning),
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        semantics,
    )
    .unwrap()
}

fn canonical_response(choice: ResponseChoice, usage: Option<Usage>) -> CanonicalResponse {
    CanonicalResponse::new(
        Operation::Responses,
        "resp_test_1".to_owned(),
        "gpt-test".to_owned(),
        Some(1_700_000_000),
        vec![choice],
        usage,
    )
}

#[test]
fn parses_minimal_completed_response() {
    let parsed = parse_json(response(
        vec![message("msg_test_1", "hello", "completed")],
        "completed",
    ))
    .unwrap();

    assert_eq!(parsed.operation, Operation::Responses);
    assert_eq!(parsed.id, "resp_test_1");
    assert_eq!(parsed.model, "gpt-test");
    assert_eq!(parsed.created_at, Some(1_700_000_000));
    assert_eq!(parsed.choices.len(), 1);
    assert_eq!(parsed.choices[0].index, 0);
    assert_eq!(parsed.choices[0].message.role, Role::Assistant);
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::Stop);
    assert!(matches!(
        parsed.choices[0].message.content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
    let usage = parsed.usage.unwrap();
    assert_eq!(usage.input_tokens().get(), 10);
    assert_eq!(usage.output_tokens().get(), 4);
    assert_eq!(usage.semantics(), UsageSemantics::Inclusive);
    assert_eq!(usage.checked_total_tokens().unwrap().get(), 14);
    assert!(parsed.raw_passthrough().is_none());
}

#[test]
fn round_trips_compaction_output_before_message() {
    let parsed = parse_json(response(
        vec![
            json!({
                "type": "compaction",
                "id": "cmp_output_1",
                "encrypted_content": "opaque-compaction"
            }),
            message("msg_output_1", "hello", "completed"),
        ],
        "completed",
    ))
    .unwrap();

    assert!(matches!(
        &parsed.choices[0].message.content[0],
        ContentBlock::Compaction(item)
            if item.as_value() == &json!({
                "type": "compaction",
                "id": "cmp_output_1",
                "encrypted_content": "opaque-compaction"
            })
    ));
    let rebuilt = build_response(&parsed).unwrap();
    assert_eq!(
        rebuilt["output"][0],
        json!({
            "type": "compaction",
            "id": "cmp_output_1",
            "encrypted_content": "opaque-compaction"
        })
    );
    assert_eq!(rebuilt["output"][1]["type"], "message");
}

#[test]
fn rejects_compaction_after_regular_output() {
    assert_eq!(
        parse_json(response(
            vec![
                message("msg_output_1", "hello", "completed"),
                json!({
                    "type": "compaction",
                    "id": "cmp_output_1",
                    "encrypted_content": "opaque-compaction"
                }),
            ],
            "completed",
        )),
        Err(ParseResponseError::UnsupportedFeature)
    );
}

#[test]
fn accepts_final_answer_phase_and_rejects_commentary() {
    let mut final_answer = response(
        vec![message("msg_final", "answer", "completed")],
        "completed",
    );
    final_answer["output"][0]["phase"] = json!("final_answer");
    let parsed = parse_json(final_answer).unwrap();
    let rebuilt = build_response(&parsed).unwrap();
    assert_eq!(rebuilt["output"][0]["phase"], "final_answer");

    let mut commentary = response(
        vec![message("msg_commentary", "working", "completed")],
        "completed",
    );
    commentary["output"][0]["phase"] = json!("commentary");
    assert_eq!(
        parse_json(commentary),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let mut final_with_tool = response(
        vec![
            message("msg_final_with_tool", "answer", "completed"),
            json!({
                "id": "fc_after_final",
                "type": "function_call",
                "status": "completed",
                "call_id": "call_after_final",
                "name": "lookup",
                "arguments": "{}",
            }),
        ],
        "completed",
    );
    final_with_tool["output"][0]["phase"] = json!("final_answer");
    assert_eq!(
        parse_json(final_with_tool),
        Err(ParseResponseError::UnsupportedFeature)
    );
}

#[test]
fn round_trips_reasoning_text_tools_usage_and_same_protocol_echoes() {
    let mut value = response(
        vec![
            json!({
                "id": "rs_test_1",
                "type": "reasoning",
                "status": "completed",
                "summary": [{ "type": "summary_text", "text": "plan" }],
                "content": [],
                "encrypted_content": "encrypted-1"
            }),
            message("msg_test_1", "calling", "completed"),
            json!({
                "id": "fc_test_1",
                "type": "function_call",
                "status": "completed",
                "call_id": "call_test_1",
                "name": "lookup",
                "arguments": "{\"id\":7}"
            }),
        ],
        "completed",
    );
    value["usage"] = json!({
        "input_tokens": 20,
        "input_tokens_details": {
            "cache_write_tokens": 0,
            "cached_tokens": 4
        },
        "output_tokens": 8,
        "output_tokens_details": { "reasoning_tokens": 3 },
        "total_tokens": 28
    });
    add_official_echoes(&mut value);

    let parsed = parse_json(value).unwrap();
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::ToolCalls);
    assert!(matches!(
        &parsed.choices[0].message.content[0],
        ContentBlock::Thinking { text, signature }
            if text == "plan" && signature.as_deref() == Some("encrypted-1")
    ));
    assert!(matches!(
        &parsed.choices[0].message.content[2],
        ContentBlock::ToolUse { id, name, input, signature }
            if id == "call_test_1"
                && name == "lookup"
                && input == &json!({ "id": 7 })
                && signature.is_none()
    ));
    let usage = parsed.usage.unwrap();
    assert_eq!(usage.details().cache_read().get(), 4);
    assert_eq!(usage.details().reasoning().get(), 3);
    let raw = parsed.raw_passthrough().unwrap();
    let fields = raw.fields_for_protocol(Protocol::OpenAiResponses).unwrap();
    assert_eq!(fields["completed_at"], 1_700_000_001);
    assert_eq!(fields["service_tier"], "default");

    let rebuilt = build_response(&parsed).unwrap();
    assert_eq!(rebuilt["output"][0]["id"], "rs_test_1_0");
    assert_eq!(rebuilt["output"][1]["id"], "msg_test_1_1");
    assert_eq!(rebuilt["output"][2]["id"], "fc_test_1_2");
    assert_eq!(rebuilt["output"][2]["call_id"], "call_test_1");
    assert_eq!(parse_json(rebuilt).unwrap(), parsed);
}

#[test]
fn accepts_response_tool_echo_without_optional_strictness() {
    let mut value = response(
        vec![message("msg_optional_strict", "answer", "completed")],
        "completed",
    );
    add_official_echoes(&mut value);
    value["tools"][0]
        .as_object_mut()
        .expect("工具定义必须是对象")
        .remove("strict");

    let parsed = parse_json(value).expect("供应商可省略函数工具的可选 strict 字段");
    assert_eq!(
        parsed.choices[0].message.content[0],
        ContentBlock::Text("answer".to_owned())
    );
}

fn add_official_echoes(value: &mut Value) {
    value["background"] = json!(false);
    value["completed_at"] = json!(1_700_000_001_i64);
    value["conversation"] = Value::Null;
    value["instructions"] = json!("answer briefly");
    value["max_output_tokens"] = json!(128);
    value["max_tool_calls"] = Value::Null;
    value["metadata"] = json!({ "trace": "test" });
    value["parallel_tool_calls"] = json!(true);
    value["previous_response_id"] = Value::Null;
    value["prompt"] = Value::Null;
    value["prompt_cache_key"] = json!("cache-test-1");
    value["prompt_cache_options"] = json!({ "mode": "implicit", "ttl": "30m" });
    value["prompt_cache_retention"] = json!("24h");
    value["reasoning"] = json!({ "effort": "high", "summary": null });
    value["safety_identifier"] = json!("safe-test-1");
    value["service_tier"] = json!("default");
    value["store"] = json!(true);
    value["temperature"] = json!(1.0);
    value["text"] = json!({ "format": { "type": "text" } });
    value["tool_choice"] = json!("auto");
    value["tools"] = json!([{
        "type": "function",
        "name": "lookup",
        "description": "lookup",
        "parameters": {
            "type": "object",
            "properties": { "id": { "type": "integer" } }
        },
        "strict": true
    }]);
    value["top_logprobs"] = json!(0);
    value["top_p"] = json!(1.0);
    value["truncation"] = json!("disabled");
    value["user"] = Value::Null;
}

#[test]
fn maps_incomplete_reasons_without_inventing_stop_sequences() {
    for (reason, expected) in [
        ("max_output_tokens", FinishReason::Length),
        ("content_filter", FinishReason::ContentFilter),
    ] {
        let mut value = response(
            vec![message("msg_partial", "partial", "incomplete")],
            "incomplete",
        );
        value["incomplete_details"] = json!({ "reason": reason });
        let parsed = parse_json(value).unwrap();
        assert_eq!(parsed.choices[0].finish_reason, expected);
        assert_eq!(parsed.choices[0].stop_sequence, None);
        let rebuilt = build_response(&parsed).unwrap();
        assert_eq!(rebuilt["status"], "incomplete");
        assert_eq!(rebuilt["incomplete_details"]["reason"], reason);
        assert_eq!(rebuilt["output"][0]["status"], "incomplete");
    }
}

#[test]
fn rejects_non_terminal_failures_and_unmodeled_output_features() {
    let mut failed = response(vec![], "failed");
    failed["error"] = json!({ "code": "server_error", "message": "hidden" });
    assert_eq!(
        parse_json(failed),
        Err(ParseResponseError::UnsupportedFeature)
    );
    assert_eq!(
        parse_json(response(vec![], "in_progress")),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let cases = [
        response(
            vec![json!({
                "id": "msg_refusal",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "refusal", "refusal": "no" }]
            })],
            "completed",
        ),
        response(
            vec![json!({
                "id": "msg_citation",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "source",
                    "annotations": [{ "type": "url_citation" }]
                }]
            })],
            "completed",
        ),
        response(
            vec![json!({
                "id": "msg_logs",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "logs",
                    "logprobs": [{ "token": "x" }]
                }]
            })],
            "completed",
        ),
        response(
            vec![json!({
                "id": "rs_content",
                "type": "reasoning",
                "status": "completed",
                "summary": [],
                "content": [{ "type": "reasoning_text", "text": "private" }]
            })],
            "completed",
        ),
        response(
            vec![json!({
                "id": "ws_test",
                "type": "web_search_call",
                "status": "completed"
            })],
            "completed",
        ),
    ];
    for value in cases {
        assert_eq!(
            parse_json(value),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }
}

#[test]
fn rejects_malformed_duplicate_unknown_and_oversized_responses() {
    assert_eq!(parse_response(b""), Err(ParseResponseError::InvalidJson));
    assert_eq!(
        parse_response(b"{}{}"),
        Err(ParseResponseError::InvalidJson)
    );
    assert_eq!(
        parse_response(br#"{"id":"a","id":"b"}"#),
        Err(ParseResponseError::DuplicateKey)
    );
    assert_eq!(
        parse_response(&vec![b' '; MAX_BODY_BYTES + 1]),
        Err(ParseResponseError::BodyTooLarge)
    );
    let mut unknown = response(vec![], "completed");
    unknown["future_field"] = json!(true);
    assert_eq!(parse_json(unknown), Err(ParseResponseError::InvalidValue));
}

#[test]
fn validates_item_order_identity_status_and_function_arguments() {
    let duplicate_items = response(
        vec![
            message("msg_duplicate", "one", "completed"),
            message("msg_duplicate", "two", "completed"),
        ],
        "completed",
    );
    assert_eq!(
        parse_json(duplicate_items),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let reversed = response(
        vec![
            message("msg_first", "text", "completed"),
            json!({
                "id": "rs_after",
                "type": "reasoning",
                "status": "completed",
                "summary": []
            }),
        ],
        "completed",
    );
    assert_eq!(
        parse_json(reversed),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let duplicate_calls = response(
        vec![
            json!({
                "type": "function_call", "call_id": "call_same", "name": "a",
                "arguments": "{}", "status": "completed"
            }),
            json!({
                "type": "function_call", "call_id": "call_same", "name": "b",
                "arguments": "{}", "status": "completed"
            }),
        ],
        "completed",
    );
    assert_eq!(
        parse_json(duplicate_calls),
        Err(ParseResponseError::InvalidValue)
    );

    let invalid_arguments = response(
        vec![json!({
            "type": "function_call", "call_id": "call_bad", "name": "lookup",
            "arguments": "not-json", "status": "completed"
        })],
        "completed",
    );
    assert_eq!(
        parse_json(invalid_arguments),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let mut wrong_status = response(
        vec![message("msg_wrong", "text", "incomplete")],
        "completed",
    );
    wrong_status["incomplete_details"] = Value::Null;
    assert_eq!(
        parse_json(wrong_status),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn validates_usage_totals_cache_writes_and_reasoning_bounds() {
    let usage_response = |usage: Value| {
        let mut value = response(vec![], "completed");
        value["usage"] = usage;
        value
    };
    let mismatch = usage_response(json!({
        "input_tokens": 3,
        "output_tokens": 2,
        "total_tokens": 6
    }));
    assert_eq!(parse_json(mismatch), Err(ParseResponseError::InvalidValue));

    let cache_write = usage_response(json!({
        "input_tokens": 3,
        "input_tokens_details": { "cached_tokens": 0, "cache_write_tokens": 1 },
        "output_tokens": 2,
        "total_tokens": 5
    }));
    assert_eq!(
        parse_json(cache_write),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let reasoning = usage_response(json!({
        "input_tokens": 3,
        "output_tokens": 2,
        "output_tokens_details": { "reasoning_tokens": 3 },
        "total_tokens": 5
    }));
    assert_eq!(parse_json(reasoning), Err(ParseResponseError::InvalidValue));

    let mut alias_only = response(
        vec![message("msg_usage_alias", "ok", "completed")],
        "completed",
    );
    alias_only["usage"] = json!({
        "input_tokens": 3,
        "prompt_tokens_details": { "cached_tokens": 2 },
        "output_tokens": 2,
        "total_tokens": 5
    });
    assert_eq!(
        parse_json(alias_only)
            .unwrap()
            .usage
            .unwrap()
            .details()
            .cache_read(),
        count(2)
    );

    let mut conflicting_alias = response(
        vec![message("msg_usage_conflict", "ok", "completed")],
        "completed",
    );
    conflicting_alias["usage"] = json!({
        "input_tokens": 3,
        "input_tokens_details": { "cached_tokens": 1 },
        "prompt_tokens_details": { "cached_tokens": 2 },
        "output_tokens": 2,
        "total_tokens": 5
    });
    assert_eq!(
        parse_json(conflicting_alias),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn builds_deterministic_official_shape_and_round_trips_inclusive_usage() {
    let choice = ResponseChoice::new(
        0,
        Message::new(
            Role::Assistant,
            vec![
                ContentBlock::Thinking {
                    text: "plan".to_owned(),
                    signature: Some("encrypted-1".to_owned()),
                },
                ContentBlock::Text("calling".to_owned()),
                ContentBlock::ToolUse {
                    id: "call_test_1".to_owned(),
                    name: "lookup".to_owned(),
                    input: json!({ "id": 7 }),
                    signature: None,
                },
            ],
        ),
        FinishReason::ToolCalls,
    );
    let response = canonical_response(
        choice,
        Some(usage(20, 8, 4, 0, 0, 3, UsageSemantics::Inclusive)),
    );
    let built = build_response(&response).unwrap();

    assert_eq!(built["object"], "response");
    assert_eq!(built["status"], "completed");
    assert_eq!(built["error"], Value::Null);
    assert_eq!(built["incomplete_details"], Value::Null);
    assert_eq!(built["output"][0]["id"], "rs_test_1_0");
    assert_eq!(built["output"][1]["id"], "msg_test_1_1");
    assert_eq!(built["output"][1]["phase"], Value::Null);
    assert_eq!(built["output"][2]["id"], "fc_test_1_2");
    assert_eq!(built["output"][2]["call_id"], "call_test_1");
    assert_eq!(built["usage"]["input_tokens"], 20);
    assert_eq!(built["usage"]["input_tokens_details"]["cached_tokens"], 4);
    assert_eq!(
        built["usage"]["output_tokens_details"]["reasoning_tokens"],
        3
    );
    assert_eq!(parse_json(built).unwrap(), response);
}

#[test]
fn build_response_revalidates_choice_content_usage_and_raw_origin() {
    let valid_choice =
        ResponseChoice::new(0, Message::new(Role::Assistant, vec![]), FinishReason::Stop);
    let valid_usage = usage(1, 1, 0, 0, 0, 0, UsageSemantics::Inclusive);

    let mut wrong_operation = canonical_response(valid_choice.clone(), Some(valid_usage));
    wrong_operation.operation = Operation::Chat;
    assert_eq!(
        build_response(&wrong_operation),
        Err(BuildResponseError::UnsupportedOperation)
    );

    let mut missing_created = canonical_response(valid_choice.clone(), Some(valid_usage));
    missing_created.created_at = None;
    assert_eq!(
        build_response(&missing_created),
        Err(BuildResponseError::InvalidValue)
    );

    let mut multiple = canonical_response(valid_choice.clone(), Some(valid_usage));
    multiple.choices.push(valid_choice.clone());
    assert_unsupported_capability(&multiple, ResponseCapability::MultipleChoices);

    let mut stopped = canonical_response(valid_choice.clone(), Some(valid_usage));
    stopped.choices[0].stop_sequence = Some("END".to_owned());
    assert_unsupported_capability(&stopped, ResponseCapability::StopSequence);

    let mismatched_tool = canonical_response(
        ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "call_test_1".to_owned(),
                    name: "lookup".to_owned(),
                    input: json!({}),
                    signature: None,
                }],
            ),
            FinishReason::Stop,
        ),
        Some(valid_usage),
    );
    assert_eq!(
        build_response(&mismatched_tool),
        Err(BuildResponseError::InvalidValue)
    );

    let reordered = canonical_response(
        ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![
                    ContentBlock::ToolUse {
                        id: "call_test_1".to_owned(),
                        name: "lookup".to_owned(),
                        input: json!({}),
                        signature: None,
                    },
                    ContentBlock::Text("late".to_owned()),
                ],
            ),
            FinishReason::ToolCalls,
        ),
        Some(valid_usage),
    );
    assert_eq!(
        build_response(&reordered),
        Err(BuildResponseError::UnsupportedFeature)
    );

    let cache_write = canonical_response(
        valid_choice.clone(),
        Some(usage(3, 1, 0, 1, 0, 0, UsageSemantics::CacheSeparated)),
    );
    assert_unsupported_capability(
        &cache_write,
        ResponseCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
    );

    let foreign_raw = canonical_response(valid_choice, Some(valid_usage))
        .with_validated_raw_passthrough(Some((Protocol::OpenAiChat, Map::new())));
    assert_eq!(
        build_response(&foreign_raw),
        Err(BuildResponseError::RawProtocolMismatch)
    );
}

#[test]
fn rejects_invalid_or_unsupported_same_protocol_echoes() {
    let mut background = response(vec![], "completed");
    background["background"] = json!(true);
    assert_eq!(
        parse_json(background),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let mut nonzero_tool_usage = response(vec![], "completed");
    nonzero_tool_usage["tool_usage"] = json!({
        "web_search": { "num_requests": 1 }
    });
    assert!(parse_json(nonzero_tool_usage).is_ok());

    let mut malformed_tool_usage = response(vec![], "completed");
    malformed_tool_usage["tool_usage"] = json!({
        "web_search": { "num_requests": "0" }
    });
    assert_eq!(
        parse_json(malformed_tool_usage),
        Err(ParseResponseError::InvalidValue)
    );

    let mut out_of_range_penalty = response(vec![], "completed");
    out_of_range_penalty["frequency_penalty"] = json!(2.1);
    assert_eq!(
        parse_json(out_of_range_penalty),
        Err(ParseResponseError::InvalidValue)
    );

    let mut early_completion = response(vec![], "completed");
    early_completion["completed_at"] = json!(1_699_999_999_i64);
    assert_eq!(
        parse_json(early_completion),
        Err(ParseResponseError::InvalidValue)
    );

    let mut unsafe_metadata = response(vec![], "completed");
    unsafe_metadata["metadata"] = json!({ "authorization": "secret" });
    assert_eq!(
        parse_json(unsafe_metadata),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn redacts_response_and_error_debug_output() {
    let mut value = response(
        vec![json!({
            "type": "function_call",
            "id": "fc-canary-51aa",
            "call_id": "call-canary-51aa",
            "name": "tool_canary_51aa",
            "arguments": "{\"secret-canary-51aa\":true}",
            "status": "completed"
        })],
        "completed",
    );
    value["id"] = json!("resp-canary-51aa");
    value["model"] = json!("model-canary-51aa");
    let parsed = parse_json(value).unwrap();
    let rendered = format!(
        "{parsed:?}|{:?}|{:?}",
        parsed.choices[0], parsed.choices[0].message.content[0]
    );
    assert!(!rendered.contains("canary-51aa"));
    assert!(rendered.contains("<已脱敏>"));

    let error = parse_response(br#"{"id":"error-canary-51aa","future":true}"#).unwrap_err();
    let rendered = format!("{error:?}|{error}");
    assert!(!rendered.contains("canary-51aa"));
    assert!(error.source().is_none());
}
#[test]
fn accepts_standard_provider_tool_echo() {
    let mut value = response(vec![], "completed");
    value["tools"] = json!([{
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
    value["tool_usage"] = json!({
        "image_gen": {
            "input_tokens": 0,
            "input_tokens_details": {"image_tokens": 0, "text_tokens": 0},
            "output_tokens": 0,
            "output_tokens_details": {"image_tokens": 0, "text_tokens": 0},
            "total_tokens": 0
        },
        "web_search": {"num_requests": 0}
    });
    assert!(parse_json(value).is_ok());
}
