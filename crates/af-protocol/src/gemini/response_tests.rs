use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

use super::{
    BuildResponseError, MAX_BODY_BYTES, ParseResponseError, build_response, parse_response,
};
use crate::{
    CanonicalResponse, ContentBlock, FinishReason, MediaSource, Message, ProtocolCapability,
    ResponseCapability, ResponseChoice, TokenCount, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};

fn assert_unsupported_capability(response: &CanonicalResponse, expected: ResponseCapability) {
    let error = build_response(response).expect_err("响应能力必须被明确拒绝");
    let BuildResponseError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::Gemini);
    assert_eq!(error.capability(), ProtocolCapability::Response(expected));
}

fn parse_json(value: Value) -> Result<CanonicalResponse, ParseResponseError> {
    parse_response(&serde_json::to_vec(&value).unwrap())
}

fn text_candidate(index: u32, text: &str, finish_reason: &str) -> Value {
    json!({
        "index": index,
        "content": {
            "role": "model",
            "parts": [{"text": text}]
        },
        "finishReason": finish_reason
    })
}

fn response(candidates: Vec<Value>) -> Value {
    json!({
        "responseId": "resp-1",
        "modelVersion": "gemini-test",
        "candidates": candidates
    })
}

fn count(value: i64) -> TokenCount {
    TokenCount::new(value).unwrap()
}

