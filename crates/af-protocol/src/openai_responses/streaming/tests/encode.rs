use af_domain::{Protocol, Role, UpstreamError};
use serde_json::{Value, json};

use super::{CREATED_AT, MODEL, RESPONSE_ID, parse_encoded_events, usage};
use crate::openai_responses::{
    EncodeStreamError, OpenAiResponsesStreamDecoder, OpenAiResponsesStreamEncoder,
};
use crate::{
    CacheHint, CanonicalStreamEvent, ContentDelta, FinishReason, ProtocolCapability,
    StreamCapability, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource,
};

fn compaction_item() -> crate::ResponsesCompactionItem {
    crate::ResponsesCompactionItem::from_validated_value(json!({
        "type": "compaction",
        "id": "cmp_stream_1",
        "encrypted_content": "opaque-compaction"
    }))
}

fn assert_unsupported_capability(error: EncodeStreamError, expected: StreamCapability) {
    let EncodeStreamError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::OpenAiResponses);
    assert_eq!(error.capability(), ProtocolCapability::Stream(expected));
}

#[test]
fn encodes_text_lifecycle_and_terminal_usage() {
    let mut encoder = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let mut bytes = Vec::new();
    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("hel".to_owned()),
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("lo".to_owned()),
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        },
        CanonicalStreamEvent::Usage(usage(8, 3, 2, 1)),
        CanonicalStreamEvent::StreamEnd,
    ] {
        bytes.extend(encoder.encode(event).unwrap());
    }

    let events = parse_encoded_events(&bytes);
    let kinds: Vec<_> = events.iter().map(|(kind, _)| kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    for (sequence, (_, event)) in events.iter().enumerate() {
        assert_eq!(
            event["sequence_number"].as_u64(),
            Some(u64::try_from(sequence).unwrap())
        );
    }
    assert_eq!(events[2].1["item"]["phase"], Value::Null);
    assert_eq!(events[8].1["item"]["phase"], "final_answer");
    let terminal = &events.last().unwrap().1["response"];
    assert_eq!(terminal["status"], "completed");
    assert_eq!(terminal["output"][0]["content"][0]["text"], "hello");
    assert_eq!(terminal["output"][0]["phase"], "final_answer");
    assert_eq!(terminal["usage"]["total_tokens"], 11);
    assert!(!format!("{encoder:?}").contains(RESPONSE_ID));

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let canonical = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert_eq!(
        canonical.first(),
        Some(&CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
    );
    assert_eq!(canonical.last(), Some(&CanonicalStreamEvent::StreamEnd));
}

#[test]
fn encodes_compaction_item_lifecycle() {
    let mut encoder = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let item = compaction_item();
    let mut bytes = Vec::new();
    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::CompactionStart {
            choice_index: 0,
            output_index: 0,
            item: item.clone(),
        },
        CanonicalStreamEvent::CompactionEnd {
            choice_index: 0,
            output_index: 0,
            item,
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        },
        CanonicalStreamEvent::StreamEnd,
    ] {
        bytes.extend(encoder.encode(event).unwrap());
    }
    let events = parse_encoded_events(&bytes);
    assert_eq!(
        events
            .iter()
            .map(|(kind, _)| kind.as_str())
            .collect::<Vec<_>>(),
        [
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.output_item.done",
            "response.completed",
        ]
    );
    assert_eq!(events[2].1["item"]["type"], "compaction");
    assert_eq!(
        events[4].1["response"]["output"][0]["encrypted_content"],
        "opaque-compaction"
    );
}

#[test]
fn encodes_reasoning_signature_and_function_call() {
    let mut encoder = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let mut bytes = Vec::new();
    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: "brief".to_owned(),
            signature: None,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: String::new(),
            signature: Some("sig-1".to_owned()),
        },
        CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: 0,
            id: "call_1".to_owned(),
            name: "weather".to_owned(),
        },
        CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: 0,
            tool_index: 0,
            partial_json: r#"{"city":"Paris"}"#.to_owned(),
        },
        CanonicalStreamEvent::ToolCallEnd {
            choice_index: 0,
            tool_index: 0,
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::ToolCalls,
            stop_sequence: None,
        },
        CanonicalStreamEvent::StreamEnd,
    ] {
        bytes.extend(encoder.encode(event).unwrap());
    }

    let events = parse_encoded_events(&bytes);
    let terminal = &events.last().unwrap().1["response"];
    assert_eq!(terminal["output"][0]["type"], "reasoning");
    assert_eq!(terminal["output"][0]["encrypted_content"], "sig-1");
    assert_eq!(terminal["output"][1]["type"], "function_call");
    assert_eq!(terminal["output"][1]["call_id"], "call_1");
    assert!(
        events
            .iter()
            .any(|(kind, _)| { kind == "response.function_call_arguments.done" })
    );
}

