//! Real wire contracts for all sixteen conversation routes.
//!
//! Each case enters through the public request-envelope parser and the shared
//! conversion plan. Responses are encoded and decoded by the same relay types
//! used in production, then replayed without trimming assistant metadata.

use std::collections::BTreeMap;

use af_adapter::Bytes;
use af_domain::{Protocol, Role};
use af_protocol::{
    CanonicalRequestEnvelope, CanonicalStreamEvent as Event, ContentBlock, ContentDelta,
    ConversationConversionError, ConversationConversionPlan, FinishReason, TokenCount, Usage,
    UsageSemantics, UsageSource, anthropic, gemini, openai_chat, openai_responses,
};
use serde_json::{Value, json};

use super::{build_response, parse_response, prepare_request};
use crate::{chat_stream_encoder::ChatStreamEncoder, conversation_decoder::ConversationDecoder};

const MODEL: &str = "matrix-model";
const CREATED: i64 = 1_700_000_000;
const COMPACTED_SUMMARY: &str = "matrix-compacted-summary\n已确认：保留用户约束、工具结果与待办事项。\nNext: continue from the summary.";
const PROTOCOLS: [Protocol; 4] = [
    Protocol::OpenAiChat,
    Protocol::OpenAiResponses,
    Protocol::Anthropic,
    Protocol::Gemini,
];

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Text,
    MultiBlock,
    SdkDefaults,
    CompleteReplay,
    ToolLoop,
    ParallelReordered,
    ReasoningHistory,
    CompactedHistory,
    RecoverableParameters,
}

const SCENARIOS: [Scenario; 9] = [
    Scenario::Text,
    Scenario::MultiBlock,
    Scenario::SdkDefaults,
    Scenario::CompleteReplay,
    Scenario::ToolLoop,
    Scenario::ParallelReordered,
    Scenario::ReasoningHistory,
    Scenario::CompactedHistory,
    Scenario::RecoverableParameters,
];

/// 4 ingress x 4 upstream x 9 wire histories x JSON/SSE = 288 contracts.
#[test]
fn all_288_wire_routes_preserve_replay_tools_and_terminal_usage() {
    let mut completed = 0;
    let mut failures = Vec::new();
    for source in PROTOCOLS {
        for target in PROTOCOLS {
            for scenario in SCENARIOS {
                for streaming in [false, true] {
                    let label = format!("{source:?}->{target:?}/{scenario:?}/stream={streaming}");
                    let outcome = std::panic::catch_unwind(|| {
                        let wire = request_fixture(source, scenario, streaming);
                        let inbound = parse_wire(source, wire.clone(), streaming);
                        let plan = inbound
                            .clone()
                            .plan_conversation(target)
                            .unwrap_or_else(|| panic!("{label}: missing conversion plan"));
                        let prepared = parse_wire(
                            target,
                            serde_json::from_slice(plan.body()).unwrap(),
                            streaming,
                        );
                        assert_request_semantics(&prepared, scenario, &label);
                        let (relay, body) = prepare_request(inbound.clone(), target)
                            .unwrap_or_else(|error| {
                                panic!("{label}: production request prepare: {error:?}")
                            });
                        assert_eq!(
                            body,
                            *plan.body(),
                            "{label}: router/relay conversion differs"
                        );
                        assert_eq!(
                            &relay,
                            plan.request().canonical(),
                            "{label}: router/relay IR differs"
                        );

                        let tool_count = match scenario {
                            Scenario::ToolLoop => 1,
                            Scenario::ParallelReordered => 2,
                            _ => 0,
                        };
                        let response = converted_response(
                            source, target, &relay, scenario, tool_count, streaming, &label,
                        );
                        let next = replay_response(source, wire, response, tool_count);
                        let replayed = parse_wire(source, next, streaming);
                        let next_plan = replayed
                            .plan_conversation(target)
                            .unwrap_or_else(|| panic!("{label}: output could not be replayed"));
                        let next_target = parse_wire(
                            target,
                            serde_json::from_slice(next_plan.body()).unwrap(),
                            streaming,
                        );
                        assert_request_semantics(&next_target, scenario, &label);
                        let text = all_text(next_target.canonical());
                        assert!(text.contains("matrix-followup"), "{label}: followup lost");
                        if tool_count == 0 {
                            assert!(
                                text.contains("matrix-answer"),
                                "{label}: assistant output lost"
                            );
                        } else {
                            assert_tool_pairs(next_target.canonical(), tool_count, &label);
                            assert!(text.contains("matrix-result"), "{label}: tool output lost");
                            for index in 0..tool_count {
                                let call_id = format!("call_output_{index}");
                                assert!(next_target.canonical().messages.iter().flat_map(|m| &m.content).any(|block| {
                                matches!(block, ContentBlock::ToolUse { id, .. } if id == &call_id)
                            }), "{label}: emitted tool ID changed or was lost");
                                assert!(next_target.canonical().messages.iter().flat_map(|m| &m.content).any(|block| {
                                matches!(block, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == &call_id)
                            }), "{label}: emitted tool result association lost");
                                assert!(
                                    text.contains(&format!("matrix-result-output-{index}")),
                                    "{label}: emitted tool result body lost"
                                );
                            }
                        }
                    });
                    completed += 1;
                    if outcome.is_err() {
                        failures.push(label);
                    }
                }
            }
        }
    }
    assert_eq!(completed, 288);
    assert!(
        failures.is_empty(),
        "Failed wire contracts: {}",
        failures.join(", ")
    );
}

