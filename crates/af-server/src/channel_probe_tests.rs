use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use af_account::{CredentialDecryptor, credential_plaintext_aad};
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{DatabaseOptions, DatabasePool, MigrationOptions, connect_and_migrate};
use af_domain::{ChannelId, CredentialKind, Protocol, Status};
use af_httpclient::{
    HttpClientConfig, HttpClientProvider, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
};
use af_scheduler::{ChannelProbe as _, ChannelProbeStatus};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use sea_orm::{ConnectionTrait as _, Database, DatabaseBackend, DatabaseConnection, Statement};
use serde_json::json;

use crate::channel_probe::DatabaseChannelProbe;

const TEST_CHANNEL_ID: i64 = 41;
const TEST_CREDENTIAL_ID: i64 = 73;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MODEL_CANARY: &str = "probe-model-canary";
const SECRET_CANARY: &str = "probe-secret-canary";
const RESPONSES_PROBE_RESPONSE: &[u8] = br#"{"id":"resp_probe","object":"response","created_at":1700000000,"status":"completed","error":null,"incomplete_details":null,"model":"probe-model-canary","output":[{"id":"msg_probe","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}],"usage":{"input_tokens":1,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":2}}"#;
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn valid_openai_response_is_healthy_and_sends_required_headers() {
    let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::OpenAiChat, false, false).await;
    let response = br#"{"id":"chatcmpl-probe","object":"chat.completion","created":1700000000,"model":"probe-model-canary","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/chat/completions http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    assert!(head.contains("x-probe-header: probe-header-value-canary"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["stream"], false);
    assert_eq!(body["max_completion_tokens"], 1);
    assert_eq!(body["messages"][0]["content"], "ping");

    let rendered = format!("{probe:?}");
    assert!(!rendered.contains(SECRET_CANARY));
    assert!(!rendered.contains(MODEL_CANARY));
    assert!(!rendered.contains("probe-upstream.example"));

    database.close().await;
}

#[tokio::test]
async fn valid_openai_responses_response_is_healthy_and_disables_storage() {
    let key = [0x43; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(
        &database.seed,
        &key,
        Protocol::OpenAiResponses,
        false,
        false,
    )
    .await;
    database
        .seed
        .execute_unprepared(&format!(
            "UPDATE channels SET status = {} WHERE id = {TEST_CHANNEL_ID}",
            Status::Enabled.code()
        ))
        .await
        .unwrap();
    let compact_response = br#"{"id":"cmp_probe","created_at":1700000001,"object":"response.compaction","output":[{"type":"compaction","encrypted_content":"opaque-probe"}],"usage":{"input_tokens":1,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":2}}"#;
    let (proxy_address, captured, server) = spawn_proxy_sequence([
        ("200 OK", RESPONSES_PROBE_RESPONSE),
        ("200 OK", compact_response.as_slice()),
    ]);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let compact_request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/responses http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["stream"], false);
    assert_eq!(body["store"], false);
    assert_eq!(body["max_output_tokens"], 1);
    assert_eq!(body["input"][0]["content"][0]["text"], "ping");
    let compact_head = compact_request.head.to_ascii_lowercase();
    assert!(
        compact_head
            .starts_with("post http://probe-upstream.example/v1/responses/compact http/1.1")
    );
    let compact_body: serde_json::Value = serde_json::from_slice(&compact_request.body).unwrap();
    assert_eq!(compact_body["model"], MODEL_CANARY);
    assert_eq!(compact_body["input"], "ping");
    assert_eq!(
        compact_probe_fact(&database.seed).await,
        (Some("supported".to_owned()), Some(200))
    );

    database.close().await;
}

