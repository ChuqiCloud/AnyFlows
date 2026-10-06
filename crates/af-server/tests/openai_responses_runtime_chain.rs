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
use serde_json::json;
use support::{
    CLIENT_KEY, GROUP_ID, OPENAI_RESPONSES_CHANNEL_ID, UPSTREAM_BASE_URL, UPSTREAM_KEY,
    UPSTREAM_MODEL, credential_encryption_key, sanitized_json_error_code,
    seed_openai_responses_runtime_channel_with, send_request_with_timeout, spawn_proxy,
};
use tokio::{sync::oneshot, time::timeout};

const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const LOOPBACK_ALLOWLIST: &str = r#"["127.0.0.1/32"]"#;
const LIVE_OPENAI_RESPONSES_CASE: &str = "live_native_openai_responses_runtime";
const LIVE_OPENAI_RESPONSES_LOW_COST_CASE: &str = "live_native_openai_responses_low_cost";
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
async fn compact_runtime_uses_the_dedicated_upstream_path() {
    let upstream_body = serde_json::to_vec(&json!({
        "id": "resp_compact_1",
        "object": "response.compaction",
        "created_at": 1,
        "output": [{
            "type": "compaction",
            "id": "cmp_1",
            "encrypted_content": "opaque-window"
        }],
        "usage": {
            "input_tokens": 2,
            "input_tokens_details": {
                "cached_tokens": 1,
                "cache_write_tokens": 0
            },
            "output_tokens": 1,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": 3
        }
    }))
    .unwrap();
    let database = TestDatabase::new();
    seed_database(&database, UPSTREAM_BASE_URL, UPSTREAM_MODEL, UPSTREAM_KEY).await;
    enable_compact_channel(&database).await;
    // 高并发门禁下数据库准备可能超过代理 I/O 窗口，准备完成后再启动单次代理。
    let (upstream_address, captured, upstream) = spawn_proxy(
        "200 OK",
        &upstream_body,
        &[("Content-Type", "application/json")],
    );
    let proxy_url = format!("http://{upstream_address}");
    let (address, shutdown, server) = start_gateway(&database, Some(&proxy_url)).await;

    let response = send_request_with_timeout(
        address,
        "POST",
        "/v1/responses/compact",
        br#"{"model":"test-model","input":"compact this context"}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {CLIENT_KEY}")),
        ],
        support::IO_TIMEOUT,
    )
    .await;
    stop_gateway(shutdown, server).await;
    let captured = captured.recv_timeout(support::IO_TIMEOUT).unwrap();
    upstream.join().unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(
        response.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["object"], "response.compaction");
    assert_eq!(body["usage"]["total_tokens"], 3);
    assert!(
        captured
            .head
            .starts_with("POST http://upstream.example/proxy/v1/responses/compact HTTP/1.1\r\n")
    );
    let upstream_head = captured.head.to_ascii_lowercase();
    assert!(upstream_head.contains("authorization: bearer configured-upstream-key"));
    let request: serde_json::Value = serde_json::from_slice(&captured.body).unwrap();
    assert_eq!(request["model"], UPSTREAM_MODEL);
    assert_eq!(request["input"], "compact this context");
    assert!(request.get("stream").is_none());
}

#[tokio::test]
async fn live_native_openai_responses_runtime() {
    if env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_CASE").as_deref() != Ok(LIVE_OPENAI_RESPONSES_CASE)
    {
        return;
    }

    run_live_responses_request(
        // 精确复现 AI SDK 无状态请求：数组输入、加密推理回传和缺省输出上限必须同链路可用。
        br#"{"model":"test-model","input":[{"role":"user","content":[{"type":"input_text","text":"hi"}]}],"store":false,"include":["reasoning.encrypted_content"],"stream":true}"#,
    )
    .await;
}

/// 以供应商最小共同字段运行真实 Responses 网关验收。
#[tokio::test]
async fn live_native_openai_responses_low_cost() {
    if env::var("ANYFLOWS_LIVE_OPENAI_RESPONSES_CASE").as_deref()
        != Ok(LIVE_OPENAI_RESPONSES_LOW_COST_CASE)
    {
        return;
    }

    run_live_responses_request(
        // 低成本验收固定最小输出上限，避免供应商默认上限放大实际消耗。
        br#"{"model":"test-model","input":"Reply with exactly OK.","max_output_tokens":4,"stream":true}"#,
    )
    .await;
}

async fn run_live_responses_request(request_body: &[u8]) {
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
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {CLIENT_KEY}")),
        ],
        LIVE_CLIENT_TIMEOUT,
    )
    .await;
    stop_gateway(shutdown, server).await;

    let body = decode_response_body(&response);
    assert_eq!(
        response.status,
        200,
        "真实 Responses HTTP 状态异常；错误码：{}",
        sanitized_json_error_code(&body).unwrap_or_else(|| "未提供".to_owned())
    );
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

    let rendered = std::str::from_utf8(&body).unwrap_or_else(|_| {
        panic!(
            "真实 Responses SSE 响应不是 UTF-8；事件摘要：{}",
            summarize_sse(&body)
        )
    });
    assert_sse_event(rendered, &body, "response.created");
    assert_sse_event(rendered, &body, "response.completed");
    assert!(rendered.contains("\"model\":\"test-model\""));
    assert!(
        !rendered.contains("event: error"),
        "真实 Responses SSE 返回错误事件；事件摘要：{}",
        summarize_sse(&body)
    );
    assert!(
        !rendered.contains("event: response.failed"),
        "真实 Responses SSE 返回失败终态；事件摘要：{}",
        summarize_sse(&body)
    );
    assert!(!rendered.contains(&upstream_model));

    let mut decoder = OpenAiResponsesStreamDecoder::new();
    let events = decoder.push(&body).unwrap_or_else(|error| {
        panic!(
            "真实 Responses SSE 解码失败：{error}；事件摘要：{}",
            summarize_sse(&body)
        )
    });
    decoder.finish().unwrap_or_else(|error| {
        panic!(
            "真实 Responses SSE 在流结束时解码失败：{error}；事件摘要：{}",
            summarize_sse(&body)
        )
    });
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

