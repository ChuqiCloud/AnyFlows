use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

use super::{BuildRequestError, build_request, parse_request};
use crate::{
    Attachment, CanonicalRequest, ContentBlock, MediaSource, Message, ProtocolCapability,
    ReasoningConfig, ReasoningEffort, RequestCapability, RequestContinuation, RequestMetadata,
    Sampling, StreamOptions, TokenCount, ToolChoice, ToolDef,
};

fn parsed_request(value: Value) -> CanonicalRequest {
    parse_request("models/gemini-test", &serde_json::to_vec(&value).unwrap()).unwrap()
}

fn minimal_request() -> CanonicalRequest {
    parsed_request(json!({
        "contents": [{
            "role": "user",
            "parts": [{"text": "hello"}]
        }]
    }))
}

fn assert_unsupported_capability(request: &CanonicalRequest, expected: RequestCapability) {
    let error = build_request(request).expect_err("请求能力必须被明确拒绝");
    let BuildRequestError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::Gemini);
    assert_eq!(error.capability(), ProtocolCapability::Request(expected));
}

fn tool_use(id: &str, signature: Option<String>) -> ContentBlock {
    ContentBlock::ToolUse {
        id: id.to_owned(),
        name: "lookup".to_owned(),
        input: json!({"query": id}),
        signature,
    }
}

fn text_tool_result(id: &str, text: &str, is_error: bool) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.to_owned(),
        content: vec![ContentBlock::Text(text.to_owned())],
        structured_content: None,
        is_error,
    }
}

