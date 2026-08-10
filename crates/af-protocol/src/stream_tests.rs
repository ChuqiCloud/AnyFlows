use af_domain::{Role, UpstreamError, UpstreamServerStatus};

use crate::{CanonicalStreamEvent, ContentDelta, FinishReason};

#[test]
fn stream_events_keep_choice_and_content_or_tool_indexes_separate() {
    let content = CanonicalStreamEvent::ContentDelta {
        choice_index: 11,
        content_index: 23,
        delta: ContentDelta::Text("内容索引测试".to_owned()),
    };
    let CanonicalStreamEvent::ContentDelta {
        choice_index,
        content_index,
        delta: ContentDelta::Text(text),
    } = content
    else {
        panic!("应构造内容增量事件");
    };
    assert_eq!(choice_index, 11);
    assert_eq!(content_index, 23);
    assert_eq!(text, "内容索引测试");

    let tool = CanonicalStreamEvent::ToolCallStart {
        choice_index: 31,
        tool_index: 47,
        id: "call-index-test".to_owned(),
        name: "lookup-index-test".to_owned(),
    };
    let CanonicalStreamEvent::ToolCallStart {
        choice_index,
        tool_index,
        id,
        name,
    } = tool
    else {
        panic!("应构造工具调用开始事件");
    };
    assert_eq!(choice_index, 31);
    assert_eq!(tool_index, 47);
    assert_eq!(id, "call-index-test");
    assert_eq!(name, "lookup-index-test");
}

#[test]
fn finish_reason_exposes_the_complete_closed_set() {
    assert_eq!(
        FinishReason::ALL,
        &[
            FinishReason::Stop,
            FinishReason::Length,
            FinishReason::ToolCalls,
            FinishReason::ContentFilter,
        ]
    );

    for reason in FinishReason::ALL {
        let name = match reason {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ToolCalls => "tool_calls",
            FinishReason::ContentFilter => "content_filter",
        };
        assert!(!name.is_empty());
    }
}

#[test]
fn sensitive_stream_payloads_are_redacted_from_debug() {
    let text_canary = "stream-text-secret-canary";
    let image_canary = "stream-image-base64-secret-canary";
    let image_mime_canary = "stream-image-mime-secret-canary";
    let audio_canary = "stream-audio-base64-secret-canary";
    let audio_mime_canary = "stream-audio-mime-secret-canary";
    let reasoning_canary = "stream-reasoning-secret-canary";
    let signature_canary = "stream-signature-secret-canary";
    let tool_id_canary = "stream-tool-id-secret-canary";
    let tool_name_canary = "stream-tool-name-secret-canary";
    let partial_json_canary = "stream-partial-json-secret-canary";

    let deltas = [
        ContentDelta::Text(text_canary.to_owned()),
        ContentDelta::Image {
            data: image_canary.to_owned(),
            mime_type: image_mime_canary.to_owned(),
        },
        ContentDelta::Audio {
            data: audio_canary.to_owned(),
            mime_type: audio_mime_canary.to_owned(),
        },
    ];
    let events = [
        CanonicalStreamEvent::ContentDelta {
            choice_index: 1,
            content_index: 2,
            delta: deltas[0].clone(),
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 1,
            content_index: 3,
            delta: deltas[1].clone(),
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 1,
            content_index: 4,
            delta: deltas[2].clone(),
        },
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: 5,
            content_index: 6,
            text: reasoning_canary.to_owned(),
            signature: Some(signature_canary.to_owned()),
        },
        CanonicalStreamEvent::ToolCallStart {
            choice_index: 7,
            tool_index: 8,
            id: tool_id_canary.to_owned(),
            name: tool_name_canary.to_owned(),
        },
        CanonicalStreamEvent::ToolCallSignature {
            choice_index: 7,
            tool_index: 8,
            signature: signature_canary.to_owned(),
        },
        CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: 7,
            tool_index: 8,
            partial_json: partial_json_canary.to_owned(),
        },
    ];

    let rendered = deltas
        .iter()
        .map(|delta| format!("{delta:?}"))
        .chain(events.iter().map(|event| format!("{event:?}")))
        .collect::<Vec<_>>()
        .join("\n");
    for canary in [
        text_canary,
        image_canary,
        image_mime_canary,
        audio_canary,
        audio_mime_canary,
        reasoning_canary,
        signature_canary,
        tool_id_canary,
        tool_name_canary,
        partial_json_canary,
    ] {
        assert!(!rendered.contains(canary));
    }
    assert!(rendered.contains("choice_index"));
    assert!(rendered.contains("content_index"));
    assert!(rendered.contains("tool_index"));
}

