use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

use super::{BuildRequestError, build_request, parse_request};
use crate::{
    Attachment, CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message,
    ProtocolCapability, ReasoningConfig, RequestCapability, RequestContinuation, RequestMetadata,
    StreamOptions, TokenCount,
};

fn parsed_request(value: Value) -> CanonicalRequest {
    parse_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn minimal_request() -> CanonicalRequest {
    let mut request = CanonicalRequest::new(
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("hello".to_owned())],
        )],
        false,
    );
    request.tool_choice = crate::ToolChoice::None;
    request
}

fn assert_unsupported_capability(request: &CanonicalRequest, expected: RequestCapability) {
    let error = build_request(request).expect_err("请求能力必须被明确拒绝");
    let BuildRequestError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::OpenAiChat);
    assert_eq!(error.capability(), ProtocolCapability::Request(expected));
}

#[test]
fn builds_minimal_non_streaming_request() {
    let value = build_request(&minimal_request()).unwrap();

    assert_eq!(
        value,
        json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false,
            "tool_choice": "none"
        })
    );
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        minimal_request()
    );
}

#[test]
fn preserves_explicit_auto_tool_choice_without_tools() {
    let mut request = minimal_request();
    request.tool_choice = crate::ToolChoice::Auto;
    let value = build_request(&request).unwrap();
    assert_eq!(value["tool_choice"], "auto");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .tool_choice,
        crate::ToolChoice::Auto
    );
}

#[test]
fn preserves_tools_reasoning_sampling_metadata_and_raw_fields() {
    let request = parsed_request(json!({
        "model": "gpt-test",
        "messages": [{ "role": "user", "content": "hello" }],
        "tools": [{
            "type": "function",
            "function": {
                "name": "lookup",
                "description": "Lookup a record",
                "parameters": {
                    "type": "object",
                    "properties": { "id": { "type": "integer" } }
                }
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": { "name": "lookup" }
        },
        "reasoning_effort": "xhigh",
        "temperature": 0.25,
        "top_p": 0.75,
        "max_tokens": 64,
        "stop": "END",
        "user": "user-1",
        "seed": 7,
        "service_tier": "flex"
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["reasoning_effort"], "xhigh");
    assert_eq!(value["max_completion_tokens"], 64);
    assert!(value.get("max_tokens").is_none());
    assert_eq!(value["stop"], json!(["END"]));
    assert_eq!(value["user"], "user-1");
    assert_eq!(value["seed"], 7);
    assert_eq!(value["service_tier"], "flex");
    assert_eq!(value["tools"][0]["function"]["name"], "lookup");
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn maps_fixed_reasoning_budget_to_effort_and_keeps_dynamic_budget_automatic() {
    let mut fixed = minimal_request();
    fixed.reasoning =
        Some(ReasoningConfig::new(None, Some(TokenCount::new(2048).unwrap()), true).unwrap());
    assert_eq!(build_request(&fixed).unwrap()["reasoning_effort"], "low");

    let mut dynamic = minimal_request();
    dynamic.reasoning = Some(
        ReasoningConfig::new(None, None, true)
            .unwrap()
            .with_dynamic_budget(true)
            .unwrap(),
    );
    assert!(
        build_request(&dynamic)
            .unwrap()
            .get("reasoning_effort")
            .is_none()
    );
}

#[test]
fn round_trips_multimodal_messages_and_tool_results() {
    let request = parsed_request(json!({
        "model": "gpt-test",
        "messages": [
            { "role": "system", "content": "system" },
            { "role": "developer", "content": [{ "type": "text", "text": "developer" }] },
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": "inspect" },
                    {
                        "type": "image_url",
                        "image_url": { "url": "https://media.example.test/image.png" }
                    },
                    {
                        "type": "image_url",
                        "image_url": { "url": "data:image/png;base64,aGk=" }
                    },
                    {
                        "type": "input_audio",
                        "input_audio": { "data": "aGk=", "format": "mp3" }
                    }
                ]
            },
            {
                "role": "assistant",
                "content": "checking",
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": { "name": "lookup", "arguments": "{\"id\":7}" }
                }]
            },
            { "role": "tool", "tool_call_id": "call-1", "content": "done" }
        ]
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(
        value["messages"][2]["content"][2]["image_url"]["url"],
        "data:image/png;base64,aGk="
    );
    assert_eq!(
        value["messages"][3]["tool_calls"][0]["function"]["arguments"],
        "{\"id\":7}"
    );
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn rejects_wrong_operation_and_unmapped_request_fields() {
    let mut wrong_operation = minimal_request();
    wrong_operation.operation = Operation::Responses;
    assert_eq!(
        build_request(&wrong_operation),
        Err(BuildRequestError::UnsupportedOperation)
    );

    let mut streaming = minimal_request();
    streaming.stream = true;
    streaming.stream_options = StreamOptions::new(true);
    let streaming_value = build_request(&streaming).unwrap();
    assert_eq!(streaming_value["stream"], true);
    assert_eq!(streaming_value["stream_options"]["include_usage"], true);
    assert_eq!(
        parse_request(&serde_json::to_vec(&streaming_value).unwrap()).unwrap(),
        streaming
    );

    let mut invalid_options = minimal_request();
    invalid_options.stream_options = StreamOptions::new(true);
    assert_eq!(
        build_request(&invalid_options),
        Err(BuildRequestError::InvalidValue)
    );

    let mut attachments = minimal_request();
    attachments.attachments.push(Attachment {
        source: MediaSource::Url("https://media.example.test/file".to_owned()),
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
}

#[test]
fn preserves_explicit_function_tool_strictness() {
    let request = parsed_request(json!({
        "model": "gpt-test",
        "messages": [{"role": "user", "content": "hello"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "lookup",
                "parameters": {"type": "object"},
                "strict": true
            }
        }]
    }));

    let value = build_request(&request).unwrap();
    assert_eq!(value["tools"][0]["function"]["strict"], true);
    assert_eq!(
        parse_request(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
}

#[test]
fn rejects_content_that_openai_chat_cannot_encode_losslessly() {
    let unsupported_blocks = [(
        ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes),
        RequestCapability::CacheControl(CacheHint::Ephemeral5Minutes),
    )];
    for (block, capability) in unsupported_blocks {
        let request = CanonicalRequest::new(
            Operation::Chat,
            "gpt-test".to_owned(),
            vec![Message::new(Role::User, vec![block])],
            false,
        );
        assert_unsupported_capability(&request, capability);
    }

    let thinking_in_user_message = CanonicalRequest::new(
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Thinking {
                text: "secret".to_owned(),
                signature: None,
            }],
        )],
        false,
    );
    assert_eq!(
        build_request(&thinking_in_user_message),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let image_without_mime = CanonicalRequest::new(
        Operation::Chat,
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
        build_request(&image_without_mime),
        Err(BuildRequestError::InvalidValue)
    );

    let url_audio = CanonicalRequest::new(
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Audio {
                source: MediaSource::Url("https://media.example.test/audio.mp3".to_owned()),
                mime_type: "audio/mpeg".to_owned(),
            }],
        )],
        false,
    );
    assert_unsupported_capability(&url_audio, RequestCapability::AudioUrl);
}

#[test]
fn preserves_reasoning_through_assistant_tool_continuations() {
    let request = parsed_request(json!({
        "model": "deepseek-flash",
        "messages": [
            {"role": "user", "content": "look this up"},
            {
                "role": "assistant",
                "content": null,
                "reasoning_content": "I should call the lookup tool",
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": {"name": "lookup", "arguments": "{}"}
                }]
            },
            {"role": "tool", "tool_call_id": "call-1", "content": "found it"},
            {
                "role": "assistant",
                "content": "Here is the result.",
                "reasoning_content": "The tool returned the requested result."
            }
        ]
    }));

    let encoded = build_request(&request).unwrap();
    assert_eq!(
        encoded["messages"][1]["reasoning_content"],
        "I should call the lookup tool"
    );
    assert_eq!(
        encoded["messages"][3]["reasoning_content"],
        "The tool returned the requested result."
    );
    assert_eq!(parsed_request(encoded), request);
}

