use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_adapter::{
    Adaptor, AdaptorResult, AdaptorTarget, Credential, HeaderMap, HttpClientConfig, HttpClientPool,
    OpenAiAdaptor, OpenCodeAdaptor, ProxyConfig, RelayContext, RemoteDnsPolicy, UpstreamRequest,
};
use af_domain::{ChannelType, OpenCodeEntry, Operation, Protocol, Role};
use af_protocol::{
    CanonicalRequest, CanonicalStreamEvent as Event, ContentBlock, ContentDelta, FinishReason,
    Message, Sampling, StreamOptions, TokenCount, ToolDef, Usage, UsageDetails, UsageSemantics,
    UsageSource,
};
use futures_core::Stream;
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

use super::*;
use crate::{
    RelayAttemptGate, RelayAttemptGateFuture, RelayAttemptPermit, RelayAttemptReleaseFuture,
    RelayCandidate, RelayCandidateRequest, conversation_decoder::ConversationDecoder,
};

#[test]
fn cancellation_preserves_completed_state_and_round_count() {
    let handle = ToolConversationReportHandle::new(RelayAttemptReport::default());
    handle.cancel();
    assert_eq!(handle.snapshot().state, ToolConversationState::Failed);
    handle.update(|report| {
        report.state = ToolConversationState::Completed;
        report.sent_rounds = 2;
    });
    handle.cancel();
    assert_eq!(handle.snapshot().state, ToolConversationState::Completed);
    assert_eq!(handle.snapshot().sent_rounds, 2);
}

struct FixtureAdaptor(Protocol);

#[async_trait::async_trait]
impl Adaptor for FixtureAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::OpenCode
    }
    fn default_protocol(&self) -> Protocol {
        self.0
    }
    fn default_base_url(&self) -> &str {
        "http://upstream.example"
    }
    fn supported_models(&self) -> Vec<String> {
        Vec::new()
    }
    fn build_url(
        &self,
        _context: &RelayContext,
        _target: AdaptorTarget<'_>,
    ) -> AdaptorResult<String> {
        Ok("http://upstream.example/v1/fixture".to_owned())
    }
    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        OpenCodeAdaptor::new(OpenCodeEntry::InferenceV1, self.0, vec![])?
            .setup_headers(headers, credential, context)
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        OpenCodeAdaptor::new(OpenCodeEntry::InferenceV1, self.0, vec![])?
            .finalize_request(request, credential, context)
            .await
    }
}

#[derive(Default)]
struct Permits {
    acquired: AtomicUsize,
    released: Arc<AtomicUsize>,
}
struct Permit(Arc<AtomicUsize>);
impl RelayAttemptGate for Permits {
    fn acquire(&self) -> RelayAttemptGateFuture<'_> {
        self.acquired.fetch_add(1, Ordering::SeqCst);
        let released = self.released.clone();
        Box::pin(async move { Ok(Box::new(Permit(released)) as Box<dyn RelayAttemptPermit>) })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl RelayAttemptPermit for Permit {
    fn release(self: Box<Self>) -> RelayAttemptReleaseFuture {
        Box::pin(async move {
            drop(self);
        })
    }
}

fn request(stream: bool) -> CanonicalRequestEnvelope {
    let mut request = CanonicalRequest::new(
        Operation::Chat,
        "free-model".to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("Reply violet".to_owned())],
        )],
        stream,
    );
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(128).unwrap()), vec![]).unwrap();
    request.stream_options = StreamOptions::new(true);
    request.into()
}

fn tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "read".to_owned(),
        description: Some("Read a file.".to_owned()),
        input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        strict: None,
    }]
}

fn plan(stream: bool, protocol: Protocol) -> ToolConversationPlan {
    ToolConversationPlan::prepare(
        request(stream),
        protocol,
        tools(),
        TokenCount::new(4096).unwrap(),
        Duration::from_secs(3),
    )
    .unwrap()
}

fn usage(input: i64, output: i64) -> Usage {
    Usage::new(
        TokenCount::new(input).unwrap(),
        TokenCount::new(output).unwrap(),
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

fn events(hidden: bool, repeated: bool) -> Vec<Event> {
    let mut result = vec![Event::MessageStart {
        choice_index: 0,
        role: Role::Assistant,
    }];
    if hidden {
        result.extend([
            Event::ToolCallStart {
                choice_index: 0,
                tool_index: 0,
                id: if repeated { "call_again" } else { "call_read" }.to_owned(),
                name: "read".to_owned(),
            },
            Event::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 0,
                partial_json: "{\"path\":\"private-file\"}".to_owned(),
            },
            Event::ToolCallEnd {
                choice_index: 0,
                tool_index: 0,
            },
        ]);
    } else {
        result.push(Event::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("violet".to_owned()),
        });
    }
    result.push(Event::Finish {
        choice_index: 0,
        reason: if hidden {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        },
        stop_sequence: None,
    });
    result
}

