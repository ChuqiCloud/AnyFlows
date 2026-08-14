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

fn parsed_request(value: Value) -> CanonicalRequest {
    parse_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn minimal_request() -> CanonicalRequest {
    let mut request = CanonicalRequest::new(
        Operation::Chat,
        "claude-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("hello".to_owned())],
        )],
        false,
    );
    request.tool_choice = ToolChoice::None;
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(256).unwrap()), Vec::new()).unwrap();
    request
}

fn assert_unsupported_capability(request: &CanonicalRequest, expected: RequestCapability) {
    let error = build_request(request).expect_err("请求能力必须被明确拒绝");
    let BuildRequestError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::Anthropic);
    assert_eq!(error.capability(), ProtocolCapability::Request(expected));
}

fn tool_use(id: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: id.to_owned(),
        name: "lookup".to_owned(),
        input: json!({"query": id}),
        signature: None,
    }
}

fn tool_result(id: &str) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.to_owned(),
        content: vec![ContentBlock::Text(format!("result-{id}"))],
        structured_content: None,
        is_error: false,
    }
}

#[test]
fn builds_minimal_non_streaming_request() {
    let request = minimal_request();
    let value = build_request(&request).unwrap();

    assert_eq!(
        value,
        json!({
            "model": "claude-test",
            "max_tokens": 256,
            "messages": [{"role": "user", "content": "hello"}]
        })
    );
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn builds_streaming_request_without_private_usage_switch() {
    let mut request = minimal_request();
    request.stream = true;

    let value = build_request(&request).unwrap();

    assert_eq!(value["stream"], true);
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn round_trips_full_supported_request() {
    let request = parsed_request(json!({
        "model": "claude-sonnet-test",
        "max_tokens": 4096,
        "system": [{
            "type": "text",
            "text": "follow policy",
            "cache_control": {"type": "ephemeral", "ttl": "1h"}
        }],
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "inspect"},
                    {
                        "type": "image",
                        "source": {"type": "url", "url": "https://example.com/image.png"}
                    }
                ]
            },
            {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "checking"},
                    {
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "lookup",
                        "input": {"query": "health"},
                        "cache_control": {"type": "ephemeral"}
                    }
                ]
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_1",
                        "content": [
                            {
                                "type": "text",
                                "text": "failed",
                                "cache_control": {"type": "ephemeral"}
                            },
                            {
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": "image/png",
                                    "data": "aGVsbG8="
                                }
                            }
                        ],
                        "is_error": true,
                        "cache_control": {"type": "ephemeral"}
                    },
                    {"type": "text", "text": "continue"}
                ]
            }
        ],
        "tools": [{
            "name": "lookup",
            "description": "look up service health",
            "input_schema": {
                "type": "object",
                "properties": {"query": {"type": "string"}}
            }
        }],
        "tool_choice": {"type": "tool", "name": "lookup"},
        "thinking": {
            "type": "enabled",
            "budget_tokens": 1024,
            "display": "omitted"
        },
        "temperature": 0.2,
        "top_p": 0.9,
        "stop_sequences": ["END"],
        "metadata": {"user_id": "opaque-user"}
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["system"][0]["cache_control"]["ttl"], "1h");
    assert_eq!(value["messages"].as_array().unwrap().len(), 3);
    assert_eq!(value["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(value["messages"][2]["content"][1]["type"], "text");
    assert_eq!(value["thinking"]["display"], "omitted");
    assert_eq!(value["tool_choice"]["name"], "lookup");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn combines_parallel_tool_results_with_following_user_content() {
    let mut request = minimal_request();
    request.messages = vec![
        Message::new(
            Role::Assistant,
            vec![tool_use("toolu_1"), tool_use("toolu_2")],
        ),
        Message::new(Role::Tool, vec![tool_result("toolu_1")]),
        Message::new(Role::Tool, vec![tool_result("toolu_2")]),
        Message::new(Role::User, vec![ContentBlock::Text("continue".to_owned())]),
    ];

    let value = build_request(&request).unwrap();
    assert_eq!(value["messages"].as_array().unwrap().len(), 2);
    assert_eq!(value["messages"][1]["content"][0]["type"], "tool_result");
    assert_eq!(value["messages"][1]["content"][1]["type"], "tool_result");
    assert_eq!(value["messages"][1]["content"][2]["type"], "text");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn preserves_tool_choice_defaults_and_reasoning_modes() {
    let mut explicit_auto = minimal_request();
    explicit_auto.tool_choice = ToolChoice::Auto;
    let value = build_request(&explicit_auto).unwrap();
    assert_eq!(value["tool_choice"]["type"], "auto");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .tool_choice,
        ToolChoice::Auto
    );

    let mut with_tools = minimal_request();
    with_tools.tools.push(ToolDef {
        name: "lookup".to_owned(),
        description: None,
        input_schema: json!({"type": "object"}),
        strict: None,
    });
    with_tools.tool_choice = ToolChoice::Auto;
    let value = build_request(&with_tools).unwrap();
    assert!(value.get("tool_choice").is_none());
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        with_tools
    );

    let mut disabled = minimal_request();
    disabled.reasoning =
        Some(ReasoningConfig::new(Some(ReasoningEffort::None), None, false).unwrap());
    let value = build_request(&disabled).unwrap();
    assert_eq!(value["thinking"], json!({"type": "disabled"}));
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        disabled
    );

    let mut adaptive = minimal_request();
    adaptive.reasoning = Some(ReasoningConfig::new(None, None, false).unwrap());
    let value = build_request(&adaptive).unwrap();
    assert_eq!(
        value["thinking"],
        json!({"type": "adaptive", "display": "omitted"})
    );
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        adaptive
    );
}