#[test]
fn error_event_carries_only_the_closed_upstream_error() {
    let error = UpstreamError::ServerError {
        status: UpstreamServerStatus::new(503).expect("503 应为有效上游服务器错误"),
    };
    let event = CanonicalStreamEvent::Error(error);
    let CanonicalStreamEvent::Error(actual) = event else {
        panic!("应构造错误事件");
    };
    assert_eq!(actual, error);
    assert_eq!(upstream_error_kind(actual), "server_error");
    assert_eq!(
        format!("{:?}", CanonicalStreamEvent::Error(actual)),
        "Error(ServerError { status: UpstreamServerStatus(503) })"
    );
}

fn upstream_error_kind(error: UpstreamError) -> &'static str {
    match error {
        UpstreamError::RateLimited { .. } => "rate_limited",
        UpstreamError::Overloaded { .. } => "overloaded",
        UpstreamError::AuthExpired => "auth_expired",
        UpstreamError::AuthRevoked => "auth_revoked",
        UpstreamError::AccountDisabled => "account_disabled",
        UpstreamError::QuotaExhausted => "quota_exhausted",
        UpstreamError::ModelUnsupported => "model_unsupported",
        UpstreamError::ProtocolError => "protocol_error",
        UpstreamError::ServerError { status: _ } => "server_error",
        UpstreamError::BadRequest => "bad_request",
        UpstreamError::Network { .. } => "network",
    }
}

fn event_kind(event: &CanonicalStreamEvent) -> &'static str {
    match event {
        CanonicalStreamEvent::MessageStart {
            choice_index: _,
            role: _,
        } => "message_start",
        CanonicalStreamEvent::ContentDelta {
            choice_index: _,
            content_index: _,
            delta: _,
        } => "content_delta",
        CanonicalStreamEvent::ReasoningDelta {
            choice_index: _,
            content_index: _,
            text: _,
            signature: _,
        } => "reasoning_delta",
        CanonicalStreamEvent::ToolCallStart {
            choice_index: _,
            tool_index: _,
            id: _,
            name: _,
        } => "tool_call_start",
        CanonicalStreamEvent::ToolCallSignature {
            choice_index: _,
            tool_index: _,
            signature: _,
        } => "tool_call_signature",
        CanonicalStreamEvent::ToolCallArgsDelta {
            choice_index: _,
            tool_index: _,
            partial_json: _,
        } => "tool_call_args_delta",
        CanonicalStreamEvent::ToolCallEnd {
            choice_index: _,
            tool_index: _,
        } => "tool_call_end",
        CanonicalStreamEvent::CompactionStart {
            choice_index: _,
            output_index: _,
            item: _,
        } => "compaction_start",
        CanonicalStreamEvent::CompactionEnd {
            choice_index: _,
            output_index: _,
            item: _,
        } => "compaction_end",
        CanonicalStreamEvent::Finish {
            choice_index: _,
            reason: _,
            stop_sequence: _,
        } => "finish",
        CanonicalStreamEvent::PromptBlocked => "prompt_blocked",
        CanonicalStreamEvent::Usage(_) => "usage",
        CanonicalStreamEvent::Ping => "ping",
        CanonicalStreamEvent::StreamEnd => "stream_end",
        CanonicalStreamEvent::Error(_) => "error",
    }
}

#[test]
fn representative_events_use_the_exhaustive_event_match() {
    for (event, expected) in [
        (
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            "message_start",
        ),
        (CanonicalStreamEvent::Ping, "ping"),
        (CanonicalStreamEvent::PromptBlocked, "prompt_blocked"),
        (CanonicalStreamEvent::StreamEnd, "stream_end"),
        (
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::Stop,
                stop_sequence: None,
            },
            "finish",
        ),
    ] {
        assert_eq!(event_kind(&event), expected);
    }
}
