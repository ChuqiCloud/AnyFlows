use std::sync::{Arc, Mutex};

use af_adapter::{
    Adaptor, AnthropicAdaptor, Bytes, Credential, HeaderMap, HttpClientConfig, HttpClientPool,
    OpenAiAdaptor, ProxyConfig, RelayContext, RemoteDnsPolicy, StatusCode, UpstreamResponse,
};
use af_domain::Role;
use af_protocol::{
    CanonicalStreamEvent, ContentBlock, ContentDelta, FinishReason, Message, ResponseChoice,
    Sampling, StreamOptions, TokenCount, UsageDetails, UsageSemantics, UsageSource,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::*;

const PROTOCOLS: [Protocol; 4] = [
    Protocol::OpenAiChat,
    Protocol::OpenAiResponses,
    Protocol::Anthropic,
    Protocol::Gemini,
];

struct RecordUsage(Arc<Mutex<Option<Usage>>>);
impl crate::GenerationCompletionHook for RecordUsage {
    fn on_complete(
        self: Box<Self>,
        usage: Result<Usage, crate::UsageResolutionError>,
    ) -> crate::GenerationCompletionFuture {
        Box::pin(async move {
            *self.0.lock().unwrap() = Some(usage.unwrap());
        })
    }
}

fn request(protocol: Protocol, stream: bool) -> CanonicalRequestEnvelope {
    let mut canonical = CanonicalRequest::new(
        if protocol == Protocol::OpenAiResponses {
            Operation::Responses
        } else {
            Operation::Chat
        },
        "gpt-4o".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("What color?".to_owned())],
        )],
        stream,
    );
    canonical.sampling =
        Sampling::new(None, None, Some(TokenCount::new(128).unwrap()), vec![]).unwrap();
    canonical.stream_options = StreamOptions::new(stream);
    canonical.into()
}

fn claimed_usage() -> Usage {
    Usage::new(
        TokenCount::new(90_000).unwrap(),
        TokenCount::new(80_000).unwrap(),
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

fn canonical_response(protocol: Protocol) -> CanonicalResponse {
    CanonicalResponse::new(
        if protocol == Protocol::OpenAiResponses {
            Operation::Responses
        } else {
            Operation::Chat
        },
        "supplier".to_owned(),
        "gpt-4o".to_owned(),
        Some(1_700_000_000),
        vec![ResponseChoice::new(
            0,
            Message::new(
                Role::Assistant,
                vec![ContentBlock::Text("violet".to_owned())],
            ),
            FinishReason::Stop,
        )],
        Some(claimed_usage()),
    )
}

#[tokio::test]
async fn merchant_full_response_ignores_supplier_usage_on_wire_and_invoice_but_self_owned_keeps_it()
{
    for upstream in [Protocol::OpenAiChat, Protocol::Anthropic] {
        for downstream in PROTOCOLS {
            let server = MockServer::start().await;
            let source = build_response(
                upstream,
                canonical_response(upstream),
                claimed_usage(),
                "supplier",
                "gpt-4o",
                1_700_000_000,
            )
            .unwrap();
            Mock::given(method("POST"))
                .and(path(if upstream == Protocol::Anthropic {
                    "/v1/messages"
                } else {
                    "/v1/chat/completions"
                }))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(source.to_vec()))
                .expect(2)
                .mount(&server)
                .await;
            let client = HttpClientPool::default()
                .get(
                    &HttpClientConfig::new(
                        ProxyConfig::parse(server.uri()).unwrap(),
                        af_adapter::HttpTimeouts::default(),
                    )
                    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy),
                )
                .unwrap();
            let adaptor: Arc<dyn Adaptor> = if upstream == Protocol::Anthropic {
                Arc::new(AnthropicAdaptor::new())
            } else {
                Arc::new(OpenAiAdaptor::new())
            };
            let machine = RelayStateMachine::new(vec![crate::RelayCandidate::new(
                adaptor,
                RelayContext::new(client)
                    .with_base_url("http://upstream.example")
                    .unwrap(),
                Credential::api_key("fixture").unwrap(),
            )])
            .unwrap();
            let (result, report) = relay_merchant_conversation_with_report(
                &machine,
                request(downstream, false),
                upstream,
                downstream,
                "merchant-local",
            )
            .await;
            let ChatResponse::Full { body, usage } = result.unwrap() else {
                panic!("expected full response")
            };
            assert!(report.successful_candidate_index().is_some());
            let billed = usage.unwrap();
            assert_eq!(billed.source(), UsageSource::Estimated);
            assert!(billed.input_tokens().get() < 100);
            assert!(billed.output_tokens().get() < 10);
            let wire = parse_response(downstream, &body).unwrap().usage.unwrap();
            assert_eq!(wire.input_tokens(), billed.input_tokens());
            assert_eq!(wire.output_tokens(), billed.output_tokens());
            let (ordinary, _) = relay_conversation_with_report(
                &machine,
                request(downstream, false),
                upstream,
                downstream,
                "ordinary",
            )
            .await;
            let ChatResponse::Full { usage, .. } = ordinary.unwrap() else {
                panic!("expected full response")
            };
            assert_eq!(
                usage.unwrap().input_tokens(),
                claimed_usage().input_tokens()
            );
        }
    }
}

