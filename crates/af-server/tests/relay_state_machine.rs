use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

use af_adapter::{
    Adaptor, AdaptorError, AdaptorResult, Bytes, ChannelType, Credential, HeaderMap, Method,
    OpenAiAdaptor, Operation, PooledClient, Protocol, RelayContext, ResponseMode,
};
use af_domain::{ChannelId, UpstreamError, UpstreamServerStatus};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_relay::{
    RelayBuildError, RelayCandidate, RelayError, RelayRequest, RelayState, RelayStateMachine,
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
}

struct RejectingAdaptor {
    stage: RejectionStage,
}

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

    fn build_url(&self, context: &RelayContext, operation: Operation) -> AdaptorResult<String> {
        match self.stage {
            RejectionStage::Credential => {
                context.append_path(self.default_base_url(), "v1/chat/completions")
            }
            RejectionStage::Url => Err(AdaptorError::UnsupportedOperation { operation }),
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
            RejectionStage::Url => Ok(()),
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
        UpstreamError::RateLimited
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
async fn credential_and_url_failures_fall_through_to_next_candidate() {
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
    let machine = RelayStateMachine::new(vec![
        rejecting_credential,
        rejecting_url,
        candidate(&client, "http://fallback.invalid", &[TEST_MODEL]),
    ])
    .unwrap();

    let response = machine.execute(request()).await.unwrap();
    assert_eq!(response.attempts().get(), 3);
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
    ] {
        assert!(!rendered.contains(secret), "Debug 泄露了 {secret}");
    }
}
