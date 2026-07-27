pub mod support;

use std::{
    net::{SocketAddr, TcpListener},
    sync::Arc,
    time::Duration,
};

use af_admin::{
    AdminChannelCreateCommand, AdminChannelCreateFuture, AdminChannelDeleteFuture,
    AdminChannelGetFuture, AdminChannelListFuture, AdminChannelListQuery, AdminChannelReadError,
    AdminChannelReader, AdminChannelUpdateCommand, AdminChannelUpdateFuture,
    AdminChannelWriteError, AdminChannelWriter, AdminCredentialCreateCommand,
    AdminCredentialCreateFuture, AdminCredentialDeleteFuture, AdminCredentialGetFuture,
    AdminCredentialListFuture, AdminCredentialListQuery, AdminCredentialUpdateCommand,
    AdminCredentialUpdateFuture, AdminGroupCreateCommand, AdminGroupCreateFuture,
    AdminGroupDeleteFuture, AdminGroupGetFuture, AdminGroupListFuture, AdminGroupListQuery,
    AdminGroupReadError, AdminGroupReader, AdminGroupUpdateCommand, AdminGroupUpdateFuture,
    AdminGroupWriteError, AdminGroupWriter, AdminTokenCreateCommand, AdminTokenCreateFuture,
    AdminTokenDeleteFuture, AdminTokenGetFuture, AdminTokenListFuture, AdminTokenListQuery,
    AdminTokenReadError, AdminTokenReader, AdminTokenUpdateCommand, AdminTokenUpdateFuture,
    AdminTokenWriteError, AdminTokenWriter, AdminUsageLogListFuture, AdminUsageLogListQuery,
    AdminUsageLogReadError, AdminUsageLogReader, AdminUserCreateCommand, AdminUserCreateFuture,
    AdminUserDeleteFuture, AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery,
    AdminUserReadError, AdminUserReader, AdminUserUpdateCommand, AdminUserUpdateFuture,
    AdminUserWriteError, AdminUserWriter, ApiKeyDigest, InitialSetup, InitialSetupCommand,
    InitialSetupError, InitialSetupFuture, InitialSetupStatusFuture, LoginCredentials,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionLoginFuture, TokenAuthentication, TokenAuthenticationFuture, TokenAuthenticator,
};
use af_analytics::{
    AdminDashboardAccess, AdminDashboardReadError, AdminDashboardReadFuture, AdminDashboardReader,
};
use af_config::ServerConfig;
use af_domain::{
    ChannelId, CredentialId, GatewayPrincipal, GroupId, TokenId, TokenModelPolicy, TrustedClientIp,
    UserId,
};
use af_http::{
    HttpListener, QueryApiKeyPolicy, ReadinessFuture, ReadinessHandle, ReadinessProbe,
    ServeOutcome, build_router, serve_with_graceful_shutdown,
};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_relay::RelayService;
use serde_json::Value;
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, TOKEN_ID, UPSTREAM_BASE_URL, UPSTREAM_KEY, UPSTREAM_MODEL,
    USER_ID, send_request, spawn_abort_observing_proxy, spawn_proxy, spawn_streaming_proxy,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

struct AlwaysReadyProbe;

impl ReadinessProbe for AlwaysReadyProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async { true })
    }
}

struct AlwaysAuthenticates;

struct RejectSessions;
struct RejectInitialSetup;
struct RejectAdminChannels;
struct RejectAdminGroups;
struct RejectAdminTokens;
struct RejectAdminDashboard;
struct RejectAdminUsageLogs;
struct RejectAdminUsers;

impl SessionAuthenticator for RejectSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidSession) })
    }
}

impl InitialSetup for RejectInitialSetup {
    fn status(&self) -> InitialSetupStatusFuture<'_> {
        Box::pin(async { Err(InitialSetupError::Internal) })
    }

    fn initialize(&self, _command: InitialSetupCommand) -> InitialSetupFuture<'_> {
        Box::pin(async { Err(InitialSetupError::Internal) })
    }
}

impl AdminChannelReader for RejectAdminChannels {
    fn list_channels<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminChannelListQuery,
    ) -> AdminChannelListFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }

    fn get_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
    ) -> AdminChannelGetFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }

    fn list_credentials<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _query: AdminCredentialListQuery,
    ) -> AdminCredentialListFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }

    fn get_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
    ) -> AdminCredentialGetFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }
}

impl AdminChannelWriter for RejectAdminChannels {
    fn create_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn update_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn delete_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn create_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn update_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
        _command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn delete_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }
}

impl AdminGroupReader for RejectAdminGroups {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }
}

impl AdminGroupWriter for RejectAdminGroups {
    fn create<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _group_id: af_domain::GroupId,
        _command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _group_id: af_domain::GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }
}

impl AdminTokenReader for RejectAdminTokens {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenGetFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }
}

