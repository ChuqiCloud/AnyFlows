use af_domain::{Operation, Protocol, Role};
use serde_json::{Value, json};

use super::normalize_content;
use crate::{
    CanonicalRequest, ContentBlock, Message, Sampling, TokenCount, gemini, openai_chat,
    openai_responses,
};

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text(value.to_owned())
}
fn thinking(value: &str) -> ContentBlock {
    ContentBlock::Thinking {
        text: value.to_owned(),
        signature: None,
    }
}
fn call(id: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: id.to_owned(),
        name: "lookup".to_owned(),
        input: json!({"id":id}),
        signature: None,
    }
}
fn result(
    id: &str,
    content: Vec<ContentBlock>,
    structured_content: Option<Value>,
    is_error: bool,
) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.to_owned(),
        content,
        structured_content,
        is_error,
    }
}
fn request(
    target: Protocol,
    assistant: Vec<ContentBlock>,
    results: Vec<ContentBlock>,
) -> CanonicalRequest {
    let mut messages = vec![
        Message::new(Role::User, vec![text("start")]),
        Message::new(Role::Assistant, assistant),
    ];
    messages.extend(
        results
            .into_iter()
            .map(|block| Message::new(Role::Tool, vec![block])),
    );
    let operation = if target == Protocol::OpenAiResponses {
        Operation::Responses
    } else {
        Operation::Chat
    };
    let mut request = CanonicalRequest::new(operation, "content-model".to_owned(), messages, false);
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(4096).unwrap()), vec![]).unwrap();
    request
}

#[test]
fn interleaved_assistant_blocks_build_in_openai_targets_without_losing_text_or_calls() {
    for target in [Protocol::OpenAiChat, Protocol::OpenAiResponses] {
        let mut request = request(
            target,
            vec![
                text("first"),
                thinking("reason-one"),
                call("a"),
                text("second"),
                thinking("reason-two"),
                call("b"),
            ],
            vec![
                result("a", vec![text("A")], None, false),
                result("b", vec![text("B")], None, false),
            ],
        );
        normalize_content(&mut request, target).unwrap();
        crate::conversation_history::normalize_history(&mut request, target).unwrap();
        let value = if target == Protocol::OpenAiChat {
            openai_chat::build_request(&request).unwrap()
        } else {
            openai_responses::build_request(&request).unwrap()
        };
        let rendered = value.to_string();
        for part in [
            "first",
            "second",
            "reason-one",
            "reason-two",
            "lookup",
            "A",
            "B",
        ] {
            assert!(rendered.contains(part), "{target:?}: missing {part}");
        }
        let once = request.clone();
        normalize_content(&mut request, target).unwrap();
        assert_eq!(request, once);
    }
}

#[test]
fn chat_merges_unsigned_reasoning_while_responses_keeps_reasoning_items() {
    for target in [Protocol::OpenAiChat, Protocol::OpenAiResponses] {
        let mut request = request(
            target,
            vec![text("visible"), thinking("one"), thinking("two")],
            vec![],
        );
        normalize_content(&mut request, target).unwrap();
        let expected = if target == Protocol::OpenAiChat { 1 } else { 2 };
        assert_eq!(
            request.messages[1]
                .content
                .iter()
                .filter(|block| matches!(block, ContentBlock::Thinking { .. }))
                .count(),
            expected
        );
        assert!(
            matches!(request.messages[1].content.last(),Some(ContentBlock::Text(value)) if value == "visible")
        );
    }
}

#[test]
fn gemini_multiple_text_and_mixed_structured_results_preserve_all_payloads() {
    for is_error in [false, true] {
        let original = json!({"result":"original-key","error":"original-data","nested":{"n":7}});
        let mut request = request(
            Protocol::Gemini,
            vec![call("a"), call("b")],
            vec![
                result("a", vec![text("first"), text("second")], None, is_error),
                result(
                    "b",
                    vec![text("third"), text("fourth")],
                    Some(original.clone()),
                    is_error,
                ),
            ],
        );
        normalize_content(&mut request, Protocol::Gemini).unwrap();
        crate::conversation_history::normalize_history(&mut request, Protocol::Gemini).unwrap();
        let value = gemini::build_request(&request).unwrap();
        let responses = value["contents"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|turn| turn["parts"].as_array().unwrap())
            .filter_map(|part| part.get("functionResponse"))
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 2);
        let key = if is_error { "error" } else { "result" };
        assert_eq!(
            responses[0]["response"][key]["content"],
            json!(["first", "second"])
        );
        assert_eq!(
            responses[1]["response"][key]["content"],
            json!(["third", "fourth"])
        );
        assert_eq!(
            responses[1]["response"][key]["structured_content"],
            original
        );
        let once = request.clone();
        normalize_content(&mut request, Protocol::Gemini).unwrap();
        assert_eq!(request, once);
    }
}

#[test]
fn gemini_matching_native_structured_result_is_preserved_exactly() {
    let original = json!({"result":{"content":["exact"],"metadata":{"n":7}}});
    let mut request = request(
        Protocol::Gemini,
        vec![call("a")],
        vec![result("a", vec![], Some(original.clone()), false)],
    );
    normalize_content(&mut request, Protocol::Gemini).unwrap();
    let ContentBlock::ToolResult {
        structured_content, ..
    } = &request.messages[2].content[0]
    else {
        unreachable!()
    };
    assert_eq!(structured_content.as_ref(), Some(&original));
}

#[test]
fn signed_reasoning_is_not_merged_into_visible_chat_text() {
    let mut request = request(
        Protocol::OpenAiChat,
        vec![
            ContentBlock::Thinking {
                text: "one".to_owned(),
                signature: Some("opaque-signature".to_owned()),
            },
            thinking("two"),
        ],
        vec![],
    );
    let before = request.clone();
    assert!(normalize_content(&mut request, Protocol::OpenAiChat).is_err());
    assert_eq!(request, before);
}