#[test]
fn builds_minimal_body_without_model_field() {
    let request = minimal_request();
    let value = build_request(&request).unwrap();

    assert_eq!(
        value,
        json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": "hello"}]
            }],
            "toolConfig": {
                "functionCallingConfig": {"mode": "NONE"}
            }
        })
    );
    assert!(value.get("model").is_none());
    assert_eq!(
        parse_request("models/gemini-test", &serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn round_trips_full_supported_request() {
    let thought_signature = STANDARD.encode("thought-signature");
    let tool_signature = STANDARD.encode("tool-signature");
    let request = parsed_request(json!({
        "systemInstruction": {
            "parts": [{"text": "follow policy"}]
        },
        "contents": [
            {
                "role": "user",
                "parts": [
                    {"text": "inspect"},
                    {"inlineData": {"mimeType": "image/png", "data": "aGk="}},
                    {"inlineData": {"mimeType": "audio/wav", "data": "aGk="}}
                ]
            },
            {
                "role": "model",
                "parts": [
                    {
                        "text": "checking",
                        "thought": true,
                        "thoughtSignature": thought_signature
                    },
                    {
                        "functionCall": {
                            "id": "call-1",
                            "name": "lookup",
                            "args": {"query": "health"}
                        },
                        "thoughtSignature": tool_signature
                    }
                ]
            },
            {
                "role": "user",
                "parts": [
                    {
                        "functionResponse": {
                            "id": "call-1",
                            "name": "lookup",
                            "response": {"result": {"ok": true}, "nullable": null}
                        }
                    },
                    {"text": "continue"}
                ]
            }
        ],
        "tools": [{
            "functionDeclarations": [{
                "name": "lookup",
                "description": "look up service health",
                "parametersJsonSchema": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}}
                }
            }]
        }],
        "toolConfig": {
            "functionCallingConfig": {
                "mode": "ANY",
                "allowedFunctionNames": ["lookup"]
            }
        },
        "generationConfig": {
            "temperature": 1.5,
            "topP": 0.9,
            "maxOutputTokens": 2048,
            "stopSequences": ["END"],
            "thinkingConfig": {
                "includeThoughts": true,
                "thinkingLevel": "HIGH"
            }
        }
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["contents"].as_array().unwrap().len(), 3);
    assert_eq!(
        value["contents"][2]["parts"][0]["functionResponse"]["response"],
        json!({"result": {"ok": true}, "nullable": null})
    );
    assert_eq!(
        value["contents"][1]["parts"][1]["thoughtSignature"],
        tool_signature
    );
    assert_eq!(
        value["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]["type"],
        "object"
    );
    assert_eq!(
        parse_request("models/gemini-test", &serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn combines_parallel_text_results_with_following_user_content() {
    let mut request = minimal_request();
    request.messages = vec![
        Message::new(
            Role::Assistant,
            vec![tool_use("call-1", None), tool_use("call-2", None)],
        ),
        Message::new(Role::Tool, vec![text_tool_result("call-1", "ready", false)]),
        Message::new(
            Role::Tool,
            vec![text_tool_result("call-2", "timeout", true)],
        ),
        Message::new(Role::User, vec![ContentBlock::Text("continue".to_owned())]),
    ];

    let value = build_request(&request).unwrap();
    assert_eq!(value["contents"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["contents"][1]["parts"][0]["functionResponse"],
        json!({
            "id": "call-1",
            "name": "lookup",
            "response": {"result": "ready"}
        })
    );
    assert_eq!(
        value["contents"][1]["parts"][1]["functionResponse"]["response"],
        json!({"error": "timeout"})
    );
    assert_eq!(
        value["contents"][1]["parts"][2],
        json!({"text": "continue"})
    );

    // Gemini 会把显式包装后的结果归一为结构化对象，不再猜测原始文本协议。
    let reparsed =
        parse_request("models/gemini-test", &serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        &reparsed.messages[1].content[0],
        ContentBlock::ToolResult {
            content,
            structured_content: Some(result),
            is_error: false,
            ..
        } if content.is_empty() && result == &json!({"result": "ready"})
    ));
    assert!(matches!(
        &reparsed.messages[2].content[0],
        ContentBlock::ToolResult {
            structured_content: Some(result),
            is_error: true,
            ..
        } if result == &json!({"error": "timeout"})
    ));
}

#[test]
fn maps_tool_choices_sampling_and_reasoning() {
    let mut request = minimal_request();
    request.tools = vec![ToolDef {
        name: "lookup".to_owned(),
        description: Some("lookup status".to_owned()),
        input_schema: json!({"type": "object"}),
        strict: None,
    }];
    request.tool_choice = ToolChoice::Required;
    request.sampling = Sampling::new(
        Some(2.0),
        Some(1.0),
        Some(TokenCount::new(1024).unwrap()),
        vec!["END".to_owned()],
    )
    .unwrap();
    request.reasoning =
        Some(ReasoningConfig::new(Some(ReasoningEffort::Minimal), None, true).unwrap());

    let value = build_request(&request).unwrap();
    assert_eq!(
        value["toolConfig"]["functionCallingConfig"],
        json!({"mode": "ANY"})
    );
    assert_eq!(
        value["generationConfig"]["thinkingConfig"],
        json!({"thinkingLevel": "MINIMAL", "includeThoughts": true})
    );

    request.tool_choice = ToolChoice::Named {
        name: "lookup".to_owned(),
    };
    request.reasoning =
        Some(ReasoningConfig::new(None, Some(TokenCount::new(256).unwrap()), false).unwrap());
    let value = build_request(&request).unwrap();
    assert_eq!(
        value["toolConfig"]["functionCallingConfig"],
        json!({"mode": "ANY", "allowedFunctionNames": ["lookup"]})
    );
    assert_eq!(
        value["generationConfig"]["thinkingConfig"],
        json!({"thinkingBudget": 256})
    );

    request.tool_choice = ToolChoice::None;
    request.reasoning =
        Some(ReasoningConfig::new(Some(ReasoningEffort::None), None, false).unwrap());
    let value = build_request(&request).unwrap();
    assert_eq!(value["toolConfig"]["functionCallingConfig"]["mode"], "NONE");
    assert_eq!(
        value["generationConfig"]["thinkingConfig"],
        json!({"thinkingBudget": 0})
    );
}

#[test]
fn rejects_unmapped_top_level_capabilities_and_invalid_parameters() {
    let mut operation = minimal_request();
    operation.operation = Operation::Embedding;
    assert_eq!(
        build_request(&operation),
        Err(BuildRequestError::UnsupportedOperation)
    );

    let mut streaming = minimal_request();
    streaming.stream = true;
    assert_unsupported_capability(&streaming, RequestCapability::Streaming);

    let mut stream_options = minimal_request();
    stream_options.stream_options = StreamOptions::new(true);
    assert_unsupported_capability(&stream_options, RequestCapability::StreamUsage);

    let mut attachment = minimal_request();
    attachment.attachments.push(Attachment {
        source: MediaSource::Base64("aGk=".to_owned()),
        mime_type: Some("image/png".to_owned()),
        filename: None,
    });
    assert_unsupported_capability(&attachment, RequestCapability::Attachments);

    let mut metadata = minimal_request();
    metadata.metadata = RequestMetadata::new(Some("opaque-user".to_owned()), None);
    assert_unsupported_capability(&metadata, RequestCapability::UserMetadata);

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

    let mut invalid_model = minimal_request();
    invalid_model.model = "models/gemini-test".to_owned();
    assert_eq!(
        build_request(&invalid_model),
        Err(BuildRequestError::InvalidValue)
    );

    let mut invalid_temperature = minimal_request();
    invalid_temperature.sampling = Sampling::new(Some(2.1), None, None, Vec::new()).unwrap();
    assert_eq!(
        build_request(&invalid_temperature),
        Err(BuildRequestError::InvalidValue)
    );

    let mut invalid_choice = minimal_request();
    invalid_choice.tool_choice = ToolChoice::Required;
    assert_eq!(
        build_request(&invalid_choice),
        Err(BuildRequestError::InvalidValue)
    );

    let mut unsupported_effort = minimal_request();
    unsupported_effort.reasoning =
        Some(ReasoningConfig::new(Some(ReasoningEffort::ExtraHigh), None, false).unwrap());
    assert_unsupported_capability(
        &unsupported_effort,
        RequestCapability::ReasoningEffort(ReasoningEffort::ExtraHigh),
    );

    let mut conflicting_reasoning = minimal_request();
    conflicting_reasoning.reasoning = Some(
        ReasoningConfig::new(
            Some(ReasoningEffort::Low),
            Some(TokenCount::new(128).unwrap()),
            false,
        )
        .unwrap(),
    );
    assert_eq!(
        build_request(&conflicting_reasoning),
        Err(BuildRequestError::InvalidValue)
    );
}

#[test]
fn revalidates_tool_call_and_result_state() {
    let mut orphan = minimal_request();
    orphan.messages = vec![Message::new(
        Role::Tool,
        vec![text_tool_result("call-1", "ready", false)],
    )];
    assert_eq!(build_request(&orphan), Err(BuildRequestError::InvalidValue));

    let mut unresolved = minimal_request();
    unresolved.messages = vec![Message::new(
        Role::Assistant,
        vec![tool_use("call-1", None)],
    )];
    assert_eq!(
        build_request(&unresolved),
        Err(BuildRequestError::InvalidValue)
    );

    let mut wrong_id = minimal_request();
    wrong_id.messages = vec![
        Message::new(Role::Assistant, vec![tool_use("call-1", None)]),
        Message::new(Role::Tool, vec![text_tool_result("call-2", "ready", false)]),
    ];
    assert_eq!(
        build_request(&wrong_id),
        Err(BuildRequestError::InvalidValue)
    );

    let mut duplicate_call = minimal_request();
    duplicate_call.messages = vec![Message::new(
        Role::Assistant,
        vec![tool_use("call-1", None), tool_use("call-1", None)],
    )];
    assert_eq!(
        build_request(&duplicate_call),
        Err(BuildRequestError::InvalidValue)
    );

    let mut structured_conflict = minimal_request();
    structured_conflict.messages = vec![
        Message::new(Role::Assistant, vec![tool_use("call-1", None)]),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_owned(),
                content: Vec::new(),
                structured_content: Some(json!({"error": "failed"})),
                is_error: false,
            }],
        ),
    ];
    assert_eq!(
        build_request(&structured_conflict),
        Err(BuildRequestError::InvalidValue)
    );

    let mut ambiguous = minimal_request();
    ambiguous.messages = vec![
        Message::new(Role::Assistant, vec![tool_use("call-1", None)]),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_owned(),
                content: vec![ContentBlock::Text("ready".to_owned())],
                structured_content: Some(json!({"result": "ready"})),
                is_error: false,
            }],
        ),
    ];
    assert_eq!(
        build_request(&ambiguous),
        Err(BuildRequestError::InvalidValue)
    );

    let mut rich_text = minimal_request();
    rich_text.messages = vec![
        Message::new(Role::Assistant, vec![tool_use("call-1", None)]),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_owned(),
                content: vec![
                    ContentBlock::Text("one".to_owned()),
                    ContentBlock::Text("two".to_owned()),
                ],
                structured_content: None,
                is_error: false,
            }],
        ),
    ];
    assert_eq!(
        build_request(&rich_text),
        Err(BuildRequestError::UnsupportedFeature)
    );
}