#[test]
fn builds_anthropic_output_effort_and_maps_xhigh_to_max() {
    let mut high = minimal_request();
    high.reasoning = Some(ReasoningConfig::new(Some(ReasoningEffort::High), None, false).unwrap());
    let value = build_request(&high).unwrap();
    assert_eq!(value["output_config"], json!({"effort": "high"}));
    assert!(value.get("thinking").is_none());
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        high
    );

    let mut extra_high = minimal_request();
    extra_high.reasoning =
        Some(ReasoningConfig::new(Some(ReasoningEffort::ExtraHigh), None, false).unwrap());
    let value = build_request(&extra_high).unwrap();
    assert_eq!(value["output_config"], json!({"effort": "max"}));
}

#[test]
fn rejects_unmapped_top_level_capabilities() {
    let mut wrong_operation = minimal_request();
    wrong_operation.operation = Operation::Responses;
    assert_eq!(
        build_request(&wrong_operation),
        Err(BuildRequestError::UnsupportedOperation)
    );

    let mut stream_options = minimal_request();
    stream_options.stream_options = StreamOptions::new(true);
    assert_unsupported_capability(&stream_options, RequestCapability::StreamUsage);

    let mut attachments = minimal_request();
    attachments.attachments.push(Attachment {
        source: MediaSource::Url("https://example.com/file".to_owned()),
        mime_type: None,
        filename: None,
    });
    assert_unsupported_capability(&attachments, RequestCapability::Attachments);

    let mut session = minimal_request();
    session.metadata = RequestMetadata::new(None, Some("session-1".to_owned()));
    assert_unsupported_capability(&session, RequestCapability::SessionMetadata);

    let mut continuation = minimal_request();
    continuation.continuation = RequestContinuation::new(Some("resp-1".to_owned()), None, None);
    assert_unsupported_capability(
        &continuation,
        RequestCapability::PreviousResponseContinuation,
    );

    let mut strict_tool = minimal_request();
    strict_tool.tools.push(ToolDef {
        name: "lookup".to_owned(),
        description: None,
        input_schema: json!({"type": "object"}),
        strict: Some(true),
    });
    assert_unsupported_capability(&strict_tool, RequestCapability::StrictToolDefinitions);

    let mut missing_max_tokens = minimal_request();
    missing_max_tokens.sampling = Sampling::EMPTY;
    assert_eq!(
        build_request(&missing_max_tokens),
        Err(BuildRequestError::InvalidValue)
    );
}

