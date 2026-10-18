use af_adapter::{HeaderMap, StatusCode, UpstreamResponse};
use af_domain::Role;
use af_protocol::{
    CanonicalStreamEvent, ContentBlock, ContentDelta, FinishReason, Message, ResponseChoice,
    Sampling, StreamOptions, TokenCount, UsageDetails, UsageSemantics, UsageSource,
};
use serde_json::{Value, json};

use super::*;

const PROTOCOLS: [Protocol; 4] = [
    Protocol::OpenAiChat,
    Protocol::OpenAiResponses,
    Protocol::Anthropic,
    Protocol::Gemini,
];

fn request(protocol: Protocol, stream: bool) -> CanonicalRequestEnvelope {
    let mut request = CanonicalRequest::new(
        if protocol == Protocol::OpenAiResponses {
            Operation::Responses
        } else {
            Operation::Chat
        },
        "public-model".to_owned(),
        vec![
            Message::new(
                Role::User,
                vec![ContentBlock::Text("Remember violet".to_owned())],
            ),
            Message::new(Role::Assistant, vec![ContentBlock::Text("OK".to_owned())]),
            Message::new(
                Role::User,
                vec![ContentBlock::Text("What was the word?".to_owned())],
            ),
        ],
        stream,
    );
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(128).unwrap()), vec![]).unwrap();
    request.into()
}