#[test]
fn validates_media_signatures_and_closed_roles() {
    let signature = STANDARD.encode("signature");
    let mut request = minimal_request();
    request.messages = vec![
        Message::new(
            Role::Assistant,
            vec![
                ContentBlock::Thinking {
                    text: "reasoning".to_owned(),
                    signature: Some(signature.clone()),
                },
                tool_use("call-1", Some(signature.clone())),
            ],
        ),
        Message::new(Role::Tool, vec![text_tool_result("call-1", "ready", false)]),
    ];
    let value = build_request(&request).unwrap();
    assert_eq!(
        value["contents"][0]["parts"][0]["thoughtSignature"],
        signature
    );
    assert_eq!(
        value["contents"][0]["parts"][1]["thoughtSignature"],
        signature
    );

    let mut invalid_signature = minimal_request();
    invalid_signature.messages = vec![Message::new(
        Role::Assistant,
        vec![ContentBlock::Thinking {
            text: "reasoning".to_owned(),
            signature: Some("***".to_owned()),
        }],
    )];
    assert_eq!(
        build_request(&invalid_signature),
        Err(BuildRequestError::InvalidValue)
    );

    let mut remote_media = minimal_request();
    remote_media.messages[0].content = vec![ContentBlock::Image {
        source: MediaSource::Url("https://example.com/image.png".to_owned()),
        mime_type: None,
    }];
    assert_unsupported_capability(&remote_media, RequestCapability::ImageUrl);

    let mut mismatched_media = minimal_request();
    mismatched_media.messages[0].content = vec![ContentBlock::Image {
        source: MediaSource::Base64("aGk=".to_owned()),
        mime_type: Some("audio/wav".to_owned()),
    }];
    assert_eq!(
        build_request(&mismatched_media),
        Err(BuildRequestError::InvalidValue)
    );

    let mut developer = minimal_request();
    developer.messages.insert(
        0,
        Message::new(
            Role::Developer,
            vec![ContentBlock::Text("policy".to_owned())],
        ),
    );
    assert_unsupported_capability(&developer, RequestCapability::MessageRole(Role::Developer));
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
        minimal_request().with_validated_raw_passthrough(Some((Protocol::Gemini, collision)));
    assert_eq!(
        build_request(&collision),
        Err(BuildRequestError::FieldConflict)
    );

    let mut extension = Map::new();
    extension.insert("seed".to_owned(), json!(7));
    let extension =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::Gemini, extension)));
    assert_unsupported_capability(&extension, RequestCapability::SameProtocolRaw);
}

#[test]
fn rejects_combined_output_budgets_and_redacts_errors() {
    let escaped_text = "\\".repeat(1024 * 1024);
    let media = STANDARD.encode(vec![0_u8; 13 * 1024 * 1024]);
    let mut blocks = (0..8)
        .map(|_| ContentBlock::Text(escaped_text.clone()))
        .collect::<Vec<_>>();
    blocks.push(ContentBlock::Image {
        source: MediaSource::Base64(media),
        mime_type: Some("image/png".to_owned()),
    });
    let mut oversized = minimal_request();
    oversized.messages = vec![Message::new(Role::User, blocks)];
    assert_eq!(
        build_request(&oversized),
        Err(BuildRequestError::StructureLimitExceeded)
    );

    let mut secret = minimal_request();
    secret.model = " secret-model-canary-8f2d".to_owned();
    let error = build_request(&secret).unwrap_err();
    let rendered = format!("{error:?}|{error}");
    assert_eq!(error, BuildRequestError::InvalidValue);
    assert!(!rendered.contains("canary-8f2d"));
    assert!(error.source().is_none());
}
