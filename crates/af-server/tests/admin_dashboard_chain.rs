pub mod support;

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
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

// 真实 ClickHouse 门禁使用固定查询构造一行闭合快照，避免依赖外部业务表结构。
const REAL_CLICKHOUSE_QUERY: &str = r#"
SELECT
    CAST(
        (5, 55, 4, 1, 3, 1, 1),
        'Tuple(request_count Int64, quota_consumed Int64, upstream_usage_count Int64, estimated_usage_count Int64, per_token_request_count Int64, per_call_request_count Int64, free_request_count Int64)'
    ) AS usage,
    CAST(
        (
            5,
            4,
            1,
            3,
            [CAST(('invalid_request', 1), 'Tuple(kind String, request_count Int64)')],
            [CAST(('openai_chat', 7508, 'ClickHouse channel', 1), 'Tuple(protocol String, channel_id Int64, channel_name String, request_count Int64)')],
            1,
            55,
            [CAST((7502, 7503, 'Dashboard', 7508, 'ClickHouse channel', 'gpt-5', 1, 55), 'Tuple(user_id Int64, group_id Int64, group_name String, channel_id Int64, channel_name String, model String, request_count Int64, quota_consumed Int64)')]
        ),
        'Tuple(request_count Int64, successful_request_count Int64, failed_request_count Int64, other_success_count Int64, failures Array(Tuple(kind String, request_count Int64)), channel_flows Array(Tuple(protocol String, channel_id Int64, channel_name String, request_count Int64)), flow_request_count Int64, flow_quota_consumed Int64, flow_paths Array(Tuple(user_id Int64, group_id Int64, group_name String, channel_id Int64, channel_name String, model String, request_count Int64, quota_consumed Int64)))'
    ) AS outcomes,
    arrayMap(
        i -> CAST(
            (
                toInt64({period_start:Int64}) + toInt64(i) * 3600,
                if(i = 23, toInt64({period_end:Int64}), toInt64({period_start:Int64}) + toInt64(i) * 3600 + 3600),
                if(i = 0, toInt64(5), toInt64(0)),
                if(i = 0, toInt64(55), toInt64(0))
            ),
            'Tuple(period_start Int64, period_end Int64, request_count Int64, quota_consumed Int64)'
        ),
        range(24)
    ) AS hourly,
    CAST(
        (2, CAST(900 AS Nullable(Int64)), 0, 2000, 4, CAST(5000 AS Nullable(Int64)), 1, 10000),
        'Tuple(first_token_sample_count Int64, average_first_token_ms Nullable(Int64), slow_first_token_count Int64, slow_first_token_threshold_ms Int64, duration_sample_count Int64, average_duration_ms Nullable(Int64), slow_request_count Int64, slow_request_threshold_ms Int64)'
    ) AS performance
"#;

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
    assert_eq!(body["per_call_request_count"], 0);
    assert_eq!(body["free_request_count"], 1);
    assert_eq!(body["enabled_channel_count"], 1);
    assert_eq!(body["disabled_channel_count"], 1);
    assert_eq!(body["auto_disabled_channel_count"], 1);
    let hourly = body["hourly"].as_array().unwrap();
    assert_eq!(hourly.len(), 24);
    assert_eq!(
        hourly
            .iter()
            .map(|bucket| bucket["request_count"].as_i64().unwrap())
            .sum::<i64>(),
        2
    );
    assert_eq!(
        hourly
            .iter()
            .map(|bucket| bucket["quota_consumed"].as_i64().unwrap())
            .sum::<i64>(),
        120
    );
    assert_eq!(body["performance"]["first_token_sample_count"], 1);
    assert_eq!(body["performance"]["average_first_token_ms"], 800);
    assert_eq!(body["performance"]["slow_first_token_count"], 0);
    assert_eq!(body["performance"]["slow_first_token_threshold_ms"], 2_000);
    assert_eq!(body["performance"]["duration_sample_count"], 2);
    assert_eq!(body["performance"]["average_duration_ms"], 8_000);
    assert_eq!(body["performance"]["slow_request_count"], 1);
    assert_eq!(body["performance"]["slow_request_threshold_ms"], 10_000);
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

    let service_levels = send_request(
        address,
        "GET",
        "/api/admin/dashboard/service-levels?dimension=model&page_size=5",
        b"",
        &[("Authorization", &admin_authorization)],
    )
    .await;
    assert_eq!(service_levels.status, 200);
    assert_eq!(service_levels.headers["cache-control"], "no-store");
    let report = response_json(&service_levels);
    assert_eq!(
        report["period_end"].as_i64().unwrap() - report["period_start"].as_i64().unwrap(),
        86_400
    );
    assert_eq!(report["total"], 1);
    assert_eq!(report["items"][0]["successful_request_count"], 1);
    assert_eq!(report["items"][0]["failed_request_count"], 1);
    assert_eq!(report["items"][0]["unknown_request_count"], 1);
    for invalid in ["page=0", "page_size=21", "dimension=user", "page=1&page=2"] {
        let response = send_request(
            address,
            "GET",
            &format!("/api/admin/dashboard/service-levels?{invalid}"),
            b"",
            &[("Authorization", &admin_authorization)],
        )
        .await;
        assert_eq!(response.status, 400);
    }
    let forbidden_sla = send_request(
        address,
        "GET",
        "/api/admin/dashboard/service-levels",
        b"",
        &[("Authorization", &member_authorization)],
    )
    .await;
    assert_eq!(forbidden_sla.status, 403);
    let anonymous_sla = send_request(
        address,
        "GET",
        "/api/admin/dashboard/service-levels",
        b"",
        &[],
    )
    .await;
    assert_eq!(anonymous_sla.status, 401);

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
}

