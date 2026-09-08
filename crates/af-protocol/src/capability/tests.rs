use std::collections::HashSet;

use af_domain::{Operation, Protocol, Role};

use super::{
    ProtocolCapability, RequestCapability, ResponseCapability, StreamCapability,
    protocol_capabilities, supports_request_capability, supports_response_capability,
    supports_stream_capability, validate_stream_event_capabilities,
};
use crate::{
    CanonicalRequest, CanonicalResponse, CanonicalStreamEvent, ContentBlock, FinishReason, Message,
    ResponseChoice, TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource, anthropic,
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
    for capability in [
        RequestCapability::Thinking,
        RequestCapability::ThinkingSignatures,
    ] {
        assert!(supports_request_capability(
            Protocol::OpenAiResponses,
            capability
        ));
        assert!(!supports_request_capability(
            Protocol::OpenAiChat,
            capability
        ));
    }

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
