use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value, json};

use super::{
    BuildResponseError, MAX_BODY_BYTES, ParseResponseError, build_response, parse_response,
};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, Message, ProtocolCapability, ResponseCapability,
    ResponseChoice, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
};

fn assert_unsupported_capability(response: &CanonicalResponse, expected: ResponseCapability) {
    let error = build_response(response).expect_err("响应能力必须被明确拒绝");
    let BuildResponseError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::Anthropic);
    assert_eq!(error.capability(), ProtocolCapability::Response(expected));
}

fn parse_json(value: Value) -> Result<CanonicalResponse, ParseResponseError> {
    parse_response(&serde_json::to_vec(&value).unwrap())
}

fn response(content: Vec<Value>, stop_reason: &str) -> Value {
    json!({
        "id": "msg_test_1",
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": "claude-test",
        "container": null,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "stop_details": null,
        "usage": {
            "input_tokens": 10,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
            "cache_creation": null,
            "output_tokens": 4,
            "output_tokens_details": null,
            "inference_geo": null,
            "server_tool_use": null,
            "service_tier": null
        }
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
        Operation::Chat,
        "msg_test_1".to_owned(),
        "claude-test".to_owned(),
        Some(1_700_000_000),
        vec![choice],
        usage,
    )
}

#[test]
fn parses_minimal_official_response_without_fabricating_created_at() {
    let parsed = parse_json(response(
        vec![json!({ "type": "text", "text": "hello", "citations": null })],
        "end_turn",
    ))
    .unwrap();

    assert_eq!(parsed.operation, Operation::Chat);
    assert_eq!(parsed.id, "msg_test_1");
    assert_eq!(parsed.model, "claude-test");
    assert_eq!(parsed.created_at, None);
    assert_eq!(parsed.choices.len(), 1);
    assert_eq!(parsed.choices[0].index, 0);
    assert_eq!(parsed.choices[0].message.role, Role::Assistant);
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::Stop);
    assert_eq!(parsed.choices[0].stop_sequence, None);
    assert!(matches!(
        parsed.choices[0].message.content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
    let usage = parsed.usage.unwrap();
    assert_eq!(usage.input_tokens().get(), 10);
    assert_eq!(usage.output_tokens().get(), 4);
    assert_eq!(usage.semantics(), UsageSemantics::CacheSeparated);
    assert_eq!(usage.checked_total_tokens().unwrap().get(), 14);
    assert!(parsed.raw_passthrough().is_none());
}

#[test]
fn parses_deepseek_anthropic_response_shape() {
    let parsed = parse_json(json!({
        "id": "msg_deepseek_test",
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "thinking",
            "thinking": "We",
            "signature": "845129ed-bc00-4e0b-8d44-fcdd97797fb1"
        }],
        "model": "deepseek-v4-flash",
        "stop_reason": "max_tokens",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 7,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
            "output_tokens": 1,
            "service_tier": "standard"
        }
    }))
    .unwrap();

    assert_eq!(parsed.id, "msg_deepseek_test");
    assert_eq!(parsed.model, "deepseek-v4-flash");
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::Length);
    assert_eq!(
        parsed.usage.unwrap().checked_total_tokens().unwrap().get(),
        8
    );
}