#[tokio::test]
async fn configured_clickhouse_supplies_analytics_while_database_supplies_channels() {
    let database = TestDatabase::new();
    seed_dashboard(&database.url).await;
    let live_clickhouse = std::env::var("ANYFLOWS_CLICKHOUSE_INTEGRATION_URL").ok();
    let (
        clickhouse_endpoint,
        clickhouse_query,
        clickhouse_username,
        clickhouse_password,
        http_client_config,
        clickhouse,
    ) = if let Some(endpoint) = live_clickhouse {
        let (proxy_address, proxy_handle) = spawn_clickhouse_forwarder(&endpoint);
        (
            endpoint,
            REAL_CLICKHOUSE_QUERY.to_owned(),
            std::env::var("ANYFLOWS_CLICKHOUSE_INTEGRATION_USERNAME")
                .unwrap_or_else(|_| "default".to_owned()),
            std::env::var("ANYFLOWS_CLICKHOUSE_INTEGRATION_PASSWORD").unwrap_or_default(),
            format!(
                "[http_client]\nproxy_url = 'http://{proxy_address}'\ntrust_proxy_dns = true\n"
            ),
            Some(proxy_handle),
        )
    } else {
        let (address, handle) = spawn_clickhouse_snapshot();
        (
            "http://clickhouse.example".to_owned(),
            "SELECT {period_start:Int64}, {period_end:Int64}".to_owned(),
            "dashboard-reader".to_owned(),
            "dashboard-secret".to_owned(),
            format!("[http_client]\nproxy_url = 'http://{address}'\ntrust_proxy_dns = true\n"),
            Some(handle),
        )
    };
    let signing_key = URL_SAFE_NO_PAD.encode([0x45; 32]);
    let config_path = database.path.with_extension("toml");
    fs::write(
        &config_path,
        format!(
            "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{}'\nhealth_check_timeout_secs = 1\n[billing]\nwal_directory = '{}'\n[credential_encryption]\nkey_id = 'clickhouse-dashboard-test'\nkey = '{signing_key}'\n[auth]\nlookup_timeout_secs = 2\nsession_signing_key = '{signing_key}'\nsession_ttl_secs = 3600\n{http_client_config}[clickhouse_analytics]\nendpoint = '{clickhouse_endpoint}'\nquery = '''\n{clickhouse_query}\n'''\nusername = '{clickhouse_username}'\npassword = '{clickhouse_password}'\ntimeout_secs = 2\nmax_response_bytes = 65536\n",
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
    assert_eq!(
        response.status,
        200,
        "ClickHouse 集成响应异常: {}",
        String::from_utf8_lossy(&response.body)
    );
    let body = response_json(&response);
    assert_eq!(body["request_count"], 5);
    assert_eq!(body["quota_consumed"], 55);
    assert_eq!(body["outcome_request_count"], 5);
    assert_eq!(body["successful_request_count"], 4);
    assert_eq!(body["failed_request_count"], 1);
    // 渠道状态来自 SQLite，证明 ClickHouse 不能覆盖当前配置权威。
    assert_eq!(body["enabled_channel_count"], 1);
    assert_eq!(body["disabled_channel_count"], 1);
    assert_eq!(body["auto_disabled_channel_count"], 1);

    shutdown.send(()).unwrap();
    let report = timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap();
    assert_eq!(report.http, ServeOutcome::Drained);
    assert_eq!(report.background, SupervisorShutdown::Drained);
    if let Some(clickhouse) = clickhouse {
        let request = clickhouse.join().unwrap();
        let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(request.contains("readonly=2"));
        assert!(request.contains("max_result_rows=1"));
        assert!(request.contains(&format!(
            "x-clickhouse-user: {}",
            clickhouse_username.to_ascii_lowercase()
        )));
        assert!(request.contains("x-clickhouse-key:"));
        assert!(request.contains("format jsoneachrow"));
    }
}

/// 真实集成门禁使用一次性本地 HTTP 转发器，保留生产 Client 的代理与 SSRF 校验。
fn spawn_clickhouse_forwarder(endpoint: &str) -> (SocketAddr, thread::JoinHandle<Vec<u8>>) {
    let upstream = url::Url::parse(endpoint).expect("ClickHouse 集成端点有效");
    let host = upstream.host_str().unwrap().to_owned();
    let port = upstream.port_or_known_default().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + IO_TIMEOUT;
        let (mut client, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "ClickHouse 转发器等待连接超时");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("ClickHouse 转发器接受连接失败: {error}"),
            }
        };
        client.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        client.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let request = read_http_request(&mut client);
        let forwarded = rewrite_proxy_request(&request);
        let mut upstream_stream = std::net::TcpStream::connect((host.as_str(), port)).unwrap();
        upstream_stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        upstream_stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        upstream_stream.write_all(&forwarded).unwrap();
        upstream_stream.shutdown(std::net::Shutdown::Write).unwrap();
        let response = read_http_response(&mut upstream_stream);
        let response_status = std::str::from_utf8(&response)
            .ok()
            .and_then(|response| response.lines().next())
            .unwrap_or("<invalid response>");
        assert!(
            response_status.contains(" 200 "),
            "ClickHouse 集成上游返回异常: {response_status}\n{}",
            String::from_utf8_lossy(&response)
        );
        client.write_all(&response).unwrap();
        client.flush().unwrap();
        request
    });
    (address, handle)
}

