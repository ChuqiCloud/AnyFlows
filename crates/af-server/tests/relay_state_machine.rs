use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use af_adapter::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, AnthropicAdaptor, BuiltInClientSimulation,
    Bytes, ChannelType, Credential, HeaderMap, Method, OpenAiAdaptor, Operation, PooledClient,
    Protocol, RelayContext, ResponseMode, UpstreamRequest,
};
use af_domain::{
    ChannelId, ClientSimulationProfile, ClientSimulationResult, UpstreamError, UpstreamServerStatus,
};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_relay::{
    RelayAttemptGate, RelayAttemptGateError, RelayAttemptGateFuture, RelayAttemptPermit,
    RelayAttemptReleaseFuture, RelayBuildError, RelayCandidate, RelayCandidateRequest, RelayError,
    RelayRequest, RelayState, RelayStateMachine,
};

const TEST_MODEL: &str = "test-model";
const TEST_REQUEST_BODY: &[u8] = br#"{"model":"test-model"}"#;
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

struct ProxyResponse {
    status_line: &'static str,
    body: &'static [u8],
}

#[derive(Clone, Copy)]
enum RejectionStage {
    Credential,
    Url,
    Finalize,
}

struct RejectingAdaptor {
    stage: RejectionStage,
}

#[derive(Clone, Copy)]
enum GateMode {
    Permit,
    Limited,
    Internal,
}

struct FixedAttemptGate {
    mode: GateMode,
    acquired: Arc<AtomicUsize>,
    released: Arc<AtomicUsize>,
}

impl RelayAttemptGate for FixedAttemptGate {
    fn acquire(&self) -> RelayAttemptGateFuture<'_> {
        self.acquired.fetch_add(1, Ordering::AcqRel);
        let mode = self.mode;
        let released = Arc::clone(&self.released);
        Box::pin(async move {
            match mode {
                GateMode::Permit => {
                    Ok(Box::new(RecordingAttemptPermit { released })
                        as Box<dyn RelayAttemptPermit>)
                }
                GateMode::Limited => Err(RelayAttemptGateError::Limited),
                GateMode::Internal => Err(RelayAttemptGateError::Internal),
            }
        })
    }
}

struct RecordingAttemptPermit {
    released: Arc<AtomicUsize>,
}

impl RelayAttemptPermit for RecordingAttemptPermit {
    fn release(self: Box<Self>) -> RelayAttemptReleaseFuture {
        Box::pin(async move {
            self.released.fetch_add(1, Ordering::AcqRel);
        })
    }
}

fn attempt_gate(mode: GateMode) -> (Arc<FixedAttemptGate>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let acquired = Arc::new(AtomicUsize::new(0));
    let released = Arc::new(AtomicUsize::new(0));
    (
        Arc::new(FixedAttemptGate {
            mode,
            acquired: Arc::clone(&acquired),
            released: Arc::clone(&released),
        }),
        acquired,
        released,
    )
}

#[async_trait::async_trait]
impl Adaptor for RejectingAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::OpenAi
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn default_base_url(&self) -> &str {
        "https://rejecting.invalid"
    }

    fn supported_models(&self) -> Vec<String> {
        vec![TEST_MODEL.to_owned()]
    }

    fn build_url(
        &self,
        context: &RelayContext,
        target: AdaptorTarget<'_>,
    ) -> AdaptorResult<String> {
        match self.stage {
            RejectionStage::Credential | RejectionStage::Finalize => {
                context.append_path(self.default_base_url(), "v1/chat/completions")
            }
            RejectionStage::Url => Err(AdaptorError::UnsupportedOperation {
                operation: target.operation(),
            }),
        }
    }

    fn setup_headers(
        &self,
        _headers: &mut HeaderMap,
        _credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<()> {
        match self.stage {
            RejectionStage::Credential => Err(AdaptorError::InvalidHeader),
            RejectionStage::Url | RejectionStage::Finalize => Ok(()),
        }
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        _credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        match self.stage {
            RejectionStage::Finalize => Err(AdaptorError::RequestSigning),
            RejectionStage::Credential | RejectionStage::Url => Ok(request),
        }
    }
}

fn direct_client() -> PooledClient {
    HttpClientPool::default()
        .get(&HttpClientConfig::default())
        .expect("测试 HTTP Client 必须可创建")
}