fn events(tool: bool) -> Vec<CanonicalStreamEvent> {
    let mut events = vec![CanonicalStreamEvent::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    }];
    if tool {
        events.extend([
            CanonicalStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 0,
                id: "call_color".to_owned(),
                name: "color".to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 0,
                partial_json: "{\"value\":\"violet\"}".to_owned(),
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

#[tokio::test]
async fn merchant_stream_matrix_uses_one_observed_usage_for_completion_and_downstream() {
    for upstream in PROTOCOLS {
        for downstream in PROTOCOLS {
            for tool in [false, true] {
                let canonical = request(downstream, true).into_canonical();
                let mut encoder = ChatStreamEncoder::new(
                    upstream,
                    "supplier",
                    "gpt-4o",
                    1_700_000_000,
                    &canonical,
                    true,
                )
                .unwrap();
                let mut raw = Vec::new();
                for event in events(tool) {
                    raw.extend(encoder.encode_event(event).unwrap());
                }
                raw.extend(
                    encoder
                        .encode_event(CanonicalStreamEvent::Usage(claimed_usage()))
                        .unwrap(),
                );
                raw.extend(encoder.finish(Ok(claimed_usage()), true).unwrap());
                let body =
                    UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(raw))
                        .unwrap()
                        .into_body();
                let downstream_encoder = ChatStreamEncoder::new(
                    downstream,
                    "client",
                    "gpt-4o",
                    1_700_000_000,
                    &canonical,
                    true,
                )
                .unwrap()
                .with_upstream_protocol(upstream);
                let (stream, handle) = OpenAiChatStream::new(
                    body,
                    downstream_encoder,
                    OpenAiChatUsageEstimator::new(&canonical).with_local_usage_only(),
                );
                let recorded = Arc::new(Mutex::new(None));
                let mut stream = stream
                    .with_decoder(ConversationDecoder::new(upstream).unwrap())
                    .with_completion_hook(Box::new(RecordUsage(recorded.clone())));
                let mut output = Vec::new();
                while let Some(chunk) = stream.next_chunk().await {
                    output.extend(chunk.unwrap());
                }
                let billed = handle.resolve().await.unwrap();
                assert_eq!(billed.source(), UsageSource::Estimated);
                assert_eq!(*recorded.lock().unwrap(), Some(billed));
                assert!(billed.input_tokens().get() < 100);
                assert!(billed.output_tokens().get() < 100);
                let mut decoder = ConversationDecoder::new(downstream).unwrap();
                let mut decoded = decoder.push(&output).unwrap();
                decoded.extend(decoder.finish().unwrap());
                let usages = decoded
                    .iter()
                    .filter_map(|event| {
                        if let CanonicalStreamEvent::Usage(usage) = event {
                            Some(*usage)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(usages.len(), 1, "{upstream:?}->{downstream:?}");
                assert_eq!(
                    usages[0].input_tokens(),
                    billed.input_tokens(),
                    "{upstream:?}->{downstream:?}"
                );
                assert_eq!(
                    usages[0].output_tokens(),
                    billed.output_tokens(),
                    "{upstream:?}->{downstream:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn merchant_gemini_prompt_block_is_a_legal_terminal_response() {
    let canonical = request(Protocol::Gemini, true).into_canonical();
    let raw = Bytes::from_static(
        b"data: {\"responseId\":\"supplier\",\"modelVersion\":\"gpt-4o\",\"promptFeedback\":{\"blockReason\":\"SAFETY\"}}\n\n",
    );
    let body = UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), raw)
        .unwrap()
        .into_body();
    let encoder = ChatStreamEncoder::new(
        Protocol::Gemini,
        "client",
        "gpt-4o",
        1_700_000_000,
        &canonical,
        true,
    )
    .unwrap();
    let (stream, handle) = OpenAiChatStream::new(
        body,
        encoder,
        OpenAiChatUsageEstimator::new(&canonical).with_local_usage_only(),
    );
    let mut stream = stream.with_decoder(ConversationDecoder::new(Protocol::Gemini).unwrap());
    while let Some(chunk) = stream.next_chunk().await {
        chunk.unwrap();
    }
    let billed = handle.resolve().await.unwrap();
    assert_eq!(billed.source(), UsageSource::Estimated);
    assert_eq!(billed.output_tokens().get(), 0);
}