fn rewrite_proxy_request(request: &[u8]) -> Vec<u8> {
    let header_end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("代理请求包含完整请求头")
        + 4;
    let headers = std::str::from_utf8(&request[..header_end]).expect("代理请求头为 UTF-8");
    let mut lines = headers.split("\r\n");
    let request_line = lines.next().unwrap();
    let target = request_line.split_whitespace().nth(1).unwrap();
    let target = url::Url::parse(target).expect("代理请求目标为绝对 URL");
    let path = if target.path().is_empty() {
        "/"
    } else {
        target.path()
    };
    let target = target
        .query()
        .map_or_else(|| path.to_owned(), |query| format!("{path}?{query}"));
    let method = request_line.split_whitespace().next().unwrap();
    let version = request_line.split_whitespace().nth(2).unwrap_or("HTTP/1.1");
    let mut forwarded = format!("{method} {target} {version}\r\n").into_bytes();
    let mut has_connection = false;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if line
            .split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("proxy-connection"))
        {
            continue;
        }
        if line
            .split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("connection"))
        {
            forwarded.extend_from_slice(b"Connection: close\r\n");
            has_connection = true;
        } else {
            forwarded.extend_from_slice(line.as_bytes());
            forwarded.extend_from_slice(b"\r\n");
        }
    }
    if !has_connection {
        forwarded.extend_from_slice(b"Connection: close\r\n");
    }
    forwarded.extend_from_slice(b"\r\n");
    forwarded.extend_from_slice(&request[header_end..]);
    forwarded
}

fn read_http_response(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected_length = None;
    loop {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            return response;
        }
        response.extend_from_slice(&buffer[..read]);
        if expected_length.is_none()
            && let Some(header_end) = response.windows(4).position(|bytes| bytes == b"\r\n\r\n")
        {
            let header_end = header_end + 4;
            let headers = std::str::from_utf8(&response[..header_end]).unwrap();
            let content_length = headers.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
            });
            expected_length = content_length.map(|length| header_end + length);
        }
        if expected_length.is_some_and(|length| response.len() >= length) {
            return response;
        }
    }
}