fn usage(
    input: i64,
    output: i64,
    cache_read: i64,
    reasoning: i64,
    audio_input: i64,
    audio_output: i64,
) -> Usage {
    Usage::new(
        count(input),
        count(output),
        UsageDetails::new(
            count(cache_read),
            TokenCount::ZERO,
            TokenCount::ZERO,
            count(reasoning),
            count(audio_input),
            count(audio_output),
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

fn canonical_response(choices: Vec<ResponseChoice>, usage: Option<Usage>) -> CanonicalResponse {
    CanonicalResponse::new(
        Operation::Chat,
        "resp-1".to_owned(),
        "gemini-test".to_owned(),
        None,
        choices,
        usage,
    )
}

#[test]
fn parses_and_builds_minimal_official_response() {
    let value = json!({
        "responseId": "resp-1",
        "modelVersion": "gemini-test",
        "candidates": [text_candidate(0, "hello", "STOP")],
        "usageMetadata": {
            "promptTokenCount": 3,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5
        }
    });

    let parsed = parse_json(value.clone()).unwrap();
    assert_eq!(parsed.id, "resp-1");
    assert_eq!(parsed.model, "gemini-test");
    assert_eq!(parsed.created_at, None);
    assert_eq!(parsed.choices.len(), 1);
    assert_eq!(parsed.choices[0].index, 0);
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::Stop);
    assert!(matches!(
        parsed.choices[0].message.content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
    let parsed_usage = parsed.usage.as_ref().unwrap();
    assert_eq!(parsed_usage.input_tokens(), count(3));
    assert_eq!(parsed_usage.output_tokens(), count(2));
    assert_eq!(build_response(&parsed).unwrap(), value);
}

#[test]
fn round_trips_multimodal_tools_safety_model_status_and_usage() {
    let thought_signature = STANDARD.encode("thought-signature");
    let tool_signature = STANDARD.encode("tool-signature");
    let value = json!({
        "responseId": "resp-full",
        "modelVersion": "gemini-2.5-pro",
        "promptFeedback": {
            "safetyRatings": [{
                "category": "HARM_CATEGORY_HARASSMENT",
                "probability": "NEGLIGIBLE",
                "blocked": false
            }]
        },
        "modelStatus": {
            "modelStage": "STABLE",
            "retirementTime": "2030-01-01T00:00:00Z",
            "message": "stable"
        },
        "candidates": [
            {
                "index": 2,
                "content": {
                    "role": "model",
                    "parts": [
                        {
                            "text": "inspect",
                            "thought": true,
                            "thoughtSignature": thought_signature
                        },
                        {"text": "done"},
                        {"inlineData": {"mimeType": "image/png", "data": "aGk="}},
                        {"inlineData": {"mimeType": "audio/wav", "data": "aGk="}}
                    ]
                },
                "finishReason": "STOP",
                "safetyRatings": [{
                    "category": "HARM_CATEGORY_HATE_SPEECH",
                    "probability": "LOW"
                }],
                "tokenCount": 5
            },
            {
                "index": 7,
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "id": "call-1",
                            "name": "lookup",
                            "args": {"query": "health"}
                        },
                        "thoughtSignature": tool_signature
                    }]
                },
                "finishReason": "STOP",
                "safetyRatings": [],
                "tokenCount": 3
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 8,
            "toolUsePromptTokenCount": 2,
            "candidatesTokenCount": 8,
            "thoughtsTokenCount": 2,
            "cachedContentTokenCount": 2,
            "totalTokenCount": 20,
            "promptTokensDetails": [
                {"modality": "TEXT", "tokenCount": 7},
                {"modality": "AUDIO", "tokenCount": 1}
            ],
            "toolUsePromptTokensDetails": [
                {"modality": "TEXT", "tokenCount": 2}
            ],
            "cacheTokensDetails": [
                {"modality": "TEXT", "tokenCount": 2}
            ],
            "candidatesTokensDetails": [
                {"modality": "TEXT", "tokenCount": 7},
                {"modality": "AUDIO", "tokenCount": 1}
            ],
            "serviceTier": "standard"
        }
    });

    let parsed = parse_json(value.clone()).unwrap();
    assert_eq!(parsed.choices.len(), 2);
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::Stop);
    assert_eq!(parsed.choices[1].finish_reason, FinishReason::ToolCalls);
    assert!(matches!(
        &parsed.choices[0].message.content[0],
        ContentBlock::Thinking { signature: Some(signature), .. }
            if signature == &thought_signature
    ));
    assert!(matches!(
        &parsed.choices[1].message.content[0],
        ContentBlock::ToolUse {
            id,
            name,
            signature: Some(signature),
            ..
        } if id == "call-1" && name == "lookup" && signature == &tool_signature
    ));
    let parsed_usage = parsed.usage.as_ref().unwrap();
    assert_eq!(parsed_usage.input_tokens(), count(10));
    assert_eq!(parsed_usage.output_tokens(), count(10));
    assert_eq!(parsed_usage.details().cache_read(), count(2));
    assert_eq!(parsed_usage.details().reasoning(), count(2));
    assert_eq!(parsed_usage.details().audio_input(), count(1));
    assert_eq!(parsed_usage.details().audio_output(), count(1));
    assert_eq!(build_response(&parsed).unwrap(), value);
    assert_eq!(
        parse_json(build_response(&parsed).unwrap()).unwrap(),
        parsed
    );
}

#[test]
fn preserves_prompt_block_without_inventing_a_candidate() {
    let value = json!({
        "responseId": "resp-blocked",
        "modelVersion": "gemini-test",
        "candidates": [],
        "promptFeedback": {
            "blockReason": "SAFETY",
            "safetyRatings": [{
                "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
                "probability": "HIGH",
                "blocked": true
            }]
        }
    });

    let parsed = parse_json(value.clone()).unwrap();
    assert!(parsed.choices.is_empty());
    assert!(parsed.usage.is_none());
    assert_eq!(build_response(&parsed).unwrap(), value);
}