/// 在真实联调失败时只保留协议事件名和受限错误码，不打印上游正文。
fn assert_sse_event(rendered: &str, body: &[u8], expected: &str) {
    let marker = format!("event: {expected}");
    assert!(
        rendered.contains(&marker),
        "真实 Responses SSE 缺少事件 `{expected}`；事件摘要：{}",
        summarize_sse(body)
    );
}

/// 生成有界的 SSE 诊断摘要，避免把提示词、模型响应或供应商正文写入构建日志。
fn summarize_sse(body: &[u8]) -> String {
    const MAX_EVENTS: usize = 64;
    const MAX_DATA_BYTES: usize = 64 * 1024;

    let text = String::from_utf8_lossy(body);
    let mut event_name: Option<String> = None;
    let mut data = Vec::new();
    let mut saw_data = false;
    let mut summaries = Vec::new();
    let mut omitted = 0usize;

    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            if saw_data {
                append_sse_summary(
                    event_name.take(),
                    &data,
                    &mut summaries,
                    &mut omitted,
                    MAX_EVENTS,
                );
                data.clear();
                saw_data = false;
            } else {
                event_name = None;
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            if saw_data && data.len() < MAX_DATA_BYTES {
                data.push(b'\n');
            }
            let remaining = MAX_DATA_BYTES.saturating_sub(data.len());
            data.extend_from_slice(&value.as_bytes()[..value.len().min(remaining)]);
            saw_data = true;
        }
    }
    if saw_data {
        append_sse_summary(event_name, &data, &mut summaries, &mut omitted, MAX_EVENTS);
    }

    if omitted > 0 {
        summaries.push(format!("其余 {omitted} 个事件已省略"));
    }
    if summaries.is_empty() {
        "未解析到完整 SSE 事件".to_owned()
    } else {
        summaries.join(", ")
    }
}

/// 把单个 SSE 帧压缩成事件名及可安全展示的错误码。
fn append_sse_summary(
    event_name: Option<String>,
    data: &[u8],
    summaries: &mut Vec<String>,
    omitted: &mut usize,
    max_events: usize,
) {
    if summaries.len() >= max_events {
        *omitted = omitted.saturating_add(1);
        return;
    }
    let event = event_name
        .as_deref()
        .map(|name| sanitize_diagnostic_token(name, "无效事件"))
        .unwrap_or_else(|| "message".to_owned());
    if let Some(code) = sanitized_json_error_code(data) {
        summaries.push(format!("{event}[code={code}]"));
    } else {
        summaries.push(event);
    }
}

/// 只接受短 ASCII 标识，防止错误字段携带正文、控制字符或日志换行。
fn sanitize_diagnostic_token(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        value.to_owned()
    } else {
        fallback.to_owned()
    }
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

async fn enable_compact_channel(database: &TestDatabase) {
    let connection = Database::connect(&database.url).await.unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE channels SET settings = ? WHERE id = ?",
            vec![
                json!({"responses_compact_mode": "force_on"})
                    .to_string()
                    .into(),
                OPENAI_RESPONSES_CHANNEL_ID.into(),
            ],
        ))
        .await
        .unwrap();
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

#[test]
fn sse_summary_only_contains_event_names_and_error_codes() {
    let summary = summarize_sse(
        br#"event: response.failed
data: {"code":"rate_limit_exceeded","message":"do not print this"}

event: response.completed
data: {"response":{"error":{"code":"server_error","message":"secret"}}}

"#,
    );

    assert_eq!(
        summary,
        "response.failed[code=rate_limit_exceeded], response.completed[code=server_error]"
    );
    assert!(!summary.contains("do not print this"));
    assert!(!summary.contains("secret"));
    assert_eq!(
        sanitized_json_error_code(
            br#"{"error":{"code":"upstream_unavailable","message":"do not print this"}}"#
        ),
        Some("upstream_unavailable".to_owned())
    );
    assert_eq!(
        sanitized_json_error_code(br#"{"code":"line\nbreak","message":"secret"}"#),
        Some("无效错误码".to_owned())
    );
    assert_eq!(sanitized_json_error_code(b"not-json"), None);
}

#[test]
fn sse_summary_bounds_event_count() {
    let body = (0..70)
        .map(|_| "event: response.created\ndata: {}\n\n")
        .collect::<String>();
    let summary = summarize_sse(body.as_bytes());

    assert!(summary.contains("其余 6 个事件已省略"));
    assert!(!summary.contains('\n'));
}
