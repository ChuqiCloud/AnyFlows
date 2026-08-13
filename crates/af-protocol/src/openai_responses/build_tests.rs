use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

use super::{BuildRequestError, build_request, parse_request};
use crate::{
    Attachment, CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message,
    ProtocolCapability, ReasoningConfig, ReasoningEffort, RequestCapability, RequestContinuation,
    RequestMetadata, Sampling, StreamOptions, TokenCount, ToolChoice, ToolDef,
};

fn assert_unsupported_capability(request: &CanonicalRequest, expected: RequestCapability) {
    let error = build_request(request).expect_err("请求能力必须被明确拒绝");
    let BuildRequestError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::OpenAiResponses);
    assert_eq!(error.capability(), ProtocolCapability::Request(expected));
}

fn parsed_request(value: Value) -> CanonicalRequest {
    parse_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn minimal_request() -> CanonicalRequest {
    let mut request = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("hello".to_owned())],
        )],
        false,
    );
    request.tool_choice = ToolChoice::None;
    request
}

fn tool_call(id: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: id.to_owned(),
        name: "lookup".to_owned(),
        input: json!({"id": 7}),
        signature: None,
    }
}

fn tool_result(id: &str) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.to_owned(),
        content: vec![ContentBlock::Text("done".to_owned())],
        structured_content: None,
        is_error: false,
    }
}

#[test]
fn builds_minimal_request_in_both_response_modes() {
    let value = build_request(&minimal_request()).unwrap();

    assert_eq!(
        value,
        json!({
            "model": "gpt-test",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            }],
            "stream": false,
            "tool_choice": "none"
        })
    );
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        minimal_request()
    );

    let mut streaming = minimal_request();
    streaming.stream = true;
    let value = build_request(&streaming).unwrap();
    assert_eq!(value["stream"], true);
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        streaming
    );
}

