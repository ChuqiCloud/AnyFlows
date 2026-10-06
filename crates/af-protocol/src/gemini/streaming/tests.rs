use af_domain::{Role, UpstreamError};
use serde_json::{Value, json};

use super::*;
use crate::{
    CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource,
};

const RESPONSE_ID: &str = "gemini-stream-response";
const MODEL_VERSION: &str = "gemini-2.5-pro";
const THOUGHT_SIGNATURE: &str = "c2lnLTE=";
const TOOL_SIGNATURE: &str = "dG9vbC1zaWc=";

fn response(candidates: Value, usage: Option<Value>) -> Value {
    let mut response = json!({
        "responseId": RESPONSE_ID,
        "modelVersion": MODEL_VERSION,
        "candidates": candidates,
    });
    if let Some(usage) = usage {
        response["usageMetadata"] = usage;
    }
    response
}

fn sse(value: &Value) -> Vec<u8> {
    let data = serde_json::to_vec(value).unwrap();
    let mut frame = Vec::with_capacity(data.len() + 8);
    frame.extend_from_slice(b"data: ");
    frame.extend_from_slice(&data);
    frame.extend_from_slice(b"\n\n");
    frame
}

fn usage(input: i64, output: i64, cache: i64, reasoning: i64) -> Usage {
    Usage::new(
        TokenCount::new(input).unwrap(),
        TokenCount::new(output).unwrap(),
        UsageDetails::new(
            TokenCount::new(cache).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::new(reasoning).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

fn final_usage_wire() -> Value {
    json!({
        "promptTokenCount": 10,
        "toolUsePromptTokenCount": 2,
        "candidatesTokenCount": 4,
        "thoughtsTokenCount": 2,
        "cachedContentTokenCount": 3,
        "totalTokenCount": 18
    })
}

#[test]
fn decoder_handles_byte_boundaries_multi_choice_tools_media_usage_and_eof() {
    let first = response(
        json!([
            {
                "index": 0,
                "content": {
                    "role": "model",
                    "parts": [
                        {"text": "think", "thought": true, "thoughtSignature": THOUGHT_SIGNATURE},
                        {"text": "answer"}
                    ]
                },
                "tokenCount": 1
            },
            {
                "index": 1,
                "content": {"role": "model", "parts": [{"text": "other"}]},
                "tokenCount": 1
            }
        ]),
        Some(json!({
            "promptTokenCount": 10,
            "toolUsePromptTokenCount": 2,
            "candidatesTokenCount": 2,
            "thoughtsTokenCount": 1,
            "cachedContentTokenCount": 3,
            "totalTokenCount": 15
        })),
    );
    let second = response(
        json!([
            {
                "index": 0,
                "content": {"role": "model", "parts": [{
                    "functionCall": {"id": "call-weather", "name": "lookup", "args": {"city": "Paris"}},
                    "thoughtSignature": TOOL_SIGNATURE
                }]},
                "finishReason": "STOP",
                "tokenCount": 3
            },
            {
                "index": 1,
                "content": {"role": "model", "parts": [{
                    "inlineData": {"mimeType": "audio/wav", "data": "AQID"}
                }]},
                "finishReason": "MAX_TOKENS",
                "tokenCount": 1
            }
        ]),
        Some(final_usage_wire()),
    );
    let bytes = [b": keepalive\r\n\r\n".to_vec(), sse(&first), sse(&second)].concat();

    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    let mut events = Vec::new();
    for byte in bytes {
        events.extend(decoder.push(std::slice::from_ref(&byte)).unwrap());
    }
    events.extend(decoder.finish().unwrap());

    assert_eq!(
        events,
        vec![
            CanonicalStreamEvent::Ping,
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 0,
                text: "think".to_owned(),
                signature: Some(THOUGHT_SIGNATURE.to_owned()),
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 1,
                delta: ContentDelta::Text("answer".to_owned()),
            },
            CanonicalStreamEvent::MessageStart {
                choice_index: 1,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 1,
                content_index: 0,
                delta: ContentDelta::Text("other".to_owned()),
            },
            CanonicalStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 2,
                id: "call-weather".to_owned(),
                name: "lookup".to_owned(),
            },
            CanonicalStreamEvent::ToolCallSignature {
                choice_index: 0,
                tool_index: 2,
                signature: TOOL_SIGNATURE.to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 2,
                partial_json: "{\"city\":\"Paris\"}".to_owned(),
            },
            CanonicalStreamEvent::ToolCallEnd {
                choice_index: 0,
                tool_index: 2,
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::ToolCalls,
                stop_sequence: None,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 1,
                content_index: 1,
                delta: ContentDelta::Audio {
                    data: "AQID".to_owned(),
                    mime_type: "audio/wav".to_owned(),
                },
            },
            CanonicalStreamEvent::Finish {
                choice_index: 1,
                reason: FinishReason::Length,
                stop_sequence: None,
            },
            CanonicalStreamEvent::Usage(usage(12, 6, 3, 2)),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
}

#[test]
fn decoder_accepts_compatible_done_marker() {
    let frame = response(
        json!([{
            "index": 0,
            "content": {"role": "model", "parts": [{"text": "done"}]},
            "finishReason": "STOP",
            "tokenCount": 1
        }]),
        Some(json!({
            "promptTokenCount": 2,
            "candidatesTokenCount": 1,
            "totalTokenCount": 3
        })),
    );
    let input = [sse(&frame), b"data: [DONE]\n\n".to_vec()].concat();
    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    let events = decoder.push(&input).unwrap();
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));
    assert!(decoder.finish().unwrap().is_empty());
}

#[test]
fn decoder_preserves_prompt_block_as_zero_candidate_event() {
    let frame = json!({
        "responseId": RESPONSE_ID,
        "modelVersion": MODEL_VERSION,
        "promptFeedback": {
            "blockReason": "SAFETY",
            "safetyRatings": [{
                "category": "HARM_CATEGORY_HARASSMENT",
                "probability": "HIGH",
                "blocked": true
            }]
        },
        "usageMetadata": {
            "promptTokenCount": 3,
            "candidatesTokenCount": 0,
            "totalTokenCount": 3
        }
    });
    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    assert_eq!(
        decoder.push(&sse(&frame)).unwrap(),
        vec![CanonicalStreamEvent::PromptBlocked]
    );
    assert_eq!(
        decoder.finish().unwrap(),
        vec![
            CanonicalStreamEvent::Usage(usage(3, 0, 0, 0)),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
}

#[test]
fn decoder_rejects_duplicate_json_missing_identity_and_index() {
    let mut duplicate = GeminiGenerateContentStreamDecoder::new();
    assert_eq!(
        duplicate
            .push(b"data: {\"responseId\":\"one\",\"responseId\":\"two\"}\n\n")
            .unwrap_err(),
        ParseStreamError::DuplicateKey
    );
    assert_eq!(
        duplicate.push(b"").unwrap_err(),
        ParseStreamError::DecoderFailed
    );

    let mut missing_identity = GeminiGenerateContentStreamDecoder::new();
    assert_eq!(
        missing_identity
            .push(&sse(&json!({"candidates": []})))
            .unwrap_err(),
        ParseStreamError::UnsupportedFeature
    );

    let mut missing_index = GeminiGenerateContentStreamDecoder::new();
    assert_eq!(
        missing_index
            .push(&sse(&json!({
                "responseId": RESPONSE_ID,
                "modelVersion": MODEL_VERSION,
                "candidates": [{"content": {"parts": [{"text": "secret"}]}}]
            })))
            .unwrap_err(),
        ParseStreamError::UnsupportedFeature
    );
}

#[test]
fn decoder_rejects_metadata_drift_unsupported_parts_and_invalid_signature() {
    let first = response(
        json!([{"index": 0, "content": {"parts": [{"text": "first"}]}}]),
        None,
    );
    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    decoder.push(&sse(&first)).unwrap();
    let changed = json!({
        "responseId": RESPONSE_ID,
        "modelVersion": "private-other-model",
        "candidates": [{"index": 0, "finishReason": "STOP"}]
    });
    assert_eq!(
        decoder.push(&sse(&changed)).unwrap_err(),
        ParseStreamError::InvalidSequence
    );

    let mut grounding = GeminiGenerateContentStreamDecoder::new();
    let unsupported = response(
        json!([{"index": 0, "groundingMetadata": {"webSearchQueries": ["private"]}}]),
        None,
    );
    assert_eq!(
        grounding.push(&sse(&unsupported)).unwrap_err(),
        ParseStreamError::UnsupportedFeature
    );

    let mut signature = GeminiGenerateContentStreamDecoder::new();
    let invalid = response(
        json!([{"index": 0, "content": {"parts": [{
            "text": "private-thought",
            "thought": true,
            "thoughtSignature": "not base64"
        }]}}]),
        None,
    );
    assert_eq!(
        signature.push(&sse(&invalid)).unwrap_err(),
        ParseStreamError::InvalidValue
    );
}

#[test]
fn decoder_rejects_usage_regression_and_unfinished_eof() {
    let first = response(
        json!([{"index": 0, "content": {"parts": [{"text": "partial"}]}}]),
        Some(json!({
            "promptTokenCount": 5,
            "candidatesTokenCount": 2,
            "totalTokenCount": 7
        })),
    );
    let mut regression = GeminiGenerateContentStreamDecoder::new();
    regression.push(&sse(&first)).unwrap();
    let lower = response(
        json!([{"index": 0, "finishReason": "STOP"}]),
        Some(json!({
            "promptTokenCount": 5,
            "candidatesTokenCount": 1,
            "totalTokenCount": 6
        })),
    );
    assert_eq!(
        regression.push(&sse(&lower)).unwrap_err(),
        ParseStreamError::InvalidSequence
    );

    let mut unfinished = GeminiGenerateContentStreamDecoder::new();
    unfinished.push(&sse(&first)).unwrap();
    assert_eq!(
        unfinished.finish().unwrap_err(),
        ParseStreamError::UnexpectedEof
    );
}

#[test]
fn decoder_maps_bounded_error_without_leaking_message() {
    let frame = json!({
        "error": {
            "code": 503,
            "status": "UNAVAILABLE",
            "message": "private upstream failure"
        }
    });
    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    assert_eq!(
        decoder.push(&sse(&frame)).unwrap(),
        vec![CanonicalStreamEvent::Error(UpstreamError::overloaded())]
    );
    assert!(decoder.finish().unwrap().is_empty());
    let rendered = format!("{decoder:?}");
    assert!(!rendered.contains("private upstream failure"));
}

#[test]
fn decoder_does_not_treat_generic_permission_denial_as_revocation() {
    for (code, status, expected) in [
        (403, "PERMISSION_DENIED", UpstreamError::ProtocolError),
        (
            429,
            "RESOURCE_EXHAUSTED",
            UpstreamError::rate_limited(af_domain::RateLimitScope::Unknown),
        ),
    ] {
        let frame = json!({
            "error": { "code": code, "status": status, "message": "private" }
        });
        let mut decoder = GeminiGenerateContentStreamDecoder::new();
        assert_eq!(
            decoder.push(&sse(&frame)).unwrap(),
            vec![CanonicalStreamEvent::Error(expected)]
        );
        assert!(decoder.finish().unwrap().is_empty());
    }
}

#[test]
fn encoder_emits_official_sse_and_round_trips_semantics() {
    let mut encoder = GeminiGenerateContentStreamEncoder::new(RESPONSE_ID, MODEL_VERSION).unwrap();
    let source_events = [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: "think".to_owned(),
            signature: Some(THOUGHT_SIGNATURE.to_owned()),
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 1,
            delta: ContentDelta::Text("answer".to_owned()),
        },
        CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: 2,
            id: "call-weather".to_owned(),
            name: "lookup".to_owned(),
        },
        CanonicalStreamEvent::ToolCallSignature {
            choice_index: 0,
            tool_index: 2,
            signature: TOOL_SIGNATURE.to_owned(),
        },
        CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: 0,
            tool_index: 2,
            partial_json: "{\"city\":\"Paris\"}".to_owned(),
        },
        CanonicalStreamEvent::ToolCallEnd {
            choice_index: 0,
            tool_index: 2,
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::ToolCalls,
            stop_sequence: None,
        },
        CanonicalStreamEvent::Usage(usage(12, 6, 3, 2)),
        CanonicalStreamEvent::StreamEnd,
    ];
    let mut encoded = Vec::new();
    for event in source_events {
        encoded.extend(encoder.encode(event).unwrap());
    }
    let text = String::from_utf8(encoded.clone()).unwrap();
    assert!(!text.contains("[DONE]"));
    assert!(text.contains("thoughtSignature"));
    assert!(text.contains("functionCall"));
    assert!(text.contains("usageMetadata"));

    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    let mut events = decoder.push(&encoded).unwrap();
    events.extend(decoder.finish().unwrap());
    assert!(events.contains(&CanonicalStreamEvent::ToolCallSignature {
        choice_index: 0,
        tool_index: 2,
        signature: TOOL_SIGNATURE.to_owned(),
    }));
    assert!(events.contains(&CanonicalStreamEvent::ReasoningDelta {
        choice_index: 0,
        content_index: 0,
        text: String::new(),
        signature: Some(THOUGHT_SIGNATURE.to_owned()),
    }));
    assert_eq!(
        events[events.len() - 2..],
        [
            CanonicalStreamEvent::Usage(usage(12, 6, 3, 2)),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
}

#[test]
fn encoder_preserves_prompt_block_without_fabricating_choice() {
    let mut encoder = GeminiGenerateContentStreamEncoder::new(RESPONSE_ID, MODEL_VERSION).unwrap();
    let mut bytes = encoder.encode(CanonicalStreamEvent::PromptBlocked).unwrap();
    bytes.extend(
        encoder
            .encode(CanonicalStreamEvent::Usage(usage(3, 0, 0, 0)))
            .unwrap(),
    );
    assert!(
        encoder
            .encode(CanonicalStreamEvent::StreamEnd)
            .unwrap()
            .is_empty()
    );
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("promptFeedback"));
    assert!(!text.contains("\"candidates\":"));

    let mut decoder = GeminiGenerateContentStreamDecoder::new();
    let mut events = decoder.push(&bytes).unwrap();
    events.extend(decoder.finish().unwrap());
    assert_eq!(events[0], CanonicalStreamEvent::PromptBlocked);
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, CanonicalStreamEvent::MessageStart { .. }))
    );
}