#[test]
fn preserves_exact_filtered_finish_reason_in_same_protocol_raw() {
    let value = json!({
        "responseId": "resp-filtered",
        "modelVersion": "gemini-test",
        "candidates": [{
            "index": 0,
            "finishReason": "MALFORMED_RESPONSE",
            "finishMessage": "response was malformed"
        }]
    });

    let parsed = parse_json(value).unwrap();
    assert_eq!(parsed.choices[0].finish_reason, FinishReason::ContentFilter);
    assert!(parsed.choices[0].message.content.is_empty());
    let rebuilt = build_response(&parsed).unwrap();
    assert_eq!(
        rebuilt["candidates"][0]["finishReason"],
        "MALFORMED_RESPONSE"
    );
    assert_eq!(
        rebuilt["candidates"][0]["finishMessage"],
        "response was malformed"
    );
    assert_eq!(
        parse_json(rebuilt).unwrap().choices[0].finish_reason,
        FinishReason::ContentFilter
    );
}

#[test]
fn builds_cross_protocol_choices_and_inclusive_usage() {
    let choices = vec![
        ResponseChoice::new(
            0,
            Message::new(Role::Assistant, vec![ContentBlock::Text("done".to_owned())]),
            FinishReason::Stop,
        ),
        ResponseChoice::new(
            1,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::Text("partial".to_owned())],
            ),
            FinishReason::Length,
        ),
        ResponseChoice::new(
            2,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "call-1".to_owned(),
                    name: "lookup".to_owned(),
                    input: json!({"query": "health"}),
                    signature: None,
                }],
            ),
            FinishReason::ToolCalls,
        ),
        ResponseChoice::new(
            3,
            Message::new(Role::Assistant, Vec::new()),
            FinishReason::ContentFilter,
        ),
    ];
    let canonical = canonical_response(choices, Some(usage(10, 8, 2, 2, 1, 1)));

    let built = build_response(&canonical).unwrap();
    assert_eq!(built["candidates"][0]["finishReason"], "STOP");
    assert_eq!(built["candidates"][1]["finishReason"], "MAX_TOKENS");
    assert_eq!(built["candidates"][2]["finishReason"], "STOP");
    assert_eq!(built["candidates"][3]["finishReason"], "SAFETY");
    assert_eq!(built["usageMetadata"]["promptTokenCount"], 10);
    assert_eq!(built["usageMetadata"]["candidatesTokenCount"], 6);
    assert_eq!(built["usageMetadata"]["thoughtsTokenCount"], 2);
    assert_eq!(built["usageMetadata"]["totalTokenCount"], 18);
    assert_eq!(parse_json(built).unwrap(), canonical);
}