#[test]
fn responses_compaction_controls_and_opaque_state_require_the_native_protocol() {
    for stream in [false, true] {
        for (controls, opaque) in [(true, false), (false, true), (true, true)] {
            let mut wire = request_fixture(Protocol::OpenAiResponses, Scenario::Text, stream);
            wire["store"] = json!(false);
            if controls {
                wire["context_management"] =
                    json!([{"type":"compaction","compact_threshold":20000}]);
            }
            if opaque {
                wire["input"].as_array_mut().unwrap().insert(
                    0,
                    json!({"type":"compaction","id":"cmp_matrix","encrypted_content":"opaque-matrix-compaction"}),
                );
            }
            let inbound = parse_wire(Protocol::OpenAiResponses, wire.clone(), stream);
            let expected_error = if controls {
                ConversationConversionError::NativeContentRequired
            } else {
                ConversationConversionError::ContinuationRequired
            };
            for target in PROTOCOLS {
                let label = format!(
                    "Responses->{target:?}/controls={controls}/opaque={opaque}/stream={stream}"
                );
                let result = ConversationConversionPlan::prepare(inbound.clone(), target);
                if target == Protocol::OpenAiResponses {
                    let plan = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
                    let native: Value = serde_json::from_slice(plan.body()).unwrap();
                    assert_eq!(native, wire, "{label}: native compaction changed");
                    let (_, body) = prepare_request(inbound.clone(), target).unwrap();
                    assert_eq!(body, *plan.body(), "{label}: relay changed native state");
                    let replay = parse_wire(target, native, stream);
                    assert_eq!(
                        replay.plan_conversation(target).unwrap().body(),
                        plan.body(),
                        "{label}: native state could not be replayed"
                    );
                } else {
                    assert_eq!(
                        result.unwrap_err(),
                        expected_error,
                        "{label}: compaction semantics must not be discarded"
                    );
                    assert!(
                        prepare_request(inbound.clone(), target).is_err(),
                        "{label}: relay accepted incompatible compaction"
                    );
                }
            }
        }
    }
}

#[test]
fn responses_v2_compaction_trigger_is_native_only_and_reaches_the_relay_body_unchanged() {
    for stream in [false, true] {
        let mut wire = request_fixture(Protocol::OpenAiResponses, Scenario::Text, stream);
        wire["store"] = json!(false);
        wire["input"].as_array_mut().unwrap().push(json!({
            "type":"compaction_trigger",
            "vendor_extension":{"preserve":true}
        }));
        let inbound = parse_wire(Protocol::OpenAiResponses, wire.clone(), stream);
        let native = inbound
            .clone()
            .plan_conversation(Protocol::OpenAiResponses)
            .expect("native Responses V2 compaction trigger is accepted");
        assert_eq!(
            serde_json::from_slice::<Value>(native.body()).unwrap(),
            wire,
            "same-protocol routing preserves the trigger item verbatim"
        );
        let (_, relay_body) = prepare_request(inbound.clone(), Protocol::OpenAiResponses).unwrap();
        assert_eq!(relay_body, *native.body());
        for target in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
            assert!(
                inbound.clone().plan_conversation(target).is_none(),
                "Responses compaction trigger must not degrade into {target:?}"
            );
            assert!(
                prepare_request(inbound.clone(), target).is_err(),
                "relay must reject the native-only trigger for {target:?}"
            );
        }
    }
}

#[test]
fn anthropic_compaction_controls_and_signed_history_remain_native_and_replayable() {
    let controls = [
        None,
        Some((
            "compaction",
            json!({"type":"summarize","instructions":"preserve constraints","future_option":true}),
        )),
        Some((
            "context_management",
            json!({"edits":[{"type":"compact_20260112","trigger":{"type":"input_tokens","value":20000},"pause_after_compaction":true,"instructions":null}],"future_option":true}),
        )),
        Some((
            "context_management",
            json!({"edits":[{"type":"clear_thinking_20251015","keep":{"type":"thinking_turns","value":1}},{"type":"clear_tool_uses_20250919","trigger":{"type":"input_tokens","value":20000},"keep":{"type":"tool_uses","value":2}}]}),
        )),
    ];
    for stream in [false, true] {
        for control in &controls {
            for summary in [None, Some(Value::Null), Some(json!(COMPACTED_SUMMARY))] {
                let block = summary.map(|summary| {
                    json!({
                        "type":"compaction",
                        "content":summary,
                        "signature":"opaque-matrix-signature",
                        "encrypted_content":"opaque-matrix-state",
                        "tool_changes":[{"type":"tool_removal","name":"retired_tool"}],
                        "cache_control":null,
                        "annotations":null,
                        "logprobs":null,
                        "future_extension":{"preserve":true}
                    })
                });
                if control.is_none() && block.is_none() {
                    continue;
                }
                let mut wire = request_fixture(Protocol::Anthropic, Scenario::Text, stream);
                if let Some(block) = &block {
                    wire["messages"].as_array_mut().unwrap().extend([
                        json!({"role":"assistant","content":[block.clone()]}),
                        json!({"role":"user","content":"matrix-summary-next"}),
                    ]);
                }
                if let Some((key, value)) = control {
                    wire[*key] = value.clone();
                }
                let inbound = parse_wire(Protocol::Anthropic, wire.clone(), stream);
                if let Some(block) = &block {
                    assert!(inbound.canonical().messages.iter().any(|message| {
                        message.content.iter().any(|content| {
                            matches!(content, ContentBlock::AnthropicCompaction(item) if item.as_value() == block)
                        })
                    }), "signed compaction state must survive ingress validation");
                }
                for target in PROTOCOLS {
                    let label = format!(
                        "Anthropic->{target:?}/control={control:?}/history={}/stream={stream}",
                        block.is_some()
                    );
                    let result = ConversationConversionPlan::prepare(inbound.clone(), target);
                    if target == Protocol::Anthropic {
                        let plan = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
                        let native: Value = serde_json::from_slice(plan.body()).unwrap();
                        assert_eq!(native, wire, "{label}: native compaction changed");
                        let (_, body) = prepare_request(inbound.clone(), target).unwrap();
                        assert_eq!(body, *plan.body(), "{label}: relay changed native state");
                        let replay = parse_wire(target, native, stream);
                        assert_eq!(
                            replay.plan_conversation(target).unwrap().body(),
                            plan.body(),
                            "{label}: native compaction could not be replayed"
                        );
                    } else {
                        assert_eq!(
                            result.unwrap_err(),
                            ConversationConversionError::NativeContentRequired,
                            "{label}: signed compaction must not become plain text"
                        );
                        assert!(
                            prepare_request(inbound.clone(), target).is_err(),
                            "{label}: relay accepted native-only compaction"
                        );
                    }
                }
            }
        }
    }
}

