pub mod support;

use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use af_http::ServeOutcome;
use af_server::{Bootstrap, SupervisorShutdown};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::{Value, json};
use support::{IO_TIMEOUT, send_request};
use tokio::{sync::oneshot, time::timeout};

const USERNAME: &str = "owner";
const PASSWORD: &str = "correct horse battery staple";

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-initial-setup-{}-{serial}.db",
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
async fn setup_is_atomic_login_ready_and_permanently_closed() {
    let database = TestDatabase::new();
    let signing_key = URL_SAFE_NO_PAD.encode([0x57; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'initial-setup-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 5\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n",
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

    let before = send_request(address, "GET", "/api/setup/status", b"", &[]).await;
    assert_eq!(before.status, 200);
    assert_eq!(response_json(&before), json!({"setup_required":true}));
    assert_eq!(
        before.headers.get("cache-control").map(String::as_str),
        Some("no-store")
    );

    let request = serde_json::to_vec(&json!({
        "username": USERNAME,
        "password": PASSWORD,
    }))
    .unwrap();
    let (first, second) = tokio::join!(
        send_request(
            address,
            "POST",
            "/api/setup",
            &request,
            &[("Content-Type", "application/json")],
        ),
        send_request(
            address,
            "POST",
            "/api/setup",
            &request,
            &[("Content-Type", "application/json")],
        ),
    );
    let (created, conflict) = if first.status == 200 {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(created.status, 200);
    assert_eq!(conflict.status, 409);
    assert_eq!(response_json(&conflict)["code"], "setup_conflict");
    assert_eq!(response_json(&created)["user"]["role"], "admin");
    assert!(!String::from_utf8_lossy(&created.body).contains(PASSWORD));

    let token = response_json(&created)["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
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
    assert_eq!(response_json(&session)["user"]["role"], "admin");

    let after = send_request(address, "GET", "/api/setup/status", b"", &[]).await;
    assert_eq!(response_json(&after), json!({"setup_required":false}));

    let connection = Database::connect(&database.url).await.unwrap();
    assert_eq!(row_count(&connection, "users").await, 1);
    assert_eq!(row_count(&connection, "groups").await, 1);
    connection.close().await.unwrap();

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

fn response_json(response: &support::RawResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}

async fn row_count(connection: &sea_orm::DatabaseConnection, table: &str) -> i64 {
    let sql = match table {
        "users" => "SELECT COUNT(*) AS row_count FROM users",
        "groups" => "SELECT COUNT(*) AS row_count FROM groups",
        _ => panic!("测试只允许读取已知身份表"),
    };
    connection
        .query_one(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .unwrap()
        .expect("计数查询必须返回一行")
        .try_get("", "row_count")
        .unwrap()
}