impl AdminTokenWriter for RejectAdminTokens {
    fn create<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminTokenCreateCommand,
    ) -> AdminTokenCreateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
        _command: AdminTokenUpdateCommand,
    ) -> AdminTokenUpdateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenDeleteFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }
}

impl AdminUsageLogReader for RejectAdminUsageLogs {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminUsageLogListQuery,
    ) -> AdminUsageLogListFuture<'a> {
        Box::pin(async { Err(AdminUsageLogReadError::Internal) })
    }
}

impl AdminDashboardReader for RejectAdminDashboard {
    fn read(&self, _access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_> {
        Box::pin(async { Err(AdminDashboardReadError::Internal) })
    }
}

impl AdminUserReader for RejectAdminUsers {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _user_id: UserId,
    ) -> AdminUserGetFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }
}

impl AdminUserWriter for RejectAdminUsers {
    fn create<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _user_id: UserId,
        _command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _user_id: UserId,
    ) -> AdminUserDeleteFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }
}

impl TokenAuthenticator for AlwaysAuthenticates {
    fn authenticate<'a>(
        &'a self,
        _digest: &'a ApiKeyDigest,
        _client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        Box::pin(async {
            Ok(TokenAuthentication::new(
                GatewayPrincipal::new(
                    TokenId::new(TOKEN_ID).unwrap(),
                    UserId::new(USER_ID).unwrap(),
                    GroupId::new(GROUP_ID).unwrap(),
                ),
                TokenModelPolicy::unrestricted(),
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
            br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_obfuscation":false}}"#.as_slice(),
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
async fn streaming_response_is_incremental_canonical_and_unbuffered() {
    let first = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{"role":"assistant","content":"first-part"},"finish_reason":null}]}

"#;
    let rest = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{"content":"second-part"},"finish_reason":null}]}

data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;
    let (proxy_address, captured, release, proxy) = spawn_streaming_proxy(first, rest);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");
    let request_body = br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}"#;
    let mut client = connect_and_send(address, request_body, &authorization).await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_lower.contains("cache-control: no-cache, no-transform"));
    assert!(first_lower.contains("x-accel-buffering: no"));
    assert!(!first_lower.contains("content-encoding:"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("[DONE]"));
    assert!(!first_text.contains("upstream-private-id"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream"], true);
    assert_eq!(upstream_json["stream_options"]["include_usage"], true);
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("\"usage\":{"));
    assert!(complete.contains("\"prompt_tokens\""));
    assert!(complete.contains("data: [DONE]"));
    assert!(complete.contains("chat.completion.chunk"));
    assert!(!complete.contains("upstream-private-id"));

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn client_disconnect_aborts_pending_upstream_stream() {
    let first = br#"data: {"id":"abort-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{"role":"assistant","content":"abort-first-part"},"finish_reason":null}]}

"#;
    let (proxy_address, captured, aborted, proxy) = spawn_abort_observing_proxy(first);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");
    let request_body =
        br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true}"#;
    let mut client = connect_and_send(address, request_body, &authorization).await;
    let response = read_until(&mut client, b"abort-first-part").await;
    let response = String::from_utf8_lossy(&response);
    assert!(!response.contains("abort-private-id"));
    assert!(!response.contains("\"usage\""));
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream_options"]["include_usage"], true);
    drop(client);

    timeout(IO_TIMEOUT, async {
        loop {
            if aborted.try_recv().is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

async fn connect_and_send(address: SocketAddr, body: &[u8], authorization: &str) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: {authorization}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    client.write_all(request.as_bytes()).await.unwrap();
    client.write_all(body).await.unwrap();
    client
}

async fn read_until(client: &mut TcpStream, needle: &[u8]) -> Vec<u8> {
    timeout(IO_TIMEOUT, async {
        let mut response = Vec::new();
        let mut buffer = [0_u8; 1_024];
        while !response
            .windows(needle.len())
            .any(|window| window == needle)
        {
            let read = client.read(&mut buffer).await.unwrap();
            assert!(read > 0, "SSE 目标内容到达前连接已关闭");
            response.extend_from_slice(&buffer[..read]);
        }
        response
    })
    .await
    .unwrap()
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
        Arc::new(RejectSessions),
        Arc::new(RejectInitialSetup),
        Arc::new(RejectAdminChannels),
        Arc::new(RejectAdminChannels),
        None,
        Arc::new(RejectAdminGroups),
        Arc::new(RejectAdminGroups),
        Arc::new(RejectAdminTokens),
        Arc::new(RejectAdminTokens),
        Arc::new(RejectAdminDashboard),
        Arc::new(RejectAdminUsageLogs),
        Arc::new(RejectAdminUsers),
        Arc::new(RejectAdminUsers),
        QueryApiKeyPolicy::Deny,
        None,
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
