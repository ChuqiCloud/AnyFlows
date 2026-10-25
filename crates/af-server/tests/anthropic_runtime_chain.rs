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
    UsageSemantics, UsageSource,
    anthropic::{AnthropicMessagesStreamDecoder, AnthropicMessagesStreamEncoder},
};
use af_server::{Bootstrap, BootstrapError, ShutdownReport};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::{Value, json};
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, RUNTIME_HEADER_NAME, RUNTIME_HEADER_VALUE, UPSTREAM_BASE_URL,
    UPSTREAM_KEY, UPSTREAM_MODEL, credential_encryption_key, sanitized_json_error_code,
    seed_anthropic_runtime_channel_with, send_request, send_request_with_timeout,
    spawn_delayed_proxy, spawn_proxy, spawn_streaming_proxy,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const LOOPBACK_ALLOWLIST: &str = r#"["127.0.0.1/32"]"#;
const LIVE_ANTHROPIC_CASE: &str = "live_native_anthropic_runtime";
/// 真实聚合上游可能执行较长推理，验收渠道保留三分钟总时限。
const LIVE_ANTHROPIC_TIMEOUT_SECS: i32 = 180;
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
            "anyflows-anthropic-runtime-{}-{serial}.db",
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
async fn native_anthropic_runtime_preserves_request_and_rebuilds_public_response() {
    let upstream_body = serde_json::to_vec(&json!({
        "id": "msg_private_upstream",
        "type": "message",
        "role": "assistant",
        "model": "private-upstream-model",
        "content": [{"type": "text", "text": "native-anthropic-answer"}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 3,
            "output_tokens": 2,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    }))
    .unwrap();
    let request_body = br#"{
  "model": "test-model",
  "max_tokens": 32,
  "messages": [{"role":"user","content":"hello"}]
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
        "/v1/messages",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("x-api-key", CLIENT_KEY),
        ],
    )
    .await;
    stop_gateway(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    let head = upstream.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://upstream.example/proxy/v1/messages http/1.1"));
    assert!(head.contains("x-api-key: configured-upstream-key"));
    assert!(head.contains("anthropic-version: 2023-06-01"));
    assert!(head.contains(&format!("{RUNTIME_HEADER_NAME}: {RUNTIME_HEADER_VALUE}")));
    assert!(!head.contains("authorization:"));
    assert!(!head.contains(&CLIENT_KEY.to_ascii_lowercase()));
    assert_eq!(upstream.body, request_body);

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["type"], "message");
    assert_eq!(body["role"], "assistant");
    assert_eq!(body["model"], UPSTREAM_MODEL);
    assert_eq!(body["content"][0]["text"], "native-anthropic-answer");
    assert_eq!(body["usage"]["input_tokens"], 3);
    assert_eq!(body["usage"]["output_tokens"], 2);
    assert!(body["id"].as_str().unwrap().starts_with("msg_"));
    let rendered = String::from_utf8_lossy(&response.body);
    assert!(!rendered.contains("msg_private_upstream"));
    assert!(!rendered.contains("private-upstream-model"));
}