fn sse(protocol: Protocol, hidden: bool, repeated: bool) -> Vec<u8> {
    wire(
        protocol,
        events(hidden, repeated),
        usage(if repeated { 50 } else { 40 }, if hidden { 10 } else { 5 }),
    )
}

fn wire(protocol: Protocol, events: Vec<Event>, usage: Usage) -> Vec<u8> {
    let request = plan(true, protocol);
    let mut encoder = ChatStreamEncoder::new(
        protocol,
        "fixture",
        "free-model",
        1_700_000_000,
        request.request().canonical(),
        true,
    )
    .unwrap();
    let mut bytes = Vec::new();
    for event in events {
        bytes.extend(encoder.encode_event(event).unwrap());
    }
    bytes.extend(encoder.encode_event(Event::Usage(usage)).unwrap());
    bytes.extend(encoder.finish(Ok(usage), true).unwrap());
    bytes
}

fn candidate(
    server: &MockServer,
    protocol: Protocol,
    gate: Arc<Permits>,
    credential: Credential,
) -> RelayCandidate {
    candidate_uri(&server.uri(), protocol, gate, credential)
}

fn candidate_uri(
    uri: &str,
    protocol: Protocol,
    gate: Arc<Permits>,
    credential: Credential,
) -> RelayCandidate {
    let client = HttpClientPool::default()
        .get(
            &HttpClientConfig::new(
                ProxyConfig::parse(uri).unwrap(),
                af_adapter::HttpTimeouts::default(),
            )
            .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy),
        )
        .unwrap();
    RelayCandidate::new(
        Arc::new(FixtureAdaptor(protocol)),
        RelayContext::new(client)
            .with_request_id("fixed-request")
            .unwrap()
            .with_client_session_scope("a".repeat(64))
            .unwrap()
            .with_credential_scope(17)
            .unwrap(),
        credential,
    )
    .with_attempt_gate(gate)
}

async fn fixture(protocol: Protocol, repeat: bool) -> MockServer {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .respond_with(move |_request: &Request| {
            let round = count.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_bytes(sse(protocol, round == 0 || repeat, round != 0))
        })
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn frozen_client_profile_overrides_adapter_identity_on_both_rounds() {
    for public in [false, true] {
        let server = fixture(Protocol::OpenAiChat, false).await;
        let gate = Arc::new(Permits::default());
        let key = if public {
            "opencode-public"
        } else {
            "opencode"
        };
        let mut profile = af_domain::built_in_client_profiles()
            .into_iter()
            .find(|profile| profile.binding.key == key)
            .unwrap();
        profile.binding.key = "custom-opencode-check".to_owned();
        profile.binding.revision = 7;
        profile
            .definition
            .identity_headers
            .insert("user-agent".to_owned(), "profile-check/7.0".to_owned());
        profile
            .definition
            .identity_headers
            .insert("x-opencode-client".to_owned(), "profile-check".to_owned());
        let credential = if public {
            Credential::public()
        } else {
            Credential::api_key("test-api-key").unwrap()
        };
        let machine = RelayStateMachine::new(vec![
            candidate(&server, Protocol::OpenAiChat, gate, credential).with_client_profile(profile),
        ])
        .unwrap()
        .with_diagnostic_policy(crate::RelayDiagnosticPolicy::new(true, false, 4096).unwrap());
        let (result, handle) = relay_tool_conversation(
            &machine,
            plan(false, Protocol::OpenAiChat),
            Protocol::OpenAiChat,
            "profile-headers",
        )
        .await;
        assert!(result.is_ok());
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.state, ToolConversationState::Completed);
        for diagnostic in [
            snapshot.report.successful_diagnostic().unwrap(),
            snapshot.continuation_diagnostic.as_ref().unwrap(),
        ] {
            let headers = diagnostic.request_headers_json().unwrap();
            assert!(headers.contains("x-opencode-client"));
            assert!(headers.contains("profile-check"));
            assert!(!headers.contains("profile-check/7.0"));
            assert!(!headers.contains("test-api-key"));
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert_eq!(request.headers["user-agent"], "profile-check/7.0");
            assert_eq!(request.headers["x-opencode-client"], "profile-check");
            assert_eq!(
                request.headers["authorization"],
                if public {
                    "Bearer public"
                } else {
                    "Bearer test-api-key"
                }
            );
        }
    }
}

