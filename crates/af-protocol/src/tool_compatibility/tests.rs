use af_domain::Protocol;
use bytes::Bytes;
use serde_json::json;

use super::*;
use crate::{RequestContinuation, SameProtocolDecision, UsageDetails, UsageSemantics};

fn tool(name: &str) -> ToolDef {
    ToolDef {
        name: name.to_owned(),
        description: Some(format!("caller description {name}")),
        input_schema: json!({"type":"object","properties":{"city":{"type":"string"}},"required":["city"],"additionalProperties":false}),
        strict: None,
    }
}

fn request() -> CanonicalRequestEnvelope {
    let mut request = crate::CanonicalRequest::new(
        Operation::Chat,
        "private-model".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("private-prompt".to_owned())],
        )],
        true,
    );
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(100).unwrap()), vec![]).unwrap();
    request.into()
}

fn session() -> ToolCompatibilitySession {
    ToolCompatibilitySession::prepare(request(), vec![tool("read")]).unwrap()
}

fn usage(output: i64) -> Usage {
    Usage::new(
        TokenCount::new(100).unwrap(),
        TokenCount::new(output).unwrap(),
        UsageDetails::new(
            TokenCount::new(60).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

fn start() -> Event {
    Event::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    }
}

fn call(index: u32, id: &str, name: &str) -> Event {
    Event::ToolCallStart {
        choice_index: 0,
        tool_index: index,
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

fn args(index: u32, text: &str) -> Event {
    Event::ToolCallArgsDelta {
        choice_index: 0,
        tool_index: index,
        partial_json: text.to_owned(),
    }
}

fn end(index: u32) -> Event {
    Event::ToolCallEnd {
        choice_index: 0,
        tool_index: index,
    }
}

fn finish(reason: FinishReason) -> Event {
    Event::Finish {
        choice_index: 0,
        reason,
        stop_sequence: None,
    }
}

fn text(value: &str) -> Event {
    Event::ContentDelta {
        choice_index: 0,
        content_index: 0,
        delta: ContentDelta::Text(value.to_owned()),
    }
}

fn forward(session: &mut ToolCompatibilitySession, event: Event) -> Vec<Event> {
    match session.push(event).unwrap() {
        ToolCompatibilityStep::Forward(events) => events,
        other => panic!("expected forwarding, got {other:?}"),
    }
}

fn hidden_round(session: &mut ToolCompatibilitySession, output: Option<i64>) {
    for event in [start(), call(0, "hidden-id", "read")] {
        assert!(forward(session, event).is_empty());
    }
    assert!(
        forward(
            session,
            Event::LogprobsDelta {
                choice_index: 0,
                logprobs: json!({"content": [{"token": "private-city"}]}),
            }
        )
        .is_empty()
    );
    for event in [
        args(0, "{\"city\":\"private-city\"}"),
        end(0),
        finish(FinishReason::ToolCalls),
    ] {
        assert!(forward(session, event).is_empty());
    }
    if let Some(output) = output {
        assert!(forward(session, Event::Usage(usage(output))).is_empty());
    }
}

#[test]
fn collisions_preserve_schemas_history_results_and_named_choice() {
    let mut canonical = request().into_canonical();
    canonical.tools = vec![tool("read"), tool("af_client_0"), tool("weather")];
    canonical.tools[0].strict = Some(true);
    canonical.tool_choice = ToolChoice::Named {
        name: "read".to_owned(),
    };
    canonical.messages.extend([
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "history-id".to_owned(),
                name: "read".to_owned(),
                input: json!({"city":"Paris"}),
                signature: None,
            }],
        ),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "history-id".to_owned(),
                content: vec![ContentBlock::Text("result".to_owned())],
                structured_content: None,
                is_error: false,
            }],
        ),
    ]);
    let original = canonical.clone();
    let session = ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
    let prepared = session.request().canonical();
    assert_eq!(prepared.tools[0].name, "af_client_1");
    let mut expected = original.tools[0].clone();
    expected.name = "af_client_1".to_owned();
    assert_eq!(prepared.tools[0], expected);
    assert_eq!(&prepared.tools[1..3], &original.tools[1..]);
    assert_eq!(prepared.tools[3], tool("read"));
    assert_eq!(
        prepared.tool_choice,
        ToolChoice::Named {
            name: "af_client_1".to_owned()
        }
    );
    assert!(
        matches!(&prepared.messages[1].content[0], ContentBlock::ToolUse { id, name, input, .. } if id == "history-id" && name == "af_client_1" && input == &json!({"city":"Paris"}))
    );
    assert_eq!(prepared.messages[2], original.messages[2]);
    assert_eq!(prepared.messages[0], original.messages[0]);
}

