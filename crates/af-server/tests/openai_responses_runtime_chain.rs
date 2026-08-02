pub mod support;

use std::{
    env, fs,
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_protocol::{
    CanonicalStreamEvent, ContentDelta, openai_responses::OpenAiResponsesStreamDecoder,
};
use af_server::{Bootstrap, BootstrapError, ShutdownReport};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use support::{
    CLIENT_KEY, GROUP_ID, UPSTREAM_MODEL, credential_encryption_key,
    seed_openai_responses_runtime_channel_with, send_request_with_timeout,
};
use tokio::{sync::oneshot, time::timeout};

const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const LOOPBACK_ALLOWLIST: &str = r#"["127.0.0.1/32"]"#;
const LIVE_OPENAI_RESPONSES_CASE: &str = "live_native_openai_responses_runtime";
/// 真实聚合上游可能执行较长推理，验收渠道保留三分钟总时限。
const LIVE_OPENAI_RESPONSES_TIMEOUT_SECS: i32 = 180;
/// 测试客户端必须比渠道总时限更长，避免辅助层抢先中断网关响应。
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
            "anyflows-openai-responses-runtime-{}-{serial}.db",
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
async fn live_native_openai_responses_runtime() {
    if env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_CASE").as_deref() != Ok(LIVE_OPENAI_RESPONSES_CASE)
    {
        return;
    }

    let base_url = env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL")
        .expect("真实 OpenAI Responses 联调必须设置 ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL");
    let api_key = env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_API_KEY")
        .expect("真实 OpenAI Responses 联调必须设置 ANYFLOWS_LIVE_OPENAI_RESPONSES_API_KEY");
    let upstream_model = env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_MODEL")
        .expect("真实 OpenAI Responses 联调必须设置 ANYFLOWS_LIVE_OPENAI_RESPONSES_MODEL");
    let proxy_url = env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_PROXY_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());

    let database = TestDatabase::new();
    seed_database(&database, &base_url, &upstream_model, &api_key).await;
    let (address, shutdown, server) = start_gateway(&database, proxy_url.as_deref()).await;
    let response = send_request_with_timeout(
        address,
        "POST",
        "/v1/responses",
        // 精确复现 AI SDK 无状态请求：数组输入、加密推理回传和缺省输出上限必须同链路可用。
        br#"{"model":"test-model","input":[{"role":"user","content":[{"type":"input_text","text":"hi"}]}],"store":false,"include":["reasoning.encrypted_content"],"stream":true}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {CLIENT_KEY}")),
        ],
        LIVE_CLIENT_TIMEOUT,
    )
    .await;
    stop_gateway(shutdown, server).await;

    assert_eq!(response.status, 200);
    assert!(
        response
            .headers
            .get("content-type")
            .is_some_and(|value| value.starts_with("text/event-stream"))
    );
    assert_eq!(
        response.headers.get("cache-control").map(String::as_str),
        Some("no-cache, no-transform")
    );

    let body = decode_response_body(&response);
    let rendered = String::from_utf8(body.clone()).unwrap();
    assert!(rendered.contains("event: response.created"));
    assert!(rendered.contains("event: response.completed"));
    assert!(rendered.contains("\"model\":\"test-model\""));
    assert!(!rendered.contains("event: error"));
    assert!(!rendered.contains("event: response.failed"));
    assert!(!rendered.contains(&upstream_model));

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&body).unwrap();
    decoder.finish().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::ContentDelta {
            delta: ContentDelta::Text(text),
            ..
        } if !text.is_empty()
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CanonicalStreamEvent::Finish { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CanonicalStreamEvent::StreamEnd))
    );
    let usage = events
        .iter()
        .find_map(|event| match event {
            CanonicalStreamEvent::Usage(usage) => Some(usage),
            _ => None,
        })
        .expect("真实 Responses 流必须返回终态用量");
    assert!(usage.input_tokens().get() > 0);
    assert!(usage.output_tokens().get() > 0);
}

async fn seed_database(
    database: &TestDatabase,
    base_url: &str,
    upstream_model: &str,
    api_key: &str,
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
    seed_openai_responses_runtime_channel_with(
        &connection,
        GROUP_ID,
        base_url,
        UPSTREAM_MODEL,
        upstream_model,
        api_key,
        Some(LIVE_OPENAI_RESPONSES_TIMEOUT_SECS),
    )
    .await;
    connection.close().await.unwrap();
}

async fn start_gateway(
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
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'gateway-test-key'\nkey = '{credential_key}'\n[auth]\nlookup_timeout_secs = 1\nsession_signing_key = '{credential_key}'\n[http_client]\nconnect_timeout_secs = 10\nread_timeout_secs = 180\nrequest_timeout_secs = 180\n{proxy_config}",
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
    timeout(support::IO_TIMEOUT, server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn decode_response_body(response: &support::RawResponse) -> Vec<u8> {
    let is_chunked = response
        .headers
        .get("transfer-encoding")
        .is_some_and(|value| {
            value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
        });
    if !is_chunked {
        return response.body.clone();
    }

    decode_chunked_body(&response.body)
}

fn decode_chunked_body(mut input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    loop {
        let line_end = input
            .windows(2)
            .position(|window| window == b"\r\n")
            .expect("分块响应缺少长度行结束符");
        let size_text = std::str::from_utf8(&input[..line_end])
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .trim();
        let size = usize::from_str_radix(size_text, 16).expect("分块响应长度不是十六进制");
        input = &input[line_end + 2..];
        if size == 0 {
            break;
        }

        // 每个分块必须完整到达，并由 CRLF 与下一长度行隔开。
        assert!(input.len() >= size + 2, "分块响应正文被截断");
        output.extend_from_slice(&input[..size]);
        assert_eq!(&input[size..size + 2], b"\r\n", "分块响应缺少结束符");
        input = &input[size + 2..];
    }
    output
}

#[test]
fn chunked_response_body_is_decoded_before_protocol_validation() {
    let encoded = b"4\r\neven\r\n8;sample=yes\r\nt stream\r\n0\r\n\r\n";
    assert_eq!(decode_chunked_body(encoded), b"event stream");
}