#[tokio::test]
async fn two_round_transport_preserves_identity_and_merges_usage_for_all_downstream_modes() {
    for protocol in [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Anthropic,
    ] {
        for downstream in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
            Protocol::Gemini,
        ] {
            for stream in [false, true] {
                let server = fixture(protocol, false).await;
                let gate = Arc::new(Permits::default());
                let plan = plan(stream, protocol);
                let bound = plan.upper_bound();
                let machine = RelayStateMachine::new(vec![candidate(
                    &server,
                    protocol,
                    gate.clone(),
                    Credential::public(),
                )])
                .unwrap();
                let (result, report) =
                    relay_tool_conversation(&machine, plan, downstream, "client").await;
                let mut bytes = Vec::new();
                let settled = match result.unwrap() {
                    ChatResponse::Full { body, usage } => {
                        bytes.extend(body);
                        usage.unwrap()
                    }
                    ChatResponse::Stream { mut body, usage } => {
                        while let Some(chunk) =
                            std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx)).await
                        {
                            bytes.extend(chunk.unwrap());
                        }
                        usage.resolve().await.unwrap()
                    }
                };
                let snapshot = report.snapshot();
                assert_eq!(
                    settled,
                    snapshot.observed_usage[0]
                        .unwrap()
                        .checked_add(snapshot.observed_usage[1].unwrap())
                        .unwrap(),
                    "{protocol:?} -> {downstream:?}"
                );
                assert!(settled.checked_input_tokens().unwrap() <= bound.input_tokens());
                let output = String::from_utf8(bytes.clone()).unwrap();
                assert!(output.contains("violet"));
                assert!(!output.contains("private-file") && !output.contains("call_read"));
                if stream {
                    let mut decoder = ConversationDecoder::new(downstream).unwrap();
                    let mut events = decoder.push(&bytes).unwrap();
                    events.extend(decoder.finish().unwrap());
                    assert_eq!(
                        events
                            .iter()
                            .filter(|event| matches!(event, Event::StreamEnd))
                            .count(),
                        1,
                        "{protocol:?} -> {downstream:?}"
                    );
                    assert_eq!(
                        events
                            .iter()
                            .filter(|event| matches!(event, Event::MessageStart { .. }))
                            .count(),
                        1,
                        "{protocol:?} -> {downstream:?}"
                    );
                }
                assert_eq!(snapshot.state, ToolConversationState::Completed);
                assert_eq!(snapshot.sent_rounds, 2);
                assert!(snapshot.observed_usage.iter().all(Option::is_some));
                assert_eq!(gate.acquired.load(Ordering::SeqCst), 1);
                assert_eq!(gate.released.load(Ordering::SeqCst), 1);
                let requests = server.received_requests().await.unwrap();
                assert_eq!(requests.len(), 2);
                assert_eq!(requests[0].url, requests[1].url);
                for header in [
                    "authorization",
                    "x-api-key",
                    "x-opencode-session",
                    "x-opencode-project",
                    "user-agent",
                ] {
                    assert_eq!(
                        requests[0].headers.get(header),
                        requests[1].headers.get(header)
                    );
                }
                let first: Value = serde_json::from_slice(&requests[0].body).unwrap();
                let second: Value = serde_json::from_slice(&requests[1].body).unwrap();
                let limit = match protocol {
                    Protocol::OpenAiChat => "max_completion_tokens",
                    Protocol::OpenAiResponses => "max_output_tokens",
                    _ => "max_tokens",
                };
                assert_eq!(
                    first[limit].as_i64().unwrap()
                        - snapshot.observed_usage[0].unwrap().output_tokens().get(),
                    second[limit].as_i64().unwrap()
                );
                assert_eq!(first["stream"], true);
                assert_eq!(second["stream"], true);
                assert!(String::from_utf8_lossy(&requests[1].body).contains("Tool unavailable"));
            }
        }
    }
}

