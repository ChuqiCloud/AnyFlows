pub mod support;

use std::{
    fs,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::TryRecvError,
    },
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_http::{REQUEST_ID_HEADER_NAME, ServeOutcome};
use af_server::{Bootstrap, SupervisorShutdown};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::{Value, json};
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, RawResponse, TOKEN_ID, UPSTREAM_MODEL, USER_ID,
    credential_encryption_key, seed_runtime_channel, send_request, spawn_proxy,
};
use tokio::{sync::oneshot, time::timeout};

const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const FORWARDED_CLIENT_ALLOWLIST: &str = r#"["198.51.100.42/32"]"#;
const ALLOWED_FORWARDED_CLIENT_IP: &str = "198.51.100.42";
const DENIED_FORWARDED_CLIENT_IP: &str = "203.0.113.11";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-trusted-proxy-auth-{}-{serial}.db",
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
async fn trusted_loopback_proxy_enforces_sqlite_token_ip_allowlist() {
    let upstream_body = serde_json::to_vec(&json!({
        "id": "chatcmpl-trusted-proxy",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": UPSTREAM_MODEL,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "allowed" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    }))
    .unwrap();
    let database = TestDatabase::new();
    seed_valid_token(&database.url).await;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        &upstream_body,
        &[("Content-Type", "application/json")],
    );
    let config_path = std::env::temp_dir().join(format!(
        "anyflows-trusted-proxy-token-auth-{}.toml",
        std::process::id()
    ));
    let credential_key = credential_encryption_key();
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\ncors_allowed_origins = ['https://console.example']\nshutdown_timeout_secs = 1\nclient_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = ['127.0.0.1/32']\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'gateway-test-key'\nkey = '{credential_key}'\n[auth]\nlookup_timeout_secs = 1\nsession_signing_key = '{credential_key}'\n[http_client]\nconnect_timeout_secs = 5\nread_timeout_secs = 5\nrequest_timeout_secs = 5\nproxy_url = 'http://{proxy_address}'\ntrust_proxy_dns = true\n",
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
    let inbound = br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}]}"#;
    let authorization = format!("Bearer {CLIENT_KEY}");

    // 先证明白名单拒绝不会触发上游，再用允许地址完成一次真实转发。
    let denied = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
            ("X-Forwarded-For", DENIED_FORWARDED_CLIENT_IP),
        ],
    )
    .await;
    assert_invalid_api_key(&denied);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    let allowed = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
            ("X-Forwarded-For", ALLOWED_FORWARDED_CLIENT_IP),
        ],
    )
    .await;
    assert_eq!(allowed.status, 200);
    assert_eq!(allowed.body, upstream_body);
    assert_eq!(
        allowed.headers["access-control-allow-origin"],
        "https://console.example"
    );
    assert_no_auth_canaries(&allowed);

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();
    let upstream_rendered = format!(
        "{}\n{}",
        upstream.head,
        String::from_utf8_lossy(&upstream.body)
    )
    .to_ascii_lowercase();
    for forbidden in [
        "x-forwarded-for",
        FORWARDED_CLIENT_ALLOWLIST,
        ALLOWED_FORWARDED_CLIENT_IP,
        DENIED_FORWARDED_CLIENT_IP,
    ] {
        assert!(!upstream_rendered.contains(forbidden));
    }
}

async fn seed_valid_token(database_url: &str) {
    let pool = connect_and_migrate(
        &DatabaseOptions::new(database_url).unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    pool.close().await.unwrap();

    let connection = Database::connect(database_url).await.unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            [
                GROUP_ID.into(),
                "default".into(),
                "Default".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO model_prices (model, billing_mode, input_price, output_price, cache_read_price, cache_creation_5m_price, cache_creation_1h_price, version) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [
                "test-model".into(),
                1_i16.into(),
                "0.000001".into(),
                "0.000001".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                1_i64.into(),
            ],
        ))
        .await
        .unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, status, default_group_id, quota, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                USER_ID.into(),
                "gateway-user".into(),
                1_i16.into(),
                GROUP_ID.into(),
                1_000_000_000_i64.into(),
                "gateway-aff".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, remain_quota, allow_ips) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [
                TOKEN_ID.into(),
                USER_ID.into(),
                CLIENT_KEY_DIGEST.into(),
                "sk-af-AAECAwQFBgcI".into(),
                "gateway-token".into(),
                1_i16.into(),
                1_000_000_000_i64.into(),
                FORWARDED_CLIENT_ALLOWLIST.into(),
            ],
        ))
        .await
        .unwrap();
    seed_runtime_channel(&connection, GROUP_ID).await;
    connection.close().await.unwrap();
}

fn assert_invalid_api_key(response: &RawResponse) {
    assert_eq!(response.status, 401);
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(value["error"]["code"], "invalid_api_key");
    assert_eq!(
        response.headers["access-control-allow-origin"],
        "https://console.example"
    );
    assert!(response.headers.contains_key(REQUEST_ID_HEADER_NAME));
    assert_no_auth_canaries(response);
}

fn assert_no_auth_canaries(response: &RawResponse) {
    let rendered = format!(
        "{:?}\n{}",
        response.headers,
        String::from_utf8_lossy(&response.body)
    );
    for forbidden in [
        CLIENT_KEY,
        CLIENT_KEY_DIGEST,
        FORWARDED_CLIENT_ALLOWLIST,
        ALLOWED_FORWARDED_CLIENT_IP,
        DENIED_FORWARDED_CLIENT_IP,
    ] {
        assert!(!rendered.contains(forbidden));
    }
    for forbidden in [TOKEN_ID, USER_ID, GROUP_ID].map(|value| value.to_string()) {
        assert!(!rendered.contains(&forbidden));
    }
}