fn proxy_client(address: SocketAddr) -> PooledClient {
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        HttpTimeouts::default(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    HttpClientPool::default()
        .get(&config)
        .expect("测试代理 Client 必须可创建")
}

fn candidate(client: &PooledClient, base_url: &str, models: &[&str]) -> RelayCandidate {
    RelayCandidate::new(
        Arc::new(OpenAiAdaptor::with_supported_models(models.iter().copied())),
        RelayContext::new(client.clone())
            .with_base_url(base_url)
            .expect("测试基础地址必须有效"),
        Credential::api_key("credential-secret").unwrap(),
    )
}

fn grouped_candidate(client: &PooledClient, base_url: &str, channel_id: i64) -> RelayCandidate {
    candidate(client, base_url, &[TEST_MODEL])
        .with_channel_group(ChannelId::new(channel_id).unwrap())
}

fn request() -> RelayRequest {
    RelayRequest::new(
        TEST_MODEL,
        Operation::Chat,
        Method::POST,
        Some(Bytes::from_static(TEST_REQUEST_BODY)),
    )
    .unwrap()
}

fn spawn_proxy(responses: Vec<ProxyResponse>) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with(b"POST http://"));
            let header = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.status_line,
                response.body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(response.body).unwrap();
            stream.flush().unwrap();
        }
    });
    (address, server)
}

fn spawn_capturing_proxy(
    response: ProxyResponse,
) -> (
    SocketAddr,
    std::sync::mpsc::Receiver<Vec<u8>>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
        sender.send(read_request(&mut stream)).unwrap();
        let header = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.status_line,
            response.body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(response.body).unwrap();
        stream.flush().unwrap();
    });
    (address, receiver, server)
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 512];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在请求头结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 16 * 1_024, "测试请求超过 16 KiB");
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() - header_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在请求体结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
    }
    request
}

#[test]
fn rejects_empty_and_oversized_candidate_lists() {
    assert_eq!(
        RelayStateMachine::new(Vec::new()).unwrap_err(),
        RelayBuildError::NoCandidates
    );

    let client = direct_client();
    let candidates = (0..=RelayStateMachine::MAX_CANDIDATES)
        .map(|_| candidate(&client, "https://candidate.invalid", &[TEST_MODEL]))
        .collect();
    assert_eq!(
        RelayStateMachine::new(candidates).unwrap_err(),
        RelayBuildError::TooManyCandidates
    );

    let candidates = vec![
        grouped_candidate(&client, "https://first.invalid", 1),
        grouped_candidate(&client, "https://second.invalid", 2),
        grouped_candidate(&client, "https://third.invalid", 1),
    ];
    assert_eq!(
        RelayStateMachine::new(candidates).unwrap_err(),
        RelayBuildError::NonContiguousChannelGroup
    );
}

#[test]
fn rejects_invalid_model_before_execution() {
    assert_eq!(
        RelayRequest::new(" model", Operation::Chat, Method::POST, None).unwrap_err(),
        RelayError::InvalidModel
    );
}