#[test]
fn historical_tools_removed_from_the_current_catalogue_still_get_aliases() {
    let mut canonical = request().into_canonical();
    canonical.messages.push(Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "old".to_owned(),
            name: "read".to_owned(),
            input: json!({}),
            signature: None,
        }],
    ));
    let session = ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
    assert!(
        matches!(&session.request().canonical().messages[1].content[0], ContentBlock::ToolUse { name, .. } if name == "af_client_0")
    );
    assert_eq!(session.request().canonical().tools.len(), 1);
}

#[test]
fn rewriting_removes_source_passthrough_and_preserves_requested_model() {
    let source = crate::openai_chat::parse_request_envelope(Bytes::from_static(br#"{"model":"public-alias","messages":[{"role":"user","content":"hello"}],"max_tokens":100}"#)).unwrap();
    let envelope = CanonicalRequestEnvelope::from_normalized_model(
        source.canonical().clone(),
        "public-alias".to_owned(),
    );
    for request in [source, envelope] {
        let session = ToolCompatibilitySession::prepare(request, vec![tool("read")]).unwrap();
        assert_eq!(session.request().requested_model(), "public-alias");
        assert_eq!(session.request().source_protocol(), None);
        assert!(matches!(
            session
                .request()
                .clone()
                .into_same_protocol(Protocol::OpenAiChat),
            SameProtocolDecision::Rebuild(_)
        ));
    }
}

#[test]
fn private_fields_opaque_state_and_signed_collisions_fail_without_data_loss() {
    let raw = crate::openai_chat::parse_request_envelope(Bytes::from_static(
        br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"max_tokens":100,"seed":42}"#,
    ))
    .unwrap();
    assert_eq!(
        ToolCompatibilitySession::prepare(raw, vec![tool("read")]).unwrap_err(),
        ToolCompatibilityError::UnsupportedRequest
    );
    let mut opaque = request().into_canonical();
    opaque.continuation = RequestContinuation::new(Some("private-response".to_owned()), None, None);
    assert_eq!(
        ToolCompatibilitySession::prepare(opaque.into(), vec![tool("read")]).unwrap_err(),
        ToolCompatibilityError::UnsupportedRequest
    );
    let mut signed = request().into_canonical();
    signed.tools.push(tool("read"));
    signed.messages.push(Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "id".to_owned(),
            name: "read".to_owned(),
            input: json!({}),
            signature: Some("private-signature".to_owned()),
        }],
    ));
    assert_eq!(
        ToolCompatibilitySession::prepare(signed.into(), vec![tool("read")]).unwrap_err(),
        ToolCompatibilityError::UnsupportedRequest
    );
}

#[test]
fn invalid_catalogues_and_missing_budgets_are_rejected() {
    for injected in [
        vec![],
        vec![tool("read"), tool("read")],
        vec![tool("bad name")],
        vec![tool("read"); 33],
    ] {
        assert_eq!(
            ToolCompatibilitySession::prepare(request(), injected).unwrap_err(),
            ToolCompatibilityError::InvalidToolSet
        );
    }
    for choice in [
        ToolChoice::Required,
        ToolChoice::Named {
            name: "read".to_owned(),
        },
    ] {
        let mut canonical = request().into_canonical();
        canonical.tool_choice = choice;
        assert_eq!(
            ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap_err(),
            ToolCompatibilityError::UnsupportedRequest
        );
    }
    let mut canonical = request().into_canonical();
    canonical.sampling = Sampling::EMPTY;
    assert_eq!(
        ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap_err(),
        ToolCompatibilityError::MissingOutputBudget
    );
}