#[test]
fn encoder_rejects_invalid_metadata_sequence_and_signature() {
    assert_eq!(
        GeminiGenerateContentStreamEncoder::new("", MODEL_VERSION).unwrap_err(),
        EncodeStreamError::InvalidMetadata
    );

    let mut sequence = GeminiGenerateContentStreamEncoder::new(RESPONSE_ID, MODEL_VERSION).unwrap();
    sequence
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    assert_eq!(
        sequence
            .encode(CanonicalStreamEvent::PromptBlocked)
            .unwrap_err(),
        EncodeStreamError::InvalidSequence
    );
    assert_eq!(
        sequence.encode(CanonicalStreamEvent::Ping).unwrap_err(),
        EncodeStreamError::EncoderFailed
    );

    let mut signature =
        GeminiGenerateContentStreamEncoder::new(RESPONSE_ID, MODEL_VERSION).unwrap();
    signature
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    signature
        .encode(CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: 0,
            id: "call-one".to_owned(),
            name: "lookup".to_owned(),
        })
        .unwrap();
    assert_eq!(
        signature
            .encode(CanonicalStreamEvent::ToolCallSignature {
                choice_index: 0,
                tool_index: 0,
                signature: "not base64".to_owned(),
            })
            .unwrap_err(),
        EncodeStreamError::InvalidSequence
    );
}

#[test]
fn debug_output_redacts_stream_metadata_and_signatures() {
    let decoder = GeminiGenerateContentStreamDecoder::new();
    let encoder =
        GeminiGenerateContentStreamEncoder::new("private-response-id", "private-model-version")
            .unwrap();
    let signature = CanonicalStreamEvent::ToolCallSignature {
        choice_index: 0,
        tool_index: 0,
        signature: "private-signature".to_owned(),
    };
    let rendered = format!("{decoder:?}\n{encoder:?}\n{signature:?}");
    assert!(!rendered.contains("private-response-id"));
    assert!(!rendered.contains("private-model-version"));
    assert!(!rendered.contains("private-signature"));
}