#[test]
fn round_trips_thinking_tools_cache_usage_and_same_protocol_metadata() {
    let mut value = response(
        vec![
            json!({
                "type": "thinking",
                "thinking": "plan",
                "signature": "sig-1"
            }),
            json!({
                "type": "tool_use",
                "id": "toolu_1",
                "name": "lookup",
                "input": { "id": 7 },
                "caller": { "type": "direct" }
            }),
        ],
        "tool_use",
    );
    value["usage"] = json!({
        "input_tokens": 10,
        "cache_creation_input_tokens": 5,
        "cache_read_input_tokens": 4,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 3,
            "ephemeral_1h_input_tokens": 2
        },
        "output_tokens": 8,
        "output_tokens_details": { "thinking_tokens": 3 },
        "inference_geo": "us",
        "server_tool_use": {
            "web_fetch_requests": 0,
            "web_search_requests": 0
        },
        "service_tier": "standard"
    });

    let parsed = parse_json(value).unwrap();
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::ToolCalls);
    assert!(matches!(
        &parsed.choices[0].message.content[0],
        ContentBlock::Thinking { text, signature }
            if text == "plan" && signature.as_deref() == Some("sig-1")
    ));
    assert!(matches!(
        &parsed.choices[0].message.content[1],
        ContentBlock::ToolUse {
            id,
            name,
            input,
            ..
        }
            if id == "toolu_1" && name == "lookup" && input == &json!({ "id": 7 })
    ));
    let usage = parsed.usage.unwrap();
    assert_eq!(usage.details().cache_read().get(), 4);
    assert_eq!(usage.details().cache_creation_5m().get(), 3);
    assert_eq!(usage.details().cache_creation_1h().get(), 2);
    assert_eq!(usage.details().reasoning().get(), 3);
    assert_eq!(usage.checked_total_tokens().unwrap().get(), 27);

    let raw = parsed.raw_passthrough().unwrap();
    let fields = raw.fields_for_protocol(Protocol::Anthropic).unwrap();
    assert_eq!(fields["usage"]["inference_geo"], "us");
    assert_eq!(fields["usage"]["service_tier"], "standard");

    let rebuilt = build_response(&parsed).unwrap();
    assert_eq!(rebuilt["usage"]["inference_geo"], "us");
    assert_eq!(rebuilt["usage"]["service_tier"], "standard");
    assert_eq!(rebuilt["usage"]["cache_creation_input_tokens"], 5);
    assert_eq!(rebuilt["content"][1]["caller"]["type"], "direct");
    assert_eq!(parse_json(rebuilt).unwrap(), parsed);
}

#[test]
fn preserves_stop_sequence_and_maps_context_window_to_length() {
    let mut stopped = response(
        vec![json!({ "type": "text", "text": "done" })],
        "stop_sequence",
    );
    stopped["stop_sequence"] = json!("END");
    let stopped = parse_json(stopped).unwrap();
    assert_eq!(stopped.choices[0].finish_reason, FinishReason::Stop);
    assert_eq!(stopped.choices[0].stop_sequence.as_deref(), Some("END"));
    let rebuilt = build_response(&stopped).unwrap();
    assert_eq!(rebuilt["stop_reason"], "stop_sequence");
    assert_eq!(rebuilt["stop_sequence"], "END");

    let context = parse_json(response(
        vec![json!({ "type": "text", "text": "partial" })],
        "model_context_window_exceeded",
    ))
    .unwrap();
    assert_eq!(context.choices[0].finish_reason, FinishReason::Length);
    assert_eq!(
        build_response(&context).unwrap()["stop_reason"],
        "max_tokens"
    );
}

#[test]
fn preserves_validated_refusal_details_only_for_anthropic_round_trip() {
    let mut value = response(
        vec![json!({ "type": "text", "text": "cannot help" })],
        "refusal",
    );
    value["stop_details"] = json!({
        "type": "refusal",
        "category": "cyber",
        "explanation": "policy"
    });
    let parsed = parse_json(value).unwrap();
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::ContentFilter);
    let rebuilt = build_response(&parsed).unwrap();
    assert_eq!(rebuilt["stop_details"]["category"], "cyber");
    assert_eq!(rebuilt["stop_details"]["explanation"], "policy");
}

