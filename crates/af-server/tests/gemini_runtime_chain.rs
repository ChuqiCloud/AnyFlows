pub mod support;

use std::{
    env, fs,
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_protocol::{
    CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource, gemini::GeminiGenerateContentStreamEncoder,
};
use af_server::{Bootstrap, BootstrapError, ShutdownReport};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::{Value, json};
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, RUNTIME_HEADER_NAME, RUNTIME_HEADER_VALUE, UPSTREAM_BASE_URL,
    UPSTREAM_KEY, UPSTREAM_MODEL, credential_encryption_key, sanitized_json_error_code,
    seed_gemini_runtime_channel_with, send_request, send_request_with_timeout, spawn_proxy,
    spawn_streaming_proxy,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const LOOPBACK_ALLOWLIST: &str = r#"["127.0.0.1/32"]"#;
const LIVE_GEMINI_CASE: &str = "live_native_gemini_runtime";
const LIVE_GEMINI_TIMEOUT_SECS: i32 = 180;
const LIVE_CLIENT_TIMEOUT: Duration = Duration::from_secs(210);
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-gemini-runtime-{}-{serial}.db",
            std::process::id()
        ));
        let mut sqlite_path = path.to_string_lossy().replace('\\', "/");
        if !sqlite_path.starts_with('/') {
            sqlite_path.insert(0, '/');
        }
        Self {
            path,
            url: format!("sqlite://{sqlite_path}?mode=rwc"),
        }
    }

    fn billing_wal_directory(&self) -> std::path::PathBuf {
        self.path.with_extension("billing-wal")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = std::path::PathBuf::from(format!("{}{}", self.path.display(), suffix));
            let _ = fs::remove_file(candidate);
        }
        let _ = fs::remove_dir_all(self.billing_wal_directory());
    }
}

#[tokio::test]
async fn native_gemini_runtime_preserves_request_and_rebuilds_public_response() {
    let upstream_body = serde_json::to_vec(&json!({
        "responseId": "private-upstream-response",
        "modelVersion": "private-upstream-model",
        "candidates": [{
            "index": 0,
            "content": {"role": "model", "parts": [{"text": "native-gemini-answer"}]},
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 3,
            "candidatesTokenCount": 2,
            "totalTokenCount": 5
        }
    }))
    .unwrap();
    let request_body = br#"{
  "contents": [{"role":"user","parts":[{"text":"hello"}]}],
  "generationConfig": {"maxOutputTokens": 32}
}"#;
    let database = TestDatabase::new();
    seed_database(
        &database,
        UPSTREAM_BASE_URL,
        UPSTREAM_MODEL,
        UPSTREAM_KEY,
        None,
    )
    .await;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        &upstream_body,
        &[("Content-Type", "application/json")],
    );
    let (address, shutdown, server) =
        start_gateway(&database, &format!("http://{proxy_address}")).await;

    let response = send_request(
        address,
        "POST",
        "/v1beta/models/test-model:generateContent",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("x-goog-api-key", CLIENT_KEY),
        ],
    )
    .await;
    stop_gateway(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    let head = upstream.head.to_ascii_lowercase();
    assert!(head.starts_with(
        "post http://upstream.example/proxy/v1beta/models/test-model:generatecontent http/1.1"
    ));
    assert!(head.contains("x-goog-api-key: configured-upstream-key"));
    assert!(head.contains(&format!("{RUNTIME_HEADER_NAME}: {RUNTIME_HEADER_VALUE}")));
    assert!(!head.contains("authorization:"));
    assert!(!head.contains(&CLIENT_KEY.to_ascii_lowercase()));
    assert_eq!(upstream.body, request_body);

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["modelVersion"], UPSTREAM_MODEL);
    assert_eq!(
        body["candidates"][0]["content"]["parts"][0]["text"],
        "native-gemini-answer"
    );
    assert_eq!(body["usageMetadata"]["promptTokenCount"], 3);
    assert_eq!(body["usageMetadata"]["candidatesTokenCount"], 2);
    assert!(
        body["responseId"]
            .as_str()
            .is_some_and(|value| value.starts_with("response-"))
    );
    let rendered = String::from_utf8_lossy(&response.body);
    assert!(!rendered.contains("private-upstream-response"));
    assert!(!rendered.contains("private-upstream-model"));
}

