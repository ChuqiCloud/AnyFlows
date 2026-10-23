use af_domain::{Operation, Protocol, Role};
use serde_json::json;

use super::normalize_history;
use crate::{
    CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message, RequestContinuation, Sampling,
    TokenCount, ToolChoice, anthropic, gemini, openai_chat, openai_responses,
};

const TARGETS: [Protocol; 4] = [
    Protocol::OpenAiChat,
    Protocol::OpenAiResponses,
    Protocol::Anthropic,
    Protocol::Gemini,
];

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text(value.to_owned())
}

fn message(role: Role, content: Vec<ContentBlock>) -> Message {
    Message::new(role, content)
}

fn call(id: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: id.to_owned(),
        name: "lookup".to_owned(),
        input: json!({"query": id}),
        signature: None,
    }
}

fn result(id: &str, value: &str) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.to_owned(),
        content: vec![text(value)],
        structured_content: None,
        is_error: false,
    }
}

fn request(messages: Vec<Message>, target: Protocol) -> CanonicalRequest {
    let operation = if target == Protocol::OpenAiResponses {
        Operation::Responses
    } else {
        Operation::Chat
    };
    let mut request = CanonicalRequest::new(operation, "history-model".to_owned(), messages, false);
    request.tool_choice = ToolChoice::None;
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(1024).unwrap()), vec![]).unwrap();
    request
}

fn assert_builds(request: &CanonicalRequest, target: Protocol) {
    match target {
        Protocol::OpenAiChat => assert!(openai_chat::build_request(request).is_ok()),
        Protocol::OpenAiResponses => assert!(openai_responses::build_request(request).is_ok()),
        Protocol::Anthropic => assert!(anthropic::build_request(request).is_ok()),
        Protocol::Gemini => assert!(gemini::build_request(request).is_ok()),
        _ => unreachable!(),
    }
}

fn visible_text(request: &CanonicalRequest) -> Vec<&str> {
    request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn reorders_parallel_results_by_explicit_id_and_preserves_intervening_text() {
    for target in TARGETS {
        let mut request = request(
            vec![
                message(Role::User, vec![text("start")]),
                message(
                    Role::Assistant,
                    vec![text("checking"), call("a"), call("b")],
                ),
                message(Role::User, vec![text("intervening user content")]),
                message(
                    Role::Tool,
                    vec![
                        result("b", "B"),
                        result("a", "A"),
                        text("trailing tool text"),
                    ],
                ),
                message(Role::User, vec![text("continue")]),
            ],
            target,
        );
        normalize_history(&mut request, target).unwrap();
        assert!(matches!(&request.messages[2].content[..],
            [ContentBlock::ToolResult { tool_use_id, content, .. }]
                if tool_use_id == "a" && content == &vec![text("A")]));
        assert!(matches!(&request.messages[3].content[..],
            [ContentBlock::ToolResult { tool_use_id, content, .. }]
                if tool_use_id == "b" && content == &vec![text("B")]));
        assert_eq!(
            visible_text(&request),
            vec![
                "start",
                "checking",
                "intervening user content",
                "trailing tool text",
                "continue"
            ]
        );
        assert_builds(&request, target);
        let once = request.clone();
        normalize_history(&mut request, target).unwrap();
        assert_eq!(request, once);
    }
}

#[test]
fn matches_explicit_id_even_if_a_result_precedes_its_call_in_history() {
    let mut request = request(
        vec![
            message(Role::Tool, vec![result("a", "recorded result")]),
            message(Role::User, vec![text("context")]),
            message(Role::Assistant, vec![call("a")]),
            message(Role::User, vec![text("continue")]),
        ],
        Protocol::OpenAiChat,
    );
    normalize_history(&mut request, Protocol::OpenAiChat).unwrap();
    assert_eq!(request.messages[0].role, Role::User);
    assert_eq!(request.messages[1].role, Role::Assistant);
    assert_eq!(request.messages[2].role, Role::Tool);
    assert_builds(&request, Protocol::OpenAiChat);
}

#[test]
fn repairs_only_unique_missing_ids_without_colliding_with_explicit_ids() {
    for target in TARGETS {
        let mut request = request(
            vec![
                message(Role::Assistant, vec![call("")]),
                message(Role::Tool, vec![result("", "inferred")]),
                message(Role::Assistant, vec![call("af_history_call_1")]),
                message(Role::Tool, vec![result("af_history_call_1", "explicit")]),
                message(Role::User, vec![text("continue")]),
            ],
            target,
        );
        normalize_history(&mut request, target).unwrap();
        let ContentBlock::ToolUse { id, .. } = &request.messages[0].content[0] else {
            panic!()
        };
        assert_eq!(id, "af_history_call_2");
        let ContentBlock::ToolResult { tool_use_id, .. } = &request.messages[1].content[0] else {
            panic!()
        };
        assert_eq!(tool_use_id, id);
        assert_builds(&request, target);
    }
}

#[test]
fn explicit_match_is_resolved_before_a_unique_missing_id() {
    let mut request = request(
        vec![
            message(Role::Assistant, vec![call("a"), call("b")]),
            message(Role::Tool, vec![result("", "A"), result("b", "B")]),
            message(Role::User, vec![text("continue")]),
        ],
        Protocol::Gemini,
    );
    normalize_history(&mut request, Protocol::Gemini).unwrap();
    assert!(
        matches!(&request.messages[1].content[0], ContentBlock::ToolResult {tool_use_id, ..} if tool_use_id == "a")
    );
    assert_builds(&request, Protocol::Gemini);
}

#[test]
fn ambiguous_missing_ids_are_text_not_fabricated_successes() {
    for target in TARGETS {
        let mut request = request(
            vec![
                message(Role::Assistant, vec![call(""), call("")]),
                message(Role::Tool, vec![result("", "first"), result("", "second")]),
                message(Role::User, vec![text("continue")]),
            ],
            target,
        );
        normalize_history(&mut request, target).unwrap();
        assert!(
            !request
                .messages
                .iter()
                .any(|message| message.role == Role::Tool)
        );
        let text = visible_text(&request).join("\n");
        assert!(text.contains("no associated result; execution is not confirmed"));
        assert!(text.contains("no associated call; execution is not confirmed"));
        assert!(text.contains("first") && text.contains("second"));
        assert_builds(&request, target);
    }
}

#[test]
fn duplicate_explicit_results_do_not_choose_an_arbitrary_execution() {
    let mut request = request(
        vec![
            message(Role::Assistant, vec![call("a")]),
            message(
                Role::Tool,
                vec![result("a", "first record"), result("a", "second record")],
            ),
        ],
        Protocol::OpenAiChat,
    );
    normalize_history(&mut request, Protocol::OpenAiChat).unwrap();
    assert!(
        !request
            .messages
            .iter()
            .any(|message| message.role == Role::Tool)
    );
    let visible = visible_text(&request).join("\n");
    assert!(visible.contains("first record") && visible.contains("second record"));
    assert_builds(&request, Protocol::OpenAiChat);
}

#[test]
fn orphan_and_dangling_signed_history_retains_visible_facts_without_signature_leak() {
    let mut signed = call("dangling");
    let ContentBlock::ToolUse { signature, .. } = &mut signed else {
        panic!()
    };
    *signature = Some("private-signature-canary".to_owned());
    let mut request = request(
        vec![
            message(Role::Assistant, vec![signed]),
            message(
                Role::Tool,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "orphan".to_owned(),
                    content: vec![text("raw failure")],
                    structured_content: Some(json!({"error": {"code": 7}})),
                    is_error: true,
                }],
            ),
        ],
        Protocol::OpenAiChat,
    );
    normalize_history(&mut request, Protocol::OpenAiChat).unwrap();
    let visible = visible_text(&request).join("\n");
    assert!(visible.contains("dangling") && visible.contains("lookup"));
    assert!(visible.contains("raw failure") && visible.contains("\"code\":7"));
    assert!(visible.contains("\"reported_error\":true"));
    assert!(!visible.contains("private-signature-canary"));
    assert_builds(&request, Protocol::OpenAiChat);
}

