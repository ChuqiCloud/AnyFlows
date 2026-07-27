use std::{
    env, fmt,
    io::{self, Write},
    process::Command,
    sync::{Arc, Mutex},
};

use af_config::SecretString;
use serde_json::Value;
use tracing::field::Empty;
use tracing_subscriber::fmt::MakeWriter;

use crate::{
    LogLevel, RequestId, RequestIdError, TelemetryError, TelemetrySettings, init_tracing,
    logging::build_subscriber, request_span,
};

const TEST_TARGET: &str = "af_telemetry::tests";
const GLOBAL_INIT_CHILD: &str = "ANYFLOWS_TELEMETRY_INIT_CHILD";
const FORMAT_FAILURE_CHILD: &str = "ANYFLOWS_TELEMETRY_FORMAT_FAILURE_CHILD";
const FORMAT_FAILURE_CANARY: &str = "formatter-failure-secret-canary";
const SENSITIVE_DEBUG_CHILD: &str = "ANYFLOWS_TELEMETRY_SENSITIVE_DEBUG_CHILD";
const SENSITIVE_DEBUG_CANARY: &str = "sensitive-debug-panic-canary";

struct BrokenDebug;

impl fmt::Debug for BrokenDebug {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(FORMAT_FAILURE_CANARY)?;
        Err(fmt::Error)
    }
}

struct PanickingDebug;

impl fmt::Debug for PanickingDebug {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("{SENSITIVE_DEBUG_CANARY}")
    }
}

struct PanickingError;

impl fmt::Debug for PanickingError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("{SENSITIVE_DEBUG_CANARY}")
    }
}

impl fmt::Display for PanickingError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("{SENSITIVE_DEBUG_CANARY}")
    }
}

impl std::error::Error for PanickingError {}

#[derive(Clone, Default)]
struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

impl CapturedOutput {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("日志捕获缓冲区锁不应中毒").clone())
            .expect("JSON 日志必须是 UTF-8")
    }
}

impl Write for CapturedOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("日志捕获缓冲区锁已中毒"))?
            .write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("日志捕获缓冲区锁已中毒"))?
            .flush()
    }
}

impl<'writer> MakeWriter<'writer> for CapturedOutput {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

fn capture(level: LogLevel, test: impl FnOnce(&crate::TracingHandle)) -> String {
    let output = CapturedOutput::default();
    let (subscriber, handle) = build_subscriber(level, output.clone());
    tracing::subscriber::with_default(subscriber, || test(&handle));
    output.text()
}

fn json_lines(output: &str) -> Vec<Value> {
    assert!(!output.contains('\u{1b}'), "JSON 日志不得包含 ANSI 转义");
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("每行日志都必须是独立 JSON 对象"))
        .collect()
}

fn field<'a>(line: &'a Value, name: &str) -> &'a Value {
    &line["fields"][name]
}

fn emit_reload_debug(marker: &'static str) {
    tracing::debug!(target: TEST_TARGET, marker);
}

#[test]
fn info_output_is_structured_json_and_filters_debug() {
    let output = capture(LogLevel::Info, |_| {
        tracing::debug!(target: TEST_TARGET, marker = "debug-hidden");
        tracing::info!(
            target: TEST_TARGET,
            marker = "info-visible",
            count = 7_u64,
            healthy = true,
            "plain message"
        );
    });
    let lines = json_lines(&output);

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["level"], "INFO");
    assert_eq!(lines[0]["target"], TEST_TARGET);
    assert_eq!(field(&lines[0], "marker"), "info-visible");
    assert_eq!(field(&lines[0], "count"), 7);
    assert_eq!(field(&lines[0], "healthy"), true);
    assert_eq!(field(&lines[0], "message"), "plain message");
    assert!(lines[0].get("marker").is_none());
    assert!(lines[0].get("span").is_none());
    assert!(!output.contains("debug-hidden"));
}

