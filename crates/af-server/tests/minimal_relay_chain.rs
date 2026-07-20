use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream as StdTcpStream},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use af_config::ServerConfig;
use af_http::{
    HttpListener, REQUEST_ID_HEADER_NAME, ServeOutcome, build_router, serve_with_graceful_shutdown,
};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_relay::RelayService;
use af_server::{Bootstrap, SupervisorShutdown};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

const IO_TIMEOUT: Duration = Duration::from_secs(5);
const UPSTREAM_BASE_URL: &str = "http://upstream.example/proxy";
const UPSTREAM_MODEL: &str = "test-model";
const UPSTREAM_KEY: &str = "configured-upstream-key";

#[derive(Debug)]
struct CapturedRequest {
    head: String,
    body: Vec<u8>,
}

#[derive(Debug)]
struct RawResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[tokio::test]
async fn full_chain_forwards_normalized_request_and_returns_validated_response() {
    let upstream_body = serde_json::to_vec(&json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": UPSTREAM_MODEL,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "hello" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    }))
    .unwrap();
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        &upstream_body,
        &[
            ("Content-Type", "application/json"),
            ("Set-Cookie", "upstream-cookie-secret=1"),
            ("Server", "private-upstream"),
            ("X-Request-ID", "upstream-selected-request-id"),
        ],
    );
    let config_path = std::env::temp_dir().join(format!(
        "anyflows-minimal-relay-chain-{}.toml",
        std::process::id()
    ));
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\ncors_allowed_origins = ['https://console.example']\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = '{UPSTREAM_BASE_URL}'\nmodel = '{UPSTREAM_MODEL}'\napi_key = '{UPSTREAM_KEY}'\n[http_client]\nconnect_timeout_secs = 5\nread_timeout_secs = 5\nrequest_timeout_secs = 5\nproxy_url = 'http://{proxy_address}'\ntrust_proxy_dns = true\n"
        ),
    )
    .unwrap();
    let config = af_config::load_from(&config_path).unwrap();
    let _ = fs::remove_file(config_path);
    let bootstrap = Bootstrap::new(config)
        .unwrap()
        .init_telemetry()
        .unwrap()
        .connect_database()
        .await
        .unwrap()
        .init_relay()
        .unwrap()
        .start_supervisor()
        .build_router()
        .bind()
        .unwrap();
    let address = bootstrap.local_addr();
    let (shutdown, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(bootstrap.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let inbound = br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}]}"#;
    let response = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer client-key-secret"),
            ("Cookie", "client-cookie-secret=1"),
            ("X-API-Key", "client-api-key-secret"),
            ("X-Request-ID", "client-selected-request-id"),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.body, upstream_body);
    assert_eq!(response.headers["content-type"], "application/json");
    assert_eq!(
        response.headers["access-control-allow-origin"],
        "https://console.example"
    );
    let request_id = &response.headers[REQUEST_ID_HEADER_NAME];
    assert_ne!(request_id, "client-selected-request-id");
    assert!(!response.headers.contains_key("set-cookie"));
    assert_ne!(
        response.headers.get("server").map(String::as_str),
        Some("private-upstream")
    );

    let head_lower = upstream.head.to_ascii_lowercase();
    assert!(
        upstream
            .head
            .starts_with("POST http://upstream.example/proxy/v1/chat/completions HTTP/1.1")
    );
    assert!(head_lower.contains("authorization: bearer configured-upstream-key"));
    assert!(head_lower.contains(&format!("x-request-id: {request_id}").to_ascii_lowercase()));
    for forbidden in [
        "client-key-secret",
        "client-cookie-secret",
        "client-api-key-secret",
        "client-selected-request-id",
        "origin: https://console.example",
    ] {
        assert!(!head_lower.contains(forbidden));
    }
    let outbound: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(outbound["model"], UPSTREAM_MODEL);
    assert_eq!(outbound["stream"], false);
    assert_eq!(outbound["messages"][0]["content"], "hello");
}