fn spawn_clickhouse_snapshot() -> (SocketAddr, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + IO_TIMEOUT;
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "ClickHouse 模拟服务等待连接超时");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("ClickHouse 模拟服务接受连接失败: {error}"),
            }
        };
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let request = read_http_request(&mut stream);
        let request_line = std::str::from_utf8(&request)
            .unwrap()
            .lines()
            .next()
            .unwrap();
        let target = request_line.split_whitespace().nth(1).unwrap();
        // 模拟服务同时充当受信 HTTP 代理，避免为集成测试放宽生产目标地址策略。
        let target = url::Url::parse(target)
            .or_else(|_| url::Url::parse(&format!("http://localhost{target}")))
            .unwrap();
        let parameters = target
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        let period_start = parameters["param_period_start"].parse::<i64>().unwrap();
        let period_end = parameters["param_period_end"].parse::<i64>().unwrap();
        assert_eq!(period_end - period_start, 86_400);
        let hourly = (0..24)
            .map(|index| {
                let start = period_start + i64::from(index) * 3_600;
                serde_json::json!({
                    "period_start": start,
                    "period_end": start + 3_600,
                    "request_count": if index == 0 { 5 } else { 0 },
                    "quota_consumed": if index == 0 { 55 } else { 0 }
                })
            })
            .collect::<Vec<_>>();
        let snapshot = serde_json::json!({
            "usage": {
                "request_count": 5,
                "quota_consumed": 55,
                "upstream_usage_count": 4,
                "estimated_usage_count": 1,
                "per_token_request_count": 3,
                "per_call_request_count": 1,
                "free_request_count": 1
            },
            "outcomes": {
                "request_count": 5,
                "successful_request_count": 4,
                "failed_request_count": 1,
                "other_success_count": 3,
                "failures": [{"kind": "invalid_request", "request_count": 1}],
                "channel_flows": [{
                    "protocol": "openai_chat",
                    "channel_id": 7508,
                    "channel_name": "ClickHouse channel",
                    "request_count": 1
                }]
            },
            "hourly": hourly,
            "performance": {
                "first_token_sample_count": 2,
                "average_first_token_ms": 900,
                "slow_first_token_count": 0,
                "slow_first_token_threshold_ms": 2000,
                "duration_sample_count": 4,
                "average_duration_ms": 5000,
                "slow_request_count": 1,
                "slow_request_threshold_ms": 10000
            }
        });
        let mut body = serde_json::to_vec(&snapshot).unwrap();
        body.push(b'\n');
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        stream.flush().unwrap();
        request
    });
    (address, handle)
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected_length = None;
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "ClickHouse 请求在正文完成前关闭");
        request.extend_from_slice(&buffer[..read]);
        if expected_length.is_none()
            && let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
        {
            let header_end = header_end + 4;
            let headers = std::str::from_utf8(&request[..header_end]).unwrap();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or(0);
            expected_length = Some(header_end + content_length);
        }
        if expected_length.is_some_and(|length| request.len() >= length) {
            return request;
        }
    }
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
    // 与生产写入保持同一时间编码，避免 SQLite 文本日期格式混用导致午夜边界误判。
    let usage_time = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    for (id, age_seconds, source, billing_mode, quota, first_token_ms, duration_ms) in [
        (
            7_505_i64,
            60 * 60_i64,
            1_i16,
            1_i16,
            120_i64,
            Some(800_i64),
            Some(4_000_i64),
        ),
        (7_506, 2 * 60 * 60, 2, 2, 0, None, Some(12_000)),
        (7_507, 25 * 60 * 60, 1, 1, 999, Some(9_000), Some(30_000)),
    ] {
        let created_at = TimeDateTimeWithTimeZone::from_unix_timestamp(
            usage_time
                .checked_sub(age_seconds)
                .expect("测试时间必须位于有效范围"),
        )
        .expect("测试时间必须可转换为 UTC 时间");
        connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO usage_logs (id, event_id, event_type, user_id, token_id, group_id, billing_mode, input_tokens, output_tokens, cache_read, cache_creation_5m, cache_creation_1h, reasoning_tokens, audio_input_tokens, audio_output_tokens, first_token_ms, duration_ms, usage_source, usage_semantics, quota, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                id.into(), format!("{id:032x}").into(), 1_i16.into(), MEMBER_ID.into(),
                TOKEN_ID.into(), GROUP_ID.into(), billing_mode.into(), 10_i64.into(), 2_i64.into(),
                0_i64.into(), 0_i64.into(), 0_i64.into(), 0_i64.into(), 0_i64.into(),
                0_i64.into(), first_token_ms.into(), duration_ms.into(), source.into(),
                1_i16.into(), quota.into(), created_at.into(),
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
    for (id, outcome, error_kind) in [
        (7_512_i64, 1_i16, None),
        (7_513, 2, Some("upstream_network")),
        (7_514, 2, Some("outcome_unknown")),
    ] {
        connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO request_outcome_logs (id, request_id, protocol, operation, model, outcome, error_kind, channel_id, duration_ms, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [id.into(), format!("sla-http-{id}").into(), "openai_chat".into(), "chat".into(), "gpt-test".into(), outcome.into(), error_kind.into(), 7_508_i64.into(), 1_000_i64.into(), TimeDateTimeWithTimeZone::from_unix_timestamp(usage_time - 60).unwrap().into()],
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
