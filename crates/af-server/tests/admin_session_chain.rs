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

const USER_ID: i64 = 7001;
const GROUP_ID: i64 = 7002;
const USERNAME: &str = "session-admin";
const PASSWORD: &str = "correct-password";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-admin-session-{}-{serial}.db",
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
async fn login_session_and_database_revocation_form_a_closed_chain() {
    let database = TestDatabase::new();
    seed_user(&database.url).await;
    let signing_key = URL_SAFE_NO_PAD.encode([0x42; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'admin-session-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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

    let wrong_password = send_request(
        address,
        "POST",
        "/api/auth/login",
        br#"{"username":"session-admin","password":"wrong-password"}"#,
        &[("Content-Type", "application/json")],
    )
    .await;
    let missing_user = send_request(
        address,
        "POST",
        "/api/auth/login",
        br#"{"username":"missing-user","password":"correct-password"}"#,
        &[("Content-Type", "application/json")],
    )
    .await;
    assert_eq!(wrong_password.status, 401);
    assert_eq!(wrong_password.body, missing_user.body);
    assert_eq!(
        response_json(&wrong_password)["code"],
        "invalid_credentials"
    );

    let login_request =
        serde_json::to_vec(&json!({"username": USERNAME, "password": PASSWORD})).unwrap();
    let login = send_request(
        address,
        "POST",
        "/api/auth/login",
        &login_request,
        &[("Content-Type", "application/json")],
    )
    .await;
    assert_eq!(login.status, 200);
    let login_body = response_json(&login);
    let token = login_body["access_token"].as_str().unwrap().to_owned();
    assert_eq!(login_body["user"], json!({"id":USER_ID,"role":"admin"}));
    assert!(!String::from_utf8_lossy(&login.body).contains(PASSWORD));

    let authorization = format!("Bearer {token}");
    let session = send_request(
        address,
        "GET",
        "/api/auth/session",
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(session.status, 200);
    assert_eq!(
        response_json(&session)["user"],
        json!({"id":USER_ID,"role":"admin"})
    );

    set_user_status(&database.url, 2).await;
    let revoked = send_request(
        address,
        "GET",
        "/api/auth/session",
        b"",
        &[("Authorization", &authorization)],
    )
    .await;
    assert_eq!(revoked.status, 401);
    assert_eq!(response_json(&revoked)["code"], "invalid_session");
    assert!(!String::from_utf8_lossy(&revoked.body).contains(&token));

    let method_not_allowed = send_request(address, "POST", "/api/auth/session", b"", &[]).await;
    assert_eq!(method_not_allowed.status, 405);

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

async fn seed_user(database_url: &str) {
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
                "session-group".into(),
                "Session Group".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, password_hash, role, status, default_group_id, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [
                USER_ID.into(),
                USERNAME.into(),
                password_hash().into(),
                1_i16.into(),
                1_i16.into(),
                GROUP_ID.into(),
                "session-admin-aff".into(),
                "{}".into(),
            ],
        ))
        .await
        .unwrap();
    connection.close().await.unwrap();
}

async fn set_user_status(database_url: &str, status: i16) {
    let connection = Database::connect(database_url).await.unwrap();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE users SET status = ? WHERE id = ?",
            [status.into(), USER_ID.into()],
        ))
        .await
        .unwrap();
    connection.close().await.unwrap();
}

fn password_hash() -> String {
    let salt = SaltString::encode_b64(b"anyflows-session-salt").unwrap();
    Argon2::default()
        .hash_password(PASSWORD.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn response_json(response: &support::RawResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}