#[tokio::test]
async fn compact_probe_404_marks_unsupported_without_harming_responses_health() {
    let key = [0x49; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(
        &database.seed,
        &key,
        Protocol::OpenAiResponses,
        false,
        false,
    )
    .await;
    let (proxy_address, captured, server) = spawn_proxy_sequence([
        ("200 OK", RESPONSES_PROBE_RESPONSE),
        ("404 Not Found", br#"{"error":"not-found"}"#.as_slice()),
    ]);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    assert_eq!(
        compact_probe_fact(&database.seed).await,
        (Some("unsupported".to_owned()), Some(404))
    );

    database.close().await;
}

#[tokio::test]
async fn compact_probe_rejects_successful_non_compaction_response() {
    let key = [0x4b; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(
        &database.seed,
        &key,
        Protocol::OpenAiResponses,
        false,
        false,
    )
    .await;
    let (proxy_address, captured, server) = spawn_proxy_sequence([
        ("200 OK", RESPONSES_PROBE_RESPONSE),
        ("200 OK", RESPONSES_PROBE_RESPONSE),
    ]);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    assert_eq!(
        compact_probe_fact(&database.seed).await,
        (Some("unsupported".to_owned()), Some(200))
    );

    database.close().await;
}

#[tokio::test]
async fn compact_probe_transient_failure_preserves_previous_fact() {
    let key = [0x4a; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(
        &database.seed,
        &key,
        Protocol::OpenAiResponses,
        false,
        false,
    )
    .await;
    set_channel_settings(
        &database.seed,
        json!({
            "responses_compact_probe_result": "supported",
            "responses_compact_probe_checked_at": 1_735_000_000_000_i64,
            "responses_compact_probe_http_status": 200
        }),
    )
    .await;
    let (proxy_address, captured, server) = spawn_proxy_sequence([
        ("200 OK", RESPONSES_PROBE_RESPONSE),
        (
            "503 Service Unavailable",
            br#"{"error":"temporary"}"#.as_slice(),
        ),
    ]);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    assert_eq!(
        compact_probe_fact(&database.seed).await,
        (Some("supported".to_owned()), Some(200))
    );

    database.close().await;
}

#[tokio::test]
async fn valid_openai_embeddings_response_is_healthy_and_uses_native_endpoint() {
    let key = [0x47; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(
        &database.seed,
        &key,
        Protocol::OpenAiEmbeddings,
        false,
        false,
    )
    .await;
    let response = br#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[0.25,0.75]}],"model":"private-probe-model","usage":{"prompt_tokens":1,"total_tokens":1}}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/embeddings http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    assert!(head.contains("x-probe-header: probe-header-value-canary"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["input"], "ping");
    assert_eq!(body["encoding_format"], "float");
    assert!(body.get("messages").is_none());
    assert!(body.get("stream").is_none());

    database.close().await;
}

#[tokio::test]
async fn valid_jina_rerank_response_is_healthy_and_uses_native_endpoint() {
    let key = [0x4a; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::JinaRerank, false, false).await;
    let response = br#"{"model":"private-probe-model","object":"list","results":[{"index":0,"relevance_score":0.9}],"usage":{"total_tokens":1}}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/rerank http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    assert!(head.contains("x-probe-header: probe-header-value-canary"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["query"], "ping");
    assert_eq!(body["documents"], serde_json::json!(["ping"]));
    assert_eq!(body["return_documents"], false);
    assert!(body.get("stream").is_none());

    database.close().await;
}

#[tokio::test]
async fn valid_cohere_rerank_response_is_healthy_and_uses_v2_endpoint() {
    let key = [0x4b; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::CohereRerank, false, false).await;
    let response = br#"{"results":[{"index":0,"relevance_score":0.9}],"id":"private-id","meta":{"api_version":{"version":"2"},"billed_units":{"search_units":1}}}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v2/rerank http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    assert!(head.contains("x-client-name: anyflows"));
    assert!(head.contains("x-probe-header: probe-header-value-canary"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["query"], "ping");
    assert_eq!(body["documents"], serde_json::json!(["ping"]));
    assert!(body.get("return_documents").is_none());

    database.close().await;
}

#[tokio::test]
async fn valid_openai_images_response_is_healthy_and_uses_low_cost_probe() {
    let key = [0x48; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::OpenAiImages, false, false).await;
    let response =
        br#"{"created":1700000000,"data":[{"b64_json":"iVBORw0KGgo="}],"output_format":"png"}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/images/generations http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["prompt"], "ping");
    assert_eq!(body["size"], "1024x1024");
    assert_eq!(body["quality"], "low");
    assert_eq!(body["output_format"], "png");
    assert!(body.get("stream").is_none());

    database.close().await;
}

#[tokio::test]
async fn valid_openai_speech_response_is_healthy_and_uses_short_wav_probe() {
    let key = [0x49; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::OpenAiSpeech, false, false).await;
    let response = one_second_speech_wav();
    let (proxy_address, captured, server) = spawn_proxy("200 OK", &response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/audio/speech http/1.1"));
    assert!(head.contains("authorization: bearer probe-secret-canary"));
    assert!(head.contains("content-type: application/json"));
    assert!(head.contains("accept: application/octet-stream"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["input"], "ping");
    assert_eq!(body["voice"], "alloy");
    assert_eq!(body["response_format"], "wav");
    assert!(body.get("stream_format").is_none());

    database.close().await;
}

#[tokio::test]
async fn valid_anthropic_response_is_healthy_and_sends_messages_headers() {
    let key = [0x44; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::Anthropic, false, false).await;
    let response = br#"{"id":"msg_probe","type":"message","role":"assistant","model":"probe-model-canary","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://probe-upstream.example/v1/messages http/1.1"));
    assert!(head.contains("x-api-key: probe-secret-canary"));
    assert!(head.contains("anthropic-version: 2023-06-01"));
    assert!(!head.contains("authorization:"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], MODEL_CANARY);
    assert_eq!(body["stream"], serde_json::Value::Null);
    assert_eq!(body["max_tokens"], 1);
    assert_eq!(body["messages"][0]["content"], "ping");

    database.close().await;
}

#[tokio::test]
async fn valid_gemini_response_is_healthy_and_sends_google_headers() {
    let key = [0x46; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::Gemini, false, false).await;
    let response = br#"{"responseId":"response-probe","modelVersion":"probe-model-canary","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#;
    let (proxy_address, captured, server) = spawn_proxy("200 OK", response);
    let probe = probe(&database.pool, &key, proxy_address);

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);

    let request = captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    let head = request.head.to_ascii_lowercase();
    assert!(head.starts_with(
        "post http://probe-upstream.example/v1beta/models/probe-model-canary:generatecontent http/1.1"
    ));
    assert!(head.contains("x-goog-api-key: probe-secret-canary"));
    assert!(!head.contains("authorization:"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert!(body.get("model").is_none());
    assert_eq!(body["generationConfig"]["maxOutputTokens"], 1);
    assert_eq!(body["contents"][0]["parts"][0]["text"], "ping");

    database.close().await;
}

#[tokio::test]
async fn channel_timeout_override_outlives_probe_default_timeout() {
    let key = [0x45; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let database = test_database().await;
    seed_probe_target(&database.seed, &key, Protocol::Anthropic, false, false).await;
    database
        .seed
        .execute_unprepared(&format!(
            "UPDATE channels SET timeout_secs = 1 WHERE id = {TEST_CHANNEL_ID}"
        ))
        .await
        .unwrap();
    let response = br#"{"id":"msg_probe","type":"message","role":"assistant","model":"probe-model-canary","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}"#;
    let (proxy_address, captured, server) =
        spawn_delayed_proxy("200 OK", response, Duration::from_millis(200));
    let probe = probe_with_default_timeout(
        &database.pool,
        &key,
        proxy_address,
        Duration::from_millis(100),
    );

    assert_eq!(probe.check(channel_id()).await, ChannelProbeStatus::Healthy);
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    server.join().unwrap();
    database.close().await;
}

#[tokio::test]
async fn non_success_and_invalid_success_response_are_unhealthy() {
    let key = [0x24; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    for (status_line, body) in [
        (
            "503 Service Unavailable",
            br#"{"error":"upstream-secret"}"#.as_slice(),
        ),
        ("200 OK", br#"{"invalid":"response-secret"}"#.as_slice()),
    ] {
        let database = test_database().await;
        seed_probe_target(&database.seed, &key, Protocol::OpenAiChat, false, false).await;
        let (proxy_address, captured, server) = spawn_proxy(status_line, body);
        let probe = probe(&database.pool, &key, proxy_address);

        assert_eq!(
            probe.check(channel_id()).await,
            ChannelProbeStatus::Unhealthy
        );
        captured.recv_timeout(IO_TIMEOUT).unwrap();
        server.join().unwrap();
        database.close().await;
    }
}

#[tokio::test]
async fn decryption_failure_and_required_proxy_never_recover_channel() {
    let key = [0x66; CREDENTIAL_ENCRYPTION_KEY_BYTES];

    let broken_database = test_database().await;
    seed_probe_target(
        &broken_database.seed,
        &key,
        Protocol::OpenAiChat,
        false,
        true,
    )
    .await;
    let broken_probe = probe_without_reachable_upstream(&broken_database.pool, &key);
    assert_eq!(
        broken_probe.check(channel_id()).await,
        ChannelProbeStatus::Unhealthy
    );
    broken_database.close().await;

    let proxy_bound_database = test_database().await;
    seed_probe_target(
        &proxy_bound_database.seed,
        &key,
        Protocol::OpenAiChat,
        true,
        false,
    )
    .await;
    let proxy_bound_probe = probe_without_reachable_upstream(&proxy_bound_database.pool, &key);
    assert_eq!(
        proxy_bound_probe.check(channel_id()).await,
        ChannelProbeStatus::Unhealthy
    );
    proxy_bound_database.close().await;
}

async fn test_database() -> TestDatabase {
    let files = TestDatabaseFiles::new();
    let pool = connect_and_migrate(
        &DatabaseOptions::new(&files.url).unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    let seed = Database::connect(&files.url).await.unwrap();
    TestDatabase { pool, seed, files }
}

async fn seed_probe_target(
    database: &DatabaseConnection,
    key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
    protocol: Protocol,
    proxy_required: bool,
    corrupt_aad: bool,
) {
    let header_override = json!({"x-probe-header": "probe-header-value-canary"}).to_string();
    let channel_type = match protocol {
        Protocol::Anthropic => "anthropic",
        Protocol::OpenAiChat
        | Protocol::OpenAiResponses
        | Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech => "openai",
        Protocol::JinaRerank => "jina",
        Protocol::CohereRerank => "cohere",
        Protocol::XaiVideo => "xai",
        Protocol::Gemini => "gemini",
    };
    let channel_sql = format!(
        "INSERT INTO channels (id, name, type, protocol, base_url, status, auto_ban, model_mapping, param_override, header_override, settings) VALUES ({TEST_CHANNEL_ID}, 'probe-channel', {}, {}, 'http://probe-upstream.example', {}, 1, '{{}}', '{{}}', {}, '{{}}')",
        sql_text(channel_type),
        sql_text(protocol.as_str()),
        Status::AutoDisabled.code(),
        sql_text(&header_override),
    );
    database.execute_unprepared(&channel_sql).await.unwrap();
    database
        .execute_unprepared(&format!(
            "INSERT INTO channel_models (channel_id, model) VALUES ({TEST_CHANNEL_ID}, {})",
            sql_text(MODEL_CANARY)
        ))
        .await
        .unwrap();

    let aad_credential_id = if corrupt_aad {
        TEST_CREDENTIAL_ID + 1
    } else {
        TEST_CREDENTIAL_ID
    };
    let envelope = encrypted_envelope(key, aad_credential_id);
    let proxy_id = if proxy_required { "91" } else { "NULL" };
    if proxy_required {
        database
            .execute_unprepared(
                "INSERT INTO proxies \
                 (id, name, active_name, scheme, host, port, trust_proxy_dns, enabled, version, created_at, updated_at) \
                 VALUES (91, 'probe-proxy', 'probe-proxy', 'http', 'proxy.example', 8080, 0, 1, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            )
            .await
            .unwrap();
    }
    let credential_sql = format!(
        "INSERT INTO credentials (id, channel_id, kind, secret, status, priority, schedulable, proxy_id) VALUES ({TEST_CREDENTIAL_ID}, {TEST_CHANNEL_ID}, 'api_key', {}, {}, 10, 1, {proxy_id})",
        sql_text(&envelope.to_string()),
        Status::Enabled.code(),
    );
    database.execute_unprepared(&credential_sql).await.unwrap();
}

async fn set_channel_settings(database: &DatabaseConnection, settings: serde_json::Value) {
    database
        .execute_unprepared(&format!(
            "UPDATE channels SET settings = {} WHERE id = {TEST_CHANNEL_ID}",
            sql_text(&settings.to_string())
        ))
        .await
        .unwrap();
}

async fn compact_probe_fact(database: &DatabaseConnection) -> (Option<String>, Option<u16>) {
    let row = database
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!(
                "SELECT json_extract(settings, '$.responses_compact_probe_result') AS result, \
                 json_extract(settings, '$.responses_compact_probe_http_status') AS http_status \
                 FROM channels WHERE id = {TEST_CHANNEL_ID}"
            ),
        ))
        .await
        .unwrap()
        .expect("测试渠道必须存在");
    let result = row.try_get::<Option<String>>("", "result").unwrap();
    let status = row
        .try_get::<Option<i64>>("", "http_status")
        .unwrap()
        .map(|status| u16::try_from(status).unwrap());
    (result, status)
}

fn encrypted_envelope(
    key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
    aad_credential_id: i64,
) -> serde_json::Value {
    let nonce = [0x5a; 24];
    let cipher = XChaCha20Poly1305::new_from_slice(key).unwrap();
    let plaintext = json!({"kind": "api_key", "api_key": SECRET_CANARY}).to_string();
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &credential_plaintext_aad(
                    channel_id(),
                    aad_credential_id,
                    CredentialKind::ApiKey,
                ),
            },
        )
        .unwrap();
    json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": "primary-key",
        "nonce": URL_SAFE_NO_PAD.encode(nonce),
        "ciphertext": URL_SAFE_NO_PAD.encode(ciphertext),
    })
}

fn probe(
    database: &DatabasePool,
    key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
    proxy_address: SocketAddr,
) -> DatabaseChannelProbe {
    probe_with_default_timeout(database, key, proxy_address, IO_TIMEOUT)
}

fn probe_with_default_timeout(
    database: &DatabasePool,
    key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
    proxy_address: SocketAddr,
    default_timeout: Duration,
) -> DatabaseChannelProbe {
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{proxy_address}")).unwrap(),
        HttpTimeouts::new(IO_TIMEOUT, default_timeout, default_timeout).unwrap(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    let clients = HttpClientProvider::new(config, 4).unwrap();
    DatabaseChannelProbe::new(database.clone(), decryptor(key), clients, default_timeout)
}

fn probe_without_reachable_upstream(
    database: &DatabasePool,
    key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
) -> DatabaseChannelProbe {
    let clients = HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap();
    DatabaseChannelProbe::new(database.clone(), decryptor(key), clients, IO_TIMEOUT)
}

fn decryptor(key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES]) -> CredentialDecryptor {
    let settings = serde_json::from_value::<CredentialEncryptionSettings>(json!({
        "key_id": "primary-key",
        "key": URL_SAFE_NO_PAD.encode(key),
    }))
    .unwrap();
    CredentialDecryptor::new(&settings).unwrap()
}

fn channel_id() -> ChannelId {
    ChannelId::new(TEST_CHANNEL_ID).unwrap()
}

fn sql_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn one_second_speech_wav() -> Vec<u8> {
    const SAMPLE_RATE: u32 = 24_000;
    const DATA_BYTES: u32 = SAMPLE_RATE * 2;
    let mut bytes = Vec::with_capacity((44 + DATA_BYTES) as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + DATA_BYTES).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&DATA_BYTES.to_le_bytes());
    bytes.resize((44 + DATA_BYTES) as usize, 0);
    bytes
}

struct TestDatabase {
    pool: DatabasePool,
    seed: DatabaseConnection,
    files: TestDatabaseFiles,
}

impl TestDatabase {
    async fn close(self) {
        self.seed.close().await.unwrap();
        self.pool.close().await.unwrap();
        drop(self.files);
    }
}

struct TestDatabaseFiles {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabaseFiles {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-channel-probe-{}-{serial}.db",
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
}

impl Drop for TestDatabaseFiles {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = std::path::PathBuf::from(format!("{}{}", self.path.display(), suffix));
            let _ = fs::remove_file(candidate);
        }
    }
}

struct CapturedRequest {
    head: String,
    body: Vec<u8>,
}

fn spawn_proxy(
    status_line: &str,
    body: &[u8],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    spawn_delayed_proxy(status_line, body, Duration::ZERO)
}

fn spawn_proxy_sequence<const N: usize>(
    responses: [(&str, &[u8]); N],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let responses = responses
        .into_iter()
        .map(|(status, body)| (status.to_owned(), body.to_vec()))
        .collect::<Vec<_>>();
    let (captured_tx, captured_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for (status_line, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
            captured_tx.send(read_request(&mut stream)).unwrap();
            let head = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            stream.flush().unwrap();
        }
    });
    (address, captured_rx, handle)
}

fn spawn_delayed_proxy(
    status_line: &str,
    body: &[u8],
    response_delay: Duration,
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let status_line = status_line.to_owned();
    let body = body.to_vec();
    let (captured_tx, captured_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        captured_tx.send(read_request(&mut stream)).unwrap();
        thread::sleep(response_delay);
        let head = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        stream.flush().unwrap();
    });
    (address, captured_rx, handle)
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut request = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 512];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "探活测试请求头尚未结束时连接已关闭");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 64 * 1024, "探活测试请求超过 64 KiB");
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
        .unwrap();
    while request.len() - header_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "探活测试请求体尚未结束时连接已关闭");
        request.extend_from_slice(&buffer[..read]);
    }
    CapturedRequest {
        head,
        body: request[header_end..header_end + content_length].to_vec(),
    }
}
