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

const ADMIN_ID: i64 = 7101;
const MEMBER_ID: i64 = 7102;
const DELETED_ID: i64 = 7103;
const DISABLED_ID: i64 = 7104;
const GROUP_ID: i64 = 7105;
const ADMIN_USERNAME: &str = "read-admin";
const ADMIN_PASSWORD: &str = "correct-password";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-admin-user-read-{}-{serial}.db",
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
async fn admin_user_list_and_detail_use_real_database_reader() {
    let database = TestDatabase::new();
    seed_users(&database.url).await;
    let signing_key = URL_SAFE_NO_PAD.encode([0x42; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'admin-user-read-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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

    let login_request =
        serde_json::to_vec(&json!({"username": ADMIN_USERNAME, "password": ADMIN_PASSWORD}))
            .unwrap();
    let login = send_request(
        address,
        "POST",
        "/api/auth/login",
        &login_request,
        &[("Content-Type", "application/json")],
    )
    .await;
    assert_eq!(login.status, 200);
    let token = response_json(&login)["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let authorization = format!("Bearer {token}");

    let first_page = send_request(
        address,
        "GET",
        "/api/admin/users?limit=2",
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(first_page.status, 200);
    let first_body = response_json(&first_page);
    assert_eq!(first_body["next_cursor"], MEMBER_ID);
    assert_eq!(first_body["users"][0]["username"], ADMIN_USERNAME);
    assert_eq!(first_body["users"][0]["role"], "admin");
    assert_eq!(first_body["users"][1]["id"], MEMBER_ID);
    assert!(!String::from_utf8_lossy(&first_page.body).contains(ADMIN_PASSWORD));

    let second_page = send_request(
        address,
        "GET",
        &format!("/api/admin/users?after={MEMBER_ID}&limit=2"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(second_page.status, 200);
    let second_body = response_json(&second_page);
    assert_eq!(second_body["next_cursor"], Value::Null);
    assert_eq!(
        second_body["users"],
        json!([{
            "id": DISABLED_ID,
            "username": "disabled-user",
            "email": Value::Null,
            "role": "user",
            "status": "disabled",
            "default_group_id": GROUP_ID,
            "quota": 0,
            "used_quota": 0,
            "frozen_quota": 0,
            "request_count": 0,
            "rpm_limit": Value::Null,
            "concurrency": Value::Null
        }])
    );

    let admin_audit = send_request(
        address,
        "GET",
        "/api/admin/audit-logs?limit=10",
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(admin_audit.status, 200);
    let admin_audit_body = response_json(&admin_audit);
    assert_eq!(admin_audit_body["logs"].as_array().unwrap().len(), 2);
    assert_eq!(
        admin_audit_body["logs"][0]["permission_code"],
        "platform.user_directory.read_all"
    );
    assert_eq!(
        admin_audit_body["logs"][0]["operation"],
        "platform.user_directory.list"
    );
    assert_eq!(admin_audit_body["logs"][0]["outcome"], "succeeded");
    assert_eq!(
        admin_audit_body["logs"][0]["operator_username"],
        ADMIN_USERNAME
    );
    assert_eq!(
        admin_audit_body["logs"][0]["audit_info"]["after"],
        MEMBER_ID
    );
    assert_eq!(admin_audit_body["logs"][0]["audit_info"]["limit"], 2);

    let self_audit = send_request(
        address,
        "GET",
        "/api/account/audit-logs?limit=10",
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(self_audit.status, 200);
    let self_audit_body = response_json(&self_audit);
    assert_eq!(self_audit_body["logs"].as_array().unwrap().len(), 2);
    assert_eq!(self_audit_body["logs"][0]["operator_user_id"], ADMIN_ID);
    assert_eq!(self_audit_body["logs"][0]["operator_username"], Value::Null);
    assert_eq!(self_audit_body["logs"][0]["before_value"], Value::Null);
    assert_eq!(self_audit_body["logs"][0]["after_value"], Value::Null);
    assert_eq!(self_audit_body["logs"][0]["audit_info"], Value::Null);

    let detail = send_request(
        address,
        "GET",
        &format!("/api/admin/users/{MEMBER_ID}"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(detail.status, 200);
    assert_eq!(
        response_json(&detail),
        json!({
            "id": MEMBER_ID,
            "username": "read-member",
            "email": "member@example.com",
            "role": "user",
            "status": "enabled",
            "default_group_id": GROUP_ID,
            "quota": 100,
            "used_quota": 20,
            "frozen_quota": 5,
            "request_count": 7,
            "rpm_limit": 60,
            "concurrency": 2
        })
    );

    let deleted = send_request(
        address,
        "GET",
        &format!("/api/admin/users/{DELETED_ID}"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(deleted.status, 404);
    assert_eq!(response_json(&deleted)["code"], "user_not_found");

    let create_body = serde_json::to_vec(&json!({
        "username": "write-created",
        "email": "created@example.com",
        "password": "created-password",
        "role": "user",
        "status": "enabled",
        "default_group_id": GROUP_ID,
        "quota": 250,
        "rpm_limit": 30,
        "concurrency": 1
    }))
    .unwrap();
    let created = send_request(
        address,
        "POST",
        "/api/admin/users",
        &create_body,
        &[
            ("Authorization", &authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(created.status, 201);
    let created_body = response_json(&created);
    let created_id = created_body["id"].as_i64().unwrap();
    assert_eq!(created_body["username"], "write-created");
    assert_eq!(created_body["email"], "created@example.com");
    assert_eq!(created_body["quota"], 250);
    assert!(!String::from_utf8_lossy(&created.body).contains("created-password"));

    let created_login_request =
        serde_json::to_vec(&json!({"username":"write-created","password":"created-password"}))
            .unwrap();
    let created_login = send_request(
        address,
        "POST",
        "/api/auth/login",
        &created_login_request,
        &[("Content-Type", "application/json")],
    )
    .await;
    assert_eq!(created_login.status, 200);
    assert_eq!(response_json(&created_login)["user"]["role"], "user");

    let conflict = send_request(
        address,
        "POST",
        "/api/admin/users",
        &serde_json::to_vec(&json!({
            "username": ADMIN_USERNAME,
            "role": "user",
            "status": "enabled",
            "default_group_id": GROUP_ID,
            "quota": 0
        }))
        .unwrap(),
        &[
            ("Authorization", &authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(conflict.status, 409);
    assert_eq!(response_json(&conflict)["code"], "user_conflict");

    let update_body = serde_json::to_vec(&json!({
        "username": "write-updated",
        "email": Value::Null,
        "password": "updated-password",
        "role": "admin",
        "status": "enabled",
        "default_group_id": GROUP_ID,
        "rpm_limit": Value::Null,
        "concurrency": 2
    }))
    .unwrap();
    let updated = send_request(
        address,
        "PUT",
        &format!("/api/admin/users/{created_id}"),
        &update_body,
        &[
            ("Authorization", &authorization),
            ("Content-Type", "application/json"),
        ],
    )
    .await;
    assert_eq!(updated.status, 200);
    assert_eq!(
        response_json(&updated),
        json!({
            "id": created_id,
            "username": "write-updated",
            "email": Value::Null,
            "role": "admin",
            "status": "enabled",
            "default_group_id": GROUP_ID,
            "quota": 250,
            "used_quota": 0,
            "frozen_quota": 0,
            "request_count": 0,
            "rpm_limit": Value::Null,
            "concurrency": 2
        })
    );

    let updated_login_request =
        serde_json::to_vec(&json!({"username":"write-updated","password":"updated-password"}))
            .unwrap();
    let updated_login = send_request(
        address,
        "POST",
        "/api/auth/login",
        &updated_login_request,
        &[("Content-Type", "application/json")],
    )
    .await;
    assert_eq!(updated_login.status, 200);
    assert_eq!(response_json(&updated_login)["user"]["role"], "admin");

    let removed = send_request(
        address,
        "DELETE",
        &format!("/api/admin/users/{created_id}"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(removed.status, 204);

    let removed_detail = send_request(
        address,
        "GET",
        &format!("/api/admin/users/{created_id}"),
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(removed_detail.status, 404);
    assert_eq!(response_json(&removed_detail)["code"], "user_not_found");

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

async fn seed_users(database_url: &str) {
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
                "admin-user-read-group".into(),
                "Admin User Read Group".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    insert_user(
        &connection,
        ADMIN_ID,
        ADMIN_USERNAME,
        None,
        Some(password_hash()),
        1,
        1,
        0,
        0,
        0,
        0,
        None,
        None,
        false,
    )
    .await;
    insert_user(
        &connection,
        MEMBER_ID,
        "read-member",
        Some("member@example.com"),
        None,
        0,
        1,
        100,
        20,
        5,
        7,
        Some(60),
        Some(2),
        false,
    )
    .await;
    insert_user(
        &connection,
        DELETED_ID,
        "deleted-user",
        None,
        None,
        0,
        1,
        0,
        0,
        0,
        0,
        None,
        None,
        true,
    )
    .await;
    insert_user(
        &connection,
        DISABLED_ID,
        "disabled-user",
        None,
        None,
        0,
        2,
        0,
        0,
        0,
        0,
        None,
        None,
        false,
    )
    .await;
    connection.close().await.unwrap();
}

#[allow(
    clippy::too_many_arguments,
    reason = "测试种子字段与用户响应契约一一对应"
)]
async fn insert_user(
    connection: &sea_orm::DatabaseConnection,
    user_id: i64,
    username: &str,
    email: Option<&str>,
    password_hash: Option<String>,
    role: i16,
    status: i16,
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
    deleted: bool,
) {
    if deleted {
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO users (id, username, email, password_hash, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, rpm_limit, concurrency, settings, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
                [
                    user_id.into(),
                    username.into(),
                    email.into(),
                    password_hash.into(),
                    role.into(),
                    status.into(),
                    GROUP_ID.into(),
                    quota.into(),
                    used_quota.into(),
                    frozen_quota.into(),
                    request_count.into(),
                    format!("{username}-aff").into(),
                    rpm_limit.into(),
                    concurrency.into(),
                    "{}".into(),
                ],
            ))
            .await
            .unwrap();
    } else {
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO users (id, username, email, password_hash, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, rpm_limit, concurrency, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    user_id.into(),
                    username.into(),
                    email.into(),
                    password_hash.into(),
                    role.into(),
                    status.into(),
                    GROUP_ID.into(),
                    quota.into(),
                    used_quota.into(),
                    frozen_quota.into(),
                    request_count.into(),
                    format!("{username}-aff").into(),
                    rpm_limit.into(),
                    concurrency.into(),
                    "{}".into(),
                ],
            ))
            .await
            .unwrap();
    }
}

fn password_hash() -> String {
    let salt = SaltString::encode_b64(b"anyflows-user-read").unwrap();
    Argon2::default()
        .hash_password(ADMIN_PASSWORD.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn response_json(response: &support::RawResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}
