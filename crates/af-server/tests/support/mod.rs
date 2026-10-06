use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream as StdTcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use af_account::credential_plaintext_aad;
use af_config::CREDENTIAL_ENCRYPTION_KEY_BYTES;
use af_domain::{ChannelId, CredentialKind};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

pub const IO_TIMEOUT: Duration = Duration::from_secs(15);
pub const UPSTREAM_BASE_URL: &str = "http://upstream.example/proxy";
pub const UPSTREAM_MODEL: &str = "test-model";
pub const UPSTREAM_KEY: &str = "configured-upstream-key";
pub const CLIENT_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
pub const TOKEN_ID: i64 = 8_100_001;
pub const USER_ID: i64 = 8_100_002;
pub const GROUP_ID: i64 = 8_100_003;
pub const CHANNEL_ID: i64 = 8_100_004;
pub const CREDENTIAL_ID: i64 = 8_100_005;
pub const ANTHROPIC_CHANNEL_ID: i64 = 8_100_006;
pub const ANTHROPIC_CREDENTIAL_ID: i64 = 8_100_007;
pub const GEMINI_CHANNEL_ID: i64 = 8_100_008;
pub const GEMINI_CREDENTIAL_ID: i64 = 8_100_009;
pub const OPENAI_RESPONSES_CHANNEL_ID: i64 = 8_100_010;
pub const OPENAI_RESPONSES_CREDENTIAL_ID: i64 = 8_100_011;
pub const OPENAI_EMBEDDINGS_CHANNEL_ID: i64 = 8_100_012;
pub const OPENAI_EMBEDDINGS_CREDENTIAL_ID: i64 = 8_100_013;
pub const OPENAI_IMAGES_CHANNEL_ID: i64 = 8_100_014;
pub const OPENAI_IMAGES_CREDENTIAL_ID: i64 = 8_100_015;
pub const OPENAI_AUDIO_CHANNEL_ID: i64 = 8_100_016;
pub const OPENAI_AUDIO_CREDENTIAL_ID: i64 = 8_100_017;
pub const OPENAI_SPEECH_CHANNEL_ID: i64 = 8_100_018;
pub const OPENAI_SPEECH_CREDENTIAL_ID: i64 = 8_100_019;
pub const JINA_RERANK_CHANNEL_ID: i64 = 8_100_020;
pub const JINA_RERANK_CREDENTIAL_ID: i64 = 8_100_021;
pub const COHERE_RERANK_CHANNEL_ID: i64 = 8_100_022;
pub const COHERE_RERANK_CREDENTIAL_ID: i64 = 8_100_023;
pub const RUNTIME_HEADER_NAME: &str = "x-runtime-header";
pub const RUNTIME_HEADER_VALUE: &str = "runtime-header-value";
const CREDENTIAL_KEY_ID: &str = "gateway-test-key";

struct OpenAiRuntimeChannel<'a> {
    channel_id: i64,
    credential_id: i64,
    name: &'a str,
    protocol: &'a str,
    base_url: &'a str,
    public_model: &'a str,
    upstream_model: &'a str,
    api_key: &'a str,
    timeout_secs: Option<i32>,
}

/// 返回真实 TCP 回归使用的 32 字节 Base64URL 凭据解密密钥。
pub fn credential_encryption_key() -> String {
    URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
}