#[test]
fn rejects_roles_and_content_outside_the_closed_subset() {
    let mut developer = minimal_request();
    developer.messages.insert(
        0,
        Message::new(
            Role::Developer,
            vec![ContentBlock::Text("developer".to_owned())],
        ),
    );
    assert_unsupported_capability(&developer, RequestCapability::MessageRole(Role::Developer));

    let mut multiple_system = minimal_request();
    multiple_system.messages.insert(
        0,
        Message::new(
            Role::System,
            vec![ContentBlock::Text("system-1".to_owned())],
        ),
    );
    multiple_system.messages.insert(
        1,
        Message::new(
            Role::System,
            vec![ContentBlock::Text("system-2".to_owned())],
        ),
    );
    assert_eq!(
        build_request(&multiple_system),
        Err(BuildRequestError::UnsupportedFeature)
    );

    for (block, capability) in [
        (
            ContentBlock::Audio {
                source: MediaSource::Base64("aGk=".to_owned()),
                mime_type: "audio/mpeg".to_owned(),
            },
            RequestCapability::AudioBase64,
        ),
        (
            ContentBlock::Thinking {
                text: "secret".to_owned(),
                signature: Some("signature".to_owned()),
            },
            RequestCapability::Thinking,
        ),
    ] {
        let mut request = minimal_request();
        request.messages[0].content = vec![block];
        assert_unsupported_capability(&request, capability);
    }

    let mut tool_result_in_user = minimal_request();
    tool_result_in_user.messages[0].content = vec![tool_result("toolu_1")];
    assert_eq!(
        build_request(&tool_result_in_user),
        Err(BuildRequestError::InvalidValue)
    );
}

