use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use figment::{
    Figment,
    error::Kind,
    providers::{Env, Format, Serialized, Toml},
};
use serde::Serialize;

use crate::{AppConfig, AuthSettings, ConfigError, ServerConfig, TelemetrySettings};

/// 默认配置文件名；文件不存在时仅使用默认值与环境变量。
pub const DEFAULT_CONFIG_FILE: &str = "anyflows.toml";
/// 配置文件选择环境变量。
pub const CONFIG_FILE_ENV: &str = "AF_CONFIG_FILE";
/// 业务配置环境变量前缀；该命名空间中的未知键会拒绝启动。
pub const ENV_PREFIX: &str = "AF_";

#[derive(Default, Serialize)]
struct ConfigDefaults {
    server: ServerConfig,
    telemetry: TelemetrySettings,
    database: DatabaseDefaults,
    redis: RedisDefaults,
    auth: AuthSettings,
    http_client: HttpClientDefaults,
}

#[derive(Default, Serialize)]
struct DatabaseDefaults {
    migration_timeout_secs: Option<u64>,
}

#[derive(Default, Serialize)]
struct RedisDefaults {
    url: Option<String>,
}

#[derive(Default, Serialize)]
struct HttpClientDefaults {
    max_cached_clients: Option<usize>,
    connect_timeout_secs: Option<u64>,
    read_timeout_secs: Option<u64>,
    request_timeout_secs: Option<u64>,
    proxy_url: Option<String>,
    trust_proxy_dns: bool,
}

/// 加载默认文件与环境变量覆盖后的应用配置。
pub fn load() -> Result<AppConfig, ConfigError> {
    match env::var_os(CONFIG_FILE_ENV) {
        Some(path) if path.is_empty() => Err(ConfigError::EmptyValue {
            field: CONFIG_FILE_ENV,
        }),
        Some(path) => load_file(PathBuf::from(path), true),
        None => load_file(PathBuf::from(DEFAULT_CONFIG_FILE), false),
    }
}

/// 从显式 TOML 文件加载配置；文件不存在时返回明确错误。
pub fn load_from(path: impl AsRef<Path>) -> Result<AppConfig, ConfigError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(ConfigError::EmptyValue {
            field: "config_file",
        });
    }
    load_file(path.to_path_buf(), true)
}

fn load_file(path: PathBuf, required: bool) -> Result<AppConfig, ConfigError> {
    let toml = match fs::read_to_string(&path) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == ErrorKind::NotFound && !required => None,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(ConfigError::FileNotFound);
        }
        Err(error) => {
            return Err(ConfigError::FileRead { kind: error.kind() });
        }
    };

    let mut figment = Figment::from(Serialized::defaults(ConfigDefaults::default()));
    if let Some(contents) = toml {
        // 先读取文件再交给 Figment，便于把缺失、权限和编码错误区分开。
        figment = figment.merge(Toml::string(&contents));
    }
    let config = figment
        .merge(environment_provider())
        .extract::<AppConfig>()
        .map_err(redact_figment_error)?;
    config.validate()?;
    Ok(config)
}

fn environment_provider() -> Env {
    Env::prefixed(ENV_PREFIX)
        // CI 的 AF_TEST_* 等变量只控制测试门控，不应成为业务配置字段。
        .ignore(&[
            "config_file",
            "test_database_url",
            "require_live_database",
            "allow_destructive_migration_test",
            "test_redis_url",
            "require_live_redis",
        ])
        .split("__")
}

fn redact_figment_error(error: figment::Error) -> ConfigError {
    let mut path = error.path.clone();
    if let Kind::MissingField(field) = error.kind {
        push_field(&mut path, field.into_owned());
    }
    match known_field(&path.join(".")) {
        Some(field) => ConfigError::InvalidField { field },
        None => ConfigError::Invalid,
    }
}

fn push_field(path: &mut Vec<String>, field: String) {
    if path.last() != Some(&field) {
        path.push(field);
    }
}