/// 写入一条可由生产运行时目录选中的 OpenAI Chat 渠道和加密凭据。
pub async fn seed_runtime_channel(connection: &DatabaseConnection, group_id: i64) {
    seed_openai_runtime_channel_with(
        connection,
        group_id,
        OpenAiRuntimeChannel {
            channel_id: CHANNEL_ID,
            credential_id: CREDENTIAL_ID,
            name: "runtime-openai",
            protocol: "openai_chat",
            base_url: UPSTREAM_BASE_URL,
            public_model: UPSTREAM_MODEL,
            upstream_model: UPSTREAM_MODEL,
            api_key: UPSTREAM_KEY,
            timeout_secs: None,
        },
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入 OpenAI Responses 测试渠道。
pub async fn seed_openai_responses_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_openai_runtime_channel_with(
        connection,
        group_id,
        OpenAiRuntimeChannel {
            channel_id: OPENAI_RESPONSES_CHANNEL_ID,
            credential_id: OPENAI_RESPONSES_CREDENTIAL_ID,
            name: "runtime-openai-responses",
            protocol: "openai_responses",
            base_url,
            public_model,
            upstream_model,
            api_key,
            timeout_secs,
        },
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入 OpenAI Embeddings 测试渠道。
pub async fn seed_openai_embeddings_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_openai_runtime_channel_with(
        connection,
        group_id,
        OpenAiRuntimeChannel {
            channel_id: OPENAI_EMBEDDINGS_CHANNEL_ID,
            credential_id: OPENAI_EMBEDDINGS_CREDENTIAL_ID,
            name: "runtime-openai-embeddings",
            protocol: "openai_embeddings",
            base_url,
            public_model,
            upstream_model,
            api_key,
            timeout_secs,
        },
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入 OpenAI Images 测试渠道。
pub async fn seed_openai_images_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_openai_runtime_channel_with(
        connection,
        group_id,
        OpenAiRuntimeChannel {
            channel_id: OPENAI_IMAGES_CHANNEL_ID,
            credential_id: OPENAI_IMAGES_CREDENTIAL_ID,
            name: "runtime-openai-images",
            protocol: "openai_images",
            base_url,
            public_model,
            upstream_model,
            api_key,
            timeout_secs,
        },
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入 OpenAI Audio 测试渠道。
pub async fn seed_openai_audio_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_openai_runtime_channel_with(
        connection,
        group_id,
        OpenAiRuntimeChannel {
            channel_id: OPENAI_AUDIO_CHANNEL_ID,
            credential_id: OPENAI_AUDIO_CREDENTIAL_ID,
            name: "runtime-openai-audio",
            protocol: "openai_audio",
            base_url,
            public_model,
            upstream_model,
            api_key,
            timeout_secs,
        },
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入 OpenAI Speech 测试渠道。
pub async fn seed_openai_speech_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_openai_runtime_channel_with(
        connection,
        group_id,
        OpenAiRuntimeChannel {
            channel_id: OPENAI_SPEECH_CHANNEL_ID,
            credential_id: OPENAI_SPEECH_CREDENTIAL_ID,
            name: "runtime-openai-speech",
            protocol: "openai_speech",
            base_url,
            public_model,
            upstream_model,
            api_key,
            timeout_secs,
        },
    )
    .await;
}

async fn seed_openai_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    channel: OpenAiRuntimeChannel<'_>,
) {
    let model_mapping = if channel.public_model == channel.upstream_model {
        "{}".to_owned()
    } else {
        serde_json::Value::Object(serde_json::Map::from_iter([(
            channel.public_model.to_owned(),
            serde_json::Value::String(channel.upstream_model.to_owned()),
        )]))
        .to_string()
    };
    for (sql, values) in [
        (
            "INSERT INTO channels (id, name, type, protocol, base_url, timeout_secs, status, model_mapping, param_override, header_override, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                channel.channel_id.into(),
                channel.name.into(),
                "openai".into(),
                channel.protocol.into(),
                channel.base_url.into(),
                channel.timeout_secs.into(),
                1_i16.into(),
                model_mapping.clone().into(),
                "{}".into(),
                json!({"x-runtime-header": RUNTIME_HEADER_VALUE})
                    .to_string()
                    .into(),
                "{}".into(),
            ],
        ),
        (
            "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
            vec![channel.channel_id.into(), channel.public_model.into()],
        ),
        (
            "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
            vec![channel.channel_id.into(), group_id.into()],
        ),
        (
            "INSERT INTO abilities (group_id, model, channel_id, enabled, priority, weight) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                group_id.into(),
                channel.public_model.into(),
                channel.channel_id.into(),
                true.into(),
                10_i32.into(),
                0_i32.into(),
            ],
        ),
        (
            "INSERT INTO credentials (id, channel_id, kind, secret, status, priority, schedulable) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                channel.credential_id.into(),
                channel.channel_id.into(),
                "api_key".into(),
                encrypted_api_key_for(channel.channel_id, channel.credential_id, channel.api_key)
                    .into(),
                1_i16.into(),
                10_i32.into(),
                true.into(),
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
}

/// 按显式地址、模型映射和凭据写入 Jina Rerank 测试渠道。
pub async fn seed_jina_rerank_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_rerank_runtime_channel_with(
        connection,
        group_id,
        JINA_RERANK_CHANNEL_ID,
        JINA_RERANK_CREDENTIAL_ID,
        "runtime-jina-rerank",
        "jina",
        "jina_rerank",
        base_url,
        public_model,
        upstream_model,
        api_key,
        timeout_secs,
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入 Cohere v2 Rerank 测试渠道。
pub async fn seed_cohere_rerank_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    seed_rerank_runtime_channel_with(
        connection,
        group_id,
        COHERE_RERANK_CHANNEL_ID,
        COHERE_RERANK_CREDENTIAL_ID,
        "runtime-cohere-rerank",
        "cohere",
        "cohere_rerank",
        base_url,
        public_model,
        upstream_model,
        api_key,
        timeout_secs,
    )
    .await;
}

#[allow(
    clippy::too_many_arguments,
    reason = "参数与测试渠道持久化字段一一对应"
)]
async fn seed_rerank_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    channel_id: i64,
    credential_id: i64,
    channel_name: &str,
    channel_type: &str,
    protocol: &str,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    let model_mapping = if public_model == upstream_model {
        "{}".to_owned()
    } else {
        serde_json::Value::Object(serde_json::Map::from_iter([(
            public_model.to_owned(),
            serde_json::Value::String(upstream_model.to_owned()),
        )]))
        .to_string()
    };
    for (sql, values) in [
        (
            "INSERT INTO channels (id, name, type, protocol, base_url, timeout_secs, status, model_mapping, param_override, header_override, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                channel_id.into(),
                channel_name.into(),
                channel_type.into(),
                protocol.into(),
                base_url.into(),
                timeout_secs.into(),
                1_i16.into(),
                model_mapping.into(),
                "{}".into(),
                json!({"x-runtime-header": RUNTIME_HEADER_VALUE})
                    .to_string()
                    .into(),
                "{}".into(),
            ],
        ),
        (
            "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
            vec![channel_id.into(), public_model.into()],
        ),
        (
            "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
            vec![channel_id.into(), group_id.into()],
        ),
        (
            "INSERT INTO abilities (group_id, model, channel_id, enabled, priority, weight) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                group_id.into(),
                public_model.into(),
                channel_id.into(),
                true.into(),
                10_i32.into(),
                0_i32.into(),
            ],
        ),
        (
            "INSERT INTO credentials (id, channel_id, kind, secret, status, priority, schedulable) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                credential_id.into(),
                channel_id.into(),
                "api_key".into(),
                encrypted_api_key_for(channel_id, credential_id, api_key).into(),
                1_i16.into(),
                10_i32.into(),
                true.into(),
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
}

/// 写入一条标准 Anthropic Messages 渠道，用于生产调度的真实 TCP 回归。
pub async fn seed_anthropic_runtime_channel(connection: &DatabaseConnection, group_id: i64) {
    seed_anthropic_runtime_channel_with(
        connection,
        group_id,
        UPSTREAM_BASE_URL,
        UPSTREAM_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_KEY,
        None,
    )
    .await;
}

/// 按显式地址、模型映射和凭据写入标准 Anthropic 测试渠道。
pub async fn seed_anthropic_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    let model_mapping = if public_model == upstream_model {
        "{}".to_owned()
    } else {
        serde_json::Value::Object(serde_json::Map::from_iter([(
            public_model.to_owned(),
            serde_json::Value::String(upstream_model.to_owned()),
        )]))
        .to_string()
    };
    for (sql, values) in [
        (
            "INSERT INTO channels (id, name, type, protocol, base_url, timeout_secs, status, model_mapping, param_override, header_override, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                ANTHROPIC_CHANNEL_ID.into(),
                "runtime-anthropic".into(),
                "anthropic".into(),
                "anthropic".into(),
                base_url.into(),
                timeout_secs.into(),
                1_i16.into(),
                model_mapping.clone().into(),
                "{}".into(),
                json!({"x-runtime-header": RUNTIME_HEADER_VALUE})
                    .to_string()
                    .into(),
                "{}".into(),
            ],
        ),
        (
            "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
            vec![ANTHROPIC_CHANNEL_ID.into(), public_model.into()],
        ),
        (
            "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
            vec![ANTHROPIC_CHANNEL_ID.into(), group_id.into()],
        ),
        (
            "INSERT INTO abilities (group_id, model, channel_id, enabled, priority, weight) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                group_id.into(),
                public_model.into(),
                ANTHROPIC_CHANNEL_ID.into(),
                true.into(),
                10_i32.into(),
                0_i32.into(),
            ],
        ),
        (
            "INSERT INTO credentials (id, channel_id, kind, secret, status, priority, schedulable) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                ANTHROPIC_CREDENTIAL_ID.into(),
                ANTHROPIC_CHANNEL_ID.into(),
                "api_key".into(),
                encrypted_api_key_for(ANTHROPIC_CHANNEL_ID, ANTHROPIC_CREDENTIAL_ID, api_key)
                    .into(),
                1_i16.into(),
                10_i32.into(),
                true.into(),
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
}