fn parse_wire(protocol: Protocol, value: Value, streaming: bool) -> CanonicalRequestEnvelope {
    let body = Bytes::from(serde_json::to_vec(&value).unwrap());
    match protocol {
        Protocol::OpenAiChat => {
            openai_chat::parse_request_envelope(body).map_err(|e| e.to_string())
        }
        Protocol::OpenAiResponses => {
            openai_responses::parse_request_envelope(body).map_err(|e| e.to_string())
        }
        Protocol::Anthropic => anthropic::parse_request_envelope(body).map_err(|e| e.to_string()),
        Protocol::Gemini => {
            if streaming {
                gemini::parse_stream_request_envelope("models/matrix-model", body)
                    .map_err(|e| e.to_string())
            } else {
                gemini::parse_request_envelope("models/matrix-model", body)
                    .map_err(|e| e.to_string())
            }
        }
        _ => unreachable!(),
    }
    .unwrap_or_else(|error| panic!("{protocol:?} real wire rejected: {error}"))
}

fn request_fixture(protocol: Protocol, scenario: Scenario, stream: bool) -> Value {
    let multi = matches!(scenario, Scenario::MultiBlock);
    let mut wire = match protocol {
        Protocol::OpenAiChat => {
            json!({"model":MODEL,"messages":[{"role":"user","content":if multi { json!([{"type":"text","text":"matrix-input"},{"type":"text","text":"matrix-second"}]) } else { json!("matrix-input") }}],"max_completion_tokens":8192,"stream":stream,"stream_options":{"include_usage":true}})
        }
        Protocol::OpenAiResponses => {
            json!({"model":MODEL,"input":[{"role":"user","content":if multi { json!([{"type":"input_text","text":"matrix-input"},{"type":"input_text","text":"matrix-second"}]) } else { json!("matrix-input") }}],"max_output_tokens":8192,"stream":stream})
        }
        Protocol::Anthropic => {
            json!({"model":MODEL,"messages":[{"role":"user","content":if multi { json!([{"type":"text","text":"matrix-input"},{"type":"text","text":"matrix-second"}]) } else { json!("matrix-input") }}],"max_tokens":8192,"stream":stream})
        }
        Protocol::Gemini => {
            json!({"contents":[{"role":"user","parts":if multi { json!([{"text":"matrix-input"},{"text":"matrix-second"}]) } else { json!([{"text":"matrix-input"}]) }}],"generationConfig":{"maxOutputTokens":8192}})
        }
        _ => unreachable!(),
    };
    if protocol == Protocol::OpenAiChat && !stream {
        wire.as_object_mut().unwrap().remove("stream_options");
    }
    match scenario {
        Scenario::SdkDefaults => match protocol {
            Protocol::OpenAiChat => {
                wire["store"] = json!(false);
                wire["n"] = json!(1);
                wire["parallel_tool_calls"] = json!(true);
                wire["metadata"] = json!({"client":"matrix-sdk"});
                wire["user"] = json!("matrix-user");
            }
            Protocol::OpenAiResponses => {
                wire["store"] = json!(false);
                wire["include"] = json!(["reasoning.encrypted_content"]);
                wire["prompt_cache_key"] = json!("matrix-cache");
                wire["truncation"] = json!("auto");
            }
            Protocol::Anthropic => {
                wire["system"] = json!([{"type":"text","text":"matrix-system","cache_control":{"type":"ephemeral"}}]);
                wire["metadata"] = json!({"user_id":"matrix-user"});
                wire["service_tier"] = json!("auto");
            }
            Protocol::Gemini => {
                wire["safetySettings"] = json!([]);
                wire["generationConfig"]["candidateCount"] = json!(1);
                wire["generationConfig"]["responseMimeType"] = json!("text/plain");
            }
            _ => unreachable!(),
        },
        Scenario::CompleteReplay => append_history(protocol, &mut wire, false),
        Scenario::ToolLoop => append_tools(protocol, &mut wire, 1),
        Scenario::ParallelReordered => append_tools(protocol, &mut wire, 2),
        Scenario::ReasoningHistory => append_history(protocol, &mut wire, true),
        Scenario::CompactedHistory => append_compacted_history(protocol, &mut wire),
        Scenario::RecoverableParameters => {
            wire["unknown_client_extension"] = json!({"value":"matrix-extension"});
            wire["temperature"] = Value::Null;
            match protocol {
                Protocol::OpenAiChat => {
                    wire["stream"] = json!(stream.to_string());
                    wire["max_completion_tokens"] = json!("8192");
                }
                Protocol::OpenAiResponses => {
                    wire["stream"] = json!(stream.to_string());
                    wire["max_output_tokens"] = json!("8192");
                    wire["store"] = Value::Null;
                }
                Protocol::Anthropic => {
                    wire["stream"] = json!(stream.to_string());
                    wire["max_tokens"] = json!("8192");
                    wire["metadata"] = Value::Null;
                }
                Protocol::Gemini => {
                    wire["generationConfig"]["maxOutputTokens"] = json!("8192");
                    wire["generationConfig"]["temperature"] = Value::Null;
                }
                _ => unreachable!(),
            }
        }
        Scenario::Text | Scenario::MultiBlock => {}
    }
    wire
}