#[test]
fn rejects_known_unmodeled_official_features() {
    let cases = [
        {
            let mut value = response(vec![], "end_turn");
            value["container"] = json!({ "id": "container_1", "expires_at": "tomorrow" });
            value
        },
        response(
            vec![json!({ "type": "redacted_thinking", "data": "ciphertext" })],
            "end_turn",
        ),
        response(
            vec![json!({
                "type": "server_tool_use",
                "id": "srvtoolu_1",
                "name": "web_search",
                "input": {},
                "caller": { "type": "direct" }
            })],
            "tool_use",
        ),
        response(
            vec![json!({
                "type": "text",
                "text": "source",
                "citations": [{ "type": "char_location" }]
            })],
            "end_turn",
        ),
        response(
            vec![json!({
                "type": "tool_use",
                "id": "toolu_1",
                "name": "lookup",
                "input": {},
                "caller": { "type": "code_execution_20260120", "tool_id": "srv_1" }
            })],
            "tool_use",
        ),
        response(
            vec![json!({ "type": "text", "text": "wait" })],
            "pause_turn",
        ),
    ];
    for value in cases {
        assert_eq!(
            parse_json(value),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }

    let mut server_usage = response(vec![], "end_turn");
    server_usage["usage"]["server_tool_use"] = json!({
        "web_fetch_requests": 0,
        "web_search_requests": 1
    });
    assert_eq!(
        parse_json(server_usage),
        Err(ParseResponseError::UnsupportedFeature)
    );
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

    let mut unknown = response(vec![], "end_turn");
    unknown["future_field"] = json!(true);
    assert_eq!(parse_json(unknown), Err(ParseResponseError::InvalidValue));

    let oversized = response(
        vec![json!({ "type": "text", "text": "x".repeat(1024 * 1024 + 1) })],
        "end_turn",
    );
    assert_eq!(
        parse_json(oversized),
        Err(ParseResponseError::StructureLimitExceeded)
    );
}

#[test]
fn validates_stop_tool_and_usage_relationships() {
    let mut missing_sequence = response(vec![], "stop_sequence");
    missing_sequence["stop_sequence"] = Value::Null;
    assert_eq!(
        parse_json(missing_sequence),
        Err(ParseResponseError::InvalidValue)
    );

    let mut stray_sequence = response(vec![], "end_turn");
    stray_sequence["stop_sequence"] = json!("END");
    assert_eq!(
        parse_json(stray_sequence),
        Err(ParseResponseError::InvalidValue)
    );

    let missing_tool = response(vec![json!({ "type": "text", "text": "x" })], "tool_use");
    assert_eq!(
        parse_json(missing_tool),
        Err(ParseResponseError::InvalidValue)
    );

    let stray_tool = response(
        vec![json!({
            "type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}
        })],
        "end_turn",
    );
    assert_eq!(
        parse_json(stray_tool),
        Err(ParseResponseError::InvalidValue)
    );

    let duplicate_tools = response(
        vec![
            json!({ "type": "tool_use", "id": "toolu_1", "name": "a", "input": {} }),
            json!({ "type": "tool_use", "id": "toolu_1", "name": "b", "input": {} }),
        ],
        "tool_use",
    );
    assert_eq!(
        parse_json(duplicate_tools),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn validates_cache_breakdown_reasoning_and_checked_totals() {
    let usage_response = |usage: Value| {
        let mut value = response(vec![], "end_turn");
        value["usage"] = usage;
        value
    };

    let mismatch = usage_response(json!({
        "input_tokens": 1,
        "cache_creation_input_tokens": 5,
        "cache_read_input_tokens": 0,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 2,
            "ephemeral_1h_input_tokens": 2
        },
        "output_tokens": 1
    }));
    assert_eq!(parse_json(mismatch), Err(ParseResponseError::InvalidValue));

    let unsplit = usage_response(json!({
        "input_tokens": 1,
        "cache_creation_input_tokens": 5,
        "cache_read_input_tokens": 0,
        "output_tokens": 1
    }));
    assert_eq!(
        parse_json(unsplit),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let reasoning = usage_response(json!({
        "input_tokens": 1,
        "output_tokens": 1,
        "output_tokens_details": { "thinking_tokens": 2 }
    }));
    assert_eq!(parse_json(reasoning), Err(ParseResponseError::InvalidValue));

    let overflow = usage_response(json!({
        "input_tokens": i64::MAX,
        "cache_creation_input_tokens": 1,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 1,
            "ephemeral_1h_input_tokens": 0
        },
        "cache_read_input_tokens": 0,
        "output_tokens": 0
    }));
    assert_eq!(parse_json(overflow), Err(ParseResponseError::InvalidValue));
}

#[test]
fn builds_official_shape_and_converts_inclusive_usage_with_checked_subtraction() {
    let choice = ResponseChoice::new(
        0,
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Text("hello".to_owned())],
        ),
        FinishReason::Stop,
    );
    let response = canonical_response(
        choice,
        Some(usage(20, 6, 4, 3, 2, 2, UsageSemantics::Inclusive)),
    );
    let built = build_response(&response).unwrap();

    assert_eq!(built["type"], "message");
    assert_eq!(built["role"], "assistant");
    assert_eq!(built["container"], Value::Null);
    assert_eq!(built["stop_reason"], "end_turn");
    assert_eq!(built["content"][0]["citations"], Value::Null);
    assert_eq!(built["usage"]["input_tokens"], 11);
    assert_eq!(built["usage"]["cache_read_input_tokens"], 4);
    assert_eq!(built["usage"]["cache_creation_input_tokens"], 5);
    assert_eq!(
        built["usage"]["cache_creation"]["ephemeral_5m_input_tokens"],
        3
    );
    assert_eq!(
        built["usage"]["output_tokens_details"]["thinking_tokens"],
        2
    );
    assert!(built.get("created_at").is_none());

    let reparsed = parse_json(built).unwrap();
    let usage = reparsed.usage.unwrap();
    assert_eq!(usage.input_tokens().get(), 11);
    assert_eq!(usage.checked_total_tokens().unwrap().get(), 26);
}