#[tokio::test]
async fn native_anthropic_runtime_streams_real_usage_and_public_identity() {
    let (first, rest) = anthropic_stream_chunks();
    let request_body = br#"{"model":"test-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}],"stream":true}"#;
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
    let mut client = connect_and_send_anthropic(address, request_body).await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_lower.contains("cache-control: no-cache, no-transform"));
    assert!(first_lower.contains("x-accel-buffering: no"));
    assert!(first_text.contains("event: message_start"));
    assert!(first_text.contains("event: content_block_delta"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("msg_private_stream"));
    assert!(!first_text.contains("private-upstream-model"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream.body, request_body);
    assert_eq!(upstream_json["stream"], true);
    assert!(
        upstream
            .head
            .to_ascii_lowercase()
            .contains("x-api-key: configured-upstream-key")
    );
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("event: message_delta"));
    assert!(complete.contains("event: message_stop"));
    assert!(complete.contains("\"input_tokens\":4"));
    assert!(complete.contains("\"output_tokens\":2"));
    assert!(!complete.contains("msg_private_stream"));
    assert!(!complete.contains("private-upstream-model"));

    stop_gateway(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn native_compaction_json_preserves_replay_state_and_bills_additional_iterations_once() {
    assert_native_compaction_json(json!("runtime summary 已保留")).await;
}

#[tokio::test]
async fn native_compaction_json_preserves_null_state_and_bills_additional_iterations_once() {
    assert_native_compaction_json(Value::Null).await;
}

async fn assert_native_compaction_json(content: Value) {
    let block = native_compaction_block(content);
    let upstream_usage = compaction_usage();
    let upstream_body = serde_json::to_vec(&json!({
        "id":"msg_private_compaction", "type":"message", "role":"assistant",
        "model":"private-upstream-model", "content":[block.clone()],
        "stop_reason":"compaction", "stop_sequence":null, "usage":upstream_usage
    }))
    .unwrap();
    let request = compaction_request(false);
    let request_body = serde_json::to_vec(&request).unwrap();
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
        "/v1/messages",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("x-api-key", CLIENT_KEY),
            ("anthropic-beta", "caller-beta, caller-beta"),
        ],
    )
    .await;
    stop_gateway(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_eq!(
        response.status,
        200,
        "原生压缩 JSON 响应异常；错误码：{}",
        sanitized_response_error_code(&response)
    );
    assert_compaction_upstream(&upstream, &request_body, "compact-2026-09-04");
    let public: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(public["content"], json!([block]));
    assert_eq!(public["stop_reason"], "compaction");
    assert_public_compaction_usage(&public["usage"]);
    assert_eq!(public["model"], UPSTREAM_MODEL);
    assert_ne!(public["id"], "msg_private_compaction");
    assert_eq!(
        compaction_billing_summary(&database).await,
        (1, 112, 7, 2, 2, 3)
    );
}

#[tokio::test]
async fn native_compaction_sse_preserves_signed_blocks_without_inflating_public_usage() {
    assert_native_compaction_sse(json!("runtime summary 已保留")).await;
}

#[tokio::test]
async fn native_compaction_sse_preserves_null_blocks_without_inflating_public_usage() {
    assert_native_compaction_sse(Value::Null).await;
}

async fn assert_native_compaction_sse(content: Value) {
    let block = native_compaction_block(content);
    let upstream_body = compaction_sse(&block);
    assert_compaction_sse_fixture(&upstream_body, &block);
    let request = compaction_request(true);
    let request_body = serde_json::to_vec(&request).unwrap();
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
        &[("Content-Type", "text/event-stream")],
    );
    let (address, shutdown, server) =
        start_gateway(&database, &format!("http://{proxy_address}")).await;

    let response = send_request(
        address,
        "POST",
        "/v1/messages",
        &request_body,
        &[
            ("Content-Type", "application/json"),
            ("x-api-key", CLIENT_KEY),
            ("anthropic-beta", "caller-beta, caller-beta"),
        ],
    )
    .await;
    // 完整响应的 EOF 必须等待结算完成；收到后立即关闭，以覆盖结束回调竞态。
    stop_gateway(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_eq!(
        response.status,
        200,
        "原生压缩 SSE 响应异常；错误码：{}",
        sanitized_response_error_code(&response)
    );
    assert_compaction_upstream(&upstream, &request_body, "compact-2026-01-12");
    assert!(
        response
            .headers
            .get("content-type")
            .unwrap()
            .starts_with("text/event-stream")
    );
    assert_eq!(
        compaction_billing_summary(&database).await,
        (1, 112, 7, 2, 2, 3)
    );
    let body = decode_http_response_body(&response);
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    let mut events = Vec::new();
    for chunk in body.chunks(7) {
        events.extend(decoder.push(chunk).unwrap());
    }
    decoder.finish().unwrap();
    assert!(events.iter().any(|event| matches!(
        event, CanonicalStreamEvent::AnthropicCompactionEnd { item, .. }
        if item.as_value() == &block
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CanonicalStreamEvent::Finish {
            reason: FinishReason::Compaction,
            ..
        }
    )));
    let wire_usage = events
        .iter()
        .find_map(|event| match event {
            CanonicalStreamEvent::Usage(usage) => Some(usage),
            _ => None,
        })
        .unwrap();
    assert_eq!(wire_usage.input_tokens().get(), 100);
    assert_eq!(wire_usage.output_tokens().get(), 4);
    assert_eq!(decoder.billing_usage().unwrap().input_tokens().get(), 112);
    assert_eq!(decoder.billing_usage().unwrap().output_tokens().get(), 7);
    assert_eq!(events.last(), Some(&CanonicalStreamEvent::StreamEnd));

    let mut parser = af_protocol::openai_chat::SseParser::new();
    let wire_events: Vec<Value> = parser
        .push(&body)
        .unwrap()
        .into_iter()
        .filter(|event| !event.is_comment())
        .map(|event| serde_json::from_slice(event.data()).unwrap())
        .collect();
    parser.finish().unwrap();
    let initial = wire_events
        .iter()
        .find(|event| event["type"] == "message_start")
        .unwrap();
    assert_eq!(initial["message"]["usage"]["input_tokens"], 120);
    assert_eq!(initial["message"]["model"], UPSTREAM_MODEL);
    assert_ne!(initial["message"]["id"], "msg_private_compaction_stream");
    let final_delta = wire_events
        .iter()
        .find(|event| event["type"] == "message_delta")
        .unwrap();
    assert_public_compaction_usage(&final_delta["usage"]);
}

fn assert_compaction_sse_fixture(body: &[u8], block: &Value) {
    let mut decoder = AnthropicMessagesStreamDecoder::new();
    let events = decoder
        .push(body)
        .expect("完整原生压缩 SSE fixture 必须能够解码");
    decoder
        .finish()
        .expect("完整原生压缩 SSE fixture 必须包含逻辑终点");
    assert!(events.iter().any(|event| matches!(
        event, CanonicalStreamEvent::AnthropicCompactionEnd { item, .. }
        if item.as_value() == block
    )));
    let mut encoder = AnthropicMessagesStreamEncoder::new(
        "msg_fixture_replay",
        UPSTREAM_MODEL,
        decoder
            .initial_usage()
            .expect("原生压缩 SSE fixture 必须包含初始用量"),
    )
    .expect("原生压缩 SSE fixture 的初始用量必须能够编码");
    let mut encoded = Vec::new();
    for (index, event) in events.into_iter().enumerate() {
        encoded.extend(encoder.encode(event).unwrap_or_else(|error| {
            panic!("原生压缩 SSE fixture 第 {index} 个事件重编码失败：{error}")
        }));
    }
    let mut replay = AnthropicMessagesStreamDecoder::new();
    let replay_events = replay
        .push(&encoded)
        .expect("重编码后的原生压缩 SSE fixture 必须能够完整解码");
    replay
        .finish()
        .expect("重编码后的原生压缩 SSE fixture 必须包含逻辑终点");
    assert!(replay_events.iter().any(|event| matches!(
        event, CanonicalStreamEvent::AnthropicCompactionEnd { item, .. }
        if item.as_value() == block
    )));
    assert_eq!(replay.billing_usage(), decoder.billing_usage());
}

fn native_compaction_block(content: Value) -> Value {
    json!({
        "type":"compaction", "content":content, "signature":"runtime-signed-summary",
        "encrypted_content":"runtime-encrypted-state",
        "tool_changes":[{"type":"tool_removal", "name":"previous_tool"}],
        "future_extension":{"kept":true, "opaque":["v1", null]}
    })
}

fn compaction_usage() -> Value {
    json!({
        "input_tokens":100, "output_tokens":4,
        "cache_creation_input_tokens":0, "cache_read_input_tokens":0,
        "service_tier":"standard", "iterations":[
            {"type":"message", "input_tokens":100, "output_tokens":4},
            {"type":"compaction", "input_tokens":12, "output_tokens":3,
             "cache_creation_input_tokens":5, "cache_read_input_tokens":2,
             "cache_creation":{"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":3},
             "future_usage":{"retained":true}}
        ]
    })
}

fn compaction_request(stream: bool) -> Value {
    let mut request = json!({
        "model":UPSTREAM_MODEL, "max_tokens":32, "stream":stream,
        "messages":[
            {"role":"assistant", "content":[native_compaction_block(json!("previous summary"))]},
            {"role":"user", "content":"continue"}
        ]
    });
    if stream {
        request["context_management"] = json!({"edits":[{
            "type":"compact_20260112", "pause_after_compaction":true,
            "trigger":{"type":"input_tokens","value":20000},
            "future_option":{"retained":true}
        }]});
    } else {
        request["compaction"] = json!({"type":"summarize", "future_option":{"retained":true}});
    }
    request
}

fn assert_compaction_upstream(upstream: &support::CapturedRequest, request: &[u8], beta: &str) {
    assert_eq!(upstream.body, request);
    let head = upstream.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://upstream.example/proxy/v1/messages http/1.1"));
    assert!(head.contains("x-api-key: configured-upstream-key"));
    assert!(head.contains("anthropic-version: 2023-06-01"));
    assert!(head.contains(&format!("{RUNTIME_HEADER_NAME}: {RUNTIME_HEADER_VALUE}")));
    let betas: Vec<_> = head
        .lines()
        .filter_map(|line| line.strip_prefix("anthropic-beta:"))
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .collect();
    assert_eq!(
        betas
            .iter()
            .filter(|value| **value == "caller-beta")
            .count(),
        1
    );
    assert_eq!(betas.iter().filter(|value| **value == beta).count(), 1);
    assert!(!head.contains(&CLIENT_KEY.to_ascii_lowercase()));
}

fn assert_public_compaction_usage(usage: &Value) {
    assert_eq!(usage["input_tokens"], 100);
    assert_eq!(usage["output_tokens"], 4);
    assert_eq!(usage["cache_creation_input_tokens"], 0);
    assert_eq!(usage["cache_read_input_tokens"], 0);
    assert_eq!(usage["service_tier"], "standard");
    assert_eq!(usage["iterations"], compaction_usage()["iterations"]);
}

fn compaction_sse(block: &Value) -> Vec<u8> {
    let mut events = vec![json!({
        "type":"message_start", "message":{
            "id":"msg_private_compaction_stream", "type":"message", "role":"assistant",
            "model":"private-upstream-model", "content":[], "stop_reason":null,
            "usage":{"input_tokens":120, "output_tokens":0, "service_tier":"standard"}
        }
    })];
    let mut initial_block = block.clone();
    if !block["content"].is_null() {
        initial_block.as_object_mut().unwrap().remove("content");
        initial_block["signature"] = json!("");
        initial_block["encrypted_content"] = json!("");
    }
    events.push(json!({"type":"content_block_start", "index":0, "content_block":initial_block}));
    if !block["content"].is_null() {
        events.push(json!({"type":"content_block_delta", "index":0, "delta":{
            "type":"compaction_delta", "content":block["content"],
            "encrypted_content":block["encrypted_content"]
        }}));
        events.push(json!({"type":"content_block_delta", "index":0, "delta":{
            "type":"signature_delta", "signature":block["signature"]
        }}));
    }
    events.extend([
        json!({"type":"content_block_stop", "index":0}),
        json!({"type":"message_delta", "delta":{"stop_reason":"compaction", "stop_sequence":null}, "usage":compaction_usage()}),
        json!({"type":"message_stop"}),
    ]);
    events
        .into_iter()
        .flat_map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
            .into_bytes()
        })
        .collect()
}