#[test]
fn text_and_reasoning_are_forwarded_before_the_round_finishes() {
    for event in [
        text("live"),
        Event::ReasoningDelta {
            choice_index: 0,
            content_index: 0,
            text: "reasoning".to_owned(),
            signature: None,
        },
    ] {
        let mut session = session();
        assert!(forward(&mut session, start()).is_empty());
        assert_eq!(forward(&mut session, event.clone()), vec![start(), event]);
        assert!(forward(&mut session, finish(FinishReason::Stop)).is_empty());
        assert!(matches!(
            session.push(Event::StreamEnd).unwrap(),
            ToolCompatibilityStep::Complete { .. }
        ));
        assert_eq!(
            session.push(Event::Ping).unwrap_err(),
            ToolCompatibilityError::Closed
        );
    }
}

#[test]
fn caller_tool_names_are_restored_in_live_events() {
    let mut canonical = request().into_canonical();
    canonical.tools.push(tool("read"));
    let mut session =
        ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
    forward(&mut session, start());
    assert_eq!(
        forward(&mut session, call(7, "real-id", "af_client_0")),
        vec![start(), call(7, "real-id", "read")]
    );
    assert_eq!(forward(&mut session, args(7, "{}")), vec![args(7, "{}")]);
    assert_eq!(forward(&mut session, end(7)), vec![end(7)]);
    forward(&mut session, finish(FinishReason::ToolCalls));
    assert!(matches!(
        session.push(Event::StreamEnd).unwrap(),
        ToolCompatibilityStep::Complete { .. }
    ));
}

#[test]
fn only_hidden_tools_allow_one_continuation_with_real_results_and_a_shared_budget() {
    let mut session = session();
    hidden_round(&mut session, Some(20));
    let ToolCompatibilityStep::Continue {
        request,
        usage: first,
    } = session.push(Event::StreamEnd).unwrap()
    else {
        panic!("expected continuation")
    };
    assert_eq!(first, usage(20));
    let canonical = request.canonical();
    assert_eq!(canonical.sampling.max_output_tokens().unwrap().get(), 80);
    assert_eq!(canonical.messages.len(), 3);
    assert!(
        matches!(&canonical.messages[1].content[0], ContentBlock::ToolUse { id, name, input, .. } if id == "hidden-id" && name == "read" && input == &json!({"city":"private-city"}))
    );
    assert!(
        matches!(&canonical.messages[2].content[0], ContentBlock::ToolResult { tool_use_id, content, is_error: false, .. } if tool_use_id == "hidden-id" && matches!(&content[0], ContentBlock::Text(text) if text.contains("Tool unavailable") && !text.contains("private-city")))
    );
    for protocol in [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
    ] {
        let request = request
            .as_ref()
            .clone()
            .for_conversation_protocol(protocol)
            .unwrap();
        let mut canonical = request.into_canonical();
        if protocol == Protocol::Gemini {
            // Gemini 的响应模式由 URL 表达，正文仍使用 generateContent 结构。
            canonical.stream = false;
        }
        let built = match protocol {
            Protocol::OpenAiChat => crate::openai_chat::build_request(&canonical).is_ok(),
            Protocol::OpenAiResponses => crate::openai_responses::build_request(&canonical).is_ok(),
            Protocol::Anthropic => crate::anthropic::build_request(&canonical).is_ok(),
            Protocol::Gemini => crate::gemini::build_request(&canonical).is_ok(),
            _ => unreachable!(),
        };
        assert!(built, "{protocol:?}");
    }
    forward(&mut session, start());
    assert_eq!(
        forward(&mut session, text("answer")),
        vec![start(), text("answer")]
    );
    forward(&mut session, finish(FinishReason::Stop));
    forward(&mut session, Event::Usage(usage(30)));
    let ToolCompatibilityStep::Complete {
        events,
        usage: Some(total),
    } = session.push(Event::StreamEnd).unwrap()
    else {
        panic!("expected completion")
    };
    assert_eq!(total.input_tokens().get(), 200);
    assert_eq!(total.output_tokens().get(), 50);
    assert_eq!(total.details().cache_read().get(), 120);
    assert_eq!(
        events,
        vec![
            finish(FinishReason::Stop),
            Event::Usage(total),
            Event::StreamEnd
        ]
    );
    assert_eq!(
        session.observed_usage(),
        &[Some(usage(20)), Some(usage(30))]
    );
}