#[tokio::test]
async fn unsupported_candidate_is_skipped_before_sending() {
    let (address, server) = spawn_proxy(vec![ProxyResponse {
        status_line: "200 OK",
        body: br#"{"ok":"second"}"#,
    }]);
    let client = proxy_client(address);
    let machine = RelayStateMachine::new(vec![
        candidate(&client, "http://first.invalid", &["other-model"]),
        candidate(&client, "http://second.invalid", &[TEST_MODEL]),
    ])
    .unwrap();

    let response = machine.execute(request()).await.unwrap();
    assert_eq!(response.attempts().get(), 2);
    assert_eq!(
        response
            .into_response()
            .into_body()
            .into_bytes()
            .await
            .unwrap(),
        Bytes::from_static(br#"{"ok":"second"}"#)
    );
    server.join().unwrap();
}

#[tokio::test]
async fn limited_candidate_gate_skips_to_the_next_candidate_before_network_io() {
    let (address, server) = spawn_proxy(vec![ProxyResponse {
        status_line: "200 OK",
        body: br#"{"ok":"fallback"}"#,
    }]);
    let client = proxy_client(address);
    let (gate, acquired, released) = attempt_gate(GateMode::Limited);
    let machine = RelayStateMachine::new(vec![
        candidate(&client, "http://limited.invalid", &[TEST_MODEL]).with_attempt_gate(gate),
        candidate(&client, "http://fallback.invalid", &[TEST_MODEL]),
    ])
    .unwrap();

    let response = machine.execute_with_report(request()).await.unwrap();
    assert_eq!(response.report().successful_candidate_index(), Some(1));
    assert_eq!(acquired.load(Ordering::Acquire), 1);
    assert_eq!(released.load(Ordering::Acquire), 0);
    server.join().unwrap();
}

#[tokio::test]
async fn gate_failures_keep_limited_and_internal_results_distinct() {
    let client = direct_client();
    let (limited, _, _) = attempt_gate(GateMode::Limited);
    let limited_machine = RelayStateMachine::new(vec![
        candidate(&client, "https://limited.invalid", &[TEST_MODEL]).with_attempt_gate(limited),
    ])
    .unwrap();
    assert_eq!(
        limited_machine.execute(request()).await.unwrap_err(),
        RelayError::ConcurrencyUnavailable
    );

    let (internal, _, _) = attempt_gate(GateMode::Internal);
    let internal_machine = RelayStateMachine::new(vec![
        candidate(&client, "https://internal.invalid", &[TEST_MODEL]).with_attempt_gate(internal),
    ])
    .unwrap();
    assert_eq!(
        internal_machine.execute(request()).await.unwrap_err(),
        RelayError::AttemptGateFailed
    );
}

#[tokio::test]
async fn attempt_permit_is_released_on_retry_and_returned_on_success() {
    let (address, server) = spawn_proxy(vec![
        ProxyResponse {
            status_line: "503 Service Unavailable",
            body: b"{}",
        },
        ProxyResponse {
            status_line: "200 OK",
            body: br#"{"ok":true}"#,
        },
    ]);
    let client = proxy_client(address);
    let (failed_gate, _, failed_released) = attempt_gate(GateMode::Permit);
    let (success_gate, _, success_released) = attempt_gate(GateMode::Permit);
    let machine = RelayStateMachine::new(vec![
        candidate(&client, "http://failed.invalid", &[TEST_MODEL]).with_attempt_gate(failed_gate),
        candidate(&client, "http://success.invalid", &[TEST_MODEL]).with_attempt_gate(success_gate),
    ])
    .unwrap();

    let response = machine.execute_with_report(request()).await.unwrap();
    assert_eq!(failed_released.load(Ordering::Acquire), 1);
    assert_eq!(success_released.load(Ordering::Acquire), 0);
    let (_, _, permit) = response.into_parts_with_permit();
    permit.expect("成功候选必须返回许可").release().await;
    assert_eq!(success_released.load(Ordering::Acquire), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn candidate_request_controls_model_filter_url_and_body_together() {
    let (address, captured, server) = spawn_capturing_proxy(ProxyResponse {
        status_line: "200 OK",
        body: br#"{"ok":true}"#,
    });
    let client = proxy_client(address);
    let mapped_body = Bytes::from_static(br#"{"model":"mapped-model","temperature":0.25}"#);
    let mapped_request =
        RelayCandidateRequest::new("mapped-model", Some(mapped_body.clone())).unwrap();
    let mapped_candidate =
        candidate(&client, "http://mapped.invalid", &["mapped-model"]).with_request(mapped_request);
    let machine = RelayStateMachine::new(vec![mapped_candidate]).unwrap();

    machine.execute(request()).await.unwrap();

    let request = captured.recv_timeout(TEST_TIMEOUT).unwrap();
    assert!(request.ends_with(mapped_body.as_ref()));
    assert!(!request.ends_with(TEST_REQUEST_BODY));
    server.join().unwrap();
}

#[tokio::test]
async fn applied_client_simulation_adds_identity_headers_and_report_metadata() {
    let (address, captured, server) = spawn_capturing_proxy(ProxyResponse {
        status_line: "200 OK",
        body: br#"{"ok":true}"#,
    });
    let client = proxy_client(address);
    let candidate = RelayCandidate::new(
        Arc::new(AnthropicAdaptor::with_supported_models([TEST_MODEL])),
        RelayContext::new(client)
            .with_base_url("http://anthropic.invalid")
            .unwrap(),
        Credential::oauth("oauth-secret").unwrap(),
    )
    .with_client_simulation(Arc::new(BuiltInClientSimulation::new(
        ClientSimulationProfile::AnthropicCliHeadersV1,
    )));
    let machine = RelayStateMachine::new(vec![candidate]).unwrap();

    let response = machine.execute_with_report(request()).await.unwrap();
    let simulation = response
        .report()
        .successful_client_simulation()
        .expect("成功 Attempt 必须保留仿真元数据");
    assert_eq!(
        simulation.profile(),
        ClientSimulationProfile::AnthropicCliHeadersV1
    );
    assert_eq!(simulation.result(), ClientSimulationResult::Applied);
    let (_, report) = response.into_parts();
    assert_eq!(
        report.successful_client_simulation().unwrap().result(),
        ClientSimulationResult::Applied
    );

    let upstream_request = captured.recv_timeout(TEST_TIMEOUT).unwrap();
    let headers = String::from_utf8_lossy(&upstream_request);
    assert!(headers.contains("user-agent: claude-cli/2.1.114 (external, sdk-cli)"));
    assert!(headers.contains("x-app: cli"));
    assert!(headers.contains("authorization: Bearer oauth-secret"));
    server.join().unwrap();
}

#[tokio::test]
async fn failed_client_simulation_is_protocol_error_without_fallback() {
    let client = direct_client();
    let simulated_candidate = candidate(&client, "https://first.invalid", &[TEST_MODEL])
        .with_client_simulation(Arc::new(BuiltInClientSimulation::new(
            ClientSimulationProfile::AnthropicCliHeadersV1,
        )));
    let machine = RelayStateMachine::new(vec![
        simulated_candidate,
        candidate(&client, "https://fallback.invalid", &["other-model"]),
    ])
    .unwrap();

    let error = machine.execute_with_report(request()).await.unwrap_err();
    assert_eq!(
        error.error(),
        RelayError::Upstream(UpstreamError::ProtocolError)
    );
    assert_eq!(error.report().failures().len(), 1);
    let simulation = error.report().failures()[0]
        .client_simulation()
        .expect("失败 Attempt 必须保留仿真元数据");
    assert_eq!(
        simulation.profile(),
        ClientSimulationProfile::AnthropicCliHeadersV1
    );
    assert_eq!(simulation.result(), ClientSimulationResult::Failed);
}

#[tokio::test]
async fn retries_rate_limit_on_the_next_candidate() {
    let (address, server) = spawn_proxy(vec![
        ProxyResponse {
            status_line: "429 Too Many Requests",
            body: br#"{"error":{"code":"rate_limit_exceeded"}}"#,
        },
        ProxyResponse {
            status_line: "200 OK",
            body: br#"{"ok":"fallback"}"#,
        },
    ]);
    let client = proxy_client(address);
    let machine = RelayStateMachine::new(vec![
        grouped_candidate(&client, "http://first.invalid", 1),
        grouped_candidate(&client, "http://second.invalid", 1),
    ])
    .unwrap();

    let response = machine.execute_with_report(request()).await.unwrap();
    assert_eq!(response.attempts().get(), 2);
    assert_eq!(response.state(), RelayState::Completed);
    assert_eq!(response.report().successful_candidate_index(), Some(1));
    assert_eq!(response.report().failures().len(), 1);
    assert_eq!(response.report().failures()[0].candidate_index(), 0);
    assert_eq!(
        response.report().failures()[0].error(),
        UpstreamError::rate_limited(af_domain::RateLimitScope::Window)
    );
    assert_eq!(
        response
            .into_response()
            .into_body()
            .into_bytes()
            .await
            .unwrap(),
        Bytes::from_static(br#"{"ok":"fallback"}"#)
    );
    server.join().unwrap();
}

#[tokio::test]
async fn channel_failure_skips_remaining_credentials_in_the_same_channel() {
    let (address, server) = spawn_proxy(vec![
        ProxyResponse {
            status_line: "503 Service Unavailable",
            body: b"{}",
        },
        ProxyResponse {
            status_line: "200 OK",
            body: br#"{"ok":"next-channel"}"#,
        },
    ]);
    let client = proxy_client(address);
    let machine = RelayStateMachine::new(vec![
        grouped_candidate(&client, "http://first-key.invalid", 1),
        grouped_candidate(&client, "http://second-key.invalid", 1),
        grouped_candidate(&client, "http://next-channel.invalid", 2),
    ])
    .unwrap();

    let response = machine.execute_with_report(request()).await.unwrap();
    assert_eq!(response.attempts().get(), 3);
    assert_eq!(response.report().successful_candidate_index(), Some(2));
    assert_eq!(response.report().failures().len(), 1);
    assert_eq!(response.report().failures()[0].candidate_index(), 0);
    assert_eq!(
        response.report().failures()[0].error(),
        UpstreamError::ServerError {
            status: UpstreamServerStatus::new(503).unwrap(),
        }
    );
    assert_eq!(
        response
            .into_response()
            .into_body()
            .into_bytes()
            .await
            .unwrap(),
        Bytes::from_static(br#"{"ok":"next-channel"}"#)
    );
    server.join().unwrap();
}

#[tokio::test]
async fn does_not_retry_client_bad_request() {
    let (address, server) = spawn_proxy(vec![ProxyResponse {
        status_line: "400 Bad Request",
        body: b"{}",
    }]);
    let client = proxy_client(address);
    let machine = RelayStateMachine::new(vec![
        candidate(&client, "http://first.invalid", &[TEST_MODEL]),
        candidate(&client, "http://second.invalid", &[TEST_MODEL]),
    ])
    .unwrap();

    assert_eq!(
        machine.execute(request()).await.unwrap_err(),
        RelayError::Upstream(UpstreamError::BadRequest)
    );
    server.join().unwrap();
}

#[tokio::test]
async fn credential_url_and_finalization_failures_fall_through_to_next_candidate() {
    let (address, server) = spawn_proxy(vec![ProxyResponse {
        status_line: "200 OK",
        body: br#"{"ok":true}"#,
    }]);
    let client = proxy_client(address);
    let rejecting_credential = RelayCandidate::new(
        Arc::new(RejectingAdaptor {
            stage: RejectionStage::Credential,
        }),
        RelayContext::new(client.clone())
            .with_base_url("http://credential.invalid")
            .unwrap(),
        Credential::api_key("credential-secret").unwrap(),
    );
    let rejecting_url = RelayCandidate::new(
        Arc::new(RejectingAdaptor {
            stage: RejectionStage::Url,
        }),
        RelayContext::new(client.clone())
            .with_base_url("http://url.invalid")
            .unwrap(),
        Credential::api_key("credential-secret").unwrap(),
    );
    let rejecting_finalization = RelayCandidate::new(
        Arc::new(RejectingAdaptor {
            stage: RejectionStage::Finalize,
        }),
        RelayContext::new(client.clone())
            .with_base_url("http://finalization.invalid")
            .unwrap(),
        Credential::api_key("credential-secret").unwrap(),
    );
    let machine = RelayStateMachine::new(vec![
        rejecting_credential,
        rejecting_url,
        rejecting_finalization,
        candidate(&client, "http://fallback.invalid", &[TEST_MODEL]),
    ])
    .unwrap();

    let response = machine.execute(request()).await.unwrap();
    assert_eq!(response.attempts().get(), 4);
    server.join().unwrap();
}

#[tokio::test]
async fn stream_success_preserves_streaming_state_and_body() {
    let (address, server) = spawn_proxy(vec![ProxyResponse {
        status_line: "200 OK",
        body: b"stream-body",
    }]);
    let client = proxy_client(address);
    let machine = RelayStateMachine::new(vec![candidate(
        &client,
        "http://stream.invalid",
        &[TEST_MODEL],
    )])
    .unwrap();

    let response = machine
        .execute(request().with_response_mode(ResponseMode::Stream))
        .await
        .unwrap();
    assert_eq!(response.state(), RelayState::Streaming);
    let body = response.into_response().into_body();
    assert!(body.is_streaming());
    assert_eq!(
        body.into_bytes().await.unwrap(),
        Bytes::from_static(b"stream-body")
    );
    server.join().unwrap();
}

#[test]
fn debug_output_redacts_model_credential_url_and_body() {
    let client = direct_client();
    let candidate = candidate(
        &client,
        "https://candidate-secret.invalid",
        &["model-secret"],
    )
    .with_request(
        RelayCandidateRequest::new(
            "mapped-model-secret",
            Some(Bytes::from_static(b"mapped-body-secret")),
        )
        .unwrap(),
    );
    let candidate_debug = format!("{candidate:?}");
    let request = RelayRequest::new(
        "model-secret",
        Operation::Chat,
        Method::POST,
        Some(Bytes::from_static(b"body-secret")),
    )
    .unwrap();
    let machine = RelayStateMachine::new(vec![candidate]).unwrap();
    let rendered = format!("{machine:?}\n{request:?}\n{candidate_debug}");
    for secret in [
        "model-secret",
        "credential-secret",
        "candidate-secret",
        "body-secret",
        "mapped-model-secret",
        "mapped-body-secret",
    ] {
        assert!(!rendered.contains(secret), "Debug 泄露了 {secret}");
    }
}