async fn compaction_billing_summary(database: &TestDatabase) -> (i64, i64, i64, i64, i64, i64) {
    let connection = Database::connect(&database.url).await.unwrap();
    let row = connection.query_one(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "SELECT COUNT(*) AS row_count, COALESCE(SUM(input_tokens), 0) AS input_tokens, COALESCE(SUM(output_tokens), 0) AS output_tokens, COALESCE(SUM(cache_read), 0) AS cache_read, COALESCE(SUM(cache_creation_5m), 0) AS cache_creation_5m, COALESCE(SUM(cache_creation_1h), 0) AS cache_creation_1h FROM usage_logs WHERE token_id = ?",
        [support::TOKEN_ID.into()],
    )).await.unwrap().unwrap();
    let summary = (
        row.try_get("", "row_count").unwrap(),
        row.try_get("", "input_tokens").unwrap(),
        row.try_get("", "output_tokens").unwrap(),
        row.try_get("", "cache_read").unwrap(),
        row.try_get("", "cache_creation_5m").unwrap(),
        row.try_get("", "cache_creation_1h").unwrap(),
    );
    connection.close().await.unwrap();
    summary
}

fn sanitized_response_error_code(response: &support::RawResponse) -> String {
    let body = decode_http_response_body(response);
    if let Some(code) = sanitized_json_error_code(&body) {
        return code;
    }
    if body.len() <= 64 * 1024
        && let Ok(value) = serde_json::from_slice::<Value>(&body)
        && let Some(kind) = value.pointer("/error/type").and_then(Value::as_str)
        && !kind.is_empty()
        && kind.len() <= 96
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return kind.to_owned();
    }
    "未提供".to_owned()
}

