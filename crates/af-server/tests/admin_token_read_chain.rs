pub mod support;

use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use af_admin::PresentedApiKey;
use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_http::ServeOutcome;
use af_server::{Bootstrap, SupervisorShutdown};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::{Value, json};
use support::{IO_TIMEOUT, send_request};
use tokio::{sync::oneshot, time::timeout};

const ADMIN_ID: i64 = 7_301;
const MEMBER_ID: i64 = 7_302;
const DEFAULT_GROUP_ID: i64 = 7_303;
const VIP_GROUP_ID: i64 = 7_304;
const PRIMARY_TOKEN_ID: i64 = 7_305;
const BACKUP_TOKEN_ID: i64 = 7_306;
const DELETED_TOKEN_ID: i64 = 7_307;
const FALLBACK_TOKEN_ID: i64 = 7_308;
const ADMIN_USERNAME: &str = "token-read-admin";
const ADMIN_PASSWORD: &str = "correct-password";
const MEMBER_USERNAME: &str = "token-read-member";
const MEMBER_PASSWORD: &str = "member-password";
const FULL_TOKEN_CANARY: &str = "sk-af-secret-full-token-canary";
const KEY_HASH_CANARY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-admin-token-read-{}-{serial}.db",
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
async fn admin_token_crud_uses_real_database_services() {
    let database = TestDatabase::new();
    seed_tokens(&database.url).await;
    let signing_key = URL_SAFE_NO_PAD.encode([0x42; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'admin-token-read-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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
    let first_page = send_request(
        address,
        "GET",
        "/api/admin/tokens?limit=2",
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(first_page.status, 200);
    assert_eq!(first_page.headers["cache-control"], "no-store");
    let first_body = response_json(&first_page);
    assert_eq!(first_body["next_cursor"], BACKUP_TOKEN_ID);
    assert_eq!(first_body["tokens"][0]["id"], PRIMARY_TOKEN_ID);
    assert_eq!(first_body["tokens"][0]["name"], "primary");
    assert_eq!(first_body["tokens"][1]["id"], BACKUP_TOKEN_ID);
    assert_response_is_sanitized(&first_page);

    let second_page = send_request(
        address,
        "GET",
        &format!("/api/admin/tokens?after={BACKUP_TOKEN_ID}&limit=3"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(second_page.status, 200);
    let second_body = response_json(&second_page);
    assert_eq!(second_body["next_cursor"], Value::Null);
    assert_eq!(second_body["tokens"].as_array().unwrap().len(), 1);
    assert_eq!(second_body["tokens"][0]["id"], FALLBACK_TOKEN_ID);
    assert_response_is_sanitized(&second_page);

    let detail = send_request(
        address,
        "GET",
        &format!("/api/admin/tokens/{PRIMARY_TOKEN_ID}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(detail.status, 200);
    assert_eq!(
        response_json(&detail),
        json!({
            "id": PRIMARY_TOKEN_ID,
            "user_id": MEMBER_ID,
            "key_prefix": "sk-af-public000001",
            "name": "primary",
            "status": "enabled",
            "group_id": VIP_GROUP_ID,
            "remain_quota": 1000,
            "unlimited_quota": false,
            "used_quota": 25,
            "expired_at": Value::Null,
            "model_limits": ["gpt-5.5", "gpt-5.5", "claude-4"],
            "allow_ips": ["192.0.2.0/24", "198.51.100.10/32", "192.0.2.0/24"],
            "cross_group_retry": true,
            "rate_limit_5h": 100,
            "rate_limit_1d": 200,
            "rate_limit_7d": 300,
            "usage_5h": 10,
            "usage_1d": 20,
            "usage_7d": 30,
            "window_5h_start": 0,
            "window_1d_start": 0,
            "window_7d_start": 0,
            "max_requests": 1000,
            "used_requests": 4
        })
    );
    assert_response_is_sanitized(&detail);

    let deleted = send_request(
        address,
        "GET",
        &format!("/api/admin/tokens/{DELETED_TOKEN_ID}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(deleted.status, 404);
    assert_eq!(response_json(&deleted)["code"], "token_not_found");
    assert_response_is_sanitized(&deleted);

    let create_body = serde_json::to_vec(&token_write_body(
        "issued",
        "enabled",
        Some(VIP_GROUP_ID),
        true,
    ))
    .unwrap();
    let created = send_request(
        address,
        "POST",
        "/api/admin/tokens",
        &create_body,
        &[
            ("Authorization", &admin_authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(created.status, 201);
    assert_eq!(created.headers["cache-control"], "no-store");
    let created_body = response_json(&created);
    let api_key = created_body["api_key"].as_str().unwrap().to_owned();
    let presented = PresentedApiKey::parse(&api_key).unwrap();
    let created_id = created_body["token"]["id"].as_i64().unwrap();
    let key_prefix = created_body["token"]["key_prefix"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(key_prefix, presented.display_prefix().as_str());
    assert_eq!(created_body["token"]["name"], "issued");
    assert_eq!(created_body["token"]["used_quota"], 0);
    assert_eq!(created_body["token"]["usage_5h"], 0);
    assert_eq!(created_body["token"]["used_requests"], 0);
    assert_eq!(
        stored_key_hash(&database.url, created_id).await,
        presented.digest().as_str()
    );

    let update_body =
        serde_json::to_vec(&token_write_body("updated", "disabled", None, false)).unwrap();
    let updated = send_request(
        address,
        "PUT",
        &format!("/api/admin/tokens/{created_id}"),
        &update_body,
        &[
            ("Authorization", &admin_authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(updated.status, 200);
    let updated_body = response_json(&updated);
    assert_eq!(updated_body["name"], "updated");
    assert_eq!(updated_body["status"], "disabled");
    assert_eq!(updated_body["group_id"], Value::Null);
    assert_eq!(updated_body["key_prefix"], key_prefix);
    assert_eq!(updated_body["model_limits"], Value::Null);
    assert_eq!(updated_body["allow_ips"], Value::Null);
    assert!(!String::from_utf8_lossy(&updated.body).contains(&api_key));
    assert_eq!(
        stored_key_hash(&database.url, created_id).await,
        presented.digest().as_str()
    );

    let removed = send_request(
        address,
        "DELETE",
        &format!("/api/admin/tokens/{created_id}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(removed.status, 204);
    assert_eq!(removed.headers["cache-control"], "no-store");
    assert!(removed.body.is_empty());
    let removed_detail = send_request(
        address,
        "GET",
        &format!("/api/admin/tokens/{created_id}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(removed_detail.status, 404);
    assert_eq!(response_json(&removed_detail)["code"], "token_not_found");
    let removed_again = send_request(
        address,
        "DELETE",
        &format!("/api/admin/tokens/{created_id}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(removed_again.status, 404);
    assert!(!String::from_utf8_lossy(&removed_again.body).contains(&api_key));

    let member_authorization = login(address, MEMBER_USERNAME, MEMBER_PASSWORD).await;
    let forbidden = send_request(
        address,
        "GET",
        "/api/admin/tokens",
        b"",
        &[("Authorization", &member_authorization)],
    )
    .await;
    assert_eq!(forbidden.status, 403);
    assert_eq!(response_json(&forbidden)["code"], "forbidden");
    assert_response_is_sanitized(&forbidden);

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

async fn login(address: std::net::SocketAddr, username: &str, password: &str) -> String {
    let request = serde_json::to_vec(&json!({"username": username, "password": password})).unwrap();
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

async fn seed_tokens(database_url: &str) {
    let pool = connect_and_migrate(
        &DatabaseOptions::new(database_url).unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    pool.close().await.unwrap();

    let connection = Database::connect(database_url).await.unwrap();
    insert_group(&connection, DEFAULT_GROUP_ID, "default", "Default").await;
    insert_group(&connection, VIP_GROUP_ID, "vip", "VIP").await;
    insert_user(&connection, ADMIN_ID, ADMIN_USERNAME, ADMIN_PASSWORD, 1).await;
    insert_user(&connection, MEMBER_ID, MEMBER_USERNAME, MEMBER_PASSWORD, 0).await;
    insert_token(
        &connection,
        PRIMARY_TOKEN_ID,
        MEMBER_ID,
        Some(VIP_GROUP_ID),
        "primary",
        "a",
        1,
        true,
        false,
    )
    .await;
    insert_token(
        &connection,
        BACKUP_TOKEN_ID,
        MEMBER_ID,
        Some(VIP_GROUP_ID),
        "backup",
        "b",
        2,
        false,
        false,
    )
    .await;
    insert_token(
        &connection,
        DELETED_TOKEN_ID,
        MEMBER_ID,
        Some(VIP_GROUP_ID),
        "deleted",
        "c",
        1,
        false,
        true,
    )
    .await;
    insert_token(
        &connection,
        FALLBACK_TOKEN_ID,
        MEMBER_ID,
        None,
        "fallback",
        "d",
        1,
        false,
        false,
    )
    .await;
    connection.close().await.unwrap();
}

async fn insert_group(
    connection: &sea_orm::DatabaseConnection,
    group_id: i64,
    name: &str,
    display_name: &str,
) {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            [
                group_id.into(),
                name.into(),
                display_name.into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
}

async fn insert_user(
    connection: &sea_orm::DatabaseConnection,
    user_id: i64,
    username: &str,
    password: &str,
    role: i16,
) {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, password_hash, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                user_id.into(),
                username.into(),
                password_hash(username, password).into(),
                role.into(),
                1_i16.into(),
                DEFAULT_GROUP_ID.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                format!("{username}-aff").into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
}

#[allow(
    clippy::too_many_arguments,
    reason = "测试令牌种子字段与只读响应契约一一对应"
)]
async fn insert_token(
    connection: &sea_orm::DatabaseConnection,
    token_id: i64,
    user_id: i64,
    group_id: Option<i64>,
    name: &str,
    hash_seed: &str,
    status: i16,
    with_limits: bool,
    deleted: bool,
) {
    let deleted_at = if deleted { "datetime('now')" } else { "NULL" };
    let model_limits = with_limits.then(|| json!(["gpt-5.5", "gpt-5.5", "claude-4"]).to_string());
    let allow_ips = with_limits
        .then(|| json!(["192.0.2.0/24", "198.51.100.10/32", "192.0.2.0/24"]).to_string());
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, group_id, remain_quota, unlimited_quota, used_quota, expired_at, model_limits, allow_ips, cross_group_retry, rate_limit_5h, rate_limit_1d, rate_limit_7d, usage_5h, usage_1d, usage_7d, window_5h_start, window_1d_start, window_7d_start, max_requests, used_requests, created_at, updated_at, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, json(?), json(?), ?, ?, ?, ?, ?, ?, ?, datetime('1970-01-01 00:00:00'), datetime('1970-01-01 00:00:00'), datetime('1970-01-01 00:00:00'), ?, ?, datetime('now'), datetime('now'), {deleted_at})"
            ),
            [
                token_id.into(),
                user_id.into(),
                hash_seed.repeat(64).into(),
                "sk-af-public000001".into(),
                name.into(),
                status.into(),
                group_id.into(),
                1_000_i64.into(),
                false.into(),
                25_i64.into(),
                model_limits.into(),
                allow_ips.into(),
                with_limits.into(),
                with_limits.then_some(100_i64).into(),
                with_limits.then_some(200_i64).into(),
                with_limits.then_some(300_i64).into(),
                10_i64.into(),
                20_i64.into(),
                30_i64.into(),
                with_limits.then_some(1_000_i64).into(),
                4_i64.into(),
            ],
        ))
        .await
        .unwrap();
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

fn token_write_body(name: &str, status: &str, group_id: Option<i64>, with_limits: bool) -> Value {
    json!({
        "user_id": MEMBER_ID,
        "name": name,
        "status": status,
        "group_id": group_id,
        "remain_quota": 5_000,
        "unlimited_quota": false,
        "expired_at": 1_900_000_000_i64,
        "model_limits": with_limits.then(|| vec!["gpt-5.5", "gpt-5.5"]),
        "allow_ips": with_limits.then(|| vec!["192.0.2.0/24", "192.0.2.0/24"]),
        "cross_group_retry": with_limits,
        "rate_limit_5h": with_limits.then_some(100),
        "rate_limit_1d": with_limits.then_some(200),
        "rate_limit_7d": with_limits.then_some(300),
        "max_requests": with_limits.then_some(1_000)
    })
}

async fn stored_key_hash(database_url: &str, token_id: i64) -> String {
    let connection = Database::connect(database_url).await.unwrap();
    let row = connection
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT key_hash FROM tokens WHERE id = ?",
            [token_id.into()],
        ))
        .await
        .unwrap()
        .expect("签发令牌必须已经持久化");
    let key_hash: String = row.try_get("", "key_hash").unwrap();
    connection.close().await.unwrap();
    key_hash
}

fn assert_response_is_sanitized(response: &support::RawResponse) {
    let rendered = format!(
        "{:?}\n{}",
        response.headers,
        String::from_utf8_lossy(&response.body)
    );
    for forbidden in [FULL_TOKEN_CANARY, KEY_HASH_CANARY, "key_hash"] {
        assert!(!rendered.contains(forbidden));
    }
}