#[test]
fn distinguishes_missing_null_and_empty_reasoning_when_rebuilding() {
    for metadata in [json!({}), json!({"reasoning_content": null})] {
        let mut assistant = json!({"role": "assistant", "content": "answer"});
        assistant
            .as_object_mut()
            .unwrap()
            .extend(metadata.as_object().unwrap().clone());
        let request = parsed_request(json!({"model": "deepseek-flash", "messages": [assistant]}));
        let encoded = build_request(&request).unwrap();
        assert!(encoded["messages"][0].get("reasoning_content").is_none());
    }
    for text in ["", "actual reasoning"] {
        let request = parsed_request(json!({
            "model": "deepseek-flash",
            "messages": [{"role": "assistant", "content": "answer", "reasoning": text}]
        }));
        let encoded = build_request(&request).unwrap();
        assert_eq!(encoded["messages"][0]["reasoning_content"], text);
        assert_eq!(parsed_request(encoded), request);
    }
}

#[test]
fn rejects_reasoning_signatures_duplicate_blocks_and_unrepresentable_order() {
    let thinking = ContentBlock::Thinking {
        text: "reasoning".to_owned(),
        signature: None,
    };
    let text = ContentBlock::Text("answer".to_owned());
    for blocks in [
        vec![thinking.clone(), thinking.clone(), text.clone()],
        vec![text, thinking.clone()],
        vec![
            ContentBlock::ToolUse {
                id: "call-1".to_owned(),
                name: "lookup".to_owned(),
                input: json!({}),
                signature: None,
            },
            thinking,
        ],
    ] {
        let mut request = minimal_request();
        request.messages = vec![Message::new(Role::Assistant, blocks)];
        assert_eq!(
            build_request(&request),
            Err(BuildRequestError::UnsupportedFeature)
        );
    }
    let mut signed = minimal_request();
    signed.messages = vec![Message::new(
        Role::Assistant,
        vec![ContentBlock::Thinking {
            text: "reasoning".to_owned(),
            signature: Some("signature".to_owned()),
        }],
    )];
    assert_unsupported_capability(&signed, RequestCapability::ThinkingSignatures);
}

