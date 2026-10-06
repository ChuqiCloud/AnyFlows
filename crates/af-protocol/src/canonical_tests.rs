use af_domain::{Operation, Protocol, Role};
use serde_json::{Map, json};

use crate::{
    Attachment, CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message,
    RawPassthroughError, ReasoningConfig, ReasoningEffort, RequestContinuation, RequestMetadata,
    Sampling, TokenCount, ToolChoice, ToolDef,
};

#[test]
fn new_request_uses_explicit_canonical_defaults() {
    let request = CanonicalRequest::new(Operation::Chat, "model".to_owned(), Vec::new(), false);

    assert!(request.tools.is_empty());
    assert_eq!(request.tool_choice, ToolChoice::Auto);
    assert_eq!(request.reasoning, None);
    assert_eq!(request.sampling.temperature(), None);
    assert_eq!(request.sampling.top_p(), None);
    assert_eq!(request.sampling.max_output_tokens(), None);
    assert!(request.sampling.stop_sequences().is_empty());
    assert_eq!(request.stream_options, crate::StreamOptions::EMPTY);
    assert!(request.attachments.is_empty());
    assert_eq!(request.metadata, RequestMetadata::EMPTY);
    assert_eq!(request.continuation, RequestContinuation::EMPTY);
    assert_eq!(request.raw_passthrough(), None);
}

#[test]
fn request_preserves_instruction_order_and_protocol_private_fields() {
    let messages = vec![
        Message::new(Role::System, vec![ContentBlock::Text("system".to_owned())]),
        Message::new(
            Role::Developer,
            vec![ContentBlock::Text("developer".to_owned())],
        ),
        Message::new(Role::User, vec![ContentBlock::Text("user".to_owned())]),
    ];
    let mut fields = Map::new();
    fields.insert("vendor_option".to_owned(), json!({"nested": [1, true]}));

    let mut request = CanonicalRequest::new(
        Operation::Responses,
        "requested-model".to_owned(),
        messages,
        true,
    );
    request.tools = vec![ToolDef {
        name: "lookup".to_owned(),
        description: Some("lookup values".to_owned()),
        input_schema: json!({"type": "object"}),
        strict: None,
    }];
    request.tool_choice = ToolChoice::Named {
        name: "lookup".to_owned(),
    };
    request.reasoning = Some(
        ReasoningConfig::new(
            Some(ReasoningEffort::High),
            Some(TokenCount::new(1024).unwrap()),
            true,
        )
        .unwrap(),
    );
    request.sampling = Sampling::new(
        Some(0.5),
        Some(0.9),
        Some(TokenCount::new(2048).unwrap()),
        vec!["stop".to_owned()],
    )
    .unwrap();
    request.attachments = vec![Attachment {
        source: MediaSource::Url("https://example.invalid/document.pdf".to_owned()),
        mime_type: Some("application/pdf".to_owned()),
        filename: Some("document.pdf".to_owned()),
    }];
    request.metadata =
        RequestMetadata::new(Some("user-1".to_owned()), Some("session-1".to_owned()));
    request.continuation = RequestContinuation::new(
        Some("resp-previous".to_owned()),
        None,
        Some("cache-key".to_owned()),
    );
    let request =
        request.with_validated_raw_passthrough(Some((Protocol::OpenAiResponses, fields.clone())));

    assert_eq!(
        request
            .messages
            .iter()
            .map(|message| message.role)
            .collect::<Vec<_>>(),
        vec![Role::System, Role::Developer, Role::User]
    );
    assert_eq!(request.tools.len(), 1);
    assert!(matches!(request.tool_choice, ToolChoice::Named { .. }));
    assert_eq!(
        request.reasoning.unwrap().effort(),
        Some(ReasoningEffort::High)
    );
    assert_eq!(request.sampling.max_output_tokens().unwrap().get(), 2048);
    assert_eq!(request.attachments.len(), 1);
    assert_eq!(request.metadata.session_id(), Some("session-1"));
    assert_eq!(
        request.continuation.previous_response_id(),
        Some("resp-previous")
    );
    assert_eq!(request.continuation.prompt_cache_key(), Some("cache-key"));
    let raw = request.raw_passthrough().unwrap();
    assert_eq!(raw.source_protocol(), Protocol::OpenAiResponses);
    assert_eq!(
        raw.fields_for_protocol(Protocol::OpenAiResponses),
        Ok(&fields)
    );
    assert_eq!(
        raw.fields_for_protocol(Protocol::Anthropic),
        Err(RawPassthroughError::ProtocolMismatch)
    );
    assert_eq!(
        RawPassthroughError::ProtocolMismatch.to_string(),
        "未归一化字段不得跨协议透传"
    );
}