/// 按显式地址、模型映射和凭据写入标准 Gemini Developer API 测试渠道。
pub async fn seed_gemini_runtime_channel_with(
    connection: &DatabaseConnection,
    group_id: i64,
    base_url: &str,
    public_model: &str,
    upstream_model: &str,
    api_key: &str,
    timeout_secs: Option<i32>,
) {
    let model_mapping = if public_model == upstream_model {
        "{}".to_owned()
    } else {
        serde_json::Value::Object(serde_json::Map::from_iter([(
            public_model.to_owned(),
            serde_json::Value::String(upstream_model.to_owned()),
        )]))
        .to_string()
    };
    for (sql, values) in [
        (
            "INSERT INTO channels (id, name, type, protocol, base_url, timeout_secs, status, model_mapping, param_override, header_override, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                GEMINI_CHANNEL_ID.into(),
                "runtime-gemini".into(),
                "gemini".into(),
                "gemini".into(),
                base_url.into(),
                timeout_secs.into(),
                1_i16.into(),
                model_mapping.clone().into(),
                "{}".into(),
                json!({"x-runtime-header": RUNTIME_HEADER_VALUE})
                    .to_string()
                    .into(),
                "{}".into(),
            ],
        ),
        (
            "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
            vec![GEMINI_CHANNEL_ID.into(), public_model.into()],
        ),
        (
            "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
            vec![GEMINI_CHANNEL_ID.into(), group_id.into()],
        ),
        (
            "INSERT INTO abilities (group_id, model, channel_id, enabled, priority, weight) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                group_id.into(),
                public_model.into(),
                GEMINI_CHANNEL_ID.into(),
                true.into(),
                10_i32.into(),
                0_i32.into(),
            ],
        ),
        (
            "INSERT INTO credentials (id, channel_id, kind, secret, status, priority, schedulable) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                GEMINI_CREDENTIAL_ID.into(),
                GEMINI_CHANNEL_ID.into(),
                "api_key".into(),
                encrypted_api_key_for(GEMINI_CHANNEL_ID, GEMINI_CREDENTIAL_ID, api_key).into(),
                1_i16.into(),
                10_i32.into(),
                true.into(),
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
}