#[test]
fn reload_updates_the_same_callsite_and_supports_off() {
    let output = capture(LogLevel::Info, |handle| {
        emit_reload_debug("debug-before-reload");
        handle.set_level(LogLevel::Debug).unwrap();
        emit_reload_debug("debug-after-reload");

        handle.set_level(LogLevel::Error).unwrap();
        tracing::warn!(target: TEST_TARGET, marker = "warn-after-error");
        tracing::error!(target: TEST_TARGET, marker = "error-visible");

        handle.set_level(LogLevel::Off).unwrap();
        tracing::error!(target: TEST_TARGET, marker = "error-after-off");

        handle.set_level(LogLevel::Info).unwrap();
        tracing::info!(target: TEST_TARGET, marker = "info-restored");
    });
    let markers = json_lines(&output)
        .into_iter()
        .map(|line| line["fields"]["marker"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        markers,
        ["debug-after-reload", "error-visible", "info-restored"]
    );
}

#[test]
fn request_span_and_nested_span_preserve_request_id() {
    let request_id = RequestId::new("req.test_01-abc").unwrap();
    let output = capture(LogLevel::Info, |_| {
        request_span(&request_id).in_scope(|| {
            tracing::info_span!(target: TEST_TARGET, "route", model = "gpt-test").in_scope(|| {
                tracing::info!(target: TEST_TARGET, marker = "nested-event");
            });
        });
    });
    let lines = json_lines(&output);
    let spans = lines[0]["spans"].as_array().unwrap();

    assert_eq!(lines.len(), 1);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["name"], "request");
    assert_eq!(spans[0]["request_id"], "req.test_01-abc");
    assert_eq!(spans[1]["name"], "route");
    assert_eq!(spans[1]["model"], "gpt-test");
    assert!(lines[0].get("span").is_none());
}

#[test]
fn error_level_keeps_request_id_but_off_disables_everything() {
    let request_id = RequestId::new("req-error-01").unwrap();
    let error_output = capture(LogLevel::Error, |_| {
        request_span(&request_id).in_scope(|| {
            tracing::trace!(target: "af_request", marker = "request-target-bypass");
            tracing::error!(target: TEST_TARGET, marker = "request-error");
        });
    });
    let lines = json_lines(&error_output);

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["spans"][0]["request_id"], "req-error-01");
    assert!(!error_output.contains("request-target-bypass"));

    let off_output = capture(LogLevel::Off, |_| {
        request_span(&request_id).in_scope(|| {
            tracing::error!(target: TEST_TARGET, marker = "off-error");
        });
    });
    assert!(off_output.is_empty());
}

