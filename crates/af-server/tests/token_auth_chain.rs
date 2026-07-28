pub mod support;

use std::{
    fs,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::TryRecvError,
    },
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_http::{
    DEFAULT_REQUEST_BODY_LIMIT_BYTES, HEALTH_PATH, READINESS_PATH, REQUEST_ID_HEADER_NAME,
    ServeOutcome,
};
use af_server::{Bootstrap, SupervisorShutdown};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement, Value as DatabaseValue};
use serde_json::{Value, json};
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, RUNTIME_HEADER_NAME, RUNTIME_HEADER_VALUE, RawResponse,
    TOKEN_ID, UPSTREAM_MODEL, USER_ID, credential_encryption_key, seed_runtime_channel,
    send_request, spawn_proxy,
};
use tokio::{sync::oneshot, time::timeout};

const UNKNOWN_CLIENT_KEY: &str = "sk-af--_________________________________________8";
const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const BOUNDARY_KEY_CANARY: &str = "boundary-key-canary";
const LOOPBACK_ALLOWLIST: &str = r#"["127.0.0.1/32"]"#;
const TRUSTED_CLIENT_IP: &str = "127.0.0.1";
const SPOOFED_CLIENT_IP: &str = "198.51.100.7";
const MODEL_POLICY_CANARY: &str = "policy-only-private-model";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-token-auth-{}-{serial}.db",
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
async fn full_chain_preserves_verified_request_and_response_bytes() {
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
    // 先完成迁移和种子写入，再启动带硬截止时间的本地代理监听线程。
    let database = TestDatabase::new();
    seed_valid_token(&database.url).await;
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
    let credential_key = credential_encryption_key();
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\ncors_allowed_origins = ['https://console.example']\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'gateway-test-key'\nkey = '{credential_key}'\n[auth]\nlookup_timeout_secs = 1\nsession_signing_key = '{credential_key}'\n[http_client]\nconnect_timeout_secs = 5\nread_timeout_secs = 5\nrequest_timeout_secs = 5\nproxy_url = 'http://{proxy_address}'\ntrust_proxy_dns = true\n",
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

    for (path, expected_body) in [
        (HEALTH_PATH, br#"{"status":"ok"}"#.as_slice()),
        (READINESS_PATH, br#"{"status":"ready"}"#.as_slice()),
    ] {
        let probe = send_request(
            address,
            "GET",
            path,
            b"",
            &[
                ("Origin", "https://console.example"),
                ("Authorization", "Bearer deliberately-invalid-probe-key"),
            ],
        )
        .await;
        assert_eq!(probe.status, 200);
        assert_eq!(probe.body, expected_body);
        assert_eq!(
            probe.headers["content-type"],
            "application/json; charset=utf-8"
        );
        assert_eq!(probe.headers["cache-control"], "no-store");
        assert!(!probe.headers.contains_key("access-control-allow-origin"));
        assert!(probe.headers.contains_key(REQUEST_ID_HEADER_NAME));
    }
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    let boundary_authorization = format!("Bearer {BOUNDARY_KEY_CANARY}");
    let preflight = send_request(
        address,
        "OPTIONS",
        "/v1/chat/completions",
        b"",
        &[
            ("Origin", "https://console.example"),
            ("Access-Control-Request-Method", "POST"),
            ("Authorization", &boundary_authorization),
        ],
    )
    .await;
    assert_eq!(preflight.status, 200);

    let not_found = send_request(
        address,
        "POST",
        "/not-found",
        b"",
        &[("Authorization", &boundary_authorization)],
    )
    .await;
    assert_eq!(not_found.status, 404);

    let method_not_allowed = send_request(
        address,
        "GET",
        "/v1/chat/completions",
        b"",
        &[("Authorization", &boundary_authorization)],
    )
    .await;
    assert_eq!(method_not_allowed.status, 405);

    let oversized_length = (DEFAULT_REQUEST_BODY_LIMIT_BYTES + 1).to_string();
    let too_large = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        b"",
        &[
            ("Content-Length", &oversized_length),
            ("Origin", "https://console.example"),
            ("Authorization", &boundary_authorization),
        ],
    )
    .await;
    assert_eq!(too_large.status, 413);
    for response in [&preflight, &not_found, &method_not_allowed, &too_large] {
        assert!(response.headers.contains_key(REQUEST_ID_HEADER_NAME));
        assert_no_auth_canaries(response);
    }
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    let inbound = br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}]}"#;
    let unknown_authorization = format!("Bearer {UNKNOWN_CLIENT_KEY}");
    let unknown = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &unknown_authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    assert_invalid_api_key(&unknown);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    let authorization = format!("Bearer {CLIENT_KEY}");
    execute_database_statement(
        &database.url,
        "UPDATE tokens SET allow_ips = ? WHERE id = ?",
        vec![r#"["198.51.100.0/24"]"#.into(), TOKEN_ID.into()],
    )
    .await;
    let spoofed = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
            ("X-Forwarded-For", SPOOFED_CLIENT_IP),
        ],
    )
    .await;
    execute_database_statement(
        &database.url,
        "UPDATE tokens SET allow_ips = ? WHERE id = ?",
        vec![LOOPBACK_ALLOWLIST.into(), TOKEN_ID.into()],
    )
    .await;
    assert_invalid_api_key(&spoofed);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    execute_database_statement(
        &database.url,
        "UPDATE tokens SET allow_ips = ? WHERE id = ?",
        vec!["null".into(), TOKEN_ID.into()],
    )
    .await;
    let invalid_allowlist = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    execute_database_statement(
        &database.url,
        "UPDATE tokens SET allow_ips = ? WHERE id = ?",
        vec![LOOPBACK_ALLOWLIST.into(), TOKEN_ID.into()],
    )
    .await;
    assert_internal_error(&invalid_allowlist);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    execute_database_statement(
        &database.url,
        "UPDATE tokens SET model_limits = ? WHERE id = ?",
        vec![
            json!([MODEL_POLICY_CANARY]).to_string().into(),
            TOKEN_ID.into(),
        ],
    )
    .await;
    let malformed_before_policy = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        br#"{"model":"test-model","model":"other-model","messages":[]}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    assert_invalid_request(&malformed_before_policy);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    let denied_model = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    assert_model_not_found(&denied_model);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    execute_database_statement(
        &database.url,
        "UPDATE tokens SET model_limits = ? WHERE id = ?",
        vec![
            json!([UPSTREAM_MODEL.to_ascii_uppercase()])
                .to_string()
                .into(),
            TOKEN_ID.into(),
        ],
    )
    .await;
    let case_mismatch = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    assert_model_not_found(&case_mismatch);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    // 损坏模型策略必须先于禁用态失败为内部错误，不能伪装成普通 401。
    execute_database_statement(
        &database.url,
        "UPDATE tokens SET status = ?, model_limits = ? WHERE id = ?",
        vec![2_i16.into(), "null".into(), TOKEN_ID.into()],
    )
    .await;
    let invalid_model_policy = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    assert_internal_error(&invalid_model_policy);
    assert!(matches!(captured.try_recv(), Err(TryRecvError::Empty)));

    execute_database_statement(
        &database.url,
        "UPDATE tokens SET status = ?, model_limits = ? WHERE id = ?",
        vec![
            1_i16.into(),
            json!([UPSTREAM_MODEL, MODEL_POLICY_CANARY, UPSTREAM_MODEL])
                .to_string()
                .into(),
            TOKEN_ID.into(),
        ],
    )
    .await;

    let response = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Cookie", "client-cookie-secret=1"),
            ("X-Request-ID", "client-selected-request-id"),
            ("Origin", "https://console.example"),
            ("X-Forwarded-For", SPOOFED_CLIENT_IP),
            ("Forwarded", "for=198.51.100.7"),
        ],
    )
    .await;
    let rejected_mutations = [
        (
            "UPDATE tokens SET status = ? WHERE id = ?",
            vec![2_i16.into(), TOKEN_ID.into()],
            "UPDATE tokens SET status = ? WHERE id = ?",
            vec![1_i16.into(), TOKEN_ID.into()],
        ),
        (
            "UPDATE tokens SET deleted_at = CURRENT_TIMESTAMP WHERE id = ?",
            vec![TOKEN_ID.into()],
            "UPDATE tokens SET deleted_at = NULL WHERE id = ?",
            vec![TOKEN_ID.into()],
        ),
        (
            "UPDATE tokens SET expired_at = CURRENT_TIMESTAMP WHERE id = ?",
            vec![TOKEN_ID.into()],
            "UPDATE tokens SET expired_at = NULL WHERE id = ?",
            vec![TOKEN_ID.into()],
        ),
        (
            "UPDATE users SET status = ? WHERE id = ?",
            vec![2_i16.into(), USER_ID.into()],
            "UPDATE users SET status = ? WHERE id = ?",
            vec![1_i16.into(), USER_ID.into()],
        ),
        (
            "UPDATE users SET deleted_at = CURRENT_TIMESTAMP WHERE id = ?",
            vec![USER_ID.into()],
            "UPDATE users SET deleted_at = NULL WHERE id = ?",
            vec![USER_ID.into()],
        ),
        (
            "UPDATE groups SET deleted_at = CURRENT_TIMESTAMP WHERE id = ?",
            vec![GROUP_ID.into()],
            "UPDATE groups SET deleted_at = NULL WHERE id = ?",
            vec![GROUP_ID.into()],
        ),
    ];
    for (apply_sql, apply_values, restore_sql, restore_values) in rejected_mutations {
        assert_rejected_after_mutation(
            &database.url,
            address,
            &authorization,
            inbound,
            (apply_sql, apply_values),
            (restore_sql, restore_values),
        )
        .await;
    }
    drop_tokens_table(&database.url).await;
    let database_failure = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    assert_internal_error(&database_failure);
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
    assert_no_auth_canaries(&response);

    let head_lower = upstream.head.to_ascii_lowercase();
    let identity_canaries = [
        TOKEN_ID.to_string(),
        USER_ID.to_string(),
        GROUP_ID.to_string(),
    ];
    let client_key_lower = CLIENT_KEY.to_ascii_lowercase();
    assert!(
        upstream
            .head
            .starts_with("POST http://upstream.example/proxy/v1/chat/completions HTTP/1.1")
    );
    assert!(head_lower.contains("authorization: bearer configured-upstream-key"));
    assert!(head_lower.contains(&format!("x-request-id: {request_id}").to_ascii_lowercase()));
    assert!(head_lower.contains(&format!("{RUNTIME_HEADER_NAME}: {RUNTIME_HEADER_VALUE}")));
    for forbidden in [
        CLIENT_KEY_DIGEST,
        "client-cookie-secret",
        "client-selected-request-id",
        "origin: https://console.example",
        "x-forwarded-for",
        "forwarded:",
        LOOPBACK_ALLOWLIST,
        TRUSTED_CLIENT_IP,
        SPOOFED_CLIENT_IP,
        MODEL_POLICY_CANARY,
    ] {
        assert!(!head_lower.contains(forbidden));
    }
    assert!(!head_lower.contains(&client_key_lower));
    for forbidden in &identity_canaries {
        assert!(!head_lower.contains(forbidden));
    }
    assert_eq!(upstream.body, inbound);
    let outbound: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(outbound["model"], UPSTREAM_MODEL);
    assert!(outbound.get("stream").is_none());
    assert_eq!(outbound["messages"][0]["content"], "hello");
    let outbound_text = String::from_utf8_lossy(&upstream.body);
    let response_text = String::from_utf8_lossy(&response.body);
    let response_headers = format!("{:?}", response.headers);
    for forbidden in [CLIENT_KEY, CLIENT_KEY_DIGEST] {
        assert!(!outbound_text.contains(forbidden));
        assert!(!response_text.contains(forbidden));
        assert!(!response_headers.contains(forbidden));
    }
    for forbidden in &identity_canaries {
        assert!(!outbound_text.contains(forbidden));
        assert!(!response_text.contains(forbidden));
        assert!(!response_headers.contains(forbidden));
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
                LOOPBACK_ALLOWLIST.into(),
            ],
        ))
        .await
        .unwrap();
    seed_runtime_channel(&connection, GROUP_ID).await;
    connection.close().await.unwrap();
}