#[test]
fn replaces_reasoning_signature_snapshot_after_message() {
    let mut encoder = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let mut bytes = Vec::new();
    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: String::new(),
            signature: Some("initial-signature".to_owned()),
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 1,
            delta: ContentDelta::Text("answer".to_owned()),
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: String::new(),
            signature: Some("terminal-signature".to_owned()),
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        },
        CanonicalStreamEvent::StreamEnd,
    ] {
        bytes.extend(encoder.encode(event).unwrap());
    }

    let events = parse_encoded_events(&bytes);
    let terminal = &events.last().unwrap().1["response"];
    assert_eq!(
        terminal["output"][0]["encrypted_content"],
        "terminal-signature"
    );
}

#[test]
fn encodes_incomplete_terminal_with_matching_item_status() {
    let mut encoder = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let mut bytes = Vec::new();
    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("partial".to_owned()),
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Length,
            stop_sequence: None,
        },
        CanonicalStreamEvent::StreamEnd,
    ] {
        bytes.extend(encoder.encode(event).unwrap());
    }
    let events = parse_encoded_events(&bytes);
    let (kind, terminal) = events.last().unwrap();
    assert_eq!(kind, "response.incomplete");
    assert_eq!(terminal["response"]["status"], "incomplete");
    assert_eq!(
        terminal["response"]["incomplete_details"]["reason"],
        "max_output_tokens"
    );
    assert_eq!(terminal["response"]["output"][0]["status"], "incomplete");

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let canonical = decoder.push(&bytes).unwrap();
    decoder.finish().unwrap();
    assert!(canonical.contains(&CanonicalStreamEvent::Finish {
        choice_index: 0,
        reason: FinishReason::Length,
        stop_sequence: None,
    }));
}

#[test]
fn encodes_normalized_error_and_requires_stream_end() {
    let mut encoder = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let bytes = encoder
        .encode(CanonicalStreamEvent::Error(UpstreamError::rate_limited(
            af_domain::RateLimitScope::Window,
        )))
        .unwrap();
    assert!(
        encoder
            .encode(CanonicalStreamEvent::StreamEnd)
            .unwrap()
            .is_empty()
    );
    let events = parse_encoded_events(&bytes);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "error");
    assert_eq!(events[0].1["code"], "rate_limit_exceeded");

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    assert_eq!(
        decoder.push(&bytes).unwrap(),
        vec![
            CanonicalStreamEvent::Error(UpstreamError::rate_limited(
                af_domain::RateLimitScope::Window,
            )),
            CanonicalStreamEvent::StreamEnd,
        ]
    );
    decoder.finish().unwrap();
}

#[test]
fn fails_closed_on_invalid_order_arguments_and_usage() {
    let mut order = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    assert_eq!(
        order.encode(CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("x".to_owned()),
        }),
        Err(EncodeStreamError::InvalidSequence)
    );
    assert_eq!(
        order.encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        }),
        Err(EncodeStreamError::EncoderFailed)
    );

    let mut arguments = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    arguments
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    arguments
        .encode(CanonicalStreamEvent::ToolCallStart {
            choice_index: 0,
            tool_index: 0,
            id: "call_1".to_owned(),
            name: "weather".to_owned(),
        })
        .unwrap();
    arguments
        .encode(CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: 0,
            tool_index: 0,
            partial_json: "not-json".to_owned(),
        })
        .unwrap();
    assert_eq!(
        arguments.encode(CanonicalStreamEvent::ToolCallEnd {
            choice_index: 0,
            tool_index: 0,
        }),
        Err(EncodeStreamError::UnsupportedEvent)
    );

    let cache_write_usage = Usage::new(
        TokenCount::new(2).unwrap(),
        TokenCount::ZERO,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::new(1).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::CacheSeparated,
    )
    .unwrap();
    let mut usage_encoder =
        OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    usage_encoder
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    usage_encoder
        .encode(CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        })
        .unwrap();
    let error = usage_encoder
        .encode(CanonicalStreamEvent::Usage(cache_write_usage))
        .expect_err("缓存写入 usage 必须被明确拒绝");
    assert_unsupported_capability(
        error,
        StreamCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
    );
}

#[test]
fn rejects_unrepresentable_events_and_multiple_choices() {
    let mut signature = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    signature
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        })
        .unwrap();
    let error = signature
        .encode(CanonicalStreamEvent::ToolCallSignature {
            choice_index: 0,
            tool_index: 0,
            signature: "opaque".to_owned(),
        })
        .expect_err("工具调用签名必须被明确拒绝");
    assert_unsupported_capability(error, StreamCapability::ToolCallSignatures);

    let mut choice = OpenAiResponsesStreamEncoder::new(RESPONSE_ID, MODEL, CREATED_AT).unwrap();
    let error = choice
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 1,
            role: Role::Assistant,
        })
        .expect_err("非零候选索引必须被明确拒绝");
    assert_unsupported_capability(error, StreamCapability::ArbitraryChoiceIndex);
}