fn append_compacted_history(protocol: Protocol, wire: &mut Value) {
    // A readable summary is ordinary conversation content; it does not claim
    // to translate a provider's encrypted continuation or compaction controls.
    let (key, summary, next) = match protocol {
        Protocol::OpenAiChat => (
            "messages",
            json!({"role":"assistant","content":COMPACTED_SUMMARY}),
            json!({"role":"user","content":"matrix-summary-next"}),
        ),
        Protocol::OpenAiResponses => (
            "input",
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":COMPACTED_SUMMARY}]}),
            json!({"role":"user","content":"matrix-summary-next"}),
        ),
        Protocol::Anthropic => (
            "messages",
            json!({"role":"assistant","content":[{"type":"text","text":COMPACTED_SUMMARY}]}),
            json!({"role":"user","content":"matrix-summary-next"}),
        ),
        Protocol::Gemini => (
            "contents",
            json!({"role":"model","parts":[{"text":COMPACTED_SUMMARY}]}),
            json!({"role":"user","parts":[{"text":"matrix-summary-next"}]}),
        ),
        _ => unreachable!(),
    };
    let items = wire[key].as_array_mut().unwrap();
    items.push(summary);
    items.push(next);
}

fn append_history(protocol: Protocol, wire: &mut Value, reasoning: bool) {
    let (key, assistant, next) = match protocol {
        Protocol::OpenAiChat => (
            "messages",
            json!({"role":"assistant","content":"matrix-assistant-memory-883","reasoning_content":if reasoning { json!("matrix-reasoning") } else { Value::Null }}),
            json!({"role":"user","content":"matrix-history-next"}),
        ),
        Protocol::OpenAiResponses => (
            "input",
            json!({"type":"message","id":"msg_matrix_history","status":"completed","phase":"final_answer","role":"assistant","content":[{"type":"output_text","text":"matrix-assistant-memory-883","annotations":[]}]}),
            json!({"role":"user","content":"matrix-history-next"}),
        ),
        Protocol::Anthropic => (
            "messages",
            json!({"role":"assistant","content":if reasoning {json!([{"type":"thinking","thinking":"matrix-reasoning","signature":"anthropic-matrix-signature"},{"type":"text","text":"matrix-assistant-memory-883"}])} else {json!([{"type":"text","text":"matrix-assistant-memory-883"}])}}),
            json!({"role":"user","content":"matrix-history-next"}),
        ),
        Protocol::Gemini => (
            "contents",
            json!({"role":"model","parts":if reasoning { json!([{"text":"matrix-reasoning","thought":true,"thoughtSignature":"Z2VtaW5pLW1hdHJpeC1zaWduYXR1cmU="},{"text":"matrix-assistant-memory-883"}]) } else { json!([{"text":"matrix-assistant-memory-883"}]) }}),
            json!({"role":"user","parts":[{"text":"matrix-history-next"}]}),
        ),
        _ => unreachable!(),
    };
    let items = wire[key].as_array_mut().unwrap();
    items.push(assistant);
    items.push(next);
    if reasoning {
        match protocol {
            Protocol::OpenAiChat => wire["reasoning_effort"] = json!("medium"),
            Protocol::OpenAiResponses => {
                wire["reasoning"] = json!({"effort":"medium","summary":"auto"});
                wire["input"].as_array_mut().unwrap().insert(1, json!({"type":"reasoning","id":"rs_matrix_history","summary":[{"type":"summary_text","text":"matrix-reasoning"}]}));
            }
            Protocol::Anthropic => {
                wire["thinking"] =
                    json!({"type":"enabled","budget_tokens":2048,"display":"summarized"})
            }
            Protocol::Gemini => {
                wire["generationConfig"]["thinkingConfig"] =
                    json!({"thinkingBudget":-1,"includeThoughts":true})
            }
            _ => unreachable!(),
        }
    }
}