#[test]
fn round_trips_stateless_encrypted_reasoning_and_include() {
    let request = parsed_request(json!({
        "model": "gpt-5.5",
        "input": [{
            "type": "reasoning",
            "encrypted_content": "encrypted-reasoning-canary",
            "summary": [{"type": "summary_text", "text": "summary"}]
        }, {
            "role": "user",
            "content": [{"type": "input_text", "text": "continue"}]
        }],
        "include": ["reasoning.encrypted_content", "future.output.details"],
        "store": false,
        "stream": true
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["input"][0]["type"], "reasoning");
    assert_eq!(
        value["input"][0]["encrypted_content"],
        "encrypted-reasoning-canary"
    );
    assert_eq!(
        value["include"],
        json!(["reasoning.encrypted_content", "future.output.details"])
    );
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn round_trips_compaction_context_and_item() {
    let request = parsed_request(json!({
        "model": "gpt-5.5",
        "context_management": [{
            "type": "compaction",
            "compact_threshold": 20000
        }],
        "input": [{
            "type": "compaction",
            "id": "cmp_input_1",
            "encrypted_content": "opaque-compaction"
        }]
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(
        value["context_management"],
        json!([
            {"type": "compaction", "compact_threshold": 20000}
        ])
    );
    assert_eq!(
        value["input"][0],
        json!({
            "type": "compaction",
            "id": "cmp_input_1",
            "encrypted_content": "opaque-compaction"
        })
    );
}

#[test]
fn round_trips_messages_tools_sampling_continuation_and_raw_fields() {
    let request = parsed_request(json!({
        "model": "gpt-test",
        "input": [
            {
                "type": "message",
                "role": "system",
                "content": [{"type": "input_text", "text": "system"}]
            },
            {
                "type": "message",
                "role": "developer",
                "content": [{"type": "input_text", "text": "developer"}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "inspect"},
                    {
                        "type": "input_image",
                        "detail": "auto",
                        "image_url": "https://media.example.test/image.png"
                    },
                    {
                        "type": "input_image",
                        "image_url": "data:image/png;base64,aGk="
                    }
                ]
            },
            {
                "type": "function_call",
                "call_id": "call-1",
                "name": "lookup",
                "arguments": "{\"id\":7}"
            },
            {
                "type": "function_call_output",
                "call_id": "call-1",
                "output": "done"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "finished"}]
            }
        ],
        "tools": [{
            "type": "function",
            "name": "lookup",
            "description": "Lookup a record",
            "parameters": {
                "type": "object",
                "properties": {"id": {"type": "integer"}}
            },
            "strict": true
        }],
        "tool_choice": {"type": "function", "name": "lookup"},
        "reasoning": {"effort": "xhigh"},
        "temperature": 0.25,
        "top_p": 0.75,
        "max_output_tokens": 64,
        "user": "user-1",
        "previous_response_id": "resp-1",
        "prompt_cache_key": "cache-1",
        "metadata": {"trace": "on"},
        "parallel_tool_calls": true,
        "prompt_cache_options": {"mode": "implicit", "ttl": "30m"},
        "safety_identifier": "safe-user",
        "service_tier": "flex",
        "store": false,
        "truncation": "disabled"
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["input"][3]["type"], "function_call");
    assert_eq!(value["input"][4]["type"], "function_call_output");
    assert_eq!(value["tools"][0]["name"], "lookup");
    assert_eq!(value["tools"][0]["strict"], true);
    assert_eq!(
        value["tool_choice"],
        json!({"type": "function", "name": "lookup"})
    );
    assert_eq!(value["reasoning"]["effort"], "xhigh");
    assert_eq!(value["previous_response_id"], "resp-1");
    assert_eq!(value["prompt_cache_key"], "cache-1");
    assert_eq!(value["metadata"]["trace"], "on");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn preserves_conversation_and_remote_tool_output() {
    let request = parsed_request(json!({
        "model": "gpt-test",
        "conversation": {"id": "conv-1"},
        "prompt_cache_key": "cache-1",
        "input": [{
            "type": "function_call_output",
            "call_id": "call-remote",
            "output": [
                {"type": "input_text", "text": "done"},
                {
                    "type": "input_image",
                    "image_url": "data:image/png;base64,aGk="
                }
            ]
        }]
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["conversation"], "conv-1");
    assert_eq!(value["prompt_cache_key"], "cache-1");
    assert!(value.get("previous_response_id").is_none());
    assert_eq!(value["input"][0]["output"][1]["detail"], "auto");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn encodes_undeclared_strictness_as_false() {
    let mut request = minimal_request();
    request.tools.push(ToolDef {
        name: "lookup".to_owned(),
        description: None,
        input_schema: json!({"type": "object"}),
        strict: None,
    });
    request.tool_choice = ToolChoice::Auto;

    let value = build_request(&request).unwrap();
    assert_eq!(value["tools"][0]["strict"], false);
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .tools[0]
            .strict,
        Some(false)
    );
}

#[test]
fn splits_assistant_text_and_function_calls_into_ordered_items() {
    let request = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![
            Message::new(
                Role::Assistant,
                vec![
                    ContentBlock::Text("checking".to_owned()),
                    tool_call("call-1"),
                ],
            ),
            Message::new(Role::Tool, vec![tool_result("call-1")]),
        ],
        false,
    );

    let value = build_request(&request).unwrap();
    assert_eq!(value["input"][0]["role"], "assistant");
    assert_eq!(value["input"][0]["content"][0]["type"], "output_text");
    assert_eq!(value["input"][1]["type"], "function_call");
    assert_eq!(value["input"][2]["type"], "function_call_output");
}

#[test]
fn serializes_unambiguous_structured_tool_output() {
    let mut request = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-remote".to_owned(),
                content: Vec::new(),
                structured_content: Some(json!({"ok": true})),
                is_error: false,
            }],
        )],
        false,
    );
    request.continuation = RequestContinuation::new(Some("resp-1".to_owned()), None, None);

    let value = build_request(&request).unwrap();
    assert_eq!(value["input"][0]["output"], "{\"ok\":true}");

    let mut ambiguous = request.clone();
    let ContentBlock::ToolResult { content, .. } = &mut ambiguous.messages[0].content[0] else {
        unreachable!();
    };
    content.push(ContentBlock::Text("also text".to_owned()));
    assert_eq!(
        build_request(&ambiguous),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let mut failed = request;
    let ContentBlock::ToolResult { is_error, .. } = &mut failed.messages[0].content[0] else {
        unreachable!();
    };
    *is_error = true;
    assert_unsupported_capability(&failed, RequestCapability::ToolResultErrors);
}

#[test]
fn rejects_wrong_operation_and_unmodeled_request_features() {
    let mut wrong_operation = minimal_request();
    wrong_operation.operation = Operation::Chat;
    assert_eq!(
        build_request(&wrong_operation),
        Err(BuildRequestError::UnsupportedOperation)
    );

    let mut stream_options = minimal_request();
    stream_options.stream_options = StreamOptions::new(true);
    assert_unsupported_capability(&stream_options, RequestCapability::StreamUsage);

    let mut attachment = minimal_request();
    attachment.attachments.push(Attachment {
        source: MediaSource::Url("https://media.example.test/file".to_owned()),
        mime_type: None,
        filename: None,
    });
    assert_unsupported_capability(&attachment, RequestCapability::Attachments);

    let mut session = minimal_request();
    session.metadata = RequestMetadata::new(None, Some("session-1".to_owned()));
    assert_unsupported_capability(&session, RequestCapability::SessionMetadata);

    let mut stop = minimal_request();
    stop.sampling = Sampling::new(None, None, None, vec!["END".to_owned()]).unwrap();
    assert_unsupported_capability(&stop, RequestCapability::StopSequences);

    let mut reasoning = minimal_request();
    reasoning.reasoning = Some(
        ReasoningConfig::new(
            Some(ReasoningEffort::High),
            Some(TokenCount::new(64).unwrap()),
            false,
        )
        .unwrap(),
    );
    assert_unsupported_capability(&reasoning, RequestCapability::ReasoningBudget);
}

#[test]
fn rejects_unrepresentable_content_and_media() {
    let unsupported = [
        (
            ContentBlock::Audio {
                source: MediaSource::Base64("aGk=".to_owned()),
                mime_type: "audio/mpeg".to_owned(),
            },
            RequestCapability::AudioBase64,
        ),
        (
            ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes),
            RequestCapability::CacheControl(CacheHint::Ephemeral5Minutes),
        ),
    ];
    for (block, capability) in unsupported {
        let request = CanonicalRequest::new(
            Operation::Responses,
            "gpt-test".to_owned(),
            vec![Message::new(Role::User, vec![block])],
            false,
        );
        assert_unsupported_capability(&request, capability);
    }

    let missing_reasoning_signature = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::Thinking {
                text: "secret".to_owned(),
                signature: None,
            }],
        )],
        false,
    );
    assert_eq!(
        build_request(&missing_reasoning_signature),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let misplaced_reasoning = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Thinking {
                text: "secret".to_owned(),
                signature: Some("encrypted".to_owned()),
            }],
        )],
        false,
    );
    assert_eq!(
        build_request(&misplaced_reasoning),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let late_reasoning = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![
                ContentBlock::Text("answer".to_owned()),
                ContentBlock::Thinking {
                    text: "secret".to_owned(),
                    signature: Some("encrypted".to_owned()),
                },
            ],
        )],
        false,
    );
    assert_eq!(
        build_request(&late_reasoning),
        Err(BuildRequestError::InvalidValue)
    );

    let missing_mime = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: MediaSource::Base64("aGk=".to_owned()),
                mime_type: None,
            }],
        )],
        false,
    );
    assert_eq!(
        build_request(&missing_mime),
        Err(BuildRequestError::InvalidValue)
    );

    let remote_mime = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: MediaSource::Url("https://media.example.test/image.png".to_owned()),
                mime_type: Some("image/png".to_owned()),
            }],
        )],
        false,
    );
    assert_eq!(
        build_request(&remote_mime),
        Err(BuildRequestError::UnsupportedFeature)
    );
}