fn encrypted_api_key_for(channel_id: i64, credential_id: i64, api_key: &str) -> String {
    let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
    let nonce = [0x24; 24];
    let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
    let plaintext = json!({"kind": "api_key", "api_key": api_key}).to_string();
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &credential_plaintext_aad(
                    ChannelId::new(channel_id).unwrap(),
                    credential_id,
                    CredentialKind::ApiKey,
                ),
            },
        )
        .unwrap();
    json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": CREDENTIAL_KEY_ID,
        "nonce": URL_SAFE_NO_PAD.encode(nonce),
        "ciphertext": URL_SAFE_NO_PAD.encode(ciphertext),
    })
    .to_string()
}

#[derive(Debug)]
pub struct CapturedRequest {
    pub head: String,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// 从 JSON 错误响应中提取受限错误码，供真实联调失败诊断使用。
///
/// 仅保留短 ASCII 标识，错误消息、正文和控制字符永不进入构建日志。
pub fn sanitized_json_error_code(body: &[u8]) -> Option<String> {
    if body.len() > 64 * 1024 {
        return None;
    }
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let code = ["/code", "/error/code", "/response/error/code"]
        .into_iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_str))?;
    let code = code.trim();
    if !code.is_empty()
        && code.len() <= 96
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Some(code.to_owned())
    } else {
        Some("无效错误码".to_owned())
    }
}