#[test]
fn revalidates_tool_order_arguments_and_result_semantics() {
    let text_after_tool = CanonicalRequest::new(
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![
                ContentBlock::ToolUse {
                    id: "call-1".to_owned(),
                    name: "lookup".to_owned(),
                    input: json!({}),
                    signature: None,
                },
                ContentBlock::Text("late".to_owned()),
            ],
        )],
        false,
    );
    assert_eq!(
        build_request(&text_after_tool),
        Err(BuildRequestError::UnsupportedFeature)
    );

    let non_object_arguments = CanonicalRequest::new(
        Operation::Chat,
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
        build_request(&non_object_arguments),
        Err(BuildRequestError::InvalidValue)
    );

    let error_result = CanonicalRequest::new(
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_owned(),
                content: vec![ContentBlock::Text("failed".to_owned())],
                structured_content: None,
                is_error: true,
            }],
        )],
        false,
    );
    assert_unsupported_capability(&error_result, RequestCapability::ToolResultErrors);

    let signed_call = CanonicalRequest::new(
        Operation::Chat,
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
    assert_unsupported_capability(&signed_call, RequestCapability::ToolCallSignatures);

    let structured_result = CanonicalRequest::new(
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_owned(),
                content: Vec::new(),
                structured_content: Some(json!({"ok": true})),
                is_error: false,
            }],
        )],
        false,
    );
    assert_unsupported_capability(&structured_result, RequestCapability::StructuredToolResults);
}

#[test]
fn rejects_cross_protocol_raw_collisions_and_invalid_raw_values() {
    let mut fields = Map::new();
    fields.insert("seed".to_owned(), json!(7));
    let cross_protocol =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::Anthropic, fields)));
    assert_eq!(
        build_request(&cross_protocol),
        Err(BuildRequestError::RawProtocolMismatch)
    );

    let mut collision = Map::new();
    collision.insert("model".to_owned(), json!("other"));
    let collision =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::OpenAiChat, collision)));
    assert_eq!(
        build_request(&collision),
        Err(BuildRequestError::FieldConflict)
    );

    let mut invalid = Map::new();
    invalid.insert("n".to_owned(), json!(0));
    let invalid =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::OpenAiChat, invalid)));
    assert_eq!(
        build_request(&invalid),
        Err(BuildRequestError::InvalidValue)
    );

    for key in ["prompt_cache_key", "prompt_cache_retention"] {
        let mut fields = Map::new();
        fields.insert(
            key.to_owned(),
            if key == "prompt_cache_key" {
                json!("cache-key")
            } else {
                json!("24h")
            },
        );
        let request =
            minimal_request().with_validated_raw_passthrough(Some((Protocol::OpenAiChat, fields)));
        assert_eq!(build_request(&request).unwrap()[key], fields_value(key));
    }
}

fn fields_value(key: &str) -> Value {
    if key == "prompt_cache_key" {
        json!("cache-key")
    } else {
        json!("24h")
    }
}

#[test]
fn forwards_logprobs_request_options_to_openai_chat() {
    let mut fields = Map::new();
    fields.insert("logprobs".to_owned(), json!(true));
    fields.insert("top_logprobs".to_owned(), json!(3));
    let request =
        minimal_request().with_validated_raw_passthrough(Some((Protocol::OpenAiChat, fields)));

    let built = build_request(&request).unwrap();
    assert_eq!(built["logprobs"], true);
    assert_eq!(built["top_logprobs"], 3);
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
        Operation::Chat,
        "gpt-test".to_owned(),
        vec![Message::new(Role::User, blocks)],
        false,
    );

    assert_eq!(
        build_request(&request),
        Err(BuildRequestError::StructureLimitExceeded)
    );
}

#[test]
fn rejects_combined_schema_node_budget() {
    let schema = |tool_index| {
        let mut properties = Map::new();
        properties.insert(
            format!("items-{tool_index}"),
            json!({ "type": "array", "items": vec![0; 4_088] }),
        );
        json!({ "type": "object", "properties": properties })
    };
    let tools = (0..25)
        .map(|index| crate::ToolDef {
            name: format!("tool-{index}"),
            description: None,
            input_schema: schema(index),
            strict: None,
        })
        .collect();
    let mut request = minimal_request();
    request.tools = tools;
    request.tool_choice = crate::ToolChoice::Auto;

    assert_eq!(
        build_request(&request),
        Err(BuildRequestError::StructureLimitExceeded)
    );
}