#[test]
fn validates_sampling_reasoning_tools_and_metadata() {
    let mut temperature = minimal_request();
    temperature.sampling = Sampling::new(
        Some(1.01),
        None,
        Some(TokenCount::new(256).unwrap()),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(
        build_request(&temperature),
        Err(BuildRequestError::InvalidValue)
    );

    let mut max_tokens = minimal_request();
    max_tokens.sampling = Sampling::new(
        None,
        None,
        Some(TokenCount::new(1_000_001).unwrap()),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(
        build_request(&max_tokens),
        Err(BuildRequestError::InvalidValue)
    );

    let mut empty_stop = minimal_request();
    empty_stop.sampling = Sampling::new(
        None,
        None,
        Some(TokenCount::new(256).unwrap()),
        vec![String::new()],
    )
    .unwrap();
    assert_eq!(
        build_request(&empty_stop),
        Err(BuildRequestError::InvalidValue)
    );

    let mut unsupported_effort = minimal_request();
    unsupported_effort.reasoning =
        Some(ReasoningConfig::new(Some(ReasoningEffort::Minimal), None, false).unwrap());
    assert_unsupported_capability(
        &unsupported_effort,
        RequestCapability::ReasoningEffort(ReasoningEffort::Minimal),
    );

    let mut invalid_budget = minimal_request();
    invalid_budget.reasoning =
        Some(ReasoningConfig::new(None, Some(TokenCount::new(256).unwrap()), true).unwrap());
    assert_eq!(
        build_request(&invalid_budget),
        Err(BuildRequestError::InvalidValue)
    );

    let mut named_unknown = minimal_request();
    named_unknown.tools.push(ToolDef {
        name: "lookup".to_owned(),
        description: None,
        input_schema: json!({"type": "object"}),
        strict: None,
    });
    named_unknown.tool_choice = ToolChoice::Named {
        name: "missing".to_owned(),
    };
    assert_eq!(
        build_request(&named_unknown),
        Err(BuildRequestError::InvalidValue)
    );

    let mut invalid_schema = minimal_request();
    invalid_schema.tools.push(ToolDef {
        name: "lookup".to_owned(),
        description: None,
        input_schema: json!([]),
        strict: None,
    });
    assert_eq!(
        build_request(&invalid_schema),
        Err(BuildRequestError::InvalidValue)
    );

    let mut remote_schema = minimal_request();
    remote_schema.tools.push(ToolDef {
        name: "lookup".to_owned(),
        description: None,
        input_schema: json!({"$ref": "https://example.com/schema.json"}),
        strict: None,
    });
    assert_eq!(
        build_request(&remote_schema),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let mut invalid_user = minimal_request();
    invalid_user.metadata = RequestMetadata::new(Some(String::new()), None);
    assert_eq!(
        build_request(&invalid_user),
        Err(BuildRequestError::InvalidValue)
    );
}

#[test]
fn revalidates_tool_call_and_result_state() {
    let mut unresolved = minimal_request();
    unresolved.messages = vec![Message::new(Role::Assistant, vec![tool_use("toolu_1")])];
    assert_eq!(
        build_request(&unresolved),
        Err(BuildRequestError::InvalidValue)
    );

    let mut unmatched = minimal_request();
    unmatched.messages = vec![Message::new(Role::Tool, vec![tool_result("toolu_1")])];
    assert_eq!(
        build_request(&unmatched),
        Err(BuildRequestError::InvalidValue)
    );

    let mut interrupted = minimal_request();
    interrupted.messages = vec![
        Message::new(Role::Assistant, vec![tool_use("toolu_1")]),
        Message::new(
            Role::User,
            vec![ContentBlock::Text("skip result".to_owned())],
        ),
    ];
    assert_eq!(
        build_request(&interrupted),
        Err(BuildRequestError::InvalidValue)
    );

    let mut duplicate = minimal_request();
    duplicate.messages = vec![
        Message::new(
            Role::Assistant,
            vec![tool_use("toolu_1"), tool_use("toolu_1")],
        ),
        Message::new(Role::Tool, vec![tool_result("toolu_1")]),
    ];
    assert_eq!(
        build_request(&duplicate),
        Err(BuildRequestError::InvalidValue)
    );

    let mut non_object = minimal_request();
    non_object.messages = vec![Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "toolu_1".to_owned(),
            name: "lookup".to_owned(),
            input: json!([]),
            signature: None,
        }],
    )];
    assert_eq!(
        build_request(&non_object),
        Err(BuildRequestError::InvalidValue)
    );

    let mut signed_call = minimal_request();
    signed_call.messages = vec![Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "toolu_1".to_owned(),
            name: "lookup".to_owned(),
            input: json!({}),
            signature: Some("c2ln".to_owned()),
        }],
    )];
    assert_unsupported_capability(&signed_call, RequestCapability::ToolCallSignatures);

    let mut structured_result = minimal_request();
    structured_result.messages = vec![
        Message::new(Role::Assistant, vec![tool_use("toolu_1")]),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_1".to_owned(),
                content: Vec::new(),
                structured_content: Some(json!({"ok": true})),
                is_error: false,
            }],
        ),
    ];
    assert_unsupported_capability(&structured_result, RequestCapability::StructuredToolResults);
}