#[test]
fn revalidates_tool_call_and_output_associations() {
    let dangling = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(Role::Assistant, vec![tool_call("call-1")])],
        false,
    );
    assert_eq!(
        build_request(&dangling),
        Err(BuildRequestError::InvalidValue)
    );

    let orphan = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(Role::Tool, vec![tool_result("call-1")])],
        false,
    );
    assert_eq!(build_request(&orphan), Err(BuildRequestError::InvalidValue));

    let interrupted = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![
            Message::new(Role::Assistant, vec![tool_call("call-1")]),
            Message::new(Role::User, vec![ContentBlock::Text("late".to_owned())]),
        ],
        false,
    );
    assert_eq!(
        build_request(&interrupted),
        Err(BuildRequestError::InvalidValue)
    );

    let duplicate = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![tool_call("call-1"), tool_call("call-1")],
        )],
        false,
    );
    assert_eq!(
        build_request(&duplicate),
        Err(BuildRequestError::InvalidValue)
    );

    let text_after_call = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![tool_call("call-1"), ContentBlock::Text("late".to_owned())],
        )],
        false,
    );
    assert_eq!(
        build_request(&text_after_call),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let non_object = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call-1".to_owned(),
                name: "lookup".to_owned(),
                input: json!([1, 2]),
                signature: None,
            }],
        )],
        false,
    );
    assert_eq!(
        build_request(&non_object),
        Err(BuildRequestError::InvalidValue)
    );

    let signed = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call-1".to_owned(),
                name: "lookup".to_owned(),
                input: json!({}),
                signature: Some("c2ln".to_owned()),
            }],
        )],
        false,
    );
    assert_unsupported_capability(&signed, RequestCapability::ToolCallSignatures);
}

