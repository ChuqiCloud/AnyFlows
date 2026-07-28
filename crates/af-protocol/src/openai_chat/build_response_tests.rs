use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, Value, json};

use super::{BuildResponseError, build_response, parse_response};
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
    assert_eq!(error.target_protocol(), Protocol::OpenAiChat);
    assert_eq!(error.capability(), ProtocolCapability::Response(expected));
}

fn count(value: i64) -> TokenCount {
    TokenCount::new(value).unwrap()
}

fn usage(details: UsageDetails, semantics: UsageSemantics) -> Usage {
    Usage::new(
        count(8),
        count(3),
        details,
        UsageSource::Upstream,
        semantics,
    )
    .unwrap()
}

fn empty_details() -> UsageDetails {
    UsageDetails::new(
        TokenCount::ZERO,
        TokenCount::ZERO,
        TokenCount::ZERO,
        TokenCount::ZERO,
        TokenCount::ZERO,
        TokenCount::ZERO,
    )
}

fn text_choice() -> ResponseChoice {
    ResponseChoice::new(
        0,
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Text("matrix-output".to_owned())],
        ),
        FinishReason::Stop,
    )
}

fn canonical_response(choice: ResponseChoice, usage: Option<Usage>) -> CanonicalResponse {
    CanonicalResponse::new(
        Operation::Chat,
        "chatcmpl-matrix".to_owned(),
        "matrix-model".to_owned(),
        Some(1_700_000_000),
        vec![choice],
        usage,
    )
}

#[test]
fn builds_minimal_response_and_round_trips() {
    let response = canonical_response(
        text_choice(),
        Some(usage(empty_details(), UsageSemantics::Inclusive)),
    );
    let expected = json!({
        "id": "chatcmpl-matrix",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "matrix-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "matrix-output"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 8,
            "completion_tokens": 3,
            "total_tokens": 11
        }
    });

    let built = build_response(&response).unwrap();
    assert_eq!(built, expected);
    assert_eq!(
        parse_response(&serde_json::to_vec(&built).unwrap()).unwrap(),
        response
    );
}

#[test]
fn preserves_tools_usage_details_and_same_protocol_metadata() {
    let source = json!({
        "id": "chatcmpl-matrix",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "matrix-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "checking",
                "tool_calls": [{
                    "id": "call-matrix",
                    "type": "function",
                    "function": {"name": "lookup", "arguments": "{\"id\":7}"}
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "service_tier": "priority",
        "system_fingerprint": "fp-matrix",
        "usage": {
            "prompt_tokens": 8,
            "completion_tokens": 3,
            "total_tokens": 11,
            "prompt_tokens_details": {"cached_tokens": 2, "audio_tokens": 1},
            "completion_tokens_details": {"reasoning_tokens": 1, "audio_tokens": 1}
        }
    });
    let parsed = parse_response(&serde_json::to_vec(&source).unwrap()).unwrap();

    let built = build_response(&parsed).unwrap();
    assert_eq!(built, source);
    assert_eq!(
        parse_response(&serde_json::to_vec(&built).unwrap()).unwrap(),
        parsed
    );
}

#[test]
fn rejects_invalid_or_lossy_canonical_responses() {
    let valid_usage = usage(empty_details(), UsageSemantics::Inclusive);
    let mut wrong_operation = canonical_response(text_choice(), Some(valid_usage));
    wrong_operation.operation = Operation::Responses;
    assert_eq!(
        build_response(&wrong_operation),
        Err(BuildResponseError::UnsupportedOperation)
    );

    let mut missing_created = canonical_response(text_choice(), Some(valid_usage));
    missing_created.created_at = None;
    assert_eq!(
        build_response(&missing_created),
        Err(BuildResponseError::InvalidValue)
    );

    let mut duplicate_choice = canonical_response(text_choice(), Some(valid_usage));
    duplicate_choice.choices.push(text_choice());
    assert_eq!(
        build_response(&duplicate_choice),
        Err(BuildResponseError::InvalidValue)
    );

    let mut wrong_role = canonical_response(text_choice(), Some(valid_usage));
    wrong_role.choices[0].message.role = Role::User;
    assert_eq!(
        build_response(&wrong_role),
        Err(BuildResponseError::InvalidValue)
    );

    let mut multiple_text = canonical_response(text_choice(), Some(valid_usage));
    multiple_text.choices[0]
        .message
        .content
        .push(ContentBlock::Text("second".to_owned()));
    assert_eq!(
        build_response(&multiple_text),
        Err(BuildResponseError::UnsupportedFeature)
    );

    let mut reordered_content = canonical_response(text_choice(), Some(valid_usage));
    reordered_content.choices[0].message.content = vec![
        ContentBlock::ToolUse {
            id: "call-matrix".to_owned(),
            name: "lookup".to_owned(),
            input: json!({}),
            signature: None,
        },
        ContentBlock::Text("late-text".to_owned()),
    ];
    reordered_content.choices[0].finish_reason = FinishReason::ToolCalls;
    assert_eq!(
        build_response(&reordered_content),
        Err(BuildResponseError::UnsupportedFeature)
    );

    let mut stop_sequence = canonical_response(text_choice(), Some(valid_usage));
    stop_sequence.choices[0].stop_sequence = Some("END".to_owned());
    assert_unsupported_capability(&stop_sequence, ResponseCapability::StopSequence);

    let mut missing_tool = canonical_response(text_choice(), Some(valid_usage));
    missing_tool.choices[0].finish_reason = FinishReason::ToolCalls;
    assert_eq!(
        build_response(&missing_tool),
        Err(BuildResponseError::InvalidValue)
    );

    let cache_creation = UsageDetails::new(
        TokenCount::ZERO,
        count(1),
        TokenCount::ZERO,
        TokenCount::ZERO,
        TokenCount::ZERO,
        TokenCount::ZERO,
    );
    let unsupported_usage = canonical_response(
        text_choice(),
        Some(usage(cache_creation, UsageSemantics::CacheSeparated)),
    );
    assert_unsupported_capability(
        &unsupported_usage,
        ResponseCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
    );
}

#[test]
fn rejects_foreign_raw_and_redacts_errors() {
    let response = canonical_response(
        text_choice(),
        Some(usage(empty_details(), UsageSemantics::Inclusive)),
    )
    .with_validated_raw_passthrough(Some((
        Protocol::Anthropic,
        Map::from_iter([(
            "service_tier".to_owned(),
            Value::String("secret".to_owned()),
        )]),
    )));
    let error = build_response(&response).unwrap_err();
    assert_eq!(error, BuildResponseError::RawProtocolMismatch);
    let rendered = format!("{error:?}|{error}");
    assert!(!rendered.contains("secret"));
    assert!(error.source().is_none());
}