#[test]
fn hidden_tools_cannot_be_mixed_with_visible_content_in_either_order() {
    for visible_first in [true, false] {
        let mut session = session();
        forward(&mut session, start());
        let (first, second) = if visible_first {
            (text("answer"), call(0, "id", "read"))
        } else {
            (call(0, "id", "read"), text("answer"))
        };
        forward(&mut session, first);
        assert_eq!(
            session.push(second).unwrap_err(),
            ToolCompatibilityError::HiddenToolAfterOutput
        );
        assert_eq!(
            session.push(Event::StreamEnd).unwrap_err(),
            ToolCompatibilityError::Closed
        );
    }
    for hidden_first in [true, false] {
        let mut canonical = request().into_canonical();
        canonical.tools.push(tool("weather"));
        let mut session =
            ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
        forward(&mut session, start());
        let (first, second) = if hidden_first {
            (call(0, "hidden", "read"), call(1, "real", "weather"))
        } else {
            (call(0, "real", "weather"), call(1, "hidden", "read"))
        };
        forward(&mut session, first);
        assert_eq!(
            session.push(second).unwrap_err(),
            ToolCompatibilityError::HiddenToolAfterOutput
        );
    }
}

#[test]
fn another_hidden_tool_in_the_second_round_fails_and_keeps_first_round_usage() {
    let mut session = session();
    hidden_round(&mut session, Some(20));
    assert!(matches!(
        session.push(Event::StreamEnd).unwrap(),
        ToolCompatibilityStep::Continue { .. }
    ));
    forward(&mut session, start());
    assert_eq!(
        session.push(call(0, "again", "read")).unwrap_err(),
        ToolCompatibilityError::RepeatedUnavailableTool
    );
    assert_eq!(session.observed_usage(), &[Some(usage(20)), None]);
}

#[test]
fn incomplete_rounds_invalid_arguments_and_unknown_tools_do_not_continue() {
    for events in [
        vec![start(), call(0, "id", "read"), Event::StreamEnd],
        vec![start(), call(0, "id", "read"), args(0, "{"), end(0)],
        vec![start(), call(0, "id", "read"), args(0, "[]"), end(0)],
        vec![start(), call(0, "id", "not-declared")],
        vec![start(), call(0, "id", "read"), call(0, "id2", "read")],
        vec![start(), call(0, "id", "read"), call(1, "id", "read")],
        vec![start(), finish(FinishReason::Stop), Event::StreamEnd],
        vec![
            start(),
            Event::MessageStart {
                choice_index: 1,
                role: Role::Assistant,
            },
        ],
        vec![
            start(),
            Event::Error(af_domain::UpstreamError::ProtocolError),
        ],
    ] {
        let mut session = session();
        assert!(events.into_iter().any(|event| session.push(event).is_err()));
        assert_eq!(
            session.push(Event::StreamEnd).unwrap_err(),
            ToolCompatibilityError::Closed
        );
    }
}

#[test]
fn missing_usage_and_budget_exhaustion_cannot_start_an_unfunded_round() {
    for output in [None, Some(0), Some(100)] {
        let mut session = session();
        hidden_round(&mut session, output);
        assert!(session.push(Event::StreamEnd).is_err());
    }
    let mut session = session();
    hidden_round(&mut session, Some(20));
    session.push(Event::StreamEnd).unwrap();
    forward(&mut session, start());
    forward(&mut session, text("answer"));
    assert_eq!(
        session.push(Event::Usage(usage(81))).unwrap_err(),
        ToolCompatibilityError::OutputBudgetExceeded
    );
    assert_eq!(
        session.observed_usage(),
        &[Some(usage(20)), Some(usage(81))]
    );
}

#[test]
fn oversized_arguments_and_abort_do_not_release_hidden_state() {
    let mut session = session();
    forward(&mut session, start());
    forward(&mut session, call(0, "id", "read"));
    assert_eq!(
        session
            .push(args(0, &"x".repeat(MAX_ARGUMENT_BYTES + 1)))
            .unwrap_err(),
        ToolCompatibilityError::ArgumentLimit
    );
    let mut aborted = super::tests::session();
    hidden_round(&mut aborted, Some(20));
    aborted.abort();
    assert_eq!(
        aborted.push(Event::StreamEnd).unwrap_err(),
        ToolCompatibilityError::Closed
    );
    assert_eq!(aborted.observed_usage(), &[Some(usage(20)), None]);
}