fn usage() -> Usage {
    Usage::new(
        TokenCount::new(41).unwrap(),
        TokenCount::new(1).unwrap(),
        UsageDetails::new(
            TokenCount::ZERO,
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

fn response(protocol: Protocol, tool: bool) -> CanonicalResponse {
    let content = if tool {
        vec![ContentBlock::ToolUse {
            id: "call_weather".to_owned(),
            name: "weather".to_owned(),
            input: json!({"city":"Paris"}),
            signature: None,
        }]
    } else {
        vec![ContentBlock::Text("violet".to_owned())]
    };
    CanonicalResponse::new(
        if protocol == Protocol::OpenAiResponses {
            Operation::Responses
        } else {
            Operation::Chat
        },
        "resp_fixture".to_owned(),
        "public-model".to_owned(),
        Some(1_700_000_000),
        vec![ResponseChoice::new(
            0,
            Message::new(Role::Assistant, content),
            if tool {
                FinishReason::ToolCalls
            } else {
                FinishReason::Stop
            },
        )],
        Some(usage()),
    )
}

fn parse_request(protocol: Protocol, body: &[u8]) -> CanonicalRequest {
    match protocol {
        Protocol::OpenAiChat => openai_chat::parse_request(body).unwrap(),
        Protocol::OpenAiResponses => openai_responses::parse_request(body).unwrap(),
        Protocol::Anthropic => anthropic::parse_request(body).unwrap(),
        Protocol::Gemini => gemini::parse_request("models/public-model", body).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn full_protocol_matrix_preserves_conversation_and_tool_calls() {
    for source in PROTOCOLS {
        for target in PROTOCOLS {
            let (canonical, body) = prepare_request(request(source, false), target).unwrap();
            let outbound = parse_request(target, &body);
            assert_eq!(outbound.messages, canonical.messages);
            assert_eq!(outbound.messages.len(), 3);
            for tool in [false, true] {
                let fixture = response(target, tool);
                let body = build_response(
                    target,
                    fixture,
                    usage(),
                    "upstream",
                    "upstream-model",
                    1_700_000_000,
                )
                .unwrap();
                let parsed = super::parse_response(target, &body).unwrap();
                let body = build_response(
                    source,
                    parsed,
                    usage(),
                    "client",
                    "public-model",
                    1_700_000_000,
                )
                .unwrap();
                let parsed = super::parse_response(source, &body).unwrap();
                assert_eq!(parsed.model, "public-model");
                assert_eq!(
                    parsed.choices[0].message,
                    response(source, tool).choices[0].message
                );
                assert!(
                    !String::from_utf8(body.to_vec())
                        .unwrap()
                        .contains("upstream-model")
                );
            }
        }
    }
}

fn fixture_events(tool: bool) -> Vec<CanonicalStreamEvent> {
    let mut events = vec![CanonicalStreamEvent::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    }];
    if tool {
        events.extend([
            CanonicalStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 0,
                id: "call_weather".to_owned(),
                name: "weather".to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 0,
                partial_json: "{\"city\":\"Paris\"}".to_owned(),
            },
            CanonicalStreamEvent::ToolCallEnd {
                choice_index: 0,
                tool_index: 0,
            },
        ]);
    } else {
        events.push(CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("violet".to_owned()),
        });
    }
    events.push(CanonicalStreamEvent::Finish {
        choice_index: 0,
        reason: if tool {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        },
        stop_sequence: None,
    });
    events
}

#[test]
fn protocol_matrix_preserves_tool_results_in_followup_requests() {
    let canonical = openai_chat::parse_request(
        br#"{"model":"public-model","max_tokens":128,"tools":[{"type":"function","function":{"name":"weather","parameters":{"type":"object","properties":{"city":{"type":"string"}}}}}],"messages":[{"role":"user","content":"Weather in Paris?"},{"role":"assistant","tool_calls":[{"id":"call_weather","type":"function","function":{"name":"weather","arguments":"{\"city\":\"Paris\"}"}}]},{"role":"tool","tool_call_id":"call_weather","content":"Sunny"},{"role":"user","content":"Should I take an umbrella?"}]}"#,
    )
    .unwrap();
    for source in PROTOCOLS {
        let (_, body) = prepare_request(canonical.clone().into(), source).unwrap();
        let inbound = match source {
            Protocol::OpenAiChat => openai_chat::parse_request_envelope(body).unwrap(),
            Protocol::OpenAiResponses => openai_responses::parse_request_envelope(body).unwrap(),
            Protocol::Anthropic => anthropic::parse_request_envelope(body).unwrap(),
            Protocol::Gemini => {
                gemini::parse_request_envelope("models/public-model", body).unwrap()
            }
            _ => unreachable!(),
        };
        for target in PROTOCOLS {
            let (_, body) = prepare_request(inbound.clone(), target).unwrap();
            let outbound = parse_request(target, &body);
            let normalized = CanonicalRequestEnvelope::from(outbound.clone())
                .for_conversation_protocol(Protocol::OpenAiChat)
                .unwrap()
                .into_canonical();
            assert_eq!(
                normalized.messages, canonical.messages,
                "{source:?} -> {target:?}"
            );
            assert_eq!(outbound.tools.len(), canonical.tools.len());
            for (actual, expected) in outbound.tools.iter().zip(&canonical.tools) {
                assert_eq!(actual.name, expected.name);
                assert_eq!(actual.description, expected.description);
                assert_eq!(actual.input_schema, expected.input_schema);
                assert_eq!(
                    actual.strict.unwrap_or(false),
                    expected.strict.unwrap_or(false)
                );
            }
        }
    }
}

#[tokio::test]
async fn streaming_protocol_matrix_finishes_with_content_tools_and_usage() {
    for upstream in PROTOCOLS {
        for downstream in PROTOCOLS {
            for tool in [false, true] {
                let mut canonical = request(downstream, true).into_canonical();
                canonical.stream_options = StreamOptions::new(true);
                let mut upstream_encoder = ChatStreamEncoder::new(
                    upstream,
                    "upstream",
                    "upstream-model",
                    1_700_000_000,
                    &canonical,
                    true,
                )
                .unwrap();
                let mut bytes = Vec::new();
                for event in fixture_events(tool) {
                    bytes.extend(upstream_encoder.encode_event(event).unwrap());
                }
                bytes.extend(
                    upstream_encoder
                        .encode_event(CanonicalStreamEvent::Usage(usage()))
                        .unwrap(),
                );
                bytes.extend(upstream_encoder.finish(Ok(usage()), true).unwrap());
                let encoder = ChatStreamEncoder::new(
                    downstream,
                    "client",
                    "public-model",
                    1_700_000_000,
                    &canonical,
                    true,
                )
                .unwrap();
                let upstream_body =
                    UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(bytes))
                        .unwrap()
                        .into_body();
                let (stream, usage_handle) = OpenAiChatStream::new(
                    upstream_body,
                    encoder,
                    OpenAiChatUsageEstimator::new(&canonical),
                );
                let mut stream = stream
                    .with_decoder(ConversationDecoder::new(upstream).unwrap())
                    .prefetch_first(Duration::from_secs(1))
                    .await
                    .unwrap();
                let mut output = Vec::new();
                while let Some(chunk) = stream.next_chunk().await {
                    output.extend(chunk.unwrap());
                }
                assert!(
                    usage_handle.resolve().await.is_ok(),
                    "{upstream:?} -> {downstream:?}"
                );
                let mut decoder = ConversationDecoder::new(downstream).unwrap();
                let mut events = Vec::new();
                for chunk in output.chunks(7) {
                    events.extend(decoder.push(chunk).unwrap());
                }
                events.extend(decoder.finish().unwrap());
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, CanonicalStreamEvent::StreamEnd))
                        .count(),
                    1
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, CanonicalStreamEvent::Usage(_)))
                        .count(),
                    1
                );
                if tool {
                    assert!(events.iter().any(|e| matches!(e, CanonicalStreamEvent::ToolCallStart { id, name, .. } if id == "call_weather" && name == "weather")));
                    assert!(events.iter().any(|e| matches!(e, CanonicalStreamEvent::ToolCallArgsDelta { partial_json, .. } if partial_json.contains("Paris"))));
                } else {
                    assert!(events.iter().any(|e| matches!(e, CanonicalStreamEvent::ContentDelta { delta: ContentDelta::Text(text), .. } if text == "violet")));
                }
                assert!(
                    !String::from_utf8(output)
                        .unwrap()
                        .contains("upstream-model")
                );
            }
        }
    }
}