#[test]
fn third_party_targets_stay_disabled_at_trace_level() {
    let request_id = RequestId::new("req-target-filter").unwrap();
    let output = capture(LogLevel::Trace, |_| {
        request_span(&request_id).in_scope(|| {
            tracing::error!(target: "reqwest", marker = "reqwest-hidden");
            tracing::error!(target: "sqlx", marker = "sqlx-hidden");
            tracing::error!(target: "sea_orm", marker = "sea-orm-hidden");
            tracing::error!(target: "third_party", marker = "third-party-hidden");
            tracing::error!(target: "af_external", marker = "prefix-hidden");
            tracing::error!(target: "af_db_evil", marker = "crate-prefix-hidden");
            tracing::error!(target: "af_request::event", marker = "request-prefix-hidden");
            tracing::info!(target: "af_db", marker = "af-db-visible");
            tracing::trace!(target: TEST_TARGET, marker = "trace-visible");
        });
    });
    let markers = json_lines(&output)
        .into_iter()
        .map(|line| line["fields"]["marker"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(markers, ["af-db-visible", "trace-visible"]);
    for hidden in [
        "reqwest-hidden",
        "sqlx-hidden",
        "sea-orm-hidden",
        "third-party-hidden",
        "prefix-hidden",
        "crate-prefix-hidden",
        "request-prefix-hidden",
    ] {
        assert!(!output.contains(hidden));
    }
}

#[test]
fn sensitive_event_span_and_recorded_fields_are_redacted() {
    let protected = SecretString::new("secret-string-canary");
    let output = capture(LogLevel::Info, |_| {
        let span = tracing::info_span!(
            target: TEST_TARGET,
            "sensitive-span",
            api_key = "span-api-key-canary",
            authorization = Empty,
            request_body = Empty,
            safe_number = 11_u64
        );
        span.record("authorization", "record-authorization-canary");
        span.record("request_body", "record-body-canary");
        span.in_scope(|| {
            tracing::info!(
                target: TEST_TARGET,
                message = "safe diagnostic",
                access_token = "event-token-canary",
                url = "https://user:event-url-canary@example.invalid/path",
                headers = "event-header-canary",
                body = "event-body-canary",
                nonce = 42_u64,
                settings = true,
                protected = ?protected,
                prompt_tokens = 17_u64,
                token_id = "safe-observation-id",
                healthy = true
            );
            tracing::info!(
                target: TEST_TARGET,
                marker = "string-counter",
                prompt_tokens = "counter-string-canary"
            );
        });
    });
    let lines = json_lines(&output);
    let event_fields = lines[0]["fields"].as_object().unwrap();
    let span = &lines[0]["spans"][0];

    for name in ["access_token", "url", "headers", "body"] {
        assert_eq!(event_fields[name], "<redacted>");
    }
    assert_eq!(event_fields["nonce"], "<redacted>");
    assert_eq!(event_fields["settings"], "<redacted>");
    assert_eq!(event_fields["message"], "safe diagnostic");
    assert_eq!(event_fields["protected"], "<redacted>");
    assert_eq!(event_fields["prompt_tokens"], 17);
    assert_eq!(event_fields["token_id"], "safe-observation-id");
    assert_eq!(event_fields["healthy"], true);
    assert_eq!(lines[1]["fields"]["prompt_tokens"], "<redacted>");
    assert_eq!(span["api_key"], "<redacted>");
    assert_eq!(span["authorization"], "<redacted>");
    assert_eq!(span["request_body"], "<redacted>");
    assert_eq!(span["safe_number"], 11);

    for canary in [
        "secret-string-canary",
        "span-api-key-canary",
        "record-authorization-canary",
        "record-body-canary",
        "event-token-canary",
        "event-url-canary",
        "event-header-canary",
        "event-body-canary",
        "counter-string-canary",
    ] {
        assert!(!output.contains(canary), "日志泄露了测试敏感值: {canary}");
    }
}

#[test]
fn sensitive_debug_values_are_not_invoked_in_an_isolated_process() {
    if env::var_os(SENSITIVE_DEBUG_CHILD).is_some() {
        let output = capture(LogLevel::Info, |_| {
            tracing::info!(
                target: TEST_TARGET,
                marker = "sensitive-event",
                api_key = ?PanickingDebug
            );
            tracing::error!(
                target: TEST_TARGET,
                marker = "sensitive-error",
                error = ?PanickingDebug
            );
            tracing::error!(
                target: TEST_TARGET,
                marker = "raw-error",
                raw_error = ?PanickingError,
                error_trait = &PanickingError as &(dyn std::error::Error + 'static)
            );

            tracing::info_span!(
                target: TEST_TARGET,
                "sensitive-create",
                password = ?PanickingDebug
            )
            .in_scope(|| tracing::info!(target: TEST_TARGET, marker = "sensitive-create"));

            let recorded = tracing::info_span!(
                target: TEST_TARGET,
                "sensitive-record",
                authorization = Empty
            );
            recorded.record("authorization", tracing::field::debug(&PanickingDebug));
            recorded.in_scope(|| {
                tracing::info!(target: TEST_TARGET, marker = "sensitive-record");
            });
        });

        assert!(!output.contains(SENSITIVE_DEBUG_CANARY));
        let lines = json_lines(&output);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0]["fields"]["api_key"], "<redacted>");
        assert_eq!(lines[1]["fields"]["error"], "<redacted>");
        assert_eq!(lines[2]["fields"]["raw_error"], "<redacted>");
        assert_eq!(lines[2]["fields"]["error_trait"], "<redacted>");
        assert_eq!(lines[3]["spans"][0]["password"], "<redacted>");
        assert_eq!(lines[4]["spans"][0]["authorization"], "<redacted>");
        return;
    }

    let output = Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("tests::sensitive_debug_values_are_not_invoked_in_an_isolated_process")
        .arg("--nocapture")
        .env_clear()
        .env(SENSITIVE_DEBUG_CHILD, "1")
        .output()
        .expect("必须能启动隔离敏感 Debug 测试进程");
    let rendered = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "隔离测试失败: {rendered}");
    assert!(!rendered.contains(SENSITIVE_DEBUG_CANARY));
}