#[test]
fn rejects_missing_identity_indexes_finish_and_function_ids() {
    let missing_id = json!({
        "modelVersion": "gemini-test",
        "candidates": [text_candidate(0, "hello", "STOP")]
    });
    assert_eq!(
        parse_json(missing_id),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let missing_model = json!({
        "responseId": "resp-1",
        "candidates": [text_candidate(0, "hello", "STOP")]
    });
    assert_eq!(
        parse_json(missing_model),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let mut missing_index = response(vec![text_candidate(0, "hello", "STOP")]);
    missing_index["candidates"][0]
        .as_object_mut()
        .unwrap()
        .remove("index");
    assert_eq!(
        parse_json(missing_index),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let mut missing_finish = response(vec![text_candidate(0, "hello", "STOP")]);
    missing_finish["candidates"][0]
        .as_object_mut()
        .unwrap()
        .remove("finishReason");
    assert_eq!(
        parse_json(missing_finish),
        Err(ParseResponseError::InvalidValue)
    );

    let missing_call_id = response(vec![json!({
        "index": 0,
        "content": {
            "role": "model",
            "parts": [{"functionCall": {"name": "lookup", "args": {}}}]
        },
        "finishReason": "STOP"
    })]);
    assert_eq!(
        parse_json(missing_call_id),
        Err(ParseResponseError::UnsupportedFeature)
    );

    let wrong_role = response(vec![json!({
        "index": 0,
        "content": {"role": "user", "parts": [{"text": "hello"}]},
        "finishReason": "STOP"
    })]);
    assert_eq!(
        parse_json(wrong_role),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn rejects_unmodeled_official_response_features() {
    for (key, value) in [
        ("groundingMetadata", json!({"webSearchQueries": ["query"]})),
        ("citationMetadata", json!({"citationSources": []})),
        ("logprobsResult", json!({"topCandidates": []})),
        ("avgLogprobs", json!(-0.1)),
        ("urlContextMetadata", json!({"urlMetadata": []})),
    ] {
        let mut candidate = text_candidate(0, "hello", "STOP");
        candidate[key] = value;
        assert_eq!(
            parse_json(response(vec![candidate])),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }

    for part in [
        json!({"fileData": {"mimeType": "image/png", "fileUri": "files/a"}}),
        json!({"functionResponse": {"id": "call-1", "name": "lookup", "response": {}}}),
        json!({"executableCode": {"language": "PYTHON", "code": "pass"}}),
    ] {
        let candidate = json!({
            "index": 0,
            "content": {"role": "model", "parts": [part]},
            "finishReason": "STOP"
        });
        assert_eq!(
            parse_json(response(vec![candidate])),
            Err(ParseResponseError::UnsupportedFeature)
        );
    }
}

#[test]
fn validates_usage_totals_modalities_and_candidate_counts() {
    let invalid_usage = [
        json!({
            "promptTokenCount": 3,
            "candidatesTokenCount": 2,
            "totalTokenCount": 6
        }),
        json!({
            "promptTokenCount": 3,
            "toolUsePromptTokenCount": 2,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5
        }),
        json!({
            "promptTokenCount": 3,
            "cachedContentTokenCount": 4,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5
        }),
        json!({
            "promptTokenCount": 3,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5,
            "promptTokensDetails": [
                {"modality": "AUDIO", "tokenCount": 2},
                {"modality": "AUDIO", "tokenCount": 1}
            ]
        }),
        json!({
            "promptTokenCount": 3,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5,
            "candidatesTokensDetails": [
                {"modality": "AUDIO", "tokenCount": 3}
            ]
        }),
        json!({
            "promptTokenCount": -1,
            "candidatesTokenCount": 2,
            "totalTokenCount": 1
        }),
        json!({
            "promptTokenCount": 3,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5,
            "serviceTier": "unspecified"
        }),
    ];
    for usage in invalid_usage {
        let mut value = response(vec![text_candidate(0, "hello", "STOP")]);
        value["usageMetadata"] = usage;
        assert_eq!(parse_json(value), Err(ParseResponseError::InvalidValue));
    }

    let mut token_mismatch = response(vec![json!({
        "index": 0,
        "content": {"role": "model", "parts": [{"text": "hello"}]},
        "finishReason": "STOP",
        "tokenCount": 3
    })]);
    token_mismatch["usageMetadata"] = json!({
        "promptTokenCount": 3,
        "candidatesTokenCount": 2,
        "totalTokenCount": 5
    });
    assert_eq!(
        parse_json(token_mismatch),
        Err(ParseResponseError::InvalidValue)
    );
}

#[test]
fn build_response_revalidates_choices_media_usage_and_raw() {
    let mut wrong_operation = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(Role::Assistant, vec![ContentBlock::Text("ok".to_owned())]),
            FinishReason::Stop,
        )],
        None,
    );
    wrong_operation.operation = Operation::Embedding;
    assert_eq!(
        build_response(&wrong_operation),
        Err(BuildResponseError::UnsupportedOperation)
    );

    let mut wrong_role = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(Role::User, vec![ContentBlock::Text("ok".to_owned())]),
            FinishReason::Stop,
        )],
        None,
    );
    wrong_role.created_at = Some(-1);
    assert_eq!(
        build_response(&wrong_role),
        Err(BuildResponseError::InvalidValue)
    );
    wrong_role.created_at = None;
    assert_eq!(
        build_response(&wrong_role),
        Err(BuildResponseError::InvalidValue)
    );

    let mismatch = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "call-1".to_owned(),
                    name: "lookup".to_owned(),
                    input: json!({}),
                    signature: None,
                }],
            ),
            FinishReason::Stop,
        )],
        None,
    );
    assert_eq!(
        build_response(&mismatch),
        Err(BuildResponseError::InvalidValue)
    );

    let remote_media = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::Image {
                    source: MediaSource::Url("https://example.com/image.png".to_owned()),
                    mime_type: None,
                }],
            ),
            FinishReason::Stop,
        )],
        None,
    );
    assert_unsupported_capability(&remote_media, ResponseCapability::ImageUrl);

    let cache_creation = Usage::new(
        count(10),
        count(2),
        UsageDetails::new(
            TokenCount::ZERO,
            count(1),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    let unsupported_usage = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(Role::Assistant, vec![ContentBlock::Text("ok".to_owned())]),
            FinishReason::Stop,
        )],
        Some(cache_creation),
    );
    assert_unsupported_capability(
        &unsupported_usage,
        ResponseCapability::UsageCacheCreation(crate::CacheHint::Ephemeral5Minutes),
    );

    let mut fields = Map::new();
    fields.insert("promptFeedback".to_owned(), json!({}));
    let cross_raw = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(Role::Assistant, vec![ContentBlock::Text("ok".to_owned())]),
            FinishReason::Stop,
        )],
        None,
    )
    .with_validated_raw_passthrough(Some((Protocol::Anthropic, fields)));
    assert_eq!(
        build_response(&cross_raw),
        Err(BuildResponseError::RawProtocolMismatch)
    );

    let mut invalid_raw = Map::new();
    invalid_raw.insert(
        "candidates".to_owned(),
        json!([{"index": 0, "finishReason": "STOP"}]),
    );
    let invalid_raw = canonical_response(
        vec![ResponseChoice::new(
            0,
            Message::new(Role::Assistant, vec![ContentBlock::Text("ok".to_owned())]),
            FinishReason::Stop,
        )],
        None,
    )
    .with_validated_raw_passthrough(Some((Protocol::Gemini, invalid_raw)));
    assert_eq!(
        build_response(&invalid_raw),
        Err(BuildResponseError::InvalidValue)
    );
}