async fn execute_database_statement(database_url: &str, sql: &str, values: Vec<DatabaseValue>) {
    let connection = Database::connect(database_url).await.unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .unwrap();
    connection.close().await.unwrap();
}

async fn assert_rejected_after_mutation(
    database_url: &str,
    address: SocketAddr,
    authorization: &str,
    inbound: &[u8],
    apply: (&str, Vec<DatabaseValue>),
    restore: (&str, Vec<DatabaseValue>),
) {
    execute_database_statement(database_url, apply.0, apply.1).await;
    let response = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        inbound,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", authorization),
            ("Origin", "https://console.example"),
        ],
    )
    .await;
    execute_database_statement(database_url, restore.0, restore.1).await;
    assert_invalid_api_key(&response);
}

async fn drop_tokens_table(database_url: &str) {
    let connection = Database::connect(database_url).await.unwrap();
    // 成功计费请求已经产生关联预留；故障注入只需要破坏认证查询，不验证外键删除策略。
    connection
        .execute_unprepared("PRAGMA foreign_keys = OFF")
        .await
        .unwrap();
    connection
        .execute_unprepared("DROP TABLE tokens")
        .await
        .unwrap();
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

fn assert_internal_error(response: &RawResponse) {
    assert_eq!(response.status, 500);
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(value["error"]["code"], "internal_error");
    assert_eq!(
        response.headers["access-control-allow-origin"],
        "https://console.example"
    );
    assert!(response.headers.contains_key(REQUEST_ID_HEADER_NAME));
    assert_no_auth_canaries(response);
}

fn assert_invalid_request(response: &RawResponse) {
    assert_eq!(response.status, 400);
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(value["error"]["code"], "invalid_request");
    assert_eq!(value["error"]["type"], "invalid_request_error");
    assert!(response.headers.contains_key(REQUEST_ID_HEADER_NAME));
    assert_no_auth_canaries(response);
}

fn assert_model_not_found(response: &RawResponse) {
    assert_eq!(response.status, 404);
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(value["error"]["code"], "model_not_found");
    assert_eq!(value["error"]["type"], "invalid_request_error");
    assert_eq!(
        value["error"]["message"],
        "The requested model was not found or is unavailable."
    );
    assert_eq!(
        response.headers["access-control-allow-origin"],
        "https://console.example"
    );
    assert!(response.headers.contains_key(REQUEST_ID_HEADER_NAME));
    let rendered = String::from_utf8_lossy(&response.body);
    assert!(!rendered.contains(UPSTREAM_MODEL));
    assert!(!rendered.contains(MODEL_POLICY_CANARY));
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
        UNKNOWN_CLIENT_KEY,
        CLIENT_KEY_DIGEST,
        BOUNDARY_KEY_CANARY,
        LOOPBACK_ALLOWLIST,
        TRUSTED_CLIENT_IP,
        SPOOFED_CLIENT_IP,
        MODEL_POLICY_CANARY,
    ] {
        assert!(!rendered.contains(forbidden));
    }
    for forbidden in [TOKEN_ID, USER_ID, GROUP_ID].map(|value| value.to_string()) {
        assert!(!rendered.contains(&forbidden));
    }
}