#[tokio::test]
async fn invalid_requests_and_wrong_model_never_reach_upstream() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let (address, shutdown, server) =
        start_server(proxy.local_addr().unwrap(), ServerConfig::default()).await;

    for (method, body, expected_status, expected_code) in [
        (
            "POST",
            br#"{"model":"test-model","model":"private-duplicate","messages":[]}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"private-wrong-model","messages":[{"role":"user","content":"hello"}]}"#.as_slice(),
            404,
            "model_not_found",
        ),
        ("GET", b"".as_slice(), 405, ""),
    ] {
        let response = send_request(
            address,
            method,
            "/v1/chat/completions",
            body,
            &[("Content-Type", "application/json")],
        )
        .await;
        assert_eq!(response.status, expected_status);
        if !expected_code.is_empty() {
            let value: Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(value["error"]["code"], expected_code);
            let text = String::from_utf8_lossy(&response.body);
            assert!(!text.contains("private-duplicate"));
            assert!(!text.contains("private-wrong-model"));
        }
    }
    stop_server(shutdown, server).await;
    assert!(matches!(
        proxy.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

#[tokio::test]
async fn upstream_errors_are_classified_without_forwarding_private_bodies() {
    let cases = [
        (
            "429 Too Many Requests",
            br#"{"error":{"message":"upstream-rate-secret","type":"tokens","code":"rate_limit_exceeded"}}"#.as_slice(),
            429,
            "rate_limited",
        ),
        (
            "429 Too Many Requests",
            br#"{"error":{"message":"upstream-quota-secret","type":"insufficient_quota","code":"insufficient_quota"}}"#.as_slice(),
            503,
            "upstream_unavailable",
        ),
        (
            "500 Internal Server Error",
            br#"{"error":{"message":"upstream-server-secret"}}"#.as_slice(),
            503,
            "upstream_unavailable",
        ),
        (
            "200 OK",
            br#"{"private":"invalid-success-secret"}"#.as_slice(),
            503,
            "upstream_unavailable",
        ),
    ];

    for (status_line, upstream_body, expected_status, expected_code) in cases {
        let (proxy_address, captured, proxy) = spawn_proxy(
            status_line,
            upstream_body,
            &[("Content-Type", "application/json")],
        );
        let (address, shutdown, server) =
            start_server(proxy_address, ServerConfig::default()).await;
        let response = send_request(
            address,
            "POST",
            "/v1/chat/completions",
            br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}]}"#,
            &[("Content-Type", "application/json")],
        )
        .await;
        stop_server(shutdown, server).await;
        captured.recv_timeout(IO_TIMEOUT).unwrap();
        proxy.join().unwrap();

        assert_eq!(response.status, expected_status);
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["error"]["code"], expected_code);
        let response_text = String::from_utf8_lossy(&response.body);
        for secret in [
            "upstream-rate-secret",
            "upstream-quota-secret",
            "upstream-server-secret",
            "invalid-success-secret",
        ] {
            assert!(!response_text.contains(secret));
        }
    }
}

async fn start_server(
    proxy_address: SocketAddr,
    server_config: ServerConfig,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<ServeOutcome, af_http::ServeError>>,
) {
    let proxy = ProxyConfig::parse(format!("http://{proxy_address}")).unwrap();
    let client_config = HttpClientConfig::new(proxy, HttpTimeouts::default())
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    let client = HttpClientPool::default().get(&client_config).unwrap();
    let relay = Arc::new(
        RelayService::new(client, UPSTREAM_BASE_URL, UPSTREAM_MODEL, UPSTREAM_KEY).unwrap(),
    );
    let router = build_router(&server_config, relay);
    let listener = HttpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(serve_with_graceful_shutdown(
        listener,
        router,
        async move {
            let _ = shutdown_rx.await;
        },
        Duration::from_secs(1),
    ));
    (address, shutdown_tx, server)
}

async fn stop_server(
    shutdown: oneshot::Sender<()>,
    server: tokio::task::JoinHandle<Result<ServeOutcome, af_http::ServeError>>,
) {
    shutdown.send(()).unwrap();
    assert_eq!(
        timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap(),
        ServeOutcome::Drained
    );
}

async fn send_request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> RawResponse {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut stream = loop {
        match TcpStream::connect(address).await {
            Ok(stream) => break stream,
            Err(error) => {
                assert!(Instant::now() < deadline, "等待测试服务监听超时: {error}");
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    };
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();

    let mut bytes = Vec::new();
    timeout(IO_TIMEOUT, stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    parse_response(bytes)
}

fn parse_response(bytes: Vec<u8>) -> RawResponse {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap();
    let head = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut lines = head.lines();
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    RawResponse {
        status,
        headers,
        body: bytes[header_end..].to_vec(),
    }
}

fn spawn_proxy(
    status_line: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let status_line = status_line.to_owned();
    let body = body.to_vec();
    let headers = headers
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect::<Vec<_>>();
    let (captured_tx, captured_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + IO_TIMEOUT;
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "本地测试代理等待连接超时");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("本地测试代理接受连接失败: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let captured = read_proxy_request(&mut stream);
        captured_tx.send(captured).unwrap();

        let mut response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(&name);
            response.push_str(": ");
            response.push_str(&value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        stream.flush().unwrap();
    });
    (address, captured_rx, handle)
}

fn read_proxy_request(stream: &mut StdTcpStream) -> CapturedRequest {
    let mut request = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 512];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "代理请求头尚未结束时连接已关闭");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 64 * 1024, "代理测试请求头超过限制");
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let head = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() < header_end + content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "代理请求体尚未结束时连接已关闭");
        request.extend_from_slice(&buffer[..read]);
    }
    CapturedRequest {
        head,
        body: request[header_end..header_end + content_length].to_vec(),
    }
}