#[test]
fn validates_continuation_tools_and_sampling() {
    let mut conflict = minimal_request();
    conflict.continuation =
        RequestContinuation::new(Some("resp-1".to_owned()), Some("conv-1".to_owned()), None);
    assert_eq!(
        build_request(&conflict),
        Err(BuildRequestError::FieldConflict)
    );

    let mut invalid_id = minimal_request();
    invalid_id.continuation = RequestContinuation::new(Some(" resp-1".to_owned()), None, None);
    assert_eq!(
        build_request(&invalid_id),
        Err(BuildRequestError::InvalidValue)
    );

    let mut unknown_choice = minimal_request();
    unknown_choice.tool_choice = ToolChoice::Named {
        name: "missing".to_owned(),
    };
    assert_eq!(
        build_request(&unknown_choice),
        Err(BuildRequestError::InvalidValue)
    );

    let mut duplicate_tools = minimal_request();
    duplicate_tools.tools = vec![
        ToolDef {
            name: "lookup".to_owned(),
            description: None,
            input_schema: json!({"type": "object"}),
            strict: Some(true),
        },
        ToolDef {
            name: "lookup".to_owned(),
            description: None,
            input_schema: json!({"type": "object"}),
            strict: Some(false),
        },
    ];
    assert_eq!(
        build_request(&duplicate_tools),
        Err(BuildRequestError::InvalidValue)
    );

    let mut invalid_temperature = minimal_request();
    invalid_temperature.sampling = Sampling::new(Some(3.0), None, None, Vec::new()).unwrap();
    assert_eq!(
        build_request(&invalid_temperature),
        Err(BuildRequestError::InvalidValue)
    );
}

#[test]
fn rejects_cross_protocol_raw_collisions_and_invalid_values() {
    let mut fields = Map::new();
    fields.insert("store".to_owned(), json!(false));
    let cross_protocol =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::Anthropic, fields)));
    assert_eq!(
        build_request(&cross_protocol),
        Err(BuildRequestError::RawProtocolMismatch)
    );

    let mut collision = Map::new();
    collision.insert("model".to_owned(), json!("other"));
    let collision = minimal_request()
        .with_validated_raw_passthrough(Some((Protocol::OpenAiResponses, collision)));
    assert_eq!(
        build_request(&collision),
        Err(BuildRequestError::FieldConflict)
    );

    let mut invalid = Map::new();
    invalid.insert("unknown".to_owned(), json!(true));
    let invalid = minimal_request()
        .with_validated_raw_passthrough(Some((Protocol::OpenAiResponses, invalid)));
    assert_eq!(
        build_request(&invalid),
        Err(BuildRequestError::InvalidValue)
    );
}

#[test]
fn build_errors_never_retain_external_values() {
    let mut request = minimal_request();
    request.model = " model-canary-8f2d".to_owned();
    let error = build_request(&request).unwrap_err();
    let rendered = format!("{error:?}|{error}");

    assert_eq!(error, BuildRequestError::InvalidValue);
    assert!(!rendered.contains("canary-8f2d"));
    assert!(error.source().is_none());
}

#[test]
fn rejects_combined_body_budget_after_local_limits_pass() {
    let escaped_text = "\\".repeat(1024 * 1024);
    let media = STANDARD.encode(vec![0_u8; 13 * 1024 * 1024]);
    let mut blocks = (0..8)
        .map(|_| ContentBlock::Text(escaped_text.clone()))
        .collect::<Vec<_>>();
    blocks.push(ContentBlock::Image {
        source: MediaSource::Base64(media),
        mime_type: Some("image/png".to_owned()),
    });
    let request = CanonicalRequest::new(
        Operation::Responses,
        "gpt-test".to_owned(),
        vec![Message::new(Role::User, blocks)],
        false,
    );

    assert_eq!(
        build_request(&request),
        Err(BuildRequestError::StructureLimitExceeded)
    );
}