#[test]
fn content_blocks_cover_multimodal_tools_thinking_and_cache_hints() {
    let blocks = [
        ContentBlock::Text("text".to_owned()),
        ContentBlock::Image {
            source: MediaSource::Url("https://example.invalid/image".to_owned()),
            mime_type: Some("image/png".to_owned()),
        },
        ContentBlock::Audio {
            source: MediaSource::Base64("audio-data".to_owned()),
            mime_type: "audio/wav".to_owned(),
        },
        ContentBlock::ToolUse {
            id: "call-1".to_owned(),
            name: "lookup".to_owned(),
            input: json!({"query": "value"}),
            signature: None,
        },
        ContentBlock::ToolResult {
            tool_use_id: "call-1".to_owned(),
            content: vec![ContentBlock::Text("result".to_owned())],
            structured_content: None,
            is_error: false,
        },
        ContentBlock::Thinking {
            text: "reasoning".to_owned(),
            signature: Some("signature".to_owned()),
        },
        ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes),
        ContentBlock::CacheControl(CacheHint::Ephemeral1Hour),
    ];

    assert_eq!(blocks.len(), 8);
    assert!(matches!(blocks[3], ContentBlock::ToolUse { .. }));
    assert!(matches!(
        blocks[7],
        ContentBlock::CacheControl(CacheHint::Ephemeral1Hour)
    ));
}

#[test]
fn sensitive_canonical_values_are_redacted_from_debug() {
    let canaries = [
        "model-secret-canary",
        "text-secret-canary",
        "url-secret-canary",
        "base64-secret-canary",
        "mime-secret-canary",
        "tool-id-secret-canary",
        "tool-name-secret-canary",
        "tool-input-secret-canary",
        "tool-result-secret-canary",
        "thinking-secret-canary",
        "signature-secret-canary",
        "raw-secret-canary",
    ];
    let blocks = [
        ContentBlock::Text(canaries[1].to_owned()),
        ContentBlock::Image {
            source: MediaSource::Url(canaries[2].to_owned()),
            mime_type: Some(canaries[4].to_owned()),
        },
        ContentBlock::Audio {
            source: MediaSource::Base64(canaries[3].to_owned()),
            mime_type: canaries[4].to_owned(),
        },
        ContentBlock::ToolUse {
            id: canaries[5].to_owned(),
            name: canaries[6].to_owned(),
            input: json!({"value": canaries[7]}),
            signature: Some(canaries[10].to_owned()),
        },
        ContentBlock::ToolResult {
            tool_use_id: canaries[5].to_owned(),
            content: vec![ContentBlock::Text(canaries[8].to_owned())],
            structured_content: Some(json!({"value": canaries[8]})),
            is_error: true,
        },
        ContentBlock::Thinking {
            text: canaries[9].to_owned(),
            signature: Some(canaries[10].to_owned()),
        },
    ];
    let message = Message::new(Role::User, blocks.to_vec());
    let mut fields = Map::new();
    fields.insert("raw".to_owned(), json!(canaries[11]));
    let request = CanonicalRequest::new(
        Operation::Chat,
        canaries[0].to_owned(),
        vec![message.clone()],
        false,
    )
    .with_validated_raw_passthrough(Some((Protocol::OpenAiChat, fields)));
    let raw = request.raw_passthrough().unwrap();

    let mut rendered = format!("{request:?}\n{message:?}\n{raw:?}");
    for block in &blocks {
        rendered.push_str(&format!("\n{block:?}"));
    }
    rendered.push_str(&format!(
        "\n{:?}\n{:?}",
        MediaSource::Url(canaries[2].to_owned()),
        MediaSource::Base64(canaries[3].to_owned())
    ));

    for canary in canaries {
        assert!(!rendered.contains(canary), "Debug 泄露敏感内容：{canary}");
    }
}