#[test]
fn build_response_revalidates_choices_content_usage_and_raw_origin() {
    let valid_choice =
        ResponseChoice::new(0, Message::new(Role::Assistant, vec![]), FinishReason::Stop);
    let valid_usage = usage(1, 1, 0, 0, 0, 0, UsageSemantics::CacheSeparated);

    let missing_usage = canonical_response(valid_choice.clone(), None);
    assert_eq!(
        build_response(&missing_usage),
        Err(BuildResponseError::InvalidValue)
    );

    let mut wrong_index = canonical_response(valid_choice.clone(), Some(valid_usage));
    wrong_index.choices[0].index = 1;
    assert_unsupported_capability(&wrong_index, ResponseCapability::ArbitraryChoiceIndex);

    let mut multiple = canonical_response(valid_choice.clone(), Some(valid_usage));
    multiple.choices.push(valid_choice.clone());
    assert_unsupported_capability(&multiple, ResponseCapability::MultipleChoices);

    let missing_signature = canonical_response(
        ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::Thinking {
                    text: "plan".to_owned(),
                    signature: None,
                }],
            ),
            FinishReason::Stop,
        ),
        Some(valid_usage),
    );
    assert_eq!(
        build_response(&missing_signature),
        Err(BuildResponseError::UnsupportedFeature)
    );

    let tool_mismatch = canonical_response(
        ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "toolu_1".to_owned(),
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
        build_response(&tool_mismatch),
        Err(BuildResponseError::InvalidValue)
    );

    let foreign_raw = canonical_response(valid_choice, Some(valid_usage))
        .with_validated_raw_passthrough(Some((Protocol::OpenAiChat, Map::new())));
    assert_eq!(
        build_response(&foreign_raw),
        Err(BuildResponseError::RawProtocolMismatch)
    );
}

#[test]
fn rejects_usage_dimensions_anthropic_cannot_express() {
    let audio_usage = Usage::new(
        count(3),
        count(2),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            count(1),
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    let response = canonical_response(
        ResponseChoice::new(0, Message::new(Role::Assistant, vec![]), FinishReason::Stop),
        Some(audio_usage),
    );
    assert_unsupported_capability(&response, ResponseCapability::UsageAudioInput);
}

#[test]
fn redacts_response_and_error_debug_output() {
    let mut value = response(
        vec![json!({
            "type": "tool_use",
            "id": "call-canary-51aa",
            "name": "tool-canary-51aa",
            "input": { "secret-canary-51aa": true }
        })],
        "tool_use",
    );
    value["id"] = json!("id-canary-51aa");
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
