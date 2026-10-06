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
use serde_json::{Value, json};
use support::{IO_TIMEOUT, send_request};
use tokio::{sync::oneshot, time::timeout};

const ADMIN_ID: i64 = 8_101;
const GROUP_ID: i64 = 8_102;
const PRIMARY_CHANNEL_ID: i64 = 8_103;
const SECONDARY_CHANNEL_ID: i64 = 8_104;
const DELETED_CHANNEL_ID: i64 = 8_105;
const PRIMARY_CREDENTIAL_ID: i64 = 8_106;
const OAUTH_CREDENTIAL_ID: i64 = 8_107;
const OTHER_CREDENTIAL_ID: i64 = 8_108;
const DELETED_CREDENTIAL_ID: i64 = 8_109;
const ADMIN_USERNAME: &str = "channel-read-admin";
const ADMIN_PASSWORD: &str = "correct-password";
const HEADER_CANARY: &str = "header-secret-canary";
const SETTINGS_CANARY: &str = "settings-secret-canary";
const CREDENTIAL_CANARY: &str = "credential-secret-canary";
const REASON_CANARY: &str = "temporary-reason-canary";
const WRITE_HEADER_CANARY: &str = "write-header-secret-canary";
const WRITE_SETTINGS_CANARY: &str = "write-settings-secret-canary";
const WRITE_CREDENTIAL_CANARY: &str = "write-credential-secret-canary";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-admin-channel-read-{}-{serial}.db",
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
async fn admin_channel_and_credential_reads_use_real_database_services() {
    let database = TestDatabase::new();
    seed_routing(&database.url).await;
    let signing_key = URL_SAFE_NO_PAD.encode([0x42; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'admin-channel-read-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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

    let authorization = login(address).await;
    let first_page = get(address, "/api/admin/channels?limit=1", &authorization).await;
    assert_eq!(first_page.status, 200);
    let first_body = response_json(&first_page);
    assert_eq!(first_body["channels"][0]["id"], PRIMARY_CHANNEL_ID);
    assert_eq!(first_body["next_cursor"], PRIMARY_CHANNEL_ID);
    assert_sanitized(&first_page);

    let second_page = get(
        address,
        &format!("/api/admin/channels?after={PRIMARY_CHANNEL_ID}&limit=2"),
        &authorization,
    )
    .await;
    let second_body = response_json(&second_page);
    assert_eq!(second_body["channels"].as_array().unwrap().len(), 1);
    assert_eq!(second_body["channels"][0]["id"], SECONDARY_CHANNEL_ID);
    assert_eq!(second_body["next_cursor"], Value::Null);

    let detail = get(
        address,
        &format!("/api/admin/channels/{PRIMARY_CHANNEL_ID}"),
        &authorization,
    )
    .await;
    assert_eq!(detail.status, 200);
    let detail_body = response_json(&detail);
    assert_eq!(detail_body["name"], "primary");
    assert_eq!(detail_body["type"], "openai");
    assert_eq!(detail_body["protocol"], "openai_chat");
    assert_eq!(detail_body["status"], "enabled");
    assert_eq!(detail_body["models"], json!(["public-model"]));
    assert_eq!(detail_body["group_ids"], json!([GROUP_ID]));
    assert_eq!(
        detail_body["model_mapping"]["public-model"],
        "upstream-model"
    );
    assert_eq!(detail_body["param_override"]["temperature"], 0);
    assert_sanitized(&detail);

    let credentials = get(
        address,
        &format!("/api/admin/channels/{PRIMARY_CHANNEL_ID}/credentials?limit=2"),
        &authorization,
    )
    .await;
    assert_eq!(credentials.status, 200);
    let credentials_body = response_json(&credentials);
    assert_eq!(credentials_body["credentials"].as_array().unwrap().len(), 2);
    assert_eq!(
        credentials_body["credentials"][0]["id"],
        PRIMARY_CREDENTIAL_ID
    );
    assert_eq!(credentials_body["credentials"][0]["kind"], "api_key");
    assert_eq!(
        credentials_body["credentials"][0]["multi_key_mode"],
        "random"
    );
    assert_eq!(
        credentials_body["credentials"][1]["id"],
        OAUTH_CREDENTIAL_ID
    );
    assert_sanitized(&credentials);

    let credential_detail = get(
        address,
        &format!("/api/admin/channels/{PRIMARY_CHANNEL_ID}/credentials/{PRIMARY_CREDENTIAL_ID}"),
        &authorization,
    )
    .await;
    assert_eq!(credential_detail.status, 200);
    assert_eq!(
        response_json(&credential_detail)["oauth_provider"],
        "example-oauth"
    );
    assert_sanitized(&credential_detail);

    let deleted_channel = get(
        address,
        &format!("/api/admin/channels/{DELETED_CHANNEL_ID}"),
        &authorization,
    )
    .await;
    assert_eq!(deleted_channel.status, 404);
    assert_eq!(response_json(&deleted_channel)["code"], "channel_not_found");
    let wrong_channel = get(
        address,
        &format!("/api/admin/channels/{PRIMARY_CHANNEL_ID}/credentials/{OTHER_CREDENTIAL_ID}"),
        &authorization,
    )
    .await;
    assert_eq!(wrong_channel.status, 404);
    assert_eq!(
        response_json(&wrong_channel)["code"],
        "credential_not_found"
    );
    let deleted_credential = get(
        address,
        &format!("/api/admin/channels/{PRIMARY_CHANNEL_ID}/credentials/{DELETED_CREDENTIAL_ID}"),
        &authorization,
    )
    .await;
    assert_eq!(deleted_credential.status, 404);
    assert_sanitized(&deleted_credential);

    let create_channel = send_admin_json(
        address,
        "POST",
        "/api/admin/channels",
        &authorization,
        json!({
            "name": "write-channel",
            "type": "openai",
            "protocol": "openai_chat",
            "base_url": "https://write.example.com/v1",
            "status": "disabled",
            "weight": 15,
            "priority": 25,
            "auto_ban": true,
            "client_simulation_profile": null,
            "client_simulation_risk_accepted": false,
            "client_simulation_body_profile": null,
            "client_simulation_body_risk_accepted": false,
            "models": ["public-model"],
            "group_ids": [GROUP_ID],
            "model_mapping": {"public-model": "write-upstream-model"},
            "param_override": {},
            "header_override": {"x-write-private": WRITE_HEADER_CANARY},
            "settings": {"private": WRITE_SETTINGS_CANARY},
            "tag": "write-test"
        }),
    )
    .await;
    assert_eq!(create_channel.status, 201);
    assert_eq!(
        response_json(&create_channel)["models"],
        json!(["public-model"])
    );
    assert_eq!(
        response_json(&create_channel)["group_ids"],
        json!([GROUP_ID])
    );
    assert_write_sanitized(&create_channel);
    let write_channel_id = response_json(&create_channel)["id"].as_i64().unwrap();

    let update_channel = send_admin_json(
        address,
        "PUT",
        &format!("/api/admin/channels/{write_channel_id}"),
        &authorization,
        json!({
            "name": "write-channel-updated",
            "type": "openai",
            "protocol": "openai_chat",
            "base_url": "https://write.example.com/v1",
            "status": "enabled",
            "weight": 16,
            "priority": 26,
            "auto_ban": false,
            "client_simulation_profile": null,
            "client_simulation_risk_accepted": false,
            "client_simulation_body_profile": null,
            "client_simulation_body_risk_accepted": false,
            "models": ["public-model", "second-model"],
            "group_ids": [GROUP_ID],
            "model_mapping": {"public-model": "write-upstream-model"},
            "param_override": {},
            "header_override": {"x-write-private": WRITE_HEADER_CANARY},
            "settings": {"private": WRITE_SETTINGS_CANARY},
            "tag": null
        }),
    )
    .await;
    assert_eq!(update_channel.status, 200);
    assert_eq!(
        response_json(&update_channel)["models"],
        json!(["public-model", "second-model"])
    );
    assert_eq!(
        response_json(&update_channel)["name"],
        "write-channel-updated"
    );
    assert_write_sanitized(&update_channel);

    let create_credential = send_admin_json(
        address,
        "POST",
        &format!("/api/admin/channels/{write_channel_id}/credentials"),
        &authorization,
        json!({
            "kind": "api_key",
            "secret": {
                "kind": "api_key",
                "api_key": WRITE_CREDENTIAL_CANARY
            },
            "status": "disabled",
            "multi_key_mode": "random",
            "priority": 5,
            "weight": 6,
            "concurrency": null,
            "load_factor_micros": null,
            "rate_multiplier_micros": null,
            "schedulable": false,
            "parent_id": null,
            "quota_dimension": "global",
            "proxy_id": null,
            "oauth_provider": null,
            "oauth_account_key": null,
            "oauth_project_id": null
        }),
    )
    .await;
    assert_eq!(create_credential.status, 201);
    assert_write_sanitized(&create_credential);
    let write_credential_id = response_json(&create_credential)["id"].as_i64().unwrap();

    let update_credential = send_admin_json(
        address,
        "PUT",
        &format!("/api/admin/channels/{write_channel_id}/credentials/{write_credential_id}"),
        &authorization,
        json!({
            "kind": "api_key",
            "secret": null,
            "status": "enabled",
            "multi_key_mode": "round_robin",
            "priority": 7,
            "weight": 8,
            "concurrency": 2,
            "load_factor_micros": 1000000,
            "rate_multiplier_micros": 900000,
            "schedulable": true,
            "parent_id": null,
            "quota_dimension": "global",
            "proxy_id": null,
            "oauth_provider": null,
            "oauth_account_key": null,
            "oauth_project_id": null
        }),
    )
    .await;
    assert_eq!(update_credential.status, 200);
    assert_eq!(response_json(&update_credential)["weight"], 8);
    assert_write_sanitized(&update_credential);

    let delete_credential = send_request(
        address,
        "DELETE",
        &format!("/api/admin/channels/{write_channel_id}/credentials/{write_credential_id}"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(delete_credential.status, 204);
    let deleted_write_credential = get(
        address,
        &format!("/api/admin/channels/{write_channel_id}/credentials/{write_credential_id}"),
        &authorization,
    )
    .await;
    assert_eq!(deleted_write_credential.status, 404);

    let delete_channel = send_request(
        address,
        "DELETE",
        &format!("/api/admin/channels/{write_channel_id}"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(delete_channel.status, 204);
    let deleted_write_channel = get(
        address,
        &format!("/api/admin/channels/{write_channel_id}"),
        &authorization,
    )
    .await;
    assert_eq!(deleted_write_channel.status, 404);

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

async fn send_admin_json(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    authorization: &str,
    body: Value,
) -> support::RawResponse {
    let body = serde_json::to_vec(&body).unwrap();
    send_request(
        address,
        method,
        path,
        &body,
        &[
            ("Authorization", authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await
}

async fn login(address: std::net::SocketAddr) -> String {
    let body = serde_json::to_vec(&json!({
        "username": ADMIN_USERNAME,
        "password": ADMIN_PASSWORD
    }))
    .unwrap();
    let response = send_request(
        address,
        "POST",
        "/api/auth/login",
        &body,
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

async fn get(
    address: std::net::SocketAddr,
    path: &str,
    authorization: &str,
) -> support::RawResponse {
    send_request(
        address,
        "GET",
        path,
        b"",
        &[("Authorization", authorization)],
    )
    .await
}

async fn seed_routing(database_url: &str) {
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
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, json(?))",
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
            "INSERT INTO users (id, username, password_hash, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, json(?))",
            [
                ADMIN_ID.into(),
                ADMIN_USERNAME.into(),
                password_hash().into(),
                1_i16.into(),
                1_i16.into(),
                GROUP_ID.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                "channel-read-aff".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    insert_channel(&connection, PRIMARY_CHANNEL_ID, "primary", false).await;
    insert_channel(&connection, SECONDARY_CHANNEL_ID, "secondary", false).await;
    insert_channel(&connection, DELETED_CHANNEL_ID, "deleted", true).await;
    insert_channel_routing(&connection, PRIMARY_CHANNEL_ID, "public-model").await;
    insert_credential(
        &connection,
        PRIMARY_CREDENTIAL_ID,
        PRIMARY_CHANNEL_ID,
        "api_key",
        Some(1),
        false,
    )
    .await;
    insert_credential(
        &connection,
        OAUTH_CREDENTIAL_ID,
        PRIMARY_CHANNEL_ID,
        "oauth",
        Some(2),
        false,
    )
    .await;
    insert_credential(
        &connection,
        OTHER_CREDENTIAL_ID,
        SECONDARY_CHANNEL_ID,
        "api_key",
        None,
        false,
    )
    .await;
    insert_credential(
        &connection,
        DELETED_CREDENTIAL_ID,
        PRIMARY_CHANNEL_ID,
        "api_key",
        None,
        true,
    )
    .await;
    connection.close().await.unwrap();
}

async fn insert_channel_routing(
    connection: &sea_orm::DatabaseConnection,
    channel_id: i64,
    model: &str,
) {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
            [channel_id.into(), model.into()],
        ))
        .await
        .unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
            [channel_id.into(), GROUP_ID.into()],
        ))
        .await
        .unwrap();
}

async fn insert_channel(
    connection: &sea_orm::DatabaseConnection,
    channel_id: i64,
    name: &str,
    deleted: bool,
) {
    let deleted_at = if deleted { "datetime('now')" } else { "NULL" };
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO channels (id, name, type, protocol, base_url, status, weight, priority, auto_ban, model_mapping, param_override, header_override, balance, used_quota, settings, tag, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, json(?), json(?), json(?), ?, ?, json(?), ?, {deleted_at})"
            ),
            [
                channel_id.into(),
                name.into(),
                "openai".into(),
                "openai_chat".into(),
                "https://api.example.com/v1".into(),
                1_i16.into(),
                10_i32.into(),
                20_i32.into(),
                true.into(),
                json!({"public-model": "upstream-model"}).to_string().into(),
                json!({"temperature": 0}).to_string().into(),
                json!({"x-private-header": HEADER_CANARY}).to_string().into(),
                1_000_i64.into(),
                25_i64.into(),
                json!({"private_setting": SETTINGS_CANARY}).to_string().into(),
                "primary".into(),
            ],
        ))
        .await
        .unwrap();
}

async fn insert_credential(
    connection: &sea_orm::DatabaseConnection,
    credential_id: i64,
    channel_id: i64,
    kind: &str,
    multi_key_mode: Option<i16>,
    deleted: bool,
) {
    let deleted_at = if deleted { "datetime('now')" } else { "NULL" };
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO credentials (id, channel_id, kind, secret, status, multi_key_mode, priority, weight, concurrency, load_factor_micros, rate_multiplier_micros, schedulable, temp_unschedulable_reason, quota_dimension, oauth_provider, oauth_account_key, oauth_project_id, deleted_at) VALUES (?, ?, ?, json(?), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, {deleted_at})"
            ),
            [
                credential_id.into(),
                channel_id.into(),
                kind.into(),
                json!({"ciphertext": CREDENTIAL_CANARY}).to_string().into(),
                1_i16.into(),
                multi_key_mode.into(),
                30_i32.into(),
                40_i32.into(),
                2_i32.into(),
                1_100_000_i64.into(),
                900_000_i64.into(),
                true.into(),
                REASON_CANARY.into(),
                "global".into(),
                "example-oauth".into(),
                "account-public-id".into(),
                "project-public-id".into(),
            ],
        ))
        .await
        .unwrap();
}

fn password_hash() -> String {
    let salt = SaltString::encode_b64(b"anyflows-channel-read").unwrap();
    Argon2::default()
        .hash_password(ADMIN_PASSWORD.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn response_json(response: &support::RawResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}

fn assert_sanitized(response: &support::RawResponse) {
    let rendered = format!(
        "{:?}\n{}",
        response.headers,
        String::from_utf8_lossy(&response.body)
    );
    for forbidden in [
        HEADER_CANARY,
        SETTINGS_CANARY,
        CREDENTIAL_CANARY,
        REASON_CANARY,
        "header_override",
        "settings",
        "secret",
        "temp_unschedulable_reason",
    ] {
        assert!(!rendered.contains(forbidden));
    }
}

fn assert_write_sanitized(response: &support::RawResponse) {
    let rendered = format!(
        "{:?}\n{}",
        response.headers,
        String::from_utf8_lossy(&response.body)
    );
    for forbidden in [
        WRITE_HEADER_CANARY,
        WRITE_SETTINGS_CANARY,
        WRITE_CREDENTIAL_CANARY,
        "header_override",
        "settings",
        "secret",
    ] {
        assert!(!rendered.contains(forbidden));
    }
}
