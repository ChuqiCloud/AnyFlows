pub mod support;

use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use af_domain::{SubscriptionCycle, SubscriptionWindow};
use af_http::ServeOutcome;
use af_server::{Bootstrap, SupervisorShutdown};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ConnectionTrait, Database, DbBackend, Statement, entity::prelude::TimeDateTimeWithTimeZone,
};
use serde_json::{Value, json};
use support::{IO_TIMEOUT, send_request};
use tokio::{sync::oneshot, time::timeout};

const ADMIN_ID: i64 = 7_201;
const MEMBER_ID: i64 = 7_202;
const DEFAULT_GROUP_ID: i64 = 7_203;
const VIP_GROUP_ID: i64 = 7_204;
const DELETED_GROUP_ID: i64 = 7_205;
const ADMIN_USERNAME: &str = "group-read-admin";
const ADMIN_PASSWORD: &str = "correct-password";
const MEMBER_USERNAME: &str = "group-read-member";
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
            "anyflows-admin-group-read-{}-{serial}.db",
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
async fn admin_group_list_and_detail_use_real_database_reader() {
    let database = TestDatabase::new();
    seed_groups_and_users(&database.url).await;
    let signing_key = URL_SAFE_NO_PAD.encode([0x42; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'admin-group-read-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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
        "/api/admin/groups?limit=1",
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(first_page.status, 200);
    assert_eq!(first_page.headers["cache-control"], "no-store");
    let first_body = response_json(&first_page);
    assert_eq!(first_body["next_cursor"], DEFAULT_GROUP_ID);
    assert_group_windows(&first_body["groups"][0], [0, 0, 0]);
    assert_eq!(
        first_body["groups"],
        json!([{
            "id": DEFAULT_GROUP_ID,
            "name": "default",
            "display_name": "Default",
            "ratio_micros": 1000000,
            "peak_ratio_micros": Value::Null,
            "peak_start": Value::Null,
            "peak_end": Value::Null,
            "is_exclusive": false,
            "daily_limit": Value::Null,
            "weekly_limit": Value::Null,
            "monthly_limit": Value::Null,
            "daily_window": first_body["groups"][0]["daily_window"].clone(),
            "weekly_window": first_body["groups"][0]["weekly_window"].clone(),
            "monthly_window": first_body["groups"][0]["monthly_window"].clone(),
            "rpm_limit": Value::Null,
            "fallback_group_id": Value::Null,
            "flags": {}
        }])
    );

    let second_page = send_request(
        address,
        "GET",
        &format!("/api/admin/groups?after={DEFAULT_GROUP_ID}&limit=2"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(second_page.status, 200);
    let second_body = response_json(&second_page);
    assert_eq!(second_body["next_cursor"], Value::Null);
    assert_eq!(second_body["groups"].as_array().unwrap().len(), 1);
    assert_eq!(second_body["groups"][0]["id"], VIP_GROUP_ID);

    let detail = send_request(
        address,
        "GET",
        &format!("/api/admin/groups/{VIP_GROUP_ID}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(detail.status, 200);
    let detail_body = response_json(&detail);
    assert_group_windows(&detail_body, [100, 200, 300]);
    assert_eq!(
        detail_body,
        json!({
            "id": VIP_GROUP_ID,
            "name": "vip",
            "display_name": "VIP",
            "ratio_micros": 1250000,
            "peak_ratio_micros": 1500000,
            "peak_start": "08:00:00",
            "peak_end": "20:30:00",
            "is_exclusive": true,
            "daily_limit": 10000,
            "weekly_limit": 60000,
            "monthly_limit": 200000,
            "daily_window": detail_body["daily_window"].clone(),
            "weekly_window": detail_body["weekly_window"].clone(),
            "monthly_window": detail_body["monthly_window"].clone(),
            "rpm_limit": 120,
            "fallback_group_id": DEFAULT_GROUP_ID,
            "flags": {"claude_code_only": true}
        })
    );

    let deleted = send_request(
        address,
        "GET",
        &format!("/api/admin/groups/{DELETED_GROUP_ID}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(deleted.status, 404);
    assert_eq!(response_json(&deleted)["code"], "group_not_found");

    let create_body = serde_json::to_vec(&json!({
        "name": "write-created",
        "display_name": "Write Created",
        "ratio_micros": 1_100_000,
        "peak_ratio_micros": 1_300_000,
        "peak_start": "09:15:00",
        "peak_end": "18:45:00",
        "is_exclusive": false,
        "daily_limit": 50_000,
        "weekly_limit": Value::Null,
        "monthly_limit": 500_000,
        "rpm_limit": 90,
        "fallback_group_id": DEFAULT_GROUP_ID,
        "flags": {"claude_code_only": false}
    }))
    .unwrap();
    let created = send_request(
        address,
        "POST",
        "/api/admin/groups",
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
    let created_id = created_body["id"].as_i64().unwrap();
    assert_eq!(created_body["name"], "write-created");
    assert_eq!(created_body["ratio_micros"], 1_100_000);
    assert_eq!(created_body["peak_start"], "09:15:00");
    assert_group_windows(&created_body, [0, 0, 0]);
    let created_windows = [
        created_body["daily_window"].clone(),
        created_body["weekly_window"].clone(),
        created_body["monthly_window"].clone(),
    ];

    let conflict = send_request(
        address,
        "POST",
        "/api/admin/groups",
        &serde_json::to_vec(&json!({
            "name": "default",
            "display_name": "Duplicate",
            "ratio_micros": 1_000_000,
            "is_exclusive": false,
            "flags": {}
        }))
        .unwrap(),
        &[
            ("Authorization", &admin_authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(conflict.status, 409);
    assert_eq!(response_json(&conflict)["code"], "group_conflict");

    let update_body = serde_json::to_vec(&json!({
        "name": "write-updated",
        "display_name": "Write Updated",
        "ratio_micros": 950_000,
        "peak_ratio_micros": Value::Null,
        "peak_start": Value::Null,
        "peak_end": Value::Null,
        "is_exclusive": true,
        "daily_limit": Value::Null,
        "weekly_limit": 250_000,
        "monthly_limit": Value::Null,
        "rpm_limit": Value::Null,
        "fallback_group_id": Value::Null,
        "flags": {"visible": true}
    }))
    .unwrap();
    let updated = send_request(
        address,
        "PUT",
        &format!("/api/admin/groups/{created_id}"),
        &update_body,
        &[
            ("Authorization", &admin_authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(updated.status, 200);
    let updated_body = response_json(&updated);
    assert_eq!(updated_body["name"], "write-updated");
    assert_eq!(updated_body["ratio_micros"], 950_000);
    assert_eq!(updated_body["peak_ratio_micros"], Value::Null);
    assert_eq!(updated_body["weekly_limit"], 250_000);
    assert_eq!(updated_body["daily_window"], created_windows[0]);
    assert_eq!(updated_body["weekly_window"], created_windows[1]);
    assert_eq!(updated_body["monthly_window"], created_windows[2]);

    let in_use = send_request(
        address,
        "DELETE",
        &format!("/api/admin/groups/{DEFAULT_GROUP_ID}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(in_use.status, 409);
    assert_eq!(response_json(&in_use)["code"], "group_in_use");

    let removed = send_request(
        address,
        "DELETE",
        &format!("/api/admin/groups/{created_id}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(removed.status, 204);
    assert_eq!(removed.headers["cache-control"], "no-store");
    let removed_detail = send_request(
        address,
        "GET",
        &format!("/api/admin/groups/{created_id}"),
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(removed_detail.status, 404);
    assert_eq!(response_json(&removed_detail)["code"], "group_not_found");

    let member_authorization = login(address, MEMBER_USERNAME, MEMBER_PASSWORD).await;
    let forbidden = send_request(
        address,
        "GET",
        "/api/admin/groups",
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

async fn seed_groups_and_users(database_url: &str) {
    let pool = connect_and_migrate(
        &DatabaseOptions::new(database_url).unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    pool.close().await.unwrap();

    let connection = Database::connect(database_url).await.unwrap();
    insert_default_group(&connection).await;
    insert_vip_group(&connection).await;
    insert_deleted_group(&connection).await;
    insert_user(&connection, ADMIN_ID, ADMIN_USERNAME, ADMIN_PASSWORD, 1).await;
    insert_user(&connection, MEMBER_ID, MEMBER_USERNAME, MEMBER_PASSWORD, 0).await;
    connection.close().await.unwrap();
}

async fn insert_default_group(connection: &sea_orm::DatabaseConnection) {
    let starts = current_window_starts();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, daily_usage, weekly_usage, monthly_usage, daily_window_start, weekly_window_start, monthly_window_start, flags) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                DEFAULT_GROUP_ID.into(),
                "default".into(),
                "Default".into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                starts[0].into(),
                starts[1].into(),
                starts[2].into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
}

async fn insert_vip_group(connection: &sea_orm::DatabaseConnection) {
    let starts = current_window_starts();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, ratio_micros, peak_ratio_micros, peak_start, peak_end, is_exclusive, daily_limit, weekly_limit, monthly_limit, daily_usage, weekly_usage, monthly_usage, daily_window_start, weekly_window_start, monthly_window_start, rpm_limit, fallback_group_id, flags) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                VIP_GROUP_ID.into(),
                "vip".into(),
                "VIP".into(),
                1_250_000_i64.into(),
                1_500_000_i64.into(),
                "08:00:00".into(),
                "20:30:00".into(),
                true.into(),
                10_000_i64.into(),
                60_000_i64.into(),
                200_000_i64.into(),
                100_i64.into(),
                200_i64.into(),
                300_i64.into(),
                starts[0].into(),
                starts[1].into(),
                starts[2].into(),
                120_i32.into(),
                DEFAULT_GROUP_ID.into(),
                json!({"claude_code_only": true}).to_string().into(),
            ],
        ))
        .await
        .unwrap();
}

fn assert_group_windows(group: &Value, expected_usages: [i64; 3]) {
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    for (name, expected_usage) in ["daily_window", "weekly_window", "monthly_window"]
        .into_iter()
        .zip(expected_usages)
    {
        let window = &group[name];
        assert_eq!(window["usage"], expected_usage);
        let started_at = window["started_at"].as_i64().expect("窗口起点必须为整数");
        let resets_at = window["resets_at"].as_i64().expect("窗口终点必须为整数");
        assert!(started_at <= now);
        assert!(resets_at > now);
    }
}

fn current_window_starts() -> [TimeDateTimeWithTimeZone; 3] {
    let now = u64::try_from(TimeDateTimeWithTimeZone::now_utc().unix_timestamp())
        .expect("测试当前时间必须有效");
    let mut starts = [TimeDateTimeWithTimeZone::UNIX_EPOCH; 3];
    for (index, cycle) in [
        SubscriptionCycle::Daily,
        SubscriptionCycle::Weekly,
        SubscriptionCycle::Monthly,
    ]
    .into_iter()
    .enumerate()
    {
        let window = SubscriptionWindow::initial(cycle, now).expect("测试窗口必须有效");
        starts[index] = TimeDateTimeWithTimeZone::from_unix_timestamp(
            i64::try_from(window.started_at()).expect("测试窗口起点必须有效"),
        )
        .expect("测试窗口起点必须可转换");
    }
    starts
}

async fn insert_deleted_group(connection: &sea_orm::DatabaseConnection) {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, flags, deleted_at) VALUES (?, ?, ?, ?, datetime('now'))",
            [
                DELETED_GROUP_ID.into(),
                "deleted".into(),
                "Deleted".into(),
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
