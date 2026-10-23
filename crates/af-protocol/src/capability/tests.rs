use std::collections::HashSet;

use af_domain::{Operation, Protocol, Role};
use serde_json::json;

use super::{
    ProtocolCapability, RequestCapability, ResponseCapability, StreamCapability,
    protocol_capabilities, supports_request_capability, supports_response_capability,
    supports_stream_capability, validate_request_capabilities, validate_stream_event_capabilities,
};
use crate::{
    CanonicalRequest, CanonicalResponse, CanonicalStreamEvent, ContentBlock, FinishReason, Message,
    ResponseChoice, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource, anthropic,
    gemini, openai_chat, openai_responses,
};

#[test]
fn capability_declarations_are_closed_and_unique() {
    for protocol in Protocol::ALL {
        let declaration = protocol_capabilities(*protocol);
        assert_eq!(declaration.protocol(), *protocol);
        assert_closed_and_unique(declaration.request(), RequestCapability::ALL);
        assert_closed_and_unique(declaration.response(), ResponseCapability::ALL);
        assert_closed_and_unique(declaration.stream(), StreamCapability::ALL);

        for capability in RequestCapability::ALL {
            assert_eq!(
                supports_request_capability(*protocol, *capability),
                declaration.request().contains(capability)
            );
        }
        for capability in ResponseCapability::ALL {
            assert_eq!(
                supports_response_capability(*protocol, *capability),
                declaration.response().contains(capability)
            );
        }
        for capability in StreamCapability::ALL {
            assert_eq!(
                supports_stream_capability(*protocol, *capability),
                declaration.stream().contains(capability)
            );
        }
    }
}

#[test]
fn matrix_exposes_protocol_specific_differences() {
    assert!(supports_request_capability(
        Protocol::OpenAiChat,
        RequestCapability::MessageRole(Role::Developer)
    ));
    assert!(supports_request_capability(
        Protocol::OpenAiResponses,
        RequestCapability::MessageRole(Role::Developer)
    ));
    assert!(!supports_request_capability(
        Protocol::Anthropic,
        RequestCapability::MessageRole(Role::Developer)
    ));
    assert!(!supports_request_capability(
        Protocol::Gemini,
        RequestCapability::MessageRole(Role::Developer)
    ));
    assert!(supports_request_capability(
        Protocol::OpenAiResponses,
        RequestCapability::Thinking
    ));
    assert!(supports_request_capability(
        Protocol::OpenAiResponses,
        RequestCapability::ThinkingSignatures
    ));
    assert!(supports_request_capability(
        Protocol::OpenAiChat,
        RequestCapability::Thinking
    ));
    assert!(!supports_request_capability(
        Protocol::OpenAiChat,
        RequestCapability::ThinkingSignatures
    ));
    for protocol in [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Gemini,
    ] {
        assert!(supports_request_capability(
            protocol,
            RequestCapability::UnsignedThinking
        ));
    }
    assert!(!supports_request_capability(
        Protocol::Anthropic,
        RequestCapability::UnsignedThinking
    ));

    assert!(supports_response_capability(
        Protocol::Anthropic,
        ResponseCapability::StopSequence
    ));
    assert!(!supports_response_capability(
        Protocol::OpenAiChat,
        ResponseCapability::StopSequence
    ));
    assert!(supports_stream_capability(
        Protocol::Gemini,
        StreamCapability::ToolCallSignatures
    ));
    assert!(supports_stream_capability(
        Protocol::OpenAiResponses,
        StreamCapability::Error
    ));
}

#[test]
fn unsigned_thinking_capability_matches_request_builders() {
    let mut request = CanonicalRequest::new(
        Operation::Chat,
        "test-model".to_owned(),
        vec![
            Message::new(Role::User, vec![ContentBlock::Text("hello".to_owned())]),
            Message::new(
                Role::Assistant,
                vec![
                    ContentBlock::Thinking {
                        text: "plain summary".to_owned(),
                        signature: None,
                    },
                    ContentBlock::Text("answer".to_owned()),
                ],
            ),
            Message::new(Role::User, vec![ContentBlock::Text("continue".to_owned())]),
        ],
        false,
    );

    let error = validate_request_capabilities(Protocol::Anthropic, &request)
        .expect_err("Anthropic requires a signature to replay thinking");
    assert_eq!(
        error.capability(),
        ProtocolCapability::Request(RequestCapability::UnsignedThinking)
    );
    assert_eq!(
        anthropic::build_request(&request),
        Err(anthropic::BuildRequestError::UnsupportedCapability(error))
    );

    assert!(validate_request_capabilities(Protocol::OpenAiChat, &request).is_ok());
    let chat = openai_chat::build_request(&request).unwrap();
    assert_eq!(chat["messages"][1]["reasoning_content"], "plain summary");

    assert!(validate_request_capabilities(Protocol::Gemini, &request).is_ok());
    let gemini = gemini::build_request(&request).unwrap();
    assert_eq!(
        gemini["contents"][1]["parts"][0],
        json!({"text": "plain summary", "thought": true})
    );

    request.operation = Operation::Responses;
    assert!(validate_request_capabilities(Protocol::OpenAiResponses, &request).is_ok());
    let responses = openai_responses::build_request(&request).unwrap();
    assert_eq!(
        responses["input"][1],
        json!({"type": "reasoning", "summary": [
            {"type": "summary_text", "text": "plain summary"}
        ]})
    );
}