fn append_tools(protocol: Protocol, wire: &mut Value, count: usize) {
    let schema =
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]});
    match protocol {
        Protocol::OpenAiChat => {
            wire["tools"] =
                json!([{"type":"function","function":{"name":"read","parameters":schema}}]);
            let calls: Vec<_> = (0..count).map(|i| json!({"id":format!("call_{i}"),"type":"function","function":{"name":"read","arguments":format!("{{\"path\":\"{i}.txt\"}}")}})).collect();
            let items = wire["messages"].as_array_mut().unwrap();
            items.push(json!({"role":"assistant","content":null,"tool_calls":calls}));
            for i in (0..count).rev() {
                items.push(json!({"role":"tool","tool_call_id":format!("call_{i}"),"content":{"output":format!("matrix-result-{i}")}}));
            }
            items.push(json!({"role":"user","content":"matrix-tools-next"}));
        }
        Protocol::OpenAiResponses => {
            wire["tools"] =
                json!([{"type":"function","name":"read","parameters":schema,"strict":false}]);
            let items = wire["input"].as_array_mut().unwrap();
            for i in 0..count {
                items.push(json!({"type":"function_call","id":format!("fc_{i}"),"call_id":format!("call_{i}"),"name":"read","arguments":{"path":format!("{i}.txt")},"status":"completed"}));
            }
            for i in (0..count).rev() {
                items.push(json!({"type":"function_call_output","call_id":format!("call_{i}"),"output":{"output":format!("matrix-result-{i}")},"status":"completed"}));
            }
            items.push(json!({"role":"user","content":"matrix-tools-next"}));
        }
        Protocol::Anthropic => {
            wire["tools"] =
                json!([{"name":"read","input_schema":schema,"eager_input_streaming":true}]);
            let calls: Vec<_> = (0..count).map(|i| json!({"type":"tool_use","id":format!("call_{i}"),"name":"read","input":{"path":format!("{i}.txt")}})).collect();
            let results: Vec<_> = (0..count).rev().map(|i| json!({"type":"tool_result","tool_use_id":format!("call_{i}"),"content":{"output":format!("matrix-result-{i}")}})).collect();
            let items = wire["messages"].as_array_mut().unwrap();
            items.push(json!({"role":"assistant","content":calls}));
            items.push(json!({"role":"user","content":results}));
            items.push(json!({"role":"user","content":"matrix-tools-next"}));
        }
        Protocol::Gemini => {
            wire["tools"] =
                json!([{"functionDeclarations":[{"name":"read","parametersJsonSchema":schema}]}]);
            let calls: Vec<_> = (0..count).map(|i| json!({"functionCall":{"id":format!("call_{i}"),"name":"read","args":{"path":format!("{i}.txt")}}})).collect();
            let results: Vec<_> = (0..count).rev().map(|i| json!({"functionResponse":{"id":format!("call_{i}"),"name":"read","response":{"output":format!("matrix-result-{i}")}}})).collect();
            let items = wire["contents"].as_array_mut().unwrap();
            items.push(json!({"role":"model","parts":calls}));
            items.push(json!({"role":"user","parts":results}));
            items.push(json!({"role":"user","parts":[{"text":"matrix-tools-next"}]}));
        }
        _ => unreachable!(),
    }
}

fn assert_request_semantics(request: &CanonicalRequestEnvelope, scenario: Scenario, label: &str) {
    let text = all_text(request.canonical());
    assert!(text.contains("matrix-input"), "{label}: input lost");
    if matches!(scenario, Scenario::MultiBlock) {
        assert!(
            text.contains("matrix-second"),
            "{label}: second content block lost"
        );
    }
    if matches!(
        scenario,
        Scenario::CompleteReplay | Scenario::ReasoningHistory
    ) {
        assert!(
            text.contains("matrix-assistant-memory-883"),
            "{label}: complete assistant history lost"
        );
    }
    if matches!(scenario, Scenario::ReasoningHistory) {
        assert!(
            text.contains("matrix-reasoning"),
            "{label}: reasoning history lost"
        );
    }
    if matches!(scenario, Scenario::CompactedHistory) {
        assert!(
            request.canonical().messages.iter().any(|message| {
                message.role == Role::Assistant
                    && message.content.iter().any(
                        |block| matches!(block, ContentBlock::Text(value) if value == COMPACTED_SUMMARY),
                    )
            }),
            "{label}: readable summary or its assistant role changed"
        );
        assert!(
            text.contains("matrix-summary-next"),
            "{label}: prompt after compaction lost"
        );
    }
    if matches!(scenario, Scenario::ToolLoop | Scenario::ParallelReordered) {
        assert_tool_pairs(
            request.canonical(),
            if matches!(scenario, Scenario::ToolLoop) {
                1
            } else {
                2
            },
            label,
        );
    }
    assert_eq!(
        request
            .canonical()
            .sampling
            .max_output_tokens()
            .unwrap()
            .get(),
        8192,
        "{label}: output budget lost"
    );
}

fn all_text(request: &af_protocol::CanonicalRequest) -> String {
    fn block_text(block: &ContentBlock, text: &mut String) {
        match block {
            ContentBlock::Text(value) | ContentBlock::Thinking { text: value, .. } => {
                text.push_str(value);
                text.push('\n');
            }
            ContentBlock::ToolResult {
                content,
                structured_content,
                ..
            } => {
                if let Some(value) = structured_content {
                    text.push_str(&value.to_string());
                }
                for child in content {
                    block_text(child, text);
                }
            }
            _ => {}
        }
    }
    let mut text = String::new();
    for message in &request.messages {
        for block in &message.content {
            block_text(block, &mut text);
        }
    }
    text
}