#[tokio::test]
async fn responses_client_posts_to_chat_endpoint_and_receives_responses_output() {
    use crate::RelayCandidate;
    use af_adapter::{
        Credential, HttpClientConfig, HttpClientPool, OpenAiAdaptor, ProxyConfig, RelayContext,
        RemoteDnsPolicy,
    };
    use std::sync::Arc;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({"model":"public-model","messages":[{"role":"user","content":"Remember violet"},{"role":"assistant","content":"OK"},{"role":"user","content":"What was the word?"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"upstream","object":"chat.completion","created":1700000000,"model":"upstream-model","choices":[{"index":0,"message":{"role":"assistant","content":"violet"},"finish_reason":"stop"}],"usage":{"prompt_tokens":41,"completion_tokens":1,"total_tokens":42}})))
        .expect(1).mount(&server).await;
    let client = HttpClientPool::default()
        .get(
            &HttpClientConfig::new(
                ProxyConfig::parse(server.uri()).unwrap(),
                af_adapter::HttpTimeouts::default(),
            )
            .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy),
        )
        .unwrap();
    let machine = RelayStateMachine::new(vec![RelayCandidate::new(
        Arc::new(OpenAiAdaptor::new()),
        RelayContext::new(client)
            .with_base_url("http://upstream.example")
            .unwrap(),
        Credential::api_key("fixture-key").unwrap(),
    )])
    .unwrap();
    let (result, report) = relay_conversation_with_report(
        &machine,
        request(Protocol::OpenAiResponses, false),
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        "client",
    )
    .await;
    let ChatResponse::Full { body, usage } = result.unwrap() else {
        panic!("expected JSON")
    };
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["object"], "response");
    assert_eq!(value["output"][0]["content"][0]["text"], "violet");
    assert_eq!(usage.unwrap().input_tokens().get(), 41);
    assert!(report.successful_candidate_index().is_some());
}
