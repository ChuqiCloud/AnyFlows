pub mod support;

use std::{
    fs,
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_server::{Bootstrap, BootstrapError, ShutdownReport};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};
use serde_json::{Value, json};
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, RUNTIME_HEADER_NAME, RUNTIME_HEADER_VALUE, UPSTREAM_BASE_URL,
    UPSTREAM_KEY, UPSTREAM_MODEL, credential_encryption_key, seed_jina_rerank_runtime_channel_with,
    send_request, spawn_proxy,
};
use tokio::{sync::oneshot, time::timeout};

const CLIENT_KEY_DIGEST: &str = "58e7607fb7ed996d551ba517addbcf51a35206b43706e233737d242773845efa";
const LOOPBACK_ALLOWLIST: &str = r#"["127.0.0.1/32"]"#;
const PRIVATE_UPSTREAM_MODEL: &str = "private-jina-reranker";
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-jina-rerank-runtime-{}-{serial}.db",
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
async fn native_rerank_maps_model_rebuilds_response_and_records_usage() {
    let database = TestDatabase::new();
    let connection = seed_base_database(&database).await;
    seed_jina_rerank_runtime_channel_with(
        &connection,
        GROUP_ID,
        UPSTREAM_BASE_URL,
        UPSTREAM_MODEL,
        PRIVATE_UPSTREAM_MODEL,
        UPSTREAM_KEY,
        None,
    )
    .await;
    connection.close().await.unwrap();

    let upstream_body = serde_json::to_vec(&json!({
        "model": PRIVATE_UPSTREAM_MODEL,
        "object": "list",
        "results": [
            {"index": 1, "relevance_score": 0.9},
            {"index": 0, "relevance_score": 0.4}
        ],
        "usage": {"total_tokens": 4}
    }))
    .unwrap();
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        &upstream_body,
        &[("Content-Type", "application/json")],
    );
    let proxy_url = format!("http://{proxy_address}");
    let (address, shutdown, server) = start_gateway(&database, Some(&proxy_url)).await;

    let response = send_request(
        address,
        "POST",
        "/v1/rerank",
        br#"{"model":"test-model","query":"hi","documents":["first",{"text":"second"}],"top_n":2}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {CLIENT_KEY}")),
        ],
    )
    .await;
    stop_gateway(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    let head = upstream.head.to_ascii_lowercase();
    assert!(head.starts_with("post http://upstream.example/proxy/v1/rerank http/1.1"));
    assert!(head.contains("authorization: bearer configured-upstream-key"));
    assert!(head.contains(&format!(
        "{}: {}",
        RUNTIME_HEADER_NAME.to_ascii_lowercase(),
        RUNTIME_HEADER_VALUE.to_ascii_lowercase()
    )));
    assert!(!head.contains(&CLIENT_KEY.to_ascii_lowercase()));
    let upstream_request: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_request["model"], PRIVATE_UPSTREAM_MODEL);
    assert_eq!(upstream_request["query"], "hi");
    assert_eq!(
        upstream_request["documents"],
        json!(["first", {"text": "second"}])
    );
    assert_eq!(upstream_request["top_n"], 2);
    assert_eq!(upstream_request["return_documents"], false);
    assert!(upstream_request.get("stream").is_none());

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["model"], UPSTREAM_MODEL);
    assert_eq!(body["results"][0]["index"], 1);
    assert_eq!(body["results"][1]["index"], 0);
    assert_eq!(body["usage"]["prompt_tokens"], 4);
    assert_eq!(body["usage"]["total_tokens"], 4);
    assert!(!String::from_utf8_lossy(&response.body).contains(PRIVATE_UPSTREAM_MODEL));
    assert_eq!(usage_summary(&database).await, (1, 4, 0));
}

#[tokio::test]
async fn missing_upstream_usage_stays_absent_and_settles_frozen_text_bound() {
    let database = TestDatabase::new();
    let connection = seed_base_database(&database).await;
    seed_jina_rerank_runtime_channel_with(
        &connection,
        GROUP_ID,
        UPSTREAM_BASE_URL,
        UPSTREAM_MODEL,
        PRIVATE_UPSTREAM_MODEL,
        UPSTREAM_KEY,
        None,
    )
    .await;
    connection.close().await.unwrap();
    let upstream_body = br#"{"model":"private-jina-reranker","object":"list","results":[{"index":0,"relevance_score":0.8}]}"#;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        upstream_body,
        &[("Content-Type", "application/json")],
    );
    let proxy_url = format!("http://{proxy_address}");
    let (address, shutdown, server) = start_gateway(&database, Some(&proxy_url)).await;

    let response = send_request(
        address,
        "POST",
        "/v1/rerank",
        br#"{"model":"test-model","query":"hi","documents":["world"]}"#,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {CLIENT_KEY}")),
        ],
    )
    .await;
    stop_gateway(shutdown, server).await;
    captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert!(body.get("usage").is_none());
    assert_eq!(usage_summary(&database).await, (1, 7, 0));
}

async fn seed_base_database(database: &TestDatabase) -> DatabaseConnection {
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
                "0".into(),
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
    connection
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

async fn usage_summary(database: &TestDatabase) -> (i64, i64, i64) {
    let connection = Database::connect(&database.url).await.unwrap();
    let row = connection
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "SELECT COUNT(*) AS row_count, COALESCE(SUM(input_tokens), 0) AS input_tokens, COALESCE(SUM(output_tokens), 0) AS output_tokens FROM usage_logs WHERE token_id = {}",
                support::TOKEN_ID
            ),
        ))
        .await
        .unwrap()
        .unwrap();
    let summary = (
        row.try_get("", "row_count").unwrap(),
        row.try_get("", "input_tokens").unwrap(),
        row.try_get("", "output_tokens").unwrap(),
    );
    connection.close().await.unwrap();
    summary
}