fn decode_http_response_body(response: &support::RawResponse) -> Vec<u8> {
    if !response
        .headers
        .get("transfer-encoding")
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
        })
    {
        return response.body.clone();
    }
    let mut input = response.body.as_slice();
    let mut decoded = Vec::new();
    loop {
        let line_end = input
            .windows(2)
            .position(|window| window == b"\r\n")
            .unwrap();
        let length = std::str::from_utf8(&input[..line_end])
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let length = usize::from_str_radix(length.trim(), 16).unwrap();
        input = &input[line_end + 2..];
        if length == 0 {
            return decoded;
        }
        decoded.extend_from_slice(&input[..length]);
        assert_eq!(&input[length..length + 2], b"\r\n");
        input = &input[length + 2..];
    }
}

#[tokio::test]
async fn channel_timeout_override_outlives_short_global_http_timeout() {
    let upstream_body = serde_json::to_vec(&json!({
        "id": "msg_delayed_upstream",
        "type": "message",
        "role": "assistant",
        "model": "private-upstream-model",
        "content": [{"type": "text", "text": "delayed-answer"}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 3,
            "output_tokens": 2,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    }))
    .unwrap();
    let database = TestDatabase::new();
    seed_database(
        &database,
        UPSTREAM_BASE_URL,
        UPSTREAM_MODEL,
        UPSTREAM_KEY,
        Some(2),
    )
    .await;
    let (proxy_address, captured, proxy) = spawn_delayed_proxy(
        "200 OK",
        &upstream_body,
        &[("Content-Type", "application/json")],
        Duration::from_millis(1_200),
    );
    let (address, shutdown, server) =
        start_gateway_with_http_timeout(&database, &format!("http://{proxy_address}"), 1).await;

    let response = send_request(
        address,
        "POST",
        "/v1/messages",
        br#"{"model":"test-model","max_tokens":4,"messages":[{"role":"user","content":"Reply with OK."}]}"#,
        &[("Content-Type", "application/json"), ("x-api-key", CLIENT_KEY)],
    )
    .await;
    stop_gateway(shutdown, server).await;
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["content"][0]["text"], "delayed-answer");
}