#[test]
fn budget_contains_injected_definitions_two_contexts_and_one_output_limit() {
    let plan = plan(false, Protocol::OpenAiChat);
    let initial = estimate_openai_request_upper_bound(plan.request().canonical()).unwrap();
    let plain = estimate_openai_request_upper_bound(request(false).canonical()).unwrap();
    assert!(initial.input_tokens() > plain.input_tokens());
    assert_eq!(
        plan.upper_bound().input_tokens().get(),
        2 * initial.input_tokens().get() + initial.output_tokens().get() + 4096
    );
    assert_eq!(plan.upper_bound().output_tokens(), initial.output_tokens());
    let debug = format!("{plan:?}");
    assert!(!debug.contains("Reply violet") && !debug.contains("Read a file"));
    for timeout in [Duration::ZERO, Duration::from_secs(601)] {
        assert!(
            ToolConversationPlan::prepare(
                request(false),
                Protocol::OpenAiChat,
                tools(),
                TokenCount::ZERO,
                timeout
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn oversized_continuation_context_is_rejected_before_second_send() {
    let server = MockServer::start().await;
    let mut events = events(true, false);
    events[2] = Event::ToolCallArgsDelta {
        choice_index: 0,
        tool_index: 0,
        partial_json: json!({"path":"payload ".repeat(4000)}).to_string(),
    };
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_bytes(wire(Protocol::OpenAiChat, events, usage(40, 10))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let gate = Arc::new(Permits::default());
    let machine = RelayStateMachine::new(vec![candidate(
        &server,
        Protocol::OpenAiChat,
        gate.clone(),
        Credential::public(),
    )])
    .unwrap();
    let plan = ToolConversationPlan::prepare(
        request(false),
        Protocol::OpenAiChat,
        tools(),
        TokenCount::ZERO,
        Duration::from_secs(3),
    )
    .unwrap();
    let (result, report) =
        relay_tool_conversation(&machine, plan, Protocol::OpenAiChat, "budget").await;
    assert!(result.is_err());
    let snapshot = report.snapshot();
    assert_eq!(snapshot.sent_rounds, 1);
    assert_eq!(snapshot.state, ToolConversationState::Failed);
    assert_eq!(snapshot.observed_usage[0].unwrap(), usage(40, 10));
    assert_eq!(gate.released.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn second_round_http_failure_preserves_consumption_and_releases_one_permit() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .respond_with(move |_request: &Request| {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_bytes(sse(Protocol::OpenAiChat, true, false))
            } else {
                ResponseTemplate::new(503).set_body_json(json!({"error":{"message":"unavailable"}}))
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let gate = Arc::new(Permits::default());
    let machine = RelayStateMachine::new(vec![candidate(
        &server,
        Protocol::OpenAiChat,
        gate.clone(),
        Credential::public(),
    )])
    .unwrap();
    let (result, report) = relay_tool_conversation(
        &machine,
        plan(false, Protocol::OpenAiChat),
        Protocol::OpenAiChat,
        "http-error",
    )
    .await;
    assert!(result.is_err());
    assert_eq!(report.snapshot().observed_usage[0], Some(usage(40, 10)));
    assert_eq!(report.snapshot().state, ToolConversationState::Failed);
    assert_eq!(gate.acquired.load(Ordering::SeqCst), 1);
    assert_eq!(gate.released.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn total_timeout_covers_both_rounds_and_retains_first_round_usage() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .respond_with(move |_request: &Request| {
            let round = count.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_bytes(sse(Protocol::OpenAiChat, round == 0, round != 0))
                .set_delay(Duration::from_millis(180))
        })
        .mount(&server)
        .await;
    let gate = Arc::new(Permits::default());
    let machine = RelayStateMachine::new(vec![candidate(
        &server,
        Protocol::OpenAiChat,
        gate.clone(),
        Credential::public(),
    )])
    .unwrap();
    let plan = ToolConversationPlan::prepare(
        request(false),
        Protocol::OpenAiChat,
        tools(),
        TokenCount::new(4096).unwrap(),
        Duration::from_millis(300),
    )
    .unwrap();
    let (result, report) =
        relay_tool_conversation(&machine, plan, Protocol::OpenAiChat, "deadline").await;
    assert!(result.is_err());
    assert_eq!(report.snapshot().sent_rounds, 2);
    assert!(report.snapshot().observed_usage[0].is_some());
    assert_eq!(gate.released.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn visible_stream_returns_before_terminal_and_drop_cancels_without_a_continuation() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::Notify,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}", listener.local_addr().unwrap());
    let finish = Arc::new(Notify::new());
    let wait = finish.clone();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let length = socket.read(&mut buffer).await.unwrap();
            assert_ne!(length, 0);
            request.extend_from_slice(&buffer[..length]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
        let first = b"data: {\"id\":\"fixture\",\"object\":\"chat.completion.chunk\",\"created\":1700000000,\"model\":\"free-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"violet\"},\"finish_reason\":null}]}\n\n";
        socket
            .write_all(format!("{:x}\r\n", first.len()).as_bytes())
            .await
            .unwrap();
        socket.write_all(first).await.unwrap();
        socket.write_all(b"\r\n").await.unwrap();
        socket.flush().await.unwrap();
        wait.notified().await;
    });
    let gate = Arc::new(Permits::default());
    let machine = RelayStateMachine::new(vec![candidate_uri(
        &uri,
        Protocol::OpenAiChat,
        gate.clone(),
        Credential::public(),
    )])
    .unwrap();
    let (result, report) = tokio::time::timeout(
        Duration::from_secs(2),
        relay_tool_conversation(
            &machine,
            plan(true, Protocol::OpenAiChat),
            Protocol::OpenAiChat,
            "immediate",
        ),
    )
    .await
    .unwrap();
    let ChatResponse::Stream { mut body, usage } = result.unwrap() else {
        panic!("stream expected")
    };
    let first = std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&first).contains("violet"));
    assert_eq!(report.snapshot().state, ToolConversationState::Pending);
    assert_eq!(gate.released.load(Ordering::SeqCst), 0);
    drop(body);
    assert_eq!(
        usage.resolve().await,
        Err(UsageResolutionError::Interrupted)
    );
    assert_eq!(report.snapshot().state, ToolConversationState::Failed);
    assert_eq!(report.snapshot().sent_rounds, 1);
    assert_eq!(gate.released.load(Ordering::SeqCst), 1);
    finish.notify_one();
    upstream.await.unwrap();
}

#[tokio::test]
async fn repeated_unavailable_tool_fails_without_rotating_or_losing_first_round_usage() {
    let server = fixture(Protocol::OpenAiChat, true).await;
    let gate = Arc::new(Permits::default());
    let machine = RelayStateMachine::new(vec![
        candidate(
            &server,
            Protocol::OpenAiChat,
            gate.clone(),
            Credential::public(),
        ),
        candidate(
            &server,
            Protocol::OpenAiChat,
            gate.clone(),
            Credential::api_key("paid-fixture").unwrap(),
        ),
    ])
    .unwrap();
    let (result, report) = relay_tool_conversation(
        &machine,
        plan(false, Protocol::OpenAiChat),
        Protocol::OpenAiChat,
        "client",
    )
    .await;
    assert!(result.is_err());
    let snapshot = report.snapshot();
    assert_eq!(snapshot.state, ToolConversationState::Failed);
    assert_eq!(
        snapshot.observed_usage[0].unwrap().output_tokens().get(),
        10
    );
    assert!(snapshot.report.successful_candidate_index().is_none());
    assert_eq!(snapshot.report.failures().len(), 1);
    assert_eq!(gate.acquired.load(Ordering::SeqCst), 1);
    assert_eq!(gate.released.load(Ordering::SeqCst), 1);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn foreign_adaptor_and_candidate_body_override_are_rejected_before_send() {
    let server = fixture(Protocol::OpenAiChat, false).await;
    let gate = Arc::new(Permits::default());
    let mut candidate = candidate(
        &server,
        Protocol::OpenAiChat,
        gate.clone(),
        Credential::public(),
    );
    candidate = candidate.with_request(
        RelayCandidateRequest::new("different-model", Some(Bytes::from_static(b"{}"))).unwrap(),
    );
    let machine = RelayStateMachine::new(vec![candidate]).unwrap();
    let (result, _) = relay_tool_conversation(
        &machine,
        plan(false, Protocol::OpenAiChat),
        Protocol::OpenAiChat,
        "client",
    )
    .await;
    assert!(result.is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
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
        RelayContext::new(client),
        Credential::api_key("fixture").unwrap(),
    )])
    .unwrap();
    let (result, _) = relay_tool_conversation(
        &machine,
        plan(false, Protocol::OpenAiChat),
        Protocol::OpenAiChat,
        "client",
    )
    .await;
    assert!(result.is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn caller_tool_schema_history_and_returned_name_survive_collision() {
    for stream in [false, true] {
        let server = MockServer::start().await;
        let mut canonical = request(stream).into_canonical();
        let mut caller = tools().remove(0);
        caller.description = Some("Caller-owned tool".to_owned());
        caller.input_schema = json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"],"additionalProperties":false});
        caller.strict = Some(true);
        canonical.tools.push(caller.clone());
        canonical.messages.push(Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "old_call".to_owned(),
                name: "read".to_owned(),
                input: json!({"url":"https://example.com"}),
                signature: None,
            }],
        ));
        canonical.messages.push(Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult {
                tool_use_id: "old_call".to_owned(),
                content: vec![ContentBlock::Text("old result".to_owned())],
                structured_content: None,
                is_error: false,
            }],
        ));
        let plan = ToolConversationPlan::prepare(
            canonical.into(),
            Protocol::OpenAiChat,
            tools(),
            TokenCount::new(4096).unwrap(),
            Duration::from_secs(3),
        )
        .unwrap();
        let renamed = plan
            .request()
            .canonical()
            .tools
            .iter()
            .find(|tool| tool.description == caller.description)
            .unwrap();
        assert_eq!(renamed.input_schema, caller.input_schema);
        assert_eq!(renamed.strict, caller.strict);
        let alias = renamed.name.clone();
        let mut output = events(true, false);
        output[1] = Event::ToolCallStart {
            choice_index: 0,
            tool_index: 0,
            id: "caller_call".to_owned(),
            name: alias.clone(),
        };
        output[2] = Event::ToolCallArgsDelta {
            choice_index: 0,
            tool_index: 0,
            partial_json: json!({"url":"https://example.com/new"}).to_string(),
        };
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_bytes(wire(Protocol::OpenAiChat, output, usage(40, 10))),
            )
            .expect(1)
            .mount(&server)
            .await;
        let gate = Arc::new(Permits::default());
        let machine = RelayStateMachine::new(vec![candidate(
            &server,
            Protocol::OpenAiChat,
            gate,
            Credential::public(),
        )])
        .unwrap();
        let (result, report) =
            relay_tool_conversation(&machine, plan, Protocol::OpenAiChat, "caller-tool").await;
        let mut bytes = Vec::new();
        match result.unwrap() {
            ChatResponse::Full { body, .. } => bytes.extend(body),
            ChatResponse::Stream { mut body, usage } => {
                while let Some(chunk) =
                    std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx)).await
                {
                    bytes.extend(chunk.unwrap());
                }
                usage.resolve().await.unwrap();
            }
        }
        let returned = String::from_utf8(bytes).unwrap();
        assert!(returned.contains("read") && returned.contains("caller_call"));
        assert!(!returned.contains(&alias));
        assert_eq!(report.snapshot().sent_rounds, 1);
        let requests = server.received_requests().await.unwrap();
        let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            sent["tools"][0]["function"]["parameters"],
            caller.input_schema
        );
        assert_eq!(
            sent["messages"][1]["tool_calls"][0]["function"]["name"],
            alias
        );
        assert_eq!(sent["messages"][2]["tool_call_id"], "old_call");
    }
}

#[tokio::test]
async fn hidden_tool_after_visible_output_ends_the_stream_as_failure() {
    let server = MockServer::start().await;
    let mut output = events(false, false);
    output.pop();
    output.extend(events(true, false).into_iter().skip(1));
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_bytes(wire(Protocol::OpenAiChat, output, usage(40, 10))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let gate = Arc::new(Permits::default());
    let machine = RelayStateMachine::new(vec![candidate(
        &server,
        Protocol::OpenAiChat,
        gate.clone(),
        Credential::public(),
    )])
    .unwrap();
    let (result, report) = relay_tool_conversation(
        &machine,
        plan(true, Protocol::OpenAiChat),
        Protocol::OpenAiChat,
        "mixed-output",
    )
    .await;
    let ChatResponse::Stream { mut body, usage } = result.unwrap() else {
        panic!("stream expected")
    };
    let first = std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&first).contains("violet"));
    assert!(
        std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(
        usage.resolve().await,
        Err(UsageResolutionError::Interrupted)
    );
    assert_eq!(report.snapshot().state, ToolConversationState::Failed);
    assert_eq!(report.snapshot().sent_rounds, 1);
    assert_eq!(gate.released.load(Ordering::SeqCst), 1);
}