#[test]
fn debug_output_and_errors_do_not_include_request_or_tool_secrets() {
    let mut session =
        ToolCompatibilitySession::prepare(request(), vec![tool("read"), tool("private-tool-name")])
            .unwrap();
    hidden_round(&mut session, Some(20));
    let debug = format!("{session:?}");
    for private in [
        "private-model",
        "private-prompt",
        "private-city",
        "hidden-id",
        "caller description",
        "private-tool-name",
    ] {
        assert!(!debug.contains(private));
    }
    for error in [
        ToolCompatibilityError::InvalidOutput,
        ToolCompatibilityError::UnknownTool,
    ] {
        assert!(!error.to_string().contains("private"));
    }
}

#[test]
fn specified_and_disabled_tool_choices_are_enforced_without_substitution() {
    for (choice, invalid_name) in [
        (ToolChoice::None, "weather"),
        (
            ToolChoice::Named {
                name: "weather".to_owned(),
            },
            "other",
        ),
        (
            ToolChoice::Named {
                name: "weather".to_owned(),
            },
            "read",
        ),
    ] {
        let mut canonical = request().into_canonical();
        canonical.tools = vec![tool("weather"), tool("other")];
        canonical.tool_choice = choice;
        let mut session =
            ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
        forward(&mut session, start());
        assert_eq!(
            session.push(call(0, "id", invalid_name)).unwrap_err(),
            ToolCompatibilityError::InvalidOutput
        );
    }
    for choice in [
        ToolChoice::Required,
        ToolChoice::Named {
            name: "weather".to_owned(),
        },
    ] {
        let mut canonical = request().into_canonical();
        canonical.tools = vec![tool("weather")];
        canonical.tool_choice = choice;
        let mut session =
            ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
        forward(&mut session, start());
        forward(&mut session, text("a tool was required"));
        forward(&mut session, finish(FinishReason::Stop));
        assert_eq!(
            session.push(Event::StreamEnd).unwrap_err(),
            ToolCompatibilityError::InvalidOutput
        );
    }
}

#[test]
fn signatures_are_preserved_only_when_the_tool_name_is_unchanged() {
    for collides in [false, true] {
        let mut canonical = request().into_canonical();
        canonical.tools = vec![tool(if collides { "read" } else { "weather" })];
        let mut session =
            ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
        forward(&mut session, start());
        forward(
            &mut session,
            call(0, "id", if collides { "af_client_0" } else { "weather" }),
        );
        let signature = Event::ToolCallSignature {
            choice_index: 0,
            tool_index: 0,
            signature: "private-signature".to_owned(),
        };
        if collides {
            assert_eq!(
                session.push(signature).unwrap_err(),
                ToolCompatibilityError::UnsupportedSignature
            );
        } else {
            assert_eq!(forward(&mut session, signature.clone()), vec![signature]);
        }
    }
}

#[test]
fn continuation_cannot_reuse_a_previous_call_id_or_lose_final_usage() {
    for reuse_id in [true, false] {
        let mut canonical = request().into_canonical();
        canonical.tools.push(tool("weather"));
        let mut session =
            ToolCompatibilitySession::prepare(canonical.into(), vec![tool("read")]).unwrap();
        hidden_round(&mut session, Some(20));
        session.push(Event::StreamEnd).unwrap();
        forward(&mut session, start());
        if reuse_id {
            assert_eq!(
                session.push(call(0, "hidden-id", "weather")).unwrap_err(),
                ToolCompatibilityError::InvalidOutput
            );
        } else {
            forward(&mut session, text("answer"));
            forward(&mut session, finish(FinishReason::Stop));
            assert_eq!(
                session.push(Event::StreamEnd).unwrap_err(),
                ToolCompatibilityError::MissingUsage
            );
        }
        assert_eq!(session.observed_usage(), &[Some(usage(20)), None]);
    }
}