#[tokio::test]
async fn native_gemini_runtime_closes_usage_on_official_eof() {
    let (first, rest) = gemini_stream_chunks();
    let request_body = br#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}],"generationConfig":{"maxOutputTokens":32}}"#;
    let database = TestDatabase::new();
    seed_database(
        &database,
        UPSTREAM_BASE_URL,
        UPSTREAM_MODEL,
        UPSTREAM_KEY,
        None,
    )
    .await;
    let (proxy_address, captured, release, proxy) = spawn_streaming_proxy(&first, &rest);
    let (address, shutdown, server) =
        start_gateway(&database, &format!("http://{proxy_address}")).await;
    let mut client = connect_and_send_gemini(address, request_body).await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_lower.contains("cache-control: no-cache, no-transform"));
    assert!(first_lower.contains("x-accel-buffering: no"));
    assert!(first_text.contains("first-part"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("private-stream-response"));
    assert!(!first_text.contains("private-upstream-model"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    assert_eq!(upstream.body, request_body);
    let upstream_head = upstream.head.to_ascii_lowercase();
    assert!(upstream_head.starts_with(
        "post http://upstream.example/proxy/v1beta/models/test-model:streamgeneratecontent?alt=sse http/1.1"
    ));
    assert!(upstream_head.contains("x-goog-api-key: configured-upstream-key"));
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("\"promptTokenCount\":4"));
    assert!(complete.contains("\"candidatesTokenCount\":2"));
    assert!(!complete.contains("[DONE]"));
    assert!(!complete.contains("private-stream-response"));
    assert!(!complete.contains("private-upstream-model"));

    stop_gateway(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn live_native_gemini_runtime() {
    if env::var("ANYFLOWS_LIVE_GEMINI_CASE").as_deref() != Ok(LIVE_GEMINI_CASE) {
        return;
    }
    let base_url = env::var("ANYFLOWS_LIVE_GEMINI_BASE_URL")
        .expect("真实 Gemini 联调必须设置 ANYFLOWS_LIVE_GEMINI_BASE_URL");
    let api_key = env::var("ANYFLOWS_LIVE_GEMINI_API_KEY")
        .expect("真实 Gemini 联调必须设置 ANYFLOWS_LIVE_GEMINI_API_KEY");
    let upstream_model = env::var("ANYFLOWS_LIVE_GEMINI_MODEL")
        .expect("真实 Gemini 联调必须设置 ANYFLOWS_LIVE_GEMINI_MODEL");
    let proxy_url = env::var("ANYFLOWS_LIVE_GEMINI_PROXY_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let database = TestDatabase::new();
    seed_database(
        &database,
        &base_url,
        &upstream_model,
        &api_key,
        Some(LIVE_GEMINI_TIMEOUT_SECS),
    )
    .await;
    let (address, shutdown, server) =
        start_gateway_with_optional_proxy(&database, proxy_url.as_deref()).await;

    let response = send_request_with_timeout(
        address,
        "POST",
        "/v1beta/models/test-model:generateContent",
        br#"{"contents":[{"role":"user","parts":[{"text":"Reply with OK."}]}],"generationConfig":{"maxOutputTokens":4}}"#,
        &[
            ("Content-Type", "application/json"),
            ("x-goog-api-key", CLIENT_KEY),
        ],
        LIVE_CLIENT_TIMEOUT,
    )
    .await;
    stop_gateway(shutdown, server).await;

    assert_eq!(
        response.status,
        200,
        "真实 Gemini HTTP 状态异常；错误码：{}",
        sanitized_json_error_code(&response.body).unwrap_or_else(|| "未提供".to_owned())
    );
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["modelVersion"], UPSTREAM_MODEL);
    assert!(
        body["candidates"]
            .as_array()
            .is_some_and(|candidates| !candidates.is_empty())
    );
    assert!(
        body["usageMetadata"]["promptTokenCount"]
            .as_i64()
            .is_some_and(|value| value > 0)
    );
}

async fn seed_database(
    database: &TestDatabase,
    base_url: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    let pool = connect_and_migrate(
        &DatabaseOptions::new(&database.url).unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    pool.close().await.unwrap();

    let connection = Database::connect(&database.url).await.unwrap();
    for (sql, values) in [
        (
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            vec![
                GROUP_ID.into(),
                "default".into(),
                "Default".into(),
                "{}".into(),
            ],
        ),
        (
            "INSERT INTO model_prices (model, billing_mode, input_price, output_price, cache_read_price, cache_creation_5m_price, cache_creation_1h_price, version) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                UPSTREAM_MODEL.into(),
                1_i16.into(),
                "0.000001".into(),
                "0.000001".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                1_i64.into(),
            ],
        ),
        (
            "INSERT INTO users (id, username, status, default_group_id, quota, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                support::USER_ID.into(),
                "gateway-user".into(),
                1_i16.into(),
                GROUP_ID.into(),
                1_000_000_000_i64.into(),
                "gateway-aff".into(),
                "{}".into(),
            ],
        ),
        (
            "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, remain_quota, allow_ips) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                support::TOKEN_ID.into(),
                support::USER_ID.into(),
                CLIENT_KEY_DIGEST.into(),
                "sk-af-AAECAwQFBgcI".into(),
                "gateway-token".into(),
                1_i16.into(),
                1_000_000_000_i64.into(),
                LOOPBACK_ALLOWLIST.into(),
            ],
        ),
    ] {
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                values,
            ))
            .await
            .unwrap();
    }
    seed_gemini_runtime_channel_with(
        &connection,
        GROUP_ID,
        base_url,
        UPSTREAM_MODEL,
        upstream_model,
        api_key,
        timeout_secs,
    )
    .await;
    connection.close().await.unwrap();
}

