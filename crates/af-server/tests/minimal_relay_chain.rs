pub mod support;

use std::{
    net::{SocketAddr, TcpListener},
    sync::Arc,
    time::Duration,
};

use af_admin::{ApiKeyDigest, TokenAuthenticationFuture, TokenAuthenticator};
use af_config::ServerConfig;
use af_domain::{GatewayPrincipal, GroupId, TokenId, TrustedClientIp, UserId};
use af_http::{
    HttpListener, QueryApiKeyPolicy, ReadinessFuture, ReadinessHandle, ReadinessProbe,
    ServeOutcome, build_router, serve_with_graceful_shutdown,
};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_relay::RelayService;
use serde_json::Value;
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, TOKEN_ID, UPSTREAM_BASE_URL, UPSTREAM_KEY, UPSTREAM_MODEL,
    USER_ID, send_request, spawn_proxy,
};
use tokio::{sync::oneshot, time::timeout};

struct AlwaysReadyProbe;

impl ReadinessProbe for AlwaysReadyProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async { true })
    }
}

struct AlwaysAuthenticates;

impl TokenAuthenticator for AlwaysAuthenticates {
    fn authenticate<'a>(
        &'a self,
        _digest: &'a ApiKeyDigest,
        _client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        Box::pin(async {
            Ok(GatewayPrincipal::new(
                TokenId::new(TOKEN_ID).unwrap(),
                UserId::new(USER_ID).unwrap(),
                GroupId::new(GROUP_ID).unwrap(),
            ))
        })
    }
}

#[tokio::test]
async fn invalid_requests_and_wrong_model_never_reach_upstream() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let (address, shutdown, server) =
        start_server(proxy.local_addr().unwrap(), ServerConfig::default()).await;

    let authorization = format!("Bearer {CLIENT_KEY}");
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
            &[
                ("Content-Type", "application/json"),
                ("Authorization", &authorization),
            ],
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
        let authorization = format!("Bearer {CLIENT_KEY}");
        let response = send_request(
            address,
            "POST",
            "/v1/chat/completions",
            br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}]}"#,
            &[
                ("Content-Type", "application/json"),
                ("Authorization", &authorization),
            ],
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
    let readiness = ReadinessHandle::new(AlwaysReadyProbe);
    readiness.mark_ready();
    let router = build_router(
        &server_config,
        relay,
        readiness,
        Arc::new(AlwaysAuthenticates),
        QueryApiKeyPolicy::Deny,
    );
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