fn known_field(path: &str) -> Option<&'static str> {
    Some(match path {
        "server" => "server",
        "server.bind" => "server.bind",
        "server.shutdown_timeout_secs" => "server.shutdown_timeout_secs",
        path if path == "server.cors_allowed_origins"
            || path.starts_with("server.cors_allowed_origins.") =>
        {
            "server.cors_allowed_origins"
        }
        "telemetry" => "telemetry",
        "telemetry.level" => "telemetry.level",
        "database" => "database",
        "database.url" => "database.url",
        "database.migration_timeout_secs" => "database.migration_timeout_secs",
        "database.health_check_timeout_secs" => "database.health_check_timeout_secs",
        "redis" => "redis",
        "redis.url" => "redis.url",
        "auth" => "auth",
        "auth.allow_query_api_key" => "auth.allow_query_api_key",
        "auth.lookup_timeout_secs" => "auth.lookup_timeout_secs",
        "openai_upstream" => "openai_upstream",
        "openai_upstream.base_url" => "openai_upstream.base_url",
        "openai_upstream.model" => "openai_upstream.model",
        "openai_upstream.api_key" => "openai_upstream.api_key",
        "http_client" => "http_client",
        "http_client.max_cached_clients" => "http_client.max_cached_clients",
        "http_client.connect_timeout_secs" => "http_client.connect_timeout_secs",
        "http_client.read_timeout_secs" => "http_client.read_timeout_secs",
        "http_client.request_timeout_secs" => "http_client.request_timeout_secs",
        "http_client.proxy_url" => "http_client.proxy_url",
        "http_client.trust_proxy_dns" => "http_client.trust_proxy_dns",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use figment::Jail;

    use super::*;
    use crate::LogLevel;

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
    const CHILD_MARKER: &str = "ANYFLOWS_CONFIG_CHILD";
    const TEST_OPENAI_UPSTREAM: &str = "\n[openai_upstream]\nbase_url = 'https://upstream.example'\nmodel = 'test-model'\napi_key = 'test-upstream-key'\n";

    fn temp_file(contents: &str) -> PathBuf {
        raw_temp_file(&format!("{contents}{TEST_OPENAI_UPSTREAM}"))
    }

    fn raw_temp_file(contents: &str) -> PathBuf {
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "anyflows-config-{}-{serial}.toml",
            std::process::id()
        ));
        fs::write(&path, contents).expect("测试配置文件必须可写");
        path
    }

    #[allow(
        clippy::result_large_err,
        reason = "Figment Jail 的固定回调签名按值返回解析错误"
    )]
    fn with_clean_env(test: impl FnOnce(&mut Jail)) {
        Jail::expect_with(|jail| {
            jail.clear_env();
            test(jail);
            Ok(())
        });
    }

    #[test]
    fn defaults_are_applied_when_toml_omits_optional_sections() {
        with_clean_env(|_| {
            let path = temp_file("[database]\nurl = 'sqlite::memory:'\n");
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);

            assert_eq!(config.server().bind().to_string(), "127.0.0.1:8080");
            assert!(config.server().cors_allowed_origins().is_empty());
            assert_eq!(config.server().shutdown_timeout_secs(), 30);
            assert_eq!(config.telemetry().level(), LogLevel::Info);
            assert_eq!(config.database().migration_timeout_secs(), None);
            assert_eq!(config.database().health_check_timeout_secs(), None);
            assert!(config.redis().url().is_none());
            assert!(!config.auth().allow_query_api_key());
            assert_eq!(config.auth().lookup_timeout_secs(), 2);
            assert_eq!(
                config.openai_upstream().base_url(),
                "https://upstream.example"
            );
            assert_eq!(config.openai_upstream().model(), "test-model");
            assert_eq!(
                config.openai_upstream().api_key().expose(),
                "test-upstream-key"
            );
            assert_eq!(config.http_client().max_cached_clients(), None);
            assert_eq!(config.http_client().request_timeout_secs(), None);
            assert!(config.http_client().proxy_url().is_none());
            assert!(!config.http_client().trust_proxy_dns());
        });
    }

    #[test]
    fn base_authentication_still_rejects_non_loopback_bind() {
        with_clean_env(|_| {
            for bind in ["0.0.0.0:8080", "[::]:8080", "192.0.2.10:8080"] {
                let path = temp_file(&format!(
                    "[server]\nbind = '{bind}'\n[database]\nurl = 'sqlite::memory:'\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(
                    error,
                    ConfigError::InvalidField {
                        field: "server.bind"
                    }
                );
            }
        });
    }

    #[test]
    fn openai_upstream_is_required_and_each_field_is_typed() {
        with_clean_env(|_| {
            let missing_section = raw_temp_file("[database]\nurl = 'sqlite::memory:'\n");
            let error = load_from(&missing_section).unwrap_err();
            let _ = fs::remove_file(missing_section);
            assert_eq!(
                error,
                ConfigError::InvalidField {
                    field: "openai_upstream"
                }
            );

            for (field, body) in [
                (
                    "openai_upstream.base_url",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nmodel = 'test-model'\napi_key = 'test-key'\n",
                ),
                (
                    "openai_upstream.model",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = 'https://upstream.example'\napi_key = 'test-key'\n",
                ),
                (
                    "openai_upstream.api_key",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = 'https://upstream.example'\nmodel = 'test-model'\n",
                ),
            ] {
                let path = raw_temp_file(body);
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::InvalidField { field });
            }
        });
    }

    #[test]
    fn rejects_invalid_openai_upstream_without_echoing_values() {
        with_clean_env(|_| {
            for (field, body, canary) in [
                (
                    "openai_upstream.base_url",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = 'https://user:base-url-secret@upstream.example'\nmodel = 'test-model'\napi_key = 'test-key'\n",
                    "base-url-secret",
                ),
                (
                    "openai_upstream.model",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = 'https://upstream.example'\nmodel = ' model-secret '\napi_key = 'test-key'\n",
                    "model-secret",
                ),
                (
                    "openai_upstream.api_key",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = 'https://upstream.example'\nmodel = 'test-model'\napi_key = ' key-secret '\n",
                    "key-secret",
                ),
                (
                    "openai_upstream.api_key",
                    "[database]\nurl = 'sqlite::memory:'\n[openai_upstream]\nbase_url = 'https://upstream.example'\nmodel = 'test-model'\napi_key = '非 ASCII 密钥'\n",
                    "非 ASCII 密钥",
                ),
            ] {
                let path = raw_temp_file(body);
                let error = load_from(&path).unwrap_err();
                let rendered = format!("{error:?}\n{error}");
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::InvalidField { field });
                assert!(!rendered.contains(canary));
            }
        });
    }

    #[test]
    fn proxy_dns_delegation_must_match_the_proxy_scheme() {
        with_clean_env(|_| {
            for body in [
                "[database]\nurl = 'sqlite::memory:'\n[http_client]\nproxy_url = 'http://127.0.0.1:10808'\n",
                "[database]\nurl = 'sqlite::memory:'\n[http_client]\nproxy_url = 'socks5://127.0.0.1:10808'\ntrust_proxy_dns = true\n",
                "[database]\nurl = 'sqlite::memory:'\n[http_client]\ntrust_proxy_dns = true\n",
            ] {
                let path = temp_file(body);
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(
                    error,
                    ConfigError::InvalidField {
                        field: "http_client.trust_proxy_dns"
                    }
                );
            }

            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[http_client]\nproxy_url = 'http://127.0.0.1:10808'\ntrust_proxy_dns = true\n",
            );
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);
            assert!(config.http_client().trust_proxy_dns());
        });
    }

    #[test]
    fn toml_values_override_defaults() {
        with_clean_env(|_| {
            let path = temp_file(
                "[server]\nbind = '127.0.0.2:9000'\ncors_allowed_origins = ['https://Console.Example:443/']\nshutdown_timeout_secs = 45\n[telemetry]\nlevel = 'debug'\n[database]\nurl = 'postgres://user:secret@db/app'\nmigration_timeout_secs = 42\nhealth_check_timeout_secs = 3\n[auth]\nallow_query_api_key = true\nlookup_timeout_secs = 7\n[http_client]\nmax_cached_clients = 7\nconnect_timeout_secs = 2\nread_timeout_secs = 8\nrequest_timeout_secs = 13\nproxy_url = 'socks5://127.0.0.1:10808'\n",
            );
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);

            assert_eq!(config.server().bind().to_string(), "127.0.0.2:9000");
            assert_eq!(
                config.server().cors_allowed_origins()[0].as_str(),
                "https://console.example"
            );
            assert_eq!(config.telemetry().level(), LogLevel::Debug);
            assert_eq!(config.server().shutdown_timeout_secs(), 45);
            assert_eq!(config.database().migration_timeout_secs(), Some(42));
            assert_eq!(config.database().health_check_timeout_secs(), Some(3));
            assert!(config.auth().allow_query_api_key());
            assert_eq!(config.auth().lookup_timeout_secs(), 7);
            assert_eq!(config.http_client().max_cached_clients(), Some(7));
            assert_eq!(config.http_client().read_timeout_secs(), Some(8));
            assert_eq!(
                config.http_client().proxy_url().unwrap().expose(),
                "socks5://127.0.0.1:10808"
            );
        });
    }

    #[test]
    fn environment_values_override_toml_in_an_isolated_process() {
        if env::var_os(CHILD_MARKER).is_some() {
            let config = load().unwrap();
            assert_eq!(config.server().bind().to_string(), "127.0.0.2:7070");
            assert_eq!(
                config.server().cors_allowed_origins()[0].as_str(),
                "http://localhost:5173"
            );
            assert_eq!(config.telemetry().level(), LogLevel::Warn);
            assert_eq!(config.server().shutdown_timeout_secs(), 12);
            assert!(config.auth().allow_query_api_key());
            assert_eq!(config.auth().lookup_timeout_secs(), 9);
            assert_eq!(config.http_client().max_cached_clients(), Some(3));
            assert_eq!(
                config.openai_upstream().base_url(),
                "https://environment-upstream.example"
            );
            assert_eq!(config.openai_upstream().model(), "environment-model");
            assert_eq!(
                config.openai_upstream().api_key().expose(),
                "environment-upstream-key"
            );
            assert_eq!(
                config.http_client().proxy_url().unwrap().expose(),
                "socks5://127.0.0.1:10808"
            );
            assert_eq!(
                config.database().url().expose(),
                "postgres://env-user:env-secret@db/env-app"
            );
            return;
        }

        with_clean_env(|_| {
            let path = temp_file(
                "[server]\nbind = '127.0.0.1:8080'\n[database]\nurl = 'sqlite::memory:'\n[auth]\nallow_query_api_key = false\nlookup_timeout_secs = 3\n[http_client]\nmax_cached_clients = 2\n",
            );
            let output = Command::new(env::current_exe().unwrap())
                .arg("--exact")
                .arg("loader::tests::environment_values_override_toml_in_an_isolated_process")
                .arg("--nocapture")
                .env_clear()
                .env(CHILD_MARKER, "1")
                .env(CONFIG_FILE_ENV, &path)
                .env("AF_SERVER__BIND", "127.0.0.2:7070")
                .env("AF_SERVER__SHUTDOWN_TIMEOUT_SECS", "12")
                .env(
                    "AF_SERVER__CORS_ALLOWED_ORIGINS",
                    "[\"http://localhost:5173\"]",
                )
                .env("AF_TELEMETRY__LEVEL", "warn")
                .env("AF_AUTH__ALLOW_QUERY_API_KEY", "true")
                .env("AF_AUTH__LOOKUP_TIMEOUT_SECS", "9")
                .env(
                    "AF_DATABASE__URL",
                    "postgres://env-user:env-secret@db/env-app",
                )
                .env(
                    "AF_OPENAI_UPSTREAM__BASE_URL",
                    "https://environment-upstream.example",
                )
                .env("AF_OPENAI_UPSTREAM__MODEL", "environment-model")
                .env("AF_OPENAI_UPSTREAM__API_KEY", "environment-upstream-key")
                .env("AF_HTTP_CLIENT__MAX_CACHED_CLIENTS", "3")
                .env("AF_HTTP_CLIENT__PROXY_URL", "socks5://127.0.0.1:10808")
                .output()
                .expect("必须能启动隔离配置测试进程");
            let _ = fs::remove_file(path);
            assert!(
                output.status.success(),
                "隔离测试失败: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        });
    }

    #[test]
    fn invalid_telemetry_level_is_typed_without_echoing_value() {
        with_clean_env(|_| {
            let path = temp_file(
                "[telemetry]\nlevel = 'private-level-canary'\n[database]\nurl = 'sqlite::memory:'\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);

            assert_eq!(
                error,
                ConfigError::InvalidField {
                    field: "telemetry.level",
                }
            );
            assert!(!rendered.contains("private-level-canary"));
        });
    }

    #[test]
    fn invalid_database_health_timeout_is_typed_without_echoing_value() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\nhealth_check_timeout_secs = 'health-timeout-secret'\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);

            assert_eq!(
                error,
                ConfigError::InvalidField {
                    field: "database.health_check_timeout_secs",
                }
            );
            assert!(!rendered.contains("health-timeout-secret"));
        });
    }

    #[test]
    fn invalid_auth_types_are_typed_without_echoing_values() {
        with_clean_env(|_| {
            for (field, body, canary) in [
                (
                    "auth.allow_query_api_key",
                    "[database]\nurl = 'sqlite::memory:'\n[auth]\nallow_query_api_key = 'auth-bool-secret'\n",
                    "auth-bool-secret",
                ),
                (
                    "auth.lookup_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[auth]\nlookup_timeout_secs = 'auth-timeout-secret'\n",
                    "auth-timeout-secret",
                ),
            ] {
                let path = temp_file(body);
                let error = load_from(&path).unwrap_err();
                let rendered = format!("{error:?}\n{error}");
                let _ = fs::remove_file(path);

                assert_eq!(error, ConfigError::InvalidField { field });
                assert!(!rendered.contains(canary));
            }
        });
    }

    #[test]
    fn invalid_cors_origin_is_typed_without_echoing_value() {
        with_clean_env(|_| {
            let path = temp_file(
                "[server]\ncors_allowed_origins = ['https://origin-secret.example/path']\n[database]\nurl = 'sqlite::memory:'\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);

            assert_eq!(
                error,
                ConfigError::InvalidField {
                    field: "server.cors_allowed_origins",
                }
            );
            assert!(!rendered.contains("origin-secret"));
        });
    }

    #[test]
    fn otel_switch_is_rejected_until_shared_redaction_is_available() {
        with_clean_env(|_| {
            let path = temp_file(
                "[telemetry.otel]\nenabled = true\nendpoint = 'https://collector-secret@example.invalid'\n[database]\nurl = 'sqlite::memory:'\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);

            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains("collector-secret"));
            assert!(!rendered.contains("telemetry.otel"));
        });
    }

    #[test]
    fn otel_environment_switch_is_rejected_without_echoing_values() {
        with_clean_env(|jail| {
            jail.set_env("AF_TELEMETRY__OTEL__ENABLED", "true");
            jail.set_env(
                "AF_TELEMETRY__OTEL__ENDPOINT",
                "https://environment-collector-secret@example.invalid",
            );
            let path = temp_file("[database]\nurl = 'sqlite::memory:'\n");
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);

            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains("environment-collector-secret"));
            assert!(!rendered.contains("OTEL"));
            assert!(!rendered.contains("otel"));
        });
    }

    #[test]
    fn explicit_missing_file_is_typed_without_content() {
        with_clean_env(|_| {
            let path = env::temp_dir().join(format!(
                "anyflows-config-missing-{}-{}.toml",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            let error = load_from(&path).unwrap_err();
            assert_eq!(error, ConfigError::FileNotFound);
        });
    }

    #[test]
    fn debug_output_redacts_database_and_redis_urls() {
        with_clean_env(|_| {
            let path = raw_temp_file(
                "[server]\ncors_allowed_origins = ['https://cors-host-secret.example']\n[database]\nurl = 'postgres://user:database-secret@db/app'\n[redis]\nurl = 'redis://user:redis-secret@cache/0'\n[auth]\nallow_query_api_key = true\nlookup_timeout_secs = 9\n[openai_upstream]\nbase_url = 'https://upstream-host-secret.example'\nmodel = 'model-secret-canary'\napi_key = 'upstream-key-secret'\n[http_client]\nproxy_url = 'socks5://proxy-user:proxy-secret@127.0.0.1:10808'\n",
            );
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);
            let debug = format!("{config:?}");
            assert!(!debug.contains("database-secret"));
            assert!(!debug.contains("redis-secret"));
            assert!(!debug.contains("cors-host-secret"));
            assert!(!debug.contains("upstream-host-secret"));
            assert!(!debug.contains("model-secret-canary"));
            assert!(!debug.contains("upstream-key-secret"));
            assert!(!debug.contains("proxy-secret"));
            assert!(debug.contains("redacted"));
            assert!(debug.contains("allow_query_api_key: true"));
            assert!(debug.contains("lookup_timeout_secs: 9"));
        });
    }

    #[test]
    fn rejects_unknown_auth_fields_without_echoing_names_or_values() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[auth]\nunknown_auth_secret = 'auth-value-secret'\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);

            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains("unknown_auth_secret"));
            assert!(!rendered.contains("auth-value-secret"));
        });
    }

    #[test]
    fn rejects_unknown_fields_without_echoing_values() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[unknown]\nsecret_value = 'do-not-echo'\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = error.to_string();
            let _ = fs::remove_file(path);
            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains("do-not-echo"));
        });
    }

    #[test]
    fn unknown_field_names_are_not_echoed() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n['field-name-secret']\nvalue = true\n",
            );
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);
            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains("field-name-secret"));
        });
    }

    #[test]
    fn unknown_environment_fields_fail_closed_without_echoing_names() {
        with_clean_env(|jail| {
            jail.set_env("AF_ENV_FIELD_SECRET", "value-secret");
            let path = temp_file("[database]\nurl = 'sqlite::memory:'\n");
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);
            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains("ENV_FIELD_SECRET"));
            assert!(!rendered.contains("env_field_secret"));
            assert!(!rendered.contains("value-secret"));
        });
    }

    #[test]
    fn repository_test_environment_keys_are_ignored() {
        with_clean_env(|jail| {
            jail.set_env("AF_TEST_DATABASE_URL", "database-test-secret");
            jail.set_env("AF_REQUIRE_LIVE_REDIS", "1");
            let path = temp_file("[database]\nurl = 'sqlite::memory:'\n");
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);
            assert_eq!(config.database().url().expose(), "sqlite::memory:");
        });
    }

    #[test]
    fn missing_default_file_allows_environment_only_startup() {
        with_clean_env(|jail| {
            assert!(!Path::new(DEFAULT_CONFIG_FILE).exists());
            jail.set_env("AF_DATABASE__URL", "sqlite::memory:");
            jail.set_env("AF_OPENAI_UPSTREAM__BASE_URL", "https://upstream.example");
            jail.set_env("AF_OPENAI_UPSTREAM__MODEL", "test-model");
            jail.set_env("AF_OPENAI_UPSTREAM__API_KEY", "test-upstream-key");
            let config = load().unwrap();
            assert_eq!(config.database().url().expose(), "sqlite::memory:");
        });
    }

    #[test]
    fn empty_config_file_selector_is_rejected() {
        with_clean_env(|jail| {
            jail.set_env(CONFIG_FILE_ENV, "");
            assert_eq!(
                load().unwrap_err(),
                ConfigError::EmptyValue {
                    field: CONFIG_FILE_ENV,
                }
            );
        });
    }

    #[test]
    fn file_errors_never_echo_user_supplied_paths() {
        with_clean_env(|_| {
            let path = env::temp_dir().join("path-secret-do-not-echo.toml");
            let error = load_from(path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            assert_eq!(error, ConfigError::FileNotFound);
            assert!(!rendered.contains("path-secret-do-not-echo"));
        });
    }

    #[test]
    fn rejects_zero_values_during_startup_validation() {
        with_clean_env(|_| {
            for (field, body) in [
                (
                    "server.shutdown_timeout_secs",
                    "[server]\nshutdown_timeout_secs = 0\n[database]\nurl = 'sqlite::memory:'\n",
                ),
                (
                    "database.migration_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\nmigration_timeout_secs = 0\n",
                ),
                (
                    "database.health_check_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\nhealth_check_timeout_secs = 0\n",
                ),
                (
                    "auth.lookup_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[auth]\nlookup_timeout_secs = 0\n",
                ),
                (
                    "http_client.max_cached_clients",
                    "[database]\nurl = 'sqlite::memory:'\n[http_client]\nmax_cached_clients = 0\n",
                ),
                (
                    "http_client.connect_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[http_client]\nconnect_timeout_secs = 0\n",
                ),
            ] {
                let path = temp_file(body);
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::NonPositive { field });
            }
        });
    }

    #[test]
    fn rejects_shutdown_timeout_above_hard_limit() {
        with_clean_env(|_| {
            let path = temp_file(
                "[server]\nshutdown_timeout_secs = 301\n[database]\nurl = 'sqlite::memory:'\n",
            );
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::OutOfRange {
                    field: "server.shutdown_timeout_secs"
                }
            );
        });
    }

    #[test]
    fn rejects_database_health_timeout_above_hard_limit() {
        with_clean_env(|_| {
            let path =
                temp_file("[database]\nurl = 'sqlite::memory:'\nhealth_check_timeout_secs = 31\n");
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::OutOfRange {
                    field: "database.health_check_timeout_secs"
                }
            );
        });
    }

    #[test]
    fn rejects_auth_lookup_timeout_above_hard_limit() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[auth]\nlookup_timeout_secs = 31\n",
            );
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::OutOfRange {
                    field: "auth.lookup_timeout_secs"
                }
            );
        });
    }

    #[test]
    fn malformed_toml_error_never_echoes_secret_source() {
        with_clean_env(|_| {
            let path = temp_file("[database]\nurl = 'parser-secret\n");
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);
            assert!(!rendered.contains("parser-secret"));
        });
    }
}