async fn start_gateway(
    database: &TestDatabase,
    proxy_url: &str,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<ShutdownReport, BootstrapError>>,
) {
    start_gateway_with_optional_proxy(database, Some(proxy_url)).await
}

async fn start_gateway_with_optional_proxy(
    database: &TestDatabase,
    proxy_url: Option<&str>,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<ShutdownReport, BootstrapError>>,
) {
    let config_path = database.path.with_extension("toml");
    let credential_key = credential_encryption_key();
    let proxy_config = proxy_url.map_or_else(String::new, |proxy_url| {
        format!(
            "proxy_url = {}\ntrust_proxy_dns = true\n",
            serde_json::to_string(proxy_url).unwrap()
        )
    });
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'gateway-test-key'\nkey = '{credential_key}'\n[auth]\nlookup_timeout_secs = 1\nsession_signing_key = '{credential_key}'\n[http_client]\nconnect_timeout_secs = 5\nread_timeout_secs = 5\nrequest_timeout_secs = 5\n{proxy_config}",
            database.url,
            database
                .billing_wal_directory()
                .to_string_lossy()
                .replace('\\', "/")
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
        .init_authentication()
        .unwrap()
        .init_relay()
        .unwrap()
        .init_billing()
        .await
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
    (address, shutdown, server)
}

async fn stop_gateway(
    shutdown: oneshot::Sender<()>,
    server: tokio::task::JoinHandle<Result<ShutdownReport, BootstrapError>>,
) {
    let _ = shutdown.send(());
    timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
}

fn gemini_stream_chunks() -> (Vec<u8>, Vec<u8>) {
    let final_usage = upstream_usage(4, 2);
    let mut encoder = GeminiGenerateContentStreamEncoder::new(
        "private-stream-response",
        "private-upstream-model",
    )
    .unwrap();
    let mut first = encoder
        .encode(CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: af_domain::Role::Assistant,
        })
        .unwrap();
    first.extend(
        encoder
            .encode(CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("first-part".to_owned()),
            })
            .unwrap(),
    );
    let mut rest = encoder
        .encode(CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("second-part".to_owned()),
        })
        .unwrap();
    for event in [
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        },
        CanonicalStreamEvent::Usage(final_usage),
        CanonicalStreamEvent::StreamEnd,
    ] {
        rest.extend(encoder.encode(event).unwrap());
    }
    (first, rest)
}

fn upstream_usage(input: i64, output: i64) -> Usage {
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

async fn connect_and_send_gemini(address: SocketAddr, body: &[u8]) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST /v1beta/models/test-model:streamGenerateContent?alt=sse HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nx-goog-api-key: {CLIENT_KEY}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