#[test]
fn validates_cache_and_media_boundaries() {
    let mut cache_first = minimal_request();
    cache_first.messages[0]
        .content
        .insert(0, ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes));
    assert_eq!(
        build_request(&cache_first),
        Err(BuildRequestError::InvalidValue)
    );

    let mut duplicate_cache = minimal_request();
    duplicate_cache.messages[0]
        .content
        .push(ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes));
    duplicate_cache.messages[0]
        .content
        .push(ContentBlock::CacheControl(CacheHint::Ephemeral1Hour));
    assert_eq!(
        build_request(&duplicate_cache),
        Err(BuildRequestError::InvalidValue)
    );

    let mut too_many_cache = minimal_request();
    too_many_cache.messages[0].content = (0..5)
        .flat_map(|index| {
            [
                ContentBlock::Text(format!("text-{index}")),
                ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes),
            ]
        })
        .collect();
    assert_eq!(
        build_request(&too_many_cache),
        Err(BuildRequestError::StructureLimitExceeded)
    );

    let invalid_images = [
        ContentBlock::Image {
            source: MediaSource::Url("http://example.com/image.png".to_owned()),
            mime_type: None,
        },
        ContentBlock::Image {
            source: MediaSource::Url("https://example.com/image.png".to_owned()),
            mime_type: Some("image/png".to_owned()),
        },
        ContentBlock::Image {
            source: MediaSource::Base64("aGk=".to_owned()),
            mime_type: None,
        },
        ContentBlock::Image {
            source: MediaSource::Base64("***=".to_owned()),
            mime_type: Some("image/png".to_owned()),
        },
        ContentBlock::Image {
            source: MediaSource::Base64("aGk=".to_owned()),
            mime_type: Some("image/bmp".to_owned()),
        },
    ];
    let expected = [
        BuildRequestError::InvalidValue,
        BuildRequestError::UnsupportedFeature,
        BuildRequestError::InvalidValue,
        BuildRequestError::InvalidValue,
        BuildRequestError::UnsupportedFeature,
    ];
    for (image, expected) in invalid_images.into_iter().zip(expected) {
        let mut request = minimal_request();
        request.messages[0].content = vec![image];
        assert_eq!(build_request(&request), Err(expected));
    }
}

#[test]
fn rejects_cross_protocol_raw_collisions_and_extensions() {
    let mut cross_fields = Map::new();
    cross_fields.insert("seed".to_owned(), json!(7));
    let cross_protocol = minimal_request()
        .with_validated_raw_passthrough(Some((Protocol::OpenAiChat, cross_fields)));
    assert_eq!(
        build_request(&cross_protocol),
        Err(BuildRequestError::RawProtocolMismatch)
    );

    let mut collision = Map::new();
    collision.insert("model".to_owned(), json!("other"));
    let collision =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::Anthropic, collision)));
    assert_eq!(
        build_request(&collision),
        Err(BuildRequestError::FieldConflict)
    );

    let mut extension = Map::new();
    extension.insert("top_k".to_owned(), json!(10));
    let extension =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::Anthropic, extension)));
    assert_unsupported_capability(&extension, RequestCapability::SameProtocolRaw);
}

#[test]
fn rejects_combined_body_and_node_budgets() {
    let escaped_text = "\\".repeat(1024 * 1024);
    let media = STANDARD.encode(vec![0_u8; 13 * 1024 * 1024]);
    let mut blocks = (0..8)
        .map(|_| ContentBlock::Text(escaped_text.clone()))
        .collect::<Vec<_>>();
    blocks.push(ContentBlock::Image {
        source: MediaSource::Base64(media),
        mime_type: Some("image/png".to_owned()),
    });
    let mut body = minimal_request();
    body.messages = vec![Message::new(Role::User, blocks)];
    assert_eq!(
        build_request(&body),
        Err(BuildRequestError::StructureLimitExceeded)
    );

    let schema = |tool_index| {
        let mut properties = Map::new();
        properties.insert(
            format!("items-{tool_index}"),
            json!({"type": "array", "items": vec![0; 4_088]}),
        );
        json!({"type": "object", "properties": properties})
    };
    let mut nodes = minimal_request();
    nodes.tools = (0..25)
        .map(|index| ToolDef {
            name: format!("tool-{index}"),
            description: None,
            input_schema: schema(index),
            strict: None,
        })
        .collect();
    nodes.tool_choice = ToolChoice::Auto;
    assert_eq!(
        build_request(&nodes),
        Err(BuildRequestError::StructureLimitExceeded)
    );
}

#[test]
fn build_errors_never_retain_external_values() {
    let mut request = minimal_request();
    request.model = " secret-model-canary-8f2d".to_owned();
    request.metadata = RequestMetadata::new(Some("secret-user-canary-8f2d".to_owned()), None);
    let error = build_request(&request).unwrap_err();
    let rendered = format!("{error:?}|{error}");

    assert_eq!(error, BuildRequestError::InvalidValue);
    assert!(!rendered.contains("canary-8f2d"));
    assert!(error.source().is_none());
}