#[test]
fn signed_thinking_still_uses_signature_capability() {
    let request = CanonicalRequest::new(
        Operation::Chat,
        "test-model".to_owned(),
        vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::Thinking {
                text: "signed thinking".to_owned(),
                signature: Some("opaque-signature".to_owned()),
            }],
        )],
        false,
    );

    assert!(validate_request_capabilities(Protocol::Anthropic, &request).is_ok());
    let error = validate_request_capabilities(Protocol::OpenAiChat, &request)
        .expect_err("OpenAI Chat cannot preserve thinking signatures");
    assert_eq!(
        error.capability(),
        ProtocolCapability::Request(RequestCapability::ThinkingSignatures)
    );
}

#[test]
fn request_builder_returns_specific_missing_capability() {
    let request = CanonicalRequest::new(
        Operation::Chat,
        "敏感模型".to_owned(),
        vec![Message::new(
            Role::Developer,
            vec![ContentBlock::Text("敏感正文".to_owned())],
        )],
        false,
    );

    let error = anthropic::build_request(&request).expect_err("Developer 角色必须被拒绝");
    let anthropic::BuildRequestError::UnsupportedCapability(error) = error else {
        panic!("必须返回结构化能力错误，实际为 {error:?}");
    };
    assert_eq!(error.target_protocol(), Protocol::Anthropic);
    assert_eq!(
        error.capability(),
        ProtocolCapability::Request(RequestCapability::MessageRole(Role::Developer))
    );
    let debug = format!("{error:?}");
    let display = error.to_string();
    assert!(!debug.contains("敏感模型"));
    assert!(!debug.contains("敏感正文"));
    assert!(!display.contains("敏感模型"));
    assert!(!display.contains("敏感正文"));
}

#[test]
fn response_validator_identifies_multiple_choice_gap() {
    let choice = |index| {
        ResponseChoice::new(
            index,
            Message::new(Role::Assistant, vec![ContentBlock::Text("内容".to_owned())]),
            FinishReason::Stop,
        )
    };
    let response = CanonicalResponse::new(
        Operation::Chat,
        "id".to_owned(),
        "model".to_owned(),
        None,
        vec![choice(0), choice(1)],
        None,
    );

    let error = super::validate_response_capabilities(Protocol::Anthropic, &response)
        .expect_err("Anthropic 不支持多候选响应");
    assert_eq!(
        error.capability(),
        ProtocolCapability::Response(ResponseCapability::MultipleChoices)
    );
}

#[test]
fn stream_validator_identifies_usage_detail_gap() {
    let usage = Usage::new(
        TokenCount::new(10).unwrap(),
        TokenCount::new(2).unwrap(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::new(1).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    let event = CanonicalStreamEvent::Usage(usage);

    let error = validate_stream_event_capabilities(Protocol::Gemini, &event)
        .expect_err("Gemini 不支持缓存写入 usage");
    assert_eq!(
        error.capability(),
        ProtocolCapability::Stream(StreamCapability::UsageCacheCreation(
            crate::CacheHint::Ephemeral5Minutes
        ))
    );
}

#[test]
fn logprobs_are_preserved_only_by_openai_chat_and_rejected_elsewhere() {
    let choice = ResponseChoice::new(
        0,
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Text("answer".to_owned())],
        ),
        FinishReason::Stop,
    )
    .with_logprobs(Some(json!({"content": [{"token": "answer"}]})));
    let response = CanonicalResponse::new(
        Operation::Chat,
        "id".to_owned(),
        "model".to_owned(),
        None,
        vec![choice],
        None,
    );
    assert!(super::validate_response_capabilities(Protocol::OpenAiChat, &response).is_ok());
    let error = super::validate_response_capabilities(Protocol::Anthropic, &response)
        .expect_err("Anthropic cannot represent OpenAI Chat token probabilities losslessly");
    assert_eq!(
        error.capability(),
        ProtocolCapability::Response(ResponseCapability::Logprobs)
    );

    let event = CanonicalStreamEvent::LogprobsDelta {
        choice_index: 0,
        logprobs: json!({"content": [{"token": "answer"}]}),
    };
    assert!(validate_stream_event_capabilities(Protocol::OpenAiChat, &event).is_ok());
    for protocol in [
        Protocol::OpenAiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
    ] {
        let error = validate_stream_event_capabilities(protocol, &event)
            .expect_err("other protocols cannot represent Chat-shaped logprobs");
        assert_eq!(
            error.capability(),
            ProtocolCapability::Stream(StreamCapability::LogprobsDelta)
        );
    }
}

fn assert_closed_and_unique<T>(declared: &[T], all: &[T])
where
    T: Copy + Eq + std::hash::Hash + std::fmt::Debug,
{
    let unique = declared.iter().copied().collect::<HashSet<_>>();
    assert_eq!(unique.len(), declared.len(), "能力声明不得重复");
    assert!(
        declared.iter().all(|capability| all.contains(capability)),
        "能力声明必须来自闭合枚举"
    );
}