#[tokio::test]
async fn live_native_anthropic_runtime() {
    if env::var("ANYFLOWS_LIVE_ANTHROPIC_CASE").as_deref() != Ok(LIVE_ANTHROPIC_CASE) {
        return;
    }
    let base_url = env::var("ANYFLOWS_LIVE_ANTHROPIC_BASE_URL")
        .expect("真实 Anthropic 联调必须设置 ANYFLOWS_LIVE_ANTHROPIC_BASE_URL");
    let api_key = env::var("ANYFLOWS_LIVE_ANTHROPIC_API_KEY")
        .expect("真实 Anthropic 联调必须设置 ANYFLOWS_LIVE_ANTHROPIC_API_KEY");
    let upstream_model = env::var("ANYFLOWS_LIVE_ANTHROPIC_MODEL")
        .expect("真实 Anthropic 联调必须设置 ANYFLOWS_LIVE_ANTHROPIC_MODEL");
    let proxy_url = env::var("ANYFLOWS_LIVE_ANTHROPIC_PROXY_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let database = TestDatabase::new();
    seed_database(
        &database,
        &base_url,
        &upstream_model,
        &api_key,
        Some(LIVE_ANTHROPIC_TIMEOUT_SECS),
    )
    .await;
    let (address, shutdown, server) =
        start_gateway_with_optional_proxy(&database, proxy_url.as_deref(), 5).await;

    let response = send_request_with_timeout(
        address,
        "POST",
        "/v1/messages",
        br#"{"model":"test-model","max_tokens":4,"messages":[{"role":"user","content":"Reply with OK."}]}"#,
        &[("Content-Type", "application/json"), ("x-api-key", CLIENT_KEY)],
        LIVE_CLIENT_TIMEOUT,
    )
    .await;
    stop_gateway(shutdown, server).await;

    assert_eq!(
        response.status,
        200,
        "真实 Anthropic HTTP 状态异常；错误码：{}",
        sanitized_json_error_code(&response.body).unwrap_or_else(|| "未提供".to_owned())
    );
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["type"], "message");
    assert_eq!(body["model"], UPSTREAM_MODEL);
    assert!(
        body["content"]
            .as_array()
            .is_some_and(|content| !content.is_empty())
    );
    assert!(
        body["usage"]["input_tokens"]
            .as_i64()
            .is_some_and(|value| value > 0)
    );
    assert!(
        body["usage"]["output_tokens"]
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
    seed_anthropic_runtime_channel_with(
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
    start_gateway_with_http_timeout(database, proxy_url, 5).await
}

async fn start_gateway_with_http_timeout(
    database: &TestDatabase,
    proxy_url: &str,
    http_timeout_secs: u64,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<ShutdownReport, BootstrapError>>,
) {
    start_gateway_with_optional_proxy(database, Some(proxy_url), http_timeout_secs).await
}

async fn start_gateway_with_optional_proxy(
    database: &TestDatabase,
    proxy_url: Option<&str>,
    http_timeout_secs: u64,
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
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'warn'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'gateway-test-key'\nkey = '{credential_key}'\n[auth]\nlookup_timeout_secs = 1\nsession_signing_key = '{credential_key}'\n[http_client]\nconnect_timeout_secs = 5\nread_timeout_secs = {http_timeout_secs}\nrequest_timeout_secs = {http_timeout_secs}\n{proxy_config}",
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

fn anthropic_stream_chunks() -> (Vec<u8>, Vec<u8>) {
    let initial_usage = upstream_usage(4, 0);
    let final_usage = upstream_usage(4, 2);
    let mut encoder = AnthropicMessagesStreamEncoder::new(
        "msg_private_stream",
        "private-upstream-model",
        initial_usage,
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
        UsageSemantics::CacheSeparated,
    )
    .unwrap()
}

async fn connect_and_send_anthropic(address: SocketAddr, body: &[u8]) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nx-api-key: {CLIENT_KEY}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