fn assert_tool_pairs(request: &af_protocol::CanonicalRequest, count: usize, label: &str) {
    let mut calls = BTreeMap::new();
    let mut results = BTreeMap::new();
    for message in &request.messages {
        for block in &message.content {
            match block {
                ContentBlock::ToolUse { id, name, .. } => {
                    calls.insert(id, name);
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    results.insert(tool_use_id, ());
                }
                _ => {}
            }
        }
    }
    assert!(calls.len() >= count, "{label}: tool calls lost");
    assert!(results.len() >= count, "{label}: tool results lost");
    assert!(
        results.keys().all(|id| calls.contains_key(id)),
        "{label}: orphan result"
    );
}

fn converted_response(
    source: Protocol,
    target: Protocol,
    request: &af_protocol::CanonicalRequest,
    scenario: Scenario,
    tools: usize,
    stream: bool,
    label: &str,
) -> Value {
    let fixture = response_fixture(
        target,
        matches!(scenario, Scenario::ReasoningHistory),
        tools,
    );
    let raw = serde_json::to_vec(&fixture).unwrap();
    let upstream =
        parse_response(target, &raw).unwrap_or_else(|e| panic!("{label}: upstream fixture: {e:?}"));
    let usage = upstream.usage.unwrap();
    let mut original_wire_response = None;
    let response = if stream {
        let events = response_events(&upstream);
        let mut encoded = Vec::new();
        if target == Protocol::Anthropic {
            // A provider fixture must use the native encoder and its supplied
            // usage. The relay's downstream encoder estimates initial input
            // before upstream metadata arrives and is not an upstream fixture.
            let initial = Usage::new(
                usage.input_tokens(),
                TokenCount::ZERO,
                *usage.details(),
                UsageSource::Upstream,
                UsageSemantics::CacheSeparated,
            )
            .unwrap();
            let mut encoder = anthropic::AnthropicMessagesStreamEncoder::new(
                "msg_matrix_upstream",
                MODEL,
                initial,
            )
            .unwrap();
            for event in events {
                encoded.extend(
                    encoder
                        .encode(event)
                        .unwrap_or_else(|e| panic!("{label}: native upstream encode: {e:?}")),
                );
            }
            encoded.extend(encoder.encode(Event::StreamEnd).unwrap());
        } else {
            let mut upstream_encoder =
                ChatStreamEncoder::new(target, "matrix-upstream", MODEL, CREATED, request, true)
                    .unwrap()
                    .with_upstream_protocol(target);
            for event in events {
                encoded.extend(
                    upstream_encoder
                        .encode_event(event)
                        .unwrap_or_else(|e| panic!("{label}: upstream encode: {e:?}")),
                );
            }
            encoded.extend(upstream_encoder.finish(Ok(usage), true).unwrap());
        }
        let decoded = decode(target, &encoded, label);
        let decoded_usage: Vec<_> = decoded
            .iter()
            .filter_map(|event| match event {
                Event::Usage(value) => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(
            decoded_usage.len(),
            1,
            "{label}: upstream fixture usage count"
        );
        assert_eq!(
            decoded_usage[0].output_tokens(),
            usage.output_tokens(),
            "{label}: upstream fixture changed output usage before conversion"
        );
        let mut encoder =
            ChatStreamEncoder::new(source, "matrix-downstream", MODEL, CREATED, request, true)
                .unwrap()
                .with_upstream_protocol(target);
        let mut wire = Vec::new();
        for event in decoded {
            if matches!(event, Event::StreamEnd) {
                wire.extend(
                    encoder
                        .finish(Ok(usage), true)
                        .unwrap_or_else(|e| panic!("{label}: downstream finish: {e:?}")),
                );
            } else {
                wire.extend(
                    encoder
                        .encode_event(event)
                        .unwrap_or_else(|e| panic!("{label}: downstream encode: {e:?}")),
                );
            }
        }
        let decoded = decode(source, &wire, label);
        if source == Protocol::OpenAiResponses {
            let completed: Vec<Value> = String::from_utf8(wire.clone())
                .unwrap()
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter(|frame| frame["type"] == "response.completed")
                .map(|frame| frame["response"].clone())
                .collect();
            assert_eq!(
                completed.len(),
                1,
                "{label}: complete Responses replay source"
            );
            original_wire_response = completed.into_iter().next();
        }
        assert_eq!(
            decoded
                .iter()
                .filter(|e| matches!(e, Event::StreamEnd))
                .count(),
            1,
            "{label}: terminal count"
        );
        assert_eq!(
            decoded
                .iter()
                .filter(|e| matches!(e, Event::Usage(_)))
                .count(),
            1,
            "{label}: usage count"
        );
        let mut aggregate = crate::conversation_aggregate::Aggregate::default();
        aggregate
            .push(decoded)
            .unwrap_or_else(|e| panic!("{label}: aggregate: {e:?}"));
        aggregate.finish().unwrap()
    } else {
        let mut upstream = upstream;
        crate::anthropic_chat_bridge::adapt_response(target, source, &mut upstream)
            .unwrap_or_else(|e| panic!("{label}: response adapt: {e:?}"));
        let body = build_response(source, upstream, usage, "matrix-downstream", MODEL, CREATED)
            .unwrap_or_else(|e| panic!("{label}: response build: {e:?}"));
        original_wire_response = Some(serde_json::from_slice(&body).unwrap());
        parse_response(source, &body).unwrap_or_else(|e| panic!("{label}: response parse: {e:?}"))
    };
    assert_eq!(response.choices.len(), 1, "{label}: response choices");
    assert_eq!(
        response.choices[0].finish_reason,
        if tools > 0 {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        },
        "{label}: finish reason"
    );
    assert_eq!(
        response.usage.unwrap().output_tokens().get(),
        usage.output_tokens().get(),
        "{label}: output usage"
    );
    original_wire_response.unwrap_or_else(|| {
        let body =
            build_response(source, response, usage, "matrix-replay", MODEL, CREATED).unwrap();
        serde_json::from_slice(&body).unwrap()
    })
}

fn response_fixture(protocol: Protocol, reasoning: bool, tools: usize) -> Value {
    let calls: Vec<_> = (0..tools).map(|i| json!({"id":format!("call_output_{i}"),"type":"function","function":{"name":"read","arguments":format!("{{\"path\":\"output-{i}.txt\"}}")}})).collect();
    match protocol {
        Protocol::OpenAiChat => {
            json!({"id":"chatcmpl-matrix","object":"chat.completion","created":CREATED,"model":MODEL,"choices":[{"index":0,"message":{"role":"assistant","content":if tools>0 {Value::Null} else {json!("matrix-answer")},"reasoning_content":if reasoning {json!("matrix-output-reasoning")} else {Value::Null},"tool_calls":if tools>0 {json!(calls)} else {Value::Null}},"finish_reason":if tools>0 {"tool_calls"} else {"stop"}}],"usage":{"prompt_tokens":41,"completion_tokens":3,"total_tokens":44}})
        }
        Protocol::OpenAiResponses => {
            let mut output = Vec::new();
            if reasoning {
                output.push(json!({"type":"reasoning","id":"rs_matrix","summary":[{"type":"summary_text","text":"matrix-output-reasoning"}]}));
            }
            if tools > 0 {
                for i in 0..tools {
                    output.push(json!({"type":"function_call","id":format!("fc_output_{i}"),"call_id":format!("call_output_{i}"),"name":"read","arguments":format!("{{\"path\":\"output-{i}.txt\"}}"),"status":"completed"}));
                }
            } else {
                output.push(json!({"type":"message","id":"msg_matrix","status":"completed","phase":"final_answer","role":"assistant","content":[{"type":"output_text","text":"matrix-answer","annotations":[]}]}));
            }
            json!({"id":"resp_matrix","object":"response","created_at":CREATED,"model":MODEL,"status":"completed","output":output,"usage":{"input_tokens":41,"output_tokens":3,"total_tokens":44},"error":null,"incomplete_details":null})
        }
        Protocol::Anthropic => {
            let mut content = Vec::new();
            if reasoning {
                content.push(json!({"type":"thinking","thinking":"matrix-output-reasoning","signature":"anthropic-output-signature"}));
            }
            if tools > 0 {
                for i in 0..tools {
                    content.push(json!({"type":"tool_use","id":format!("call_output_{i}"),"name":"read","input":{"path":format!("output-{i}.txt")}}));
                }
            } else {
                content.push(json!({"type":"text","text":"matrix-answer"}));
            }
            json!({"id":"msg_matrix","type":"message","role":"assistant","model":MODEL,"content":content,"stop_reason":if tools>0 {"tool_use"} else {"end_turn"},"stop_sequence":null,"usage":{"input_tokens":41,"output_tokens":3}})
        }
        Protocol::Gemini => {
            let mut parts = Vec::new();
            if reasoning {
                parts.push(json!({"text":"matrix-output-reasoning","thought":true,"thoughtSignature":"Z2VtaW5pLW91dHB1dC1zaWduYXR1cmU="}));
            }
            if tools > 0 {
                for i in 0..tools {
                    parts.push(json!({"functionCall":{"id":format!("call_output_{i}"),"name":"read","args":{"path":format!("output-{i}.txt")}}}));
                }
            } else {
                parts.push(json!({"text":"matrix-answer"}));
            }
            json!({"responseId":"response-matrix","modelVersion":MODEL,"candidates":[{"index":0,"content":{"role":"model","parts":parts},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":41,"candidatesTokenCount":3,"totalTokenCount":44}})
        }
        _ => unreachable!(),
    }
}

fn response_events(response: &af_protocol::CanonicalResponse) -> Vec<Event> {
    let mut events = vec![Event::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    }];
    let mut tool_index = 0;
    for (index, block) in response.choices[0].message.content.iter().enumerate() {
        match block {
            ContentBlock::Text(text) => events.push(Event::ContentDelta {
                choice_index: 0,
                content_index: index as u32,
                delta: ContentDelta::Text(text.clone()),
            }),
            ContentBlock::Thinking { text, signature } => {
                events.push(Event::ReasoningDelta {
                    choice_index: 0,
                    content_index: index as u32,
                    text: text.clone(),
                    signature: None,
                });
                if let Some(signature) = signature {
                    events.push(Event::ReasoningDelta {
                        choice_index: 0,
                        content_index: index as u32,
                        text: String::new(),
                        signature: Some(signature.clone()),
                    });
                }
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
            } => {
                events.push(Event::ToolCallStart {
                    choice_index: 0,
                    tool_index,
                    id: id.clone(),
                    name: name.clone(),
                });
                events.push(Event::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index,
                    partial_json: serde_json::to_string(input).unwrap(),
                });
                if let Some(signature) = signature {
                    events.push(Event::ToolCallSignature {
                        choice_index: 0,
                        tool_index,
                        signature: signature.clone(),
                    });
                }
                events.push(Event::ToolCallEnd {
                    choice_index: 0,
                    tool_index,
                });
                tool_index += 1;
            }
            other => panic!("Unexpected upstream fixture block: {other:?}"),
        }
    }
    events.push(Event::Finish {
        choice_index: 0,
        reason: response.choices[0].finish_reason,
        stop_sequence: None,
    });
    events.push(Event::Usage(response.usage.unwrap()));
    events
}

fn decode(protocol: Protocol, bytes: &[u8], label: &str) -> Vec<Event> {
    let mut decoder = ConversationDecoder::new(protocol).unwrap();
    let mut events = Vec::new();
    for chunk in bytes.chunks(7) {
        events.extend(
            decoder
                .push(chunk)
                .unwrap_or_else(|e| panic!("{label}: {protocol:?} fragmented SSE: {e:?}")),
        );
    }
    events.extend(
        decoder
            .finish()
            .unwrap_or_else(|e| panic!("{label}: {protocol:?} SSE finish: {e:?}")),
    );
    events
}

fn replay_response(protocol: Protocol, mut request: Value, response: Value, tools: usize) -> Value {
    match protocol {
        Protocol::OpenAiChat => {
            let items = request["messages"].as_array_mut().unwrap();
            items.push(response["choices"][0]["message"].clone());
            for i in (0..tools).rev() {
                items.push(json!({"role":"tool","tool_call_id":format!("call_output_{i}"),"content":format!("matrix-result-output-{i}")}));
            }
            items.push(json!({"role":"user","content":"matrix-followup"}));
        }
        Protocol::OpenAiResponses => {
            let items = request["input"].as_array_mut().unwrap();
            items.extend(response["output"].as_array().unwrap().iter().cloned());
            for i in (0..tools).rev() {
                items.push(json!({"type":"function_call_output","call_id":format!("call_output_{i}"),"output":format!("matrix-result-output-{i}")}));
            }
            items.push(json!({"role":"user","content":"matrix-followup"}));
        }
        Protocol::Anthropic => {
            let items = request["messages"].as_array_mut().unwrap();
            items.push(json!({"role":"assistant","content":response["content"]}));
            if tools > 0 {
                let results: Vec<_> = (0..tools).rev().map(|i| json!({"type":"tool_result","tool_use_id":format!("call_output_{i}"),"content":format!("matrix-result-output-{i}")})).collect();
                items.push(json!({"role":"user","content":results}));
            }
            items.push(json!({"role":"user","content":"matrix-followup"}));
        }
        Protocol::Gemini => {
            let items = request["contents"].as_array_mut().unwrap();
            items.push(response["candidates"][0]["content"].clone());
            if tools > 0 {
                let results: Vec<_> = (0..tools).rev().map(|i| json!({"functionResponse":{"id":format!("call_output_{i}"),"name":"read","response":{"output":format!("matrix-result-output-{i}")}}})).collect();
                items.push(json!({"role":"user","parts":results}));
            }
            items.push(json!({"role":"user","parts":[{"text":"matrix-followup"}]}));
        }
        _ => unreachable!(),
    }
    request
}

#[test]
fn interleaved_reasoning_history_is_replayable_to_every_target() {
    for source in [Protocol::Anthropic, Protocol::Gemini] {
        let mut wire = request_fixture(source, Scenario::ReasoningHistory, false);
        if source == Protocol::Anthropic {
            let content = wire["messages"][1]["content"].as_array_mut().unwrap();
            content.push(json!({"type":"thinking","thinking":"matrix-late-reasoning","signature":"anthropic-late-signature"}));
            content.push(json!({"type":"text","text":"matrix-late-answer"}));
        } else {
            let content = wire["contents"][1]["parts"].as_array_mut().unwrap();
            content.push(json!({"text":"matrix-late-reasoning","thought":true,"thoughtSignature":"bGF0ZS1zaWduYXR1cmU="}));
            content.push(json!({"text":"matrix-late-answer"}));
        }
        let inbound = parse_wire(source, wire, false);
        for target in PROTOCOLS {
            let plan = inbound
                .clone()
                .plan_conversation(target)
                .unwrap_or_else(|| {
                    panic!("{source:?}->{target:?}: interleaved reasoning rejected")
                });
            let parsed = parse_wire(target, serde_json::from_slice(plan.body()).unwrap(), false);
            let text = all_text(parsed.canonical());
            for marker in [
                "matrix-reasoning",
                "matrix-assistant-memory-883",
                "matrix-late-reasoning",
                "matrix-late-answer",
            ] {
                assert!(
                    text.contains(marker),
                    "{source:?}->{target:?}: {marker} lost"
                );
            }
        }
    }
}

#[test]
fn multiple_text_tool_results_remain_complete_for_every_target() {
    let mut wire = request_fixture(Protocol::Anthropic, Scenario::ToolLoop, false);
    wire["messages"][2]["content"][0]["content"] = json!([
        {"type":"text","text":"matrix-result-first"},
        {"type":"text","text":"matrix-result-second"}
    ]);
    let inbound = parse_wire(Protocol::Anthropic, wire, false);
    for target in PROTOCOLS {
        let plan = inbound
            .clone()
            .plan_conversation(target)
            .unwrap_or_else(|| panic!("Anthropic->{target:?}: multi-block tool result rejected"));
        let parsed = parse_wire(target, serde_json::from_slice(plan.body()).unwrap(), false);
        assert_tool_pairs(parsed.canonical(), 1, "multi-block tool result");
        let text = all_text(parsed.canonical());
        assert!(
            text.contains("matrix-result-first"),
            "{target:?}: first tool result block lost"
        );
        assert!(
            text.contains("matrix-result-second"),
            "{target:?}: second tool result block lost"
        );
    }
}