/// 通过原始 TCP 请求测试服务，避免客户端自动改写协议边界。
pub async fn send_request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> RawResponse {
    send_request_with_timeout(address, method, path, body, headers, IO_TIMEOUT).await
}

/// 发送一次原始 HTTP 请求，并允许真实供应商联调使用更长的响应等待上限。
pub async fn send_request_with_timeout(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
    headers: &[(&str, &str)],
    response_timeout: Duration,
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
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"))
    {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("Connection: close\r\n");
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
    timeout(response_timeout, stream.read_to_end(&mut bytes))
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

/// 启动只处理一次请求的本地代理，并返回完整的上游请求快照。
pub fn spawn_proxy(
    status_line: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    spawn_delayed_proxy(status_line, body, headers, Duration::ZERO)
}

/// 启动只处理一次请求且可延迟响应头的本地代理。
pub fn spawn_delayed_proxy(
    status_line: &str,
    body: &[u8],
    headers: &[(&str, &str)],
    response_delay: Duration,
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
        thread::sleep(response_delay);

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

/// 启动分两段发送的 SSE 上游；第二段只在测试显式放行后写出。
pub fn spawn_streaming_proxy(
    first: &[u8],
    rest: &[u8],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let first = first.to_vec();
    let rest = rest.to_vec();
    let (captured_tx, captured_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        captured_tx.send(read_proxy_request(&mut stream)).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        stream.write_all(&first).unwrap();
        stream.flush().unwrap();
        release_rx.recv_timeout(IO_TIMEOUT).unwrap();
        stream.write_all(&rest).unwrap();
        stream.flush().unwrap();
    });
    (address, captured_rx, release_tx, handle)
}

/// 启动在首段 SSE 后等待连接关闭的上游，用于验证客户端断连传播。
pub fn spawn_abort_observing_proxy(
    first: &[u8],
) -> (
    SocketAddr,
    mpsc::Receiver<CapturedRequest>,
    mpsc::Receiver<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let first = first.to_vec();
    let (captured_tx, captured_rx) = mpsc::channel();
    let (aborted_tx, aborted_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        captured_tx.send(read_proxy_request(&mut stream)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
            .unwrap();
        stream.write_all(&first).unwrap();
        stream.flush().unwrap();

        let mut byte = [0_u8; 1];
        assert_eq!(stream.read(&mut byte).unwrap(), 0, "上游连接未随客户端断开");
        aborted_tx.send(()).unwrap();
    });
    (address, captured_rx, aborted_rx, handle)
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