#[test]
fn rejects_duplicate_unknown_oversized_and_redacts_debug() {
    let duplicate = br#"{
        "responseId":"resp-1",
        "modelVersion":"gemini-test",
        "candidates":[{
            "index":0,
            "content":{"role":"model","parts":[{"text":"a","text":"b"}]},
            "finishReason":"STOP"
        }]
    }"#;
    assert_eq!(
        parse_response(duplicate),
        Err(ParseResponseError::DuplicateKey)
    );

    let mut unknown = response(vec![text_candidate(0, "hello", "STOP")]);
    unknown["unknown"] = json!(true);
    assert_eq!(parse_json(unknown), Err(ParseResponseError::InvalidValue));

    let oversized_text = "x".repeat(1024 * 1024 + 1);
    assert_eq!(
        parse_json(response(vec![text_candidate(0, &oversized_text, "STOP")])),
        Err(ParseResponseError::StructureLimitExceeded)
    );
    assert_eq!(
        parse_response(&vec![b' '; MAX_BODY_BYTES + 1]),
        Err(ParseResponseError::BodyTooLarge)
    );

    let mut secret = response(vec![text_candidate(0, "secret-body-canary-8f2d", "STOP")]);
    secret["responseId"] = json!(" secret-id-canary-8f2d");
    let error = parse_json(secret).unwrap_err();
    let rendered = format!("{error:?}|{error}");
    assert_eq!(error, ParseResponseError::InvalidValue);
    assert!(!rendered.contains("canary-8f2d"));
    assert!(error.source().is_none());

    let mut canonical = canonical_response(Vec::new(), None);
    canonical.id = " secret-id-canary-8f2d".to_owned();
    let error = build_response(&canonical).unwrap_err();
    let rendered = format!("{error:?}|{error}");
    assert!(!rendered.contains("canary-8f2d"));
    assert!(error.source().is_none());
}