#[test]
fn sensitive_name_matching_preserves_observability_fields() {
    for name in [
        "API-Key",
        "access_token.value",
        "api_key.value",
        "http.request.body",
        "r#password",
        "service_access_token",
        "accessToken",
        "authorization.value",
        "clientSecret",
        "db.connection_string",
        "error",
        "error.message",
        "last_error",
        "private_key.value",
        "request_body_content",
        "refreshToken",
        "secret_error_kind",
        "secret_prompt_tokens",
        "upstream_error",
        "url.full",
        "database-url",
        "X-Goog-Api-Key",
    ] {
        assert!(
            crate::fields::is_sensitive_field(name),
            "应脱敏字段: {name}"
        );
    }
    for name in [
        "request_id",
        "user_id",
        "prompt_tokens",
        "usage.prompt_tokens",
        "completion_tokens",
        "token_id",
        "error_code",
        "has_error",
        "message",
        "reason",
        "upstream_error_kind",
        "model",
    ] {
        assert!(
            !crate::fields::is_sensitive_field(name),
            "不应脱敏观测字段: {name}"
        );
    }
}

#[test]
fn formatter_failures_replace_raw_values_in_an_isolated_process() {
    if env::var_os(FORMAT_FAILURE_CHILD).is_some() {
        let output = capture(LogLevel::Info, |_| {
            tracing::info!(target: TEST_TARGET, risky = ?BrokenDebug);

            tracing::info_span!(target: TEST_TARGET, "broken-create", risky = ?BrokenDebug)
                .in_scope(|| tracing::info!(target: TEST_TARGET, marker = "after-create"));

            let recorded = tracing::info_span!(target: TEST_TARGET, "broken-record", risky = Empty);
            recorded.record("risky", tracing::field::debug(&BrokenDebug));
            recorded.in_scope(|| {
                tracing::info!(target: TEST_TARGET, marker = "after-record");
            });
        });

        assert!(!output.contains(FORMAT_FAILURE_CANARY));
        let lines = json_lines(&output);
        assert_eq!(lines.len(), 3);
        assert_eq!(field(&lines[0], "risky"), "<redacted>");
        assert_eq!(field(&lines[1], "marker"), "after-create");
        assert_eq!(field(&lines[2], "marker"), "after-record");
        return;
    }

    let output = Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("tests::formatter_failures_replace_raw_values_in_an_isolated_process")
        .arg("--nocapture")
        .env_clear()
        .env(FORMAT_FAILURE_CHILD, "1")
        .output()
        .expect("必须能启动隔离 formatter 测试进程");
    let rendered = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "隔离测试失败: {rendered}");
    assert!(!rendered.contains(FORMAT_FAILURE_CANARY));
}

#[test]
fn request_id_accepts_only_canonical_server_values() {
    assert_eq!(RequestId::new("a").unwrap().as_str(), "a");
    assert!(RequestId::new("A".repeat(64)).is_ok());

    for invalid in [
        String::new(),
        "A".repeat(65),
        "has space".to_owned(),
        "path/value".to_owned(),
        "中文".to_owned(),
        "line\r\nbreak".to_owned(),
        "key:value".to_owned(),
    ] {
        let error = RequestId::new(invalid.clone()).unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error, RequestIdError::Invalid);
        if !invalid.is_empty() {
            assert!(!rendered.contains(&invalid));
        }
    }
}

#[test]
fn dropped_subscriber_returns_typed_reload_error() {
    let (subscriber, handle) = build_subscriber(LogLevel::Info, CapturedOutput::default());
    drop(subscriber);

    assert_eq!(
        handle.set_level(LogLevel::Debug),
        Err(TelemetryError::ReloadFailed)
    );
}

#[test]
fn repeated_global_initialization_is_typed_in_child_process() {
    if env::var_os(GLOBAL_INIT_CHILD).is_some() {
        let first = init_tracing(&TelemetrySettings::new(LogLevel::Info)).unwrap();
        first.set_level(LogLevel::Debug).unwrap();
        let error = init_tracing(&TelemetrySettings::new(LogLevel::Trace)).unwrap_err();
        assert_eq!(error, TelemetryError::InitializationConflict);
        assert_eq!(error.to_string(), "全局 tracing 已初始化");
        first.set_level(LogLevel::Error).unwrap();
        return;
    }

    let output = Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("tests::repeated_global_initialization_is_typed_in_child_process")
        .arg("--nocapture")
        .env_clear()
        .env(GLOBAL_INIT_CHILD, "1")
        .output()
        .expect("必须能启动隔离 tracing 测试进程");
    assert!(
        output.status.success(),
        "隔离测试失败: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