#[test]
fn paired_structured_results_and_result_cache_hints_are_not_flattened() {
    let payload = json!({"error": {"code": 7}, "items": [1, null]});
    let mut request = request(
        vec![
            message(Role::Assistant, vec![call("a")]),
            message(
                Role::Tool,
                vec![
                    ContentBlock::ToolResult {
                        tool_use_id: "a".to_owned(),
                        content: vec![],
                        structured_content: Some(payload.clone()),
                        is_error: true,
                    },
                    ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes),
                ],
            ),
        ],
        Protocol::Gemini,
    );
    normalize_history(&mut request, Protocol::Gemini).unwrap();
    assert!(matches!(&request.messages[1].content[..],
        [ContentBlock::ToolResult { structured_content: Some(value), is_error: true, .. },
         ContentBlock::CacheControl(CacheHint::Ephemeral5Minutes)] if value == &payload));
}

#[test]
fn orphan_media_is_retained_and_anthropic_only_merges_adjacent_same_roles() {
    let image = ContentBlock::Image {
        source: MediaSource::Base64("aGk=".to_owned()),
        mime_type: Some("image/png".to_owned()),
    };
    let mut request = request(
        vec![
            message(Role::User, vec![text("one")]),
            message(Role::User, vec![text("two")]),
            message(
                Role::Tool,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "orphan".to_owned(),
                    content: vec![image.clone()],
                    structured_content: None,
                    is_error: false,
                }],
            ),
            message(Role::Assistant, vec![text("answer")]),
        ],
        Protocol::Anthropic,
    );
    normalize_history(&mut request, Protocol::Anthropic).unwrap();
    assert_eq!(request.messages.len(), 2);
    assert!(request.messages[0].content.contains(&image));
    assert_eq!(visible_text(&request)[..2], ["one", "two"]);
    assert_builds(&request, Protocol::Anthropic);
}

#[test]
fn responses_remote_result_reference_is_preserved_without_inventing_a_local_call() {
    let mut request = request(
        vec![message(Role::Tool, vec![result("remote-call", "recorded")])],
        Protocol::OpenAiResponses,
    );
    request.continuation = RequestContinuation::new(Some("response-id".to_owned()), None, None);
    normalize_history(&mut request, Protocol::OpenAiResponses).unwrap();
    assert_eq!(request.messages[0].role, Role::Tool);
    assert!(
        matches!(&request.messages[0].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "remote-call")
    );
    assert_builds(&request, Protocol::OpenAiResponses);
}
