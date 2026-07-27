pub mod support;

use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_http::ServeOutcome;
use af_server::{Bootstrap, SupervisorShutdown};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::Value;
use support::{IO_TIMEOUT, send_request};
use tokio::{sync::oneshot, time::timeout};

const ADMIN_ID: i64 = 7_501;
const MEMBER_ID: i64 = 7_502;
const GROUP_ID: i64 = 7_503;
const TOKEN_ID: i64 = 7_504;
const ADMIN_USERNAME: &str = "dashboard-admin";
const ADMIN_PASSWORD: &str = "correct-password";
const MEMBER_USERNAME: &str = "dashboard-member";
const MEMBER_PASSWORD: &str = "member-password";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-admin-dashboard-{}-{serial}.db",
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
async fn admin_dashboard_uses_real_aggregates_and_role_boundary() {
    let database = TestDatabase::new();
    seed_dashboard(&database.url).await;
    let signing_key = URL_SAFE_NO_PAD.encode([0x44; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'admin-dashboard-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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

    let admin_authorization = login(address, ADMIN_USERNAME, ADMIN_PASSWORD).await;
    let response = send_request(
        address,
        "GET",
        "/api/admin/dashboard",
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.headers["cache-control"], "no-store");
    let body = response_json(&response);
    assert_eq!(body["request_count"], 2);
    assert_eq!(body["quota_consumed"], 120);
    assert_eq!(body["upstream_usage_count"], 1);
    assert_eq!(body["estimated_usage_count"], 1);
    assert_eq!(body["per_token_request_count"], 1);
    assert_eq!(body["free_request_count"], 1);
    assert_eq!(body["enabled_channel_count"], 1);
    assert_eq!(body["disabled_channel_count"], 1);
    assert_eq!(body["auto_disabled_channel_count"], 1);
    assert_eq!(
        body["period_end"].as_i64().unwrap() - body["period_start"].as_i64().unwrap(),
        86_400
    );

    let member_authorization = login(address, MEMBER_USERNAME, MEMBER_PASSWORD).await;
    let forbidden = send_request(
        address,
        "GET",
        "/api/admin/dashboard",
        b"",
        &[("Authorization", &member_authorization)],
    )
    .await;
    assert_eq!(forbidden.status, 403);
    assert_eq!(response_json(&forbidden)["code"], "forbidden");

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

async fn login(address: std::net::SocketAddr, username: &str, password: &str) -> String {
    let request =
        serde_json::to_vec(&serde_json::json!({"username": username, "password": password}))
            .unwrap();
    let response = send_request(
        address,
        "POST",
        "/api/auth/login",
        &request,
        &[("Content-Type", "application/json")],
    )
    .await;
    assert_eq!(response.status, 200);
    let token = response_json(&response)["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    format!("Bearer {token}")
}

async fn seed_dashboard(database_url: &str) {
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
                "dashboard".into(),
                "Dashboard".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    insert_user(&connection, ADMIN_ID, ADMIN_USERNAME, ADMIN_PASSWORD, 1).await;
    insert_user(&connection, MEMBER_ID, MEMBER_USERNAME, MEMBER_PASSWORD, 0).await;
    connection.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, group_id, remain_quota, unlimited_quota, used_quota, model_limits, allow_ips, cross_group_retry, usage_5h, usage_1d, usage_7d, window_5h_start, window_1d_start, window_7d_start, used_requests, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?, ?, datetime('1970-01-01 00:00:00'), datetime('1970-01-01 00:00:00'), datetime('1970-01-01 00:00:00'), ?, datetime('now'), datetime('now'))",
        [
            TOKEN_ID.into(), MEMBER_ID.into(), "b".repeat(64).into(),
            "sk-af-dashboard001".into(), "dashboard-token".into(), 1_i16.into(),
            GROUP_ID.into(), 10_000_i64.into(), false.into(), 0_i64.into(), false.into(),
            0_i64.into(), 0_i64.into(), 0_i64.into(), 0_i64.into(),
        ],
    )).await.unwrap();
    for (id, modifier, source, billing_mode, quota) in [
        (7_505_i64, "-1 hour", 1_i16, 1_i16, 120_i64),
        (7_506, "-2 hours", 2, 2, 0),
        (7_507, "-25 hours", 1, 1, 999),
    ] {
        connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO usage_logs (id, event_id, event_type, user_id, token_id, group_id, billing_mode, input_tokens, output_tokens, cache_read, cache_creation_5m, cache_creation_1h, reasoning_tokens, audio_input_tokens, audio_output_tokens, usage_source, usage_semantics, quota, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now', ?))",
            [
                id.into(), format!("{id:032x}").into(), 1_i16.into(), MEMBER_ID.into(),
                TOKEN_ID.into(), GROUP_ID.into(), billing_mode.into(), 10_i64.into(), 2_i64.into(),
                0_i64.into(), 0_i64.into(), 0_i64.into(), 0_i64.into(), 0_i64.into(),
                0_i64.into(), source.into(), 1_i16.into(), quota.into(), modifier.into(),
            ],
        )).await.unwrap();
    }
    for (id, status, deleted) in [
        (7_508_i64, 1_i16, false),
        (7_509, 2, false),
        (7_510, 3, false),
        (7_511, 1, true),
    ] {
        let deleted_at = if deleted { "datetime('now')" } else { "NULL" };
        connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("INSERT INTO channels (id, name, type, protocol, status, weight, priority, auto_ban, model_mapping, param_override, header_override, used_quota, settings, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, {deleted_at})"),
            [
                id.into(), format!("dashboard-{id}").into(), "openai".into(),
                "openai_chat".into(), status.into(), 1_i32.into(), 0_i32.into(), false.into(),
                "{}".into(), "{}".into(), "{}".into(), 0_i64.into(), "{}".into(),
            ],
        )).await.unwrap();
    }
    connection.close().await.unwrap();
}

async fn insert_user(
    connection: &sea_orm::DatabaseConnection,
    user_id: i64,
    username: &str,
    password: &str,
    role: i16,
) {
    connection.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO users (id, username, password_hash, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [
            user_id.into(), username.into(), password_hash(username, password).into(), role.into(),
            1_i16.into(), GROUP_ID.into(), 0_i64.into(), 0_i64.into(), 0_i64.into(),
            0_i64.into(), format!("{username}-aff").into(), "{}".into(),
        ],
    )).await.unwrap();
}

fn password_hash(username: &str, password: &str) -> String {
    let salt = SaltString::encode_b64(format!("anyflows-{username}").as_bytes()).unwrap();
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn response_json(response: &support::RawResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}
