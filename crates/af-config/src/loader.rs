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

use crate::{
    AlipayVerificationSettings, AppConfig, BillingSettings, ChannelProbeSettings, ConfigError,
    DEFAULT_AUTH_LOOKUP_TIMEOUT_SECS, DEFAULT_AUTH_SESSION_TTL_SECS,
    DEFAULT_PAYMENT_SIGNATURE_TOLERANCE_SECS, ServerConfig, SubscriptionSettings,
    TelemetrySettings,
};

/// 默认配置文件名；文件不存在时兼容读取旧版 `anyflows.toml`。
pub const DEFAULT_CONFIG_FILE: &str = "anyflows.conf";
const LEGACY_CONFIG_FILE: &str = "anyflows.toml";
/// 默认环境文件名；服务启动时从当前工作目录加载（若存在）。
pub const DEFAULT_ENV_FILE: &str = ".env";
/// 配置文件选择环境变量。
pub const CONFIG_FILE_ENV: &str = "AF_CONFIG_FILE";
/// 业务配置环境变量前缀；该命名空间中的未知键会拒绝启动。
pub const ENV_PREFIX: &str = "AF_";

#[derive(Default, Serialize)]
struct ConfigDefaults {
    server: ServerConfig,
    telemetry: TelemetrySettings,
    database: DatabaseDefaults,
    billing: BillingSettings,
    subscription: SubscriptionSettings,
    payment: PaymentDefaults,
    account_verification: AccountVerificationDefaults,
    channel_probe: ChannelProbeSettings,
    credential_encryption: CredentialEncryptionDefaults,
    redis: RedisDefaults,
    auth: AuthDefaults,
    turnstile: TurnstileDefaults,
    oauth: OAuthDefaults,
    http_client: HttpClientDefaults,
}

#[derive(Serialize)]
struct PaymentDefaults {
    stripe_secret_key: Option<String>,
    stripe_publishable_key: Option<String>,
    stripe_webhook_secret: Option<String>,
    stripe_signature_tolerance_secs: u64,
}

#[derive(Default, Serialize)]
struct AccountVerificationDefaults {
    alipay: AlipayVerificationDefaults,
}

#[derive(Serialize)]
struct AlipayVerificationDefaults {
    enabled: bool,
    app_id: Option<String>,
    private_key: Option<String>,
    public_key: Option<String>,
    gateway_url: String,
    biz_code: String,
    timeout_secs: u64,
}

impl Default for AlipayVerificationDefaults {
    fn default() -> Self {
        let settings = AlipayVerificationSettings::default();
        Self {
            enabled: settings.enabled(),
            app_id: None,
            private_key: None,
            public_key: None,
            gateway_url: settings.gateway_url().to_owned(),
            biz_code: settings.biz_code().to_owned(),
            timeout_secs: settings.timeout_secs(),
        }
    }
}

impl Default for PaymentDefaults {
    fn default() -> Self {
        Self {
            stripe_secret_key: None,
            stripe_publishable_key: None,
            stripe_webhook_secret: None,
            stripe_signature_tolerance_secs: DEFAULT_PAYMENT_SIGNATURE_TOLERANCE_SECS,
        }
    }
}

#[derive(Serialize)]
struct AuthDefaults {
    allow_query_api_key: bool,
    lookup_timeout_secs: u64,
    session_signing_key: Option<String>,
    session_ttl_secs: u64,
}

#[derive(Serialize)]
struct TurnstileDefaults {
    site_key: Option<String>,
    secret_key: Option<String>,
    timeout_secs: u64,
}

impl Default for TurnstileDefaults {
    fn default() -> Self {
        Self {
            site_key: None,
            secret_key: None,
            timeout_secs: crate::DEFAULT_TURNSTILE_TIMEOUT_SECS,
        }
    }
}

impl Default for AuthDefaults {
    fn default() -> Self {
        Self {
            allow_query_api_key: false,
            lookup_timeout_secs: DEFAULT_AUTH_LOOKUP_TIMEOUT_SECS,
            session_signing_key: None,
            session_ttl_secs: DEFAULT_AUTH_SESSION_TTL_SECS,
        }
    }
}

#[derive(Default, Serialize)]
struct DatabaseDefaults {
    migration_timeout_secs: Option<u64>,
}

#[derive(Default, Serialize)]
struct RedisDefaults {
    url: Option<String>,
    request_rate_limit_namespace: Option<String>,
}

#[derive(Default, Serialize)]
struct CredentialEncryptionDefaults {
    key_id: Option<String>,
    key: Option<String>,
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

#[derive(Serialize)]
struct OAuthDefaults {
    max_pending_authorizations: usize,
    session_ttl_secs: u64,
    refresh_enabled: bool,
    refresh_interval_secs: u64,
    refresh_before_expiry_secs: u64,
    refresh_batch_size: usize,
    refresh_concurrency: usize,
}

impl Default for OAuthDefaults {
    fn default() -> Self {
        Self {
            max_pending_authorizations: crate::DEFAULT_OAUTH_MAX_PENDING_AUTHORIZATIONS,
            session_ttl_secs: crate::DEFAULT_OAUTH_SESSION_TTL_SECS,
            refresh_enabled: crate::DEFAULT_OAUTH_REFRESH_ENABLED,
            refresh_interval_secs: crate::DEFAULT_OAUTH_REFRESH_INTERVAL_SECS,
            refresh_before_expiry_secs: crate::DEFAULT_OAUTH_REFRESH_BEFORE_EXPIRY_SECS,
            refresh_batch_size: crate::DEFAULT_OAUTH_REFRESH_BATCH_SIZE,
            refresh_concurrency: crate::DEFAULT_OAUTH_REFRESH_CONCURRENCY,
        }
    }
}

/// 加载默认文件与环境变量覆盖后的应用配置。
pub fn load() -> Result<AppConfig, ConfigError> {
    load_dotenv()?;
    match env::var_os(CONFIG_FILE_ENV) {
        Some(path) if path.is_empty() => Err(ConfigError::EmptyValue {
            field: CONFIG_FILE_ENV,
        }),
        Some(path) => load_file(PathBuf::from(path), true),
        None => {
            let default_path = PathBuf::from(DEFAULT_CONFIG_FILE);
            if default_path.exists() {
                load_file(default_path, false)
            } else {
                // 旧版部署仍使用 anyflows.toml，保留无迁移读取兼容。
                load_file(PathBuf::from(LEGACY_CONFIG_FILE), false)
            }
        }
    }
}

/// 将当前工作目录下的 `.env` 作为环境变量来源加载。
///
/// `.env` 是 `KEY=VALUE` 格式的环境文件，不是 TOML 配置文件。已经存在于
/// 进程环境中的变量优先，因此命令行、容器和 systemd 注入的值不会被覆盖。
fn load_dotenv() -> Result<(), ConfigError> {
    match dotenvy::from_path(DEFAULT_ENV_FILE) {
        Ok(()) => Ok(()),
        Err(error) if error.not_found() => Ok(()),
        Err(dotenvy::Error::Io(error)) => Err(ConfigError::FileRead { kind: error.kind() }),
        Err(_) => Err(ConfigError::Invalid),
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
        "server.client_ip_source" => "server.client_ip_source",
        "server.shutdown_timeout_secs" => "server.shutdown_timeout_secs",
        "server.frontend_template_directory" => "server.frontend_template_directory",
        path if path == "server.cors_allowed_origins"
            || path.starts_with("server.cors_allowed_origins.") =>
        {
            "server.cors_allowed_origins"
        }
        path if path == "server.trusted_proxy_cidrs"
            || path.starts_with("server.trusted_proxy_cidrs.") =>
        {
            "server.trusted_proxy_cidrs"
        }
        "telemetry" => "telemetry",
        "telemetry.level" => "telemetry.level",
        "database" => "database",
        "database.url" => "database.url",
        "database.migration_timeout_secs" => "database.migration_timeout_secs",
        "database.health_check_timeout_secs" => "database.health_check_timeout_secs",
        "billing" => "billing",
        "billing.batch_enabled" => "billing.batch_enabled",
        "billing.flush_interval_secs" => "billing.flush_interval_secs",
        "billing.flush_timeout_secs" => "billing.flush_timeout_secs",
        "billing.usage_record_queue_capacity" => "billing.usage_record_queue_capacity",
        "billing.usage_record_worker_count" => "billing.usage_record_worker_count",
        "billing.wal_directory" => "billing.wal_directory",
        "subscription" => "subscription",
        "subscription.enabled" => "subscription.enabled",
        "subscription.batch_size" => "subscription.batch_size",
        "subscription.interval_secs" => "subscription.interval_secs",
        "subscription.max_batches_per_run" => "subscription.max_batches_per_run",
        "payment" => "payment",
        "payment.stripe_secret_key" => "payment.stripe_secret_key",
        "payment.stripe_publishable_key" => "payment.stripe_publishable_key",
        "payment.stripe_webhook_secret" => "payment.stripe_webhook_secret",
        "payment.stripe_signature_tolerance_secs" => "payment.stripe_signature_tolerance_secs",
        "account_verification" => "account_verification",
        "account_verification.alipay" => "account_verification.alipay",
        "account_verification.alipay.enabled" => "account_verification.alipay.enabled",
        "account_verification.alipay.app_id" => "account_verification.alipay.app_id",
        "account_verification.alipay.private_key" => "account_verification.alipay.private_key",
        "account_verification.alipay.public_key" => "account_verification.alipay.public_key",
        "account_verification.alipay.gateway_url" => "account_verification.alipay.gateway_url",
        "account_verification.alipay.biz_code" => "account_verification.alipay.biz_code",
        "account_verification.alipay.timeout_secs" => "account_verification.alipay.timeout_secs",
        "channel_probe" => "channel_probe",
        "channel_probe.enabled" => "channel_probe.enabled",
        "channel_probe.batch_size" => "channel_probe.batch_size",
        "channel_probe.interval_secs" => "channel_probe.interval_secs",
        "channel_probe.probe_timeout_secs" => "channel_probe.probe_timeout_secs",
        "credential_encryption" => "credential_encryption",
        "credential_encryption.key_id" => "credential_encryption.key_id",
        "credential_encryption.key" => "credential_encryption.key",
        "redis" => "redis",
        "redis.url" => "redis.url",
        "redis.request_rate_limit_namespace" => "redis.request_rate_limit_namespace",
        "clickhouse_analytics" => "clickhouse_analytics",
        "clickhouse_analytics.endpoint" => "clickhouse_analytics.endpoint",
        "clickhouse_analytics.query" => "clickhouse_analytics.query",
        "clickhouse_analytics.username" => "clickhouse_analytics.username",
        "clickhouse_analytics.password" => "clickhouse_analytics.password",
        "clickhouse_analytics.timeout_secs" => "clickhouse_analytics.timeout_secs",
        "clickhouse_analytics.max_response_bytes" => "clickhouse_analytics.max_response_bytes",
        "clickhouse_export" => "clickhouse_export",
        "clickhouse_export.endpoint" => "clickhouse_export.endpoint",
        "clickhouse_export.usage_insert_query" => "clickhouse_export.usage_insert_query",
        "clickhouse_export.outcome_insert_query" => "clickhouse_export.outcome_insert_query",
        "clickhouse_export.username" => "clickhouse_export.username",
        "clickhouse_export.password" => "clickhouse_export.password",
        "clickhouse_export.batch_size" => "clickhouse_export.batch_size",
        "clickhouse_export.interval_secs" => "clickhouse_export.interval_secs",
        "clickhouse_export.timeout_secs" => "clickhouse_export.timeout_secs",
        "clickhouse_export.max_request_bytes" => "clickhouse_export.max_request_bytes",
        "clickhouse_export.backfill_batch_size" => "clickhouse_export.backfill_batch_size",
        "auth" => "auth",
        "auth.allow_query_api_key" => "auth.allow_query_api_key",
        "auth.lookup_timeout_secs" => "auth.lookup_timeout_secs",
        "auth.session_signing_key" => "auth.session_signing_key",
        "auth.session_ttl_secs" => "auth.session_ttl_secs",
        "turnstile" => "turnstile",
        "turnstile.site_key" => "turnstile.site_key",
        "turnstile.secret_key" => "turnstile.secret_key",
        "turnstile.timeout_secs" => "turnstile.timeout_secs",
        "oauth" => "oauth",
        "oauth.max_pending_authorizations" => "oauth.max_pending_authorizations",
        "oauth.session_ttl_secs" => "oauth.session_ttl_secs",
        "oauth.refresh_enabled" => "oauth.refresh_enabled",
        "oauth.refresh_interval_secs" => "oauth.refresh_interval_secs",
        "oauth.refresh_before_expiry_secs" => "oauth.refresh_before_expiry_secs",
        "oauth.refresh_batch_size" => "oauth.refresh_batch_size",
        "oauth.refresh_concurrency" => "oauth.refresh_concurrency",
        "oauth.claude_code" => "oauth.claude_code",
        "oauth.claude_code.client_id" => "oauth.claude_code.client_id",
        "oauth.codex" => "oauth.codex",
        "oauth.codex.client_id" => "oauth.codex.client_id",
        "oauth.gemini" => "oauth.gemini",
        "oauth.gemini.client_id" => "oauth.gemini.client_id",
        "oauth.gemini.client_secret" => "oauth.gemini.client_secret",
        "oauth.antigravity" => "oauth.antigravity",
        "oauth.antigravity.client_id" => "oauth.antigravity.client_id",
        "oauth.antigravity.client_secret" => "oauth.antigravity.client_secret",
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
    use crate::{ClientIpSource, LogLevel, MAX_TRUSTED_PROXY_CIDRS};

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
            assert_eq!(
                config.server().frontend_template_directory(),
                Path::new("public/templates")
            );
            assert_eq!(config.server().client_ip_source(), ClientIpSource::Peer);
            assert!(config.server().trusted_proxy_cidrs().is_empty());
            assert_eq!(config.telemetry().level(), LogLevel::Info);
            assert_eq!(config.database().migration_timeout_secs(), None);
            assert_eq!(config.database().health_check_timeout_secs(), None);
            assert!(!config.billing().batch_enabled());
            assert_eq!(config.billing().flush_interval_secs(), 5);
            assert_eq!(config.billing().flush_timeout_secs(), 30);
            assert_eq!(config.billing().usage_record_queue_capacity(), 4_096);
            assert_eq!(config.billing().usage_record_worker_count(), 1);
            assert_eq!(
                config.billing().wal_directory(),
                Path::new("data/billing-wal")
            );
            assert!(config.subscription().enabled());
            assert_eq!(config.subscription().batch_size(), 64);
            assert_eq!(config.subscription().interval_secs(), 60);
            assert_eq!(config.subscription().max_batches_per_run(), 8);
            assert!(config.payment().stripe_secret_key().is_none());
            assert!(config.payment().stripe_publishable_key().is_none());
            assert!(config.payment().stripe_webhook_secret().is_none());
            assert_eq!(
                config.payment().stripe_signature_tolerance_secs(),
                DEFAULT_PAYMENT_SIGNATURE_TOLERANCE_SECS
            );
            assert!(!config.channel_probe().enabled());
            assert_eq!(config.channel_probe().batch_size(), 16);
            assert_eq!(config.channel_probe().interval_secs(), 60);
            assert_eq!(config.channel_probe().probe_timeout_secs(), 10);
            assert!(!config.credential_encryption().is_configured());
            assert!(config.redis().url().is_none());
            assert!(config.clickhouse_analytics().is_none());
            assert!(config.clickhouse_export().is_none());
            assert!(!config.auth().allow_query_api_key());
            assert_eq!(config.auth().lookup_timeout_secs(), 2);
            assert!(config.auth().session_signing_key().is_none());
            assert_eq!(config.auth().session_ttl_secs(), 3_600);
            assert!(!config.turnstile().enabled());
            assert!(config.turnstile().site_key().is_none());
            assert!(config.turnstile().secret_key().is_none());
            assert_eq!(config.turnstile().timeout_secs(), 5);
            assert_eq!(config.oauth().max_pending_authorizations(), 256);
            assert_eq!(config.oauth().session_ttl_secs(), 600);
            assert!(config.oauth().refresh_enabled());
            assert_eq!(config.oauth().refresh_interval_secs(), 60);
            assert_eq!(config.oauth().refresh_before_expiry_secs(), 300);
            assert_eq!(config.oauth().refresh_batch_size(), 64);
            assert_eq!(config.oauth().refresh_concurrency(), 8);
            assert_eq!(config.oauth().configured_provider_count(), 0);
            assert_eq!(
                config.openai_upstream().unwrap().base_url(),
                "https://upstream.example"
            );
            assert_eq!(config.openai_upstream().unwrap().model(), "test-model");
            assert_eq!(
                config.openai_upstream().unwrap().api_key().expose(),
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
    fn turnstile_requires_complete_bounded_startup_credentials() {
        with_clean_env(|_| {
            let valid = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[turnstile]\nsite_key = 'site-key'\nsecret_key = 'secret-key'\ntimeout_secs = 7\n",
            );
            let config = load_from(&valid).unwrap();
            let _ = fs::remove_file(valid);
            assert!(config.turnstile().enabled());
            assert_eq!(config.turnstile().site_key(), Some("site-key"));
            assert_eq!(
                config.turnstile().secret_key().unwrap().expose(),
                "secret-key"
            );
            assert_eq!(config.turnstile().timeout_secs(), 7);
            let debug = format!("{:?}", config.turnstile());
            assert!(!debug.contains("secret-key"));

            for (field, turnstile) in [
                ("turnstile.secret_key", "site_key = 'site-key'"),
                ("turnstile.site_key", "secret_key = 'secret-key'"),
                (
                    "turnstile.timeout_secs",
                    "site_key = 'site-key'\nsecret_key = 'secret-key'\ntimeout_secs = 0",
                ),
            ] {
                let path = temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[turnstile]\n{turnstile}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert!(matches!(
                    error,
                    ConfigError::InvalidField { field: actual }
                        | ConfigError::NonPositive { field: actual }
                        if actual == field
                ));
            }
        });
    }

    #[test]
    fn clickhouse_analytics_is_optional_complete_bounded_and_redacted() {
        with_clean_env(|_| {
            let valid = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[clickhouse_analytics]\nendpoint = 'https://clickhouse.example:8443/'\nquery = 'SELECT {period_start:Int64}, {period_end:Int64}'\nusername = 'dashboard-reader'\npassword = 'dashboard-secret'\ntimeout_secs = 7\nmax_response_bytes = 65536\n",
            );
            let config = load_from(&valid).unwrap();
            let _ = fs::remove_file(valid);
            let settings = config.clickhouse_analytics().unwrap();
            assert_eq!(
                settings.endpoint().expose(),
                "https://clickhouse.example:8443/"
            );
            assert_eq!(
                settings.query().expose(),
                "SELECT {period_start:Int64}, {period_end:Int64}"
            );
            assert_eq!(settings.username().expose(), "dashboard-reader");
            assert_eq!(settings.password().expose(), "dashboard-secret");
            assert_eq!(settings.timeout_secs(), 7);
            assert_eq!(settings.max_response_bytes(), 65_536);
            let debug = format!("{settings:?}");
            for secret in [
                "clickhouse.example",
                "period_start",
                "dashboard-reader",
                "dashboard-secret",
            ] {
                assert!(!debug.contains(secret));
            }

            for field in ["endpoint", "query", "username", "password"] {
                let values = [
                    ("endpoint", "endpoint = 'https://clickhouse.example'"),
                    (
                        "query",
                        "query = 'SELECT {period_start:Int64}, {period_end:Int64}'",
                    ),
                    ("username", "username = 'dashboard-reader'"),
                    ("password", "password = 'dashboard-secret'"),
                ]
                .into_iter()
                .filter_map(|(name, value)| (name != field).then_some(value))
                .collect::<Vec<_>>()
                .join("\n");
                let path = raw_temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[clickhouse_analytics]\n{values}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(
                    error,
                    ConfigError::InvalidField {
                        field: match field {
                            "endpoint" => "clickhouse_analytics.endpoint",
                            "query" => "clickhouse_analytics.query",
                            "username" => "clickhouse_analytics.username",
                            "password" => "clickhouse_analytics.password",
                            _ => unreachable!(),
                        }
                    }
                );
            }

            for (field, override_value, expected) in [
                (
                    "clickhouse_analytics.timeout_secs",
                    "timeout_secs = 0",
                    ConfigError::NonPositive {
                        field: "clickhouse_analytics.timeout_secs",
                    },
                ),
                (
                    "clickhouse_analytics.timeout_secs",
                    "timeout_secs = 301",
                    ConfigError::OutOfRange {
                        field: "clickhouse_analytics.timeout_secs",
                    },
                ),
                (
                    "clickhouse_analytics.max_response_bytes",
                    "max_response_bytes = 0",
                    ConfigError::NonPositive {
                        field: "clickhouse_analytics.max_response_bytes",
                    },
                ),
                (
                    "clickhouse_analytics.max_response_bytes",
                    "max_response_bytes = 4194305",
                    ConfigError::OutOfRange {
                        field: "clickhouse_analytics.max_response_bytes",
                    },
                ),
            ] {
                let path = temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[clickhouse_analytics]\nendpoint = 'https://clickhouse.example'\nquery = 'SELECT {{period_start:Int64}}, {{period_end:Int64}}'\nusername = 'dashboard-reader'\npassword = 'dashboard-secret'\n{override_value}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, expected, "{field}");
            }
        });
    }

    #[test]
    fn clickhouse_analytics_rejects_invalid_values_without_echoing_them() {
        with_clean_env(|_| {
            for (field, override_value, canary) in [
                (
                    "clickhouse_analytics.endpoint",
                    "endpoint = 'https://user:endpoint-canary@clickhouse.example'",
                    "endpoint-canary",
                ),
                (
                    "clickhouse_analytics.query",
                    "query = 'SELECT query-canary; DROP TABLE usage_logs'",
                    "query-canary",
                ),
                (
                    "clickhouse_analytics.username",
                    "username = ' username-canary '",
                    "username-canary",
                ),
                (
                    "clickhouse_analytics.password",
                    "password = '密码-password-canary'",
                    "password-canary",
                ),
            ] {
                let defaults = [
                    ("endpoint", "endpoint = 'https://clickhouse.example'"),
                    (
                        "query",
                        "query = 'SELECT {period_start:Int64}, {period_end:Int64}'",
                    ),
                    ("username", "username = 'dashboard-reader'"),
                    ("password", "password = 'dashboard-secret'"),
                ];
                let overridden_name = field.rsplit('.').next().unwrap();
                let values = defaults
                    .into_iter()
                    .filter_map(|(name, value)| (name != overridden_name).then_some(value))
                    .chain(std::iter::once(override_value))
                    .collect::<Vec<_>>()
                    .join("\n");
                let path = raw_temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[clickhouse_analytics]\n{values}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let rendered = format!("{error:?}\n{error}");
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::InvalidField { field });
                assert!(!rendered.contains(canary));
            }
        });
    }

    #[test]
    fn clickhouse_export_is_optional_complete_bounded_and_redacted() {
        with_clean_env(|_| {
            let valid = raw_temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[clickhouse_export]\nendpoint = 'https://clickhouse.example:8443'\nusage_insert_query = 'INSERT INTO usage_facts (fact_id)'\noutcome_insert_query = 'INSERT INTO outcome_facts (fact_id)'\nusername = 'export-writer'\npassword = 'export-secret'\nbatch_size = 12\ninterval_secs = 9\ntimeout_secs = 17\nmax_request_bytes = 65536\nbackfill_batch_size = 23\n",
            );
            let config = load_from(&valid).unwrap();
            let _ = fs::remove_file(valid);
            let settings = config.clickhouse_export().unwrap();
            assert_eq!(
                settings.endpoint().expose(),
                "https://clickhouse.example:8443"
            );
            assert_eq!(
                settings.usage_insert_query().expose(),
                "INSERT INTO usage_facts (fact_id)"
            );
            assert_eq!(
                settings.outcome_insert_query().expose(),
                "INSERT INTO outcome_facts (fact_id)"
            );
            assert_eq!(settings.username().expose(), "export-writer");
            assert_eq!(settings.password().expose(), "export-secret");
            assert_eq!(settings.batch_size(), 12);
            assert_eq!(settings.interval_secs(), 9);
            assert_eq!(settings.timeout_secs(), 17);
            assert_eq!(settings.max_request_bytes(), 65_536);
            assert_eq!(settings.backfill_batch_size(), 23);

            let debug = format!("{settings:?}");
            for secret in [
                "clickhouse.example",
                "usage_facts",
                "outcome_facts",
                "export-writer",
                "export-secret",
            ] {
                assert!(!debug.contains(secret));
            }

            for field in [
                "endpoint",
                "usage_insert_query",
                "outcome_insert_query",
                "username",
                "password",
            ] {
                let values = [
                    ("endpoint", "endpoint = 'https://clickhouse.example'"),
                    (
                        "usage_insert_query",
                        "usage_insert_query = 'INSERT INTO usage_facts (fact_id)'",
                    ),
                    (
                        "outcome_insert_query",
                        "outcome_insert_query = 'INSERT INTO outcome_facts (fact_id)'",
                    ),
                    ("username", "username = 'export-writer'"),
                    ("password", "password = 'export-secret'"),
                ]
                .into_iter()
                .filter_map(|(name, value)| (name != field).then_some(value))
                .collect::<Vec<_>>()
                .join("\n");
                let path = raw_temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[clickhouse_export]\n{values}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(
                    error,
                    ConfigError::InvalidField {
                        field: match field {
                            "endpoint" => "clickhouse_export.endpoint",
                            "usage_insert_query" => "clickhouse_export.usage_insert_query",
                            "outcome_insert_query" => "clickhouse_export.outcome_insert_query",
                            "username" => "clickhouse_export.username",
                            "password" => "clickhouse_export.password",
                            _ => unreachable!(),
                        }
                    }
                );
            }

            for (field, override_value, expected) in [
                (
                    "clickhouse_export.batch_size",
                    "batch_size = 0",
                    ConfigError::NonPositive {
                        field: "clickhouse_export.batch_size",
                    },
                ),
                (
                    "clickhouse_export.interval_secs",
                    "interval_secs = 3601",
                    ConfigError::OutOfRange {
                        field: "clickhouse_export.interval_secs",
                    },
                ),
                (
                    "clickhouse_export.timeout_secs",
                    "timeout_secs = 0",
                    ConfigError::NonPositive {
                        field: "clickhouse_export.timeout_secs",
                    },
                ),
                (
                    "clickhouse_export.max_request_bytes",
                    "max_request_bytes = 16777217",
                    ConfigError::OutOfRange {
                        field: "clickhouse_export.max_request_bytes",
                    },
                ),
                (
                    "clickhouse_export.backfill_batch_size",
                    "backfill_batch_size = 257",
                    ConfigError::OutOfRange {
                        field: "clickhouse_export.backfill_batch_size",
                    },
                ),
            ] {
                let path = raw_temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[clickhouse_export]\nendpoint = 'https://clickhouse.example'\nusage_insert_query = 'INSERT INTO usage_facts (fact_id)'\noutcome_insert_query = 'INSERT INTO outcome_facts (fact_id)'\nusername = 'export-writer'\npassword = 'export-secret'\n{override_value}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, expected, "{field}");
            }

            for (field, override_value, canary) in [
                (
                    "clickhouse_export.endpoint",
                    "endpoint = 'https://user:endpoint-canary@clickhouse.example'",
                    "endpoint-canary",
                ),
                (
                    "clickhouse_export.usage_insert_query",
                    "usage_insert_query = 'INSERT INTO usage_facts (usage-query-canary); DROP TABLE usage_logs'",
                    "usage-query-canary",
                ),
                (
                    "clickhouse_export.username",
                    "username = ' username-canary ' \n",
                    "username-canary",
                ),
                (
                    "clickhouse_export.password",
                    "password = '密码-password-canary'",
                    "password-canary",
                ),
            ] {
                let defaults = [
                    ("endpoint", "endpoint = 'https://clickhouse.example'"),
                    (
                        "usage_insert_query",
                        "usage_insert_query = 'INSERT INTO usage_facts (fact_id)'",
                    ),
                    (
                        "outcome_insert_query",
                        "outcome_insert_query = 'INSERT INTO outcome_facts (fact_id)'",
                    ),
                    ("username", "username = 'export-writer'"),
                    ("password", "password = 'export-secret'"),
                ];
                let overridden_name = field.rsplit('.').next().unwrap();
                let values = defaults
                    .into_iter()
                    .filter_map(|(name, value)| (name != overridden_name).then_some(value))
                    .chain(std::iter::once(override_value))
                    .collect::<Vec<_>>()
                    .join("\n");
                let path = raw_temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[clickhouse_export]\n{values}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let rendered = format!("{error:?}\n{error}");
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::InvalidField { field });
                assert!(!rendered.contains(canary));
            }
        });
    }

    #[test]
    fn trusted_proxy_source_requires_a_bounded_static_network_set() {
        with_clean_env(|_| {
            let valid = temp_file(
                "[server]\nclient_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = ['10.23.45.0/24', '2001:db8::/48']\n[database]\nurl = 'sqlite::memory:'\n",
            );
            let config = load_from(&valid).unwrap();
            let _ = fs::remove_file(valid);
            assert_eq!(
                config.server().client_ip_source(),
                ClientIpSource::XForwardedFor
            );
            assert_eq!(config.server().trusted_proxy_cidrs().len(), 2);
            let debug = format!("{:?}", config.server());
            assert!(!debug.contains("10.23.45"));
            assert!(!debug.contains("2001:db8"));

            for (field, server) in [
                (
                    "server.trusted_proxy_cidrs",
                    "trusted_proxy_cidrs = ['127.0.0.1/32']",
                ),
                (
                    "server.client_ip_source",
                    "client_ip_source = 'x-forwarded-for'",
                ),
                (
                    "server.trusted_proxy_cidrs",
                    "client_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = ['0.0.0.0/0']",
                ),
                (
                    "server.trusted_proxy_cidrs",
                    "client_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = ['::/0']",
                ),
                (
                    "server.trusted_proxy_cidrs",
                    "client_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = ['::ffff:0.0.0.0/96']",
                ),
            ] {
                let path = temp_file(&format!(
                    "[server]\n{server}\n[database]\nurl = 'sqlite::memory:'\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::InvalidField { field });
            }

            let networks = (0..=MAX_TRUSTED_PROXY_CIDRS)
                .map(|index| format!("'10.0.{}.0/24'", index % 256))
                .collect::<Vec<_>>()
                .join(",");
            let too_many = temp_file(&format!(
                "[server]\nclient_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = [{networks}]\n[database]\nurl = 'sqlite::memory:'\n"
            ));
            let error = load_from(&too_many).unwrap_err();
            let _ = fs::remove_file(too_many);
            assert_eq!(
                error,
                ConfigError::OutOfRange {
                    field: "server.trusted_proxy_cidrs"
                }
            );

            for (field, value) in [
                ("server.client_ip_source", "proxy-source-canary"),
                ("server.trusted_proxy_cidrs", "192.0.2.1/proxy-cidr-canary"),
            ] {
                let server = if field.ends_with("source") {
                    format!("client_ip_source = '{value}'")
                } else {
                    format!(
                        "client_ip_source = 'x-forwarded-for'\ntrusted_proxy_cidrs = ['{value}']"
                    )
                };
                let path = temp_file(&format!(
                    "[server]\n{server}\n[database]\nurl = 'sqlite::memory:'\n"
                ));
                let error = load_from(&path).unwrap_err();
                let rendered = format!("{error:?}\n{error}");
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::InvalidField { field });
                assert!(!rendered.contains(value));
            }
        });
    }

    #[test]
    fn openai_upstream_is_optional_and_present_fields_remain_typed() {
        with_clean_env(|_| {
            let missing_section = raw_temp_file("[database]\nurl = 'sqlite::memory:'\n");
            let config = load_from(&missing_section).unwrap();
            let _ = fs::remove_file(missing_section);
            assert!(config.openai_upstream().is_none());

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
                "[server]\nbind = '127.0.0.2:9000'\ncors_allowed_origins = ['https://Console.Example:443/']\nshutdown_timeout_secs = 45\n[telemetry]\nlevel = 'debug'\n[database]\nurl = 'postgres://user:secret@db/app'\nmigration_timeout_secs = 42\nhealth_check_timeout_secs = 3\n[billing]\nbatch_enabled = true\nflush_interval_secs = 11\nflush_timeout_secs = 17\nwal_directory = 'private-billing-wal'\n[subscription]\nenabled = false\nbatch_size = 48\ninterval_secs = 75\nmax_batches_per_run = 12\n[payment]\nstripe_secret_key = 'sk-test-secret'\nstripe_publishable_key = 'pk_test_public'\nstripe_webhook_secret = 'whsec-test-secret'\nstripe_signature_tolerance_secs = 42\n[auth]\nallow_query_api_key = true\nlookup_timeout_secs = 7\n[http_client]\nmax_cached_clients = 7\nconnect_timeout_secs = 2\nread_timeout_secs = 8\nrequest_timeout_secs = 13\nproxy_url = 'socks5://127.0.0.1:10808'\n",
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
            assert!(config.billing().batch_enabled());
            assert_eq!(config.billing().flush_interval_secs(), 11);
            assert_eq!(config.billing().flush_timeout_secs(), 17);
            assert_eq!(
                config.billing().wal_directory(),
                Path::new("private-billing-wal")
            );
            assert!(!format!("{:?}", config.billing()).contains("private-billing-wal"));
            assert!(!config.subscription().enabled());
            assert_eq!(config.subscription().batch_size(), 48);
            assert_eq!(config.subscription().interval_secs(), 75);
            assert_eq!(config.subscription().max_batches_per_run(), 12);
            assert_eq!(
                config.payment().stripe_secret_key().unwrap().expose(),
                "sk-test-secret"
            );
            assert_eq!(
                config.payment().stripe_publishable_key(),
                Some("pk_test_public")
            );
            assert_eq!(
                config.payment().stripe_webhook_secret().unwrap().expose(),
                "whsec-test-secret"
            );
            assert_eq!(config.payment().stripe_signature_tolerance_secs(), 42);
            assert!(!format!("{:?}", config.payment()).contains("sk-test-secret"));
            assert!(!format!("{:?}", config.payment()).contains("whsec-test-secret"));
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
    fn stripe_api_key_requires_a_webhook_secret_before_client_configuration() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[payment]\nstripe_secret_key = 'sk-test-secret'\n",
            );
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);

            assert_eq!(
                error,
                ConfigError::EmptyValue {
                    field: "payment.stripe_webhook_secret"
                }
            );
        });
    }

    #[test]
    fn stripe_api_key_requires_a_publishable_key() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[payment]\nstripe_secret_key = 'sk-test-secret'\nstripe_webhook_secret = 'whsec-test-secret'\n",
            );
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);

            assert_eq!(
                error,
                ConfigError::EmptyValue {
                    field: "payment.stripe_publishable_key"
                }
            );
        });
    }

    #[test]
    fn oauth_clients_are_closed_validated_and_redacted() {
        with_clean_env(|_| {
            let path = temp_file(
                "[database]\nurl = 'sqlite::memory:'\n[oauth]\nmax_pending_authorizations = 32\nsession_ttl_secs = 120\nrefresh_enabled = false\nrefresh_interval_secs = 45\nrefresh_before_expiry_secs = 240\nrefresh_batch_size = 24\nrefresh_concurrency = 6\n[oauth.claude_code]\nclient_id = 'claude-client-marker'\n[oauth.codex]\nclient_id = 'codex-client-marker'\n[oauth.gemini]\nclient_id = 'gemini-client-marker'\nclient_secret = 'gemini-secret-marker'\n[oauth.antigravity]\nclient_id = 'antigravity-client-marker'\nclient_secret = 'antigravity-secret-marker'\n",
            );
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);

            assert_eq!(config.oauth().max_pending_authorizations(), 32);
            assert_eq!(config.oauth().session_ttl_secs(), 120);
            assert!(!config.oauth().refresh_enabled());
            assert_eq!(config.oauth().refresh_interval_secs(), 45);
            assert_eq!(config.oauth().refresh_before_expiry_secs(), 240);
            assert_eq!(config.oauth().refresh_batch_size(), 24);
            assert_eq!(config.oauth().refresh_concurrency(), 6);
            assert_eq!(config.oauth().configured_provider_count(), 4);
            assert_eq!(
                config.oauth().claude_code().unwrap().client_id().expose(),
                "claude-client-marker"
            );
            assert_eq!(
                config.oauth().codex().unwrap().client_id().expose(),
                "codex-client-marker"
            );
            assert_eq!(
                config.oauth().gemini().unwrap().client_secret().expose(),
                "gemini-secret-marker"
            );
            let debug = format!("{:?}", config.oauth());
            for private in [
                "claude-client-marker",
                "codex-client-marker",
                "gemini-client-marker",
                "gemini-secret-marker",
                "antigravity-client-marker",
                "antigravity-secret-marker",
            ] {
                assert!(!debug.contains(private));
            }
        });
    }

    #[test]
    fn oauth_configuration_rejects_incomplete_or_unbounded_values() {
        with_clean_env(|_| {
            for (body, expected) in [
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth]\nmax_pending_authorizations = 0\n",
                    ConfigError::NonPositive {
                        field: "oauth.max_pending_authorizations",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth]\nsession_ttl_secs = 59\n",
                    ConfigError::OutOfRange {
                        field: "oauth.session_ttl_secs",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth]\nrefresh_interval_secs = 0\n",
                    ConfigError::NonPositive {
                        field: "oauth.refresh_interval_secs",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth]\nrefresh_before_expiry_secs = 86401\n",
                    ConfigError::OutOfRange {
                        field: "oauth.refresh_before_expiry_secs",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth]\nrefresh_batch_size = 257\n",
                    ConfigError::OutOfRange {
                        field: "oauth.refresh_batch_size",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth]\nrefresh_batch_size = 8\nrefresh_concurrency = 9\n",
                    ConfigError::OutOfRange {
                        field: "oauth.refresh_concurrency",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth.claude_code]\nclient_id = ''\n",
                    ConfigError::InvalidField {
                        field: "oauth.claude_code.client_id",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth.codex]\nclient_id = ''\n",
                    ConfigError::InvalidField {
                        field: "oauth.codex.client_id",
                    },
                ),
                (
                    "[database]\nurl = 'sqlite::memory:'\n[oauth.gemini]\nclient_id = 'client'\n",
                    ConfigError::InvalidField {
                        field: "oauth.gemini.client_secret",
                    },
                ),
            ] {
                let path = raw_temp_file(body);
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, expected);
            }
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
            assert_eq!(
                config.server().client_ip_source(),
                ClientIpSource::XForwardedFor
            );
            assert_eq!(config.server().trusted_proxy_cidrs().len(), 1);
            assert!(config.auth().allow_query_api_key());
            assert_eq!(config.auth().lookup_timeout_secs(), 9);
            assert!(config.billing().batch_enabled());
            assert_eq!(config.billing().flush_interval_secs(), 19);
            assert_eq!(config.billing().flush_timeout_secs(), 23);
            assert_eq!(
                config.billing().wal_directory(),
                Path::new("environment-billing-wal")
            );
            assert!(!config.subscription().enabled());
            assert_eq!(config.subscription().batch_size(), 32);
            assert_eq!(config.subscription().interval_secs(), 90);
            assert_eq!(config.subscription().max_batches_per_run(), 6);
            assert_eq!(
                config.payment().stripe_secret_key().unwrap().expose(),
                "sk-environment-secret"
            );
            assert_eq!(
                config.payment().stripe_publishable_key(),
                Some("pk_test_environment")
            );
            assert_eq!(
                config.payment().stripe_webhook_secret().unwrap().expose(),
                "whsec-environment-secret"
            );
            assert_eq!(config.payment().stripe_signature_tolerance_secs(), 120);
            assert!(config.channel_probe().enabled());
            assert_eq!(config.channel_probe().batch_size(), 8);
            assert_eq!(config.channel_probe().interval_secs(), 120);
            assert_eq!(config.channel_probe().probe_timeout_secs(), 15);
            assert!(!config.oauth().refresh_enabled());
            assert_eq!(config.oauth().refresh_interval_secs(), 90);
            assert_eq!(config.oauth().refresh_before_expiry_secs(), 420);
            assert_eq!(config.oauth().refresh_batch_size(), 12);
            assert_eq!(config.oauth().refresh_concurrency(), 3);
            assert_eq!(
                config.oauth().claude_code().unwrap().client_id().expose(),
                "environment-claude-client"
            );
            assert_eq!(config.credential_encryption().key_id(), Some("primary"));
            assert_eq!(
                config.credential_encryption().key().unwrap().expose(),
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            );
            assert_eq!(config.http_client().max_cached_clients(), Some(3));
            assert_eq!(
                config.openai_upstream().unwrap().base_url(),
                "https://environment-upstream.example"
            );
            assert_eq!(
                config.openai_upstream().unwrap().model(),
                "environment-model"
            );
            assert_eq!(
                config.openai_upstream().unwrap().api_key().expose(),
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

        with_clean_env(|jail| {
            jail.create_file(
                "environment-override.toml",
                "[server]\nbind = '127.0.0.1:8080'\n[database]\nurl = 'sqlite::memory:'\n[auth]\nallow_query_api_key = false\nlookup_timeout_secs = 3\n[http_client]\nmax_cached_clients = 2\n",
            )
            .unwrap();
            let path = jail.directory().join("environment-override.toml");
            let output = Command::new(env::current_exe().unwrap())
                .arg("--exact")
                .arg("loader::tests::environment_values_override_toml_in_an_isolated_process")
                .arg("--nocapture")
                .env_clear()
                .env(CHILD_MARKER, "1")
                .env(CONFIG_FILE_ENV, &path)
                .env("AF_SERVER__BIND", "127.0.0.2:7070")
                .env("AF_SERVER__SHUTDOWN_TIMEOUT_SECS", "12")
                .env("AF_SERVER__CLIENT_IP_SOURCE", "x-forwarded-for")
                .env("AF_SERVER__TRUSTED_PROXY_CIDRS", "[\"127.0.0.0/8\"]")
                .env(
                    "AF_SERVER__CORS_ALLOWED_ORIGINS",
                    "[\"http://localhost:5173\"]",
                )
                .env("AF_TELEMETRY__LEVEL", "warn")
                .env("AF_AUTH__ALLOW_QUERY_API_KEY", "true")
                .env("AF_AUTH__LOOKUP_TIMEOUT_SECS", "9")
                .env("AF_BILLING__BATCH_ENABLED", "true")
                .env("AF_BILLING__FLUSH_INTERVAL_SECS", "19")
                .env("AF_BILLING__FLUSH_TIMEOUT_SECS", "23")
                .env("AF_BILLING__WAL_DIRECTORY", "environment-billing-wal")
                .env("AF_SUBSCRIPTION__ENABLED", "false")
                .env("AF_SUBSCRIPTION__BATCH_SIZE", "32")
                .env("AF_SUBSCRIPTION__INTERVAL_SECS", "90")
                .env("AF_SUBSCRIPTION__MAX_BATCHES_PER_RUN", "6")
                .env("AF_PAYMENT__STRIPE_SECRET_KEY", "sk-environment-secret")
                .env("AF_PAYMENT__STRIPE_PUBLISHABLE_KEY", "pk_test_environment")
                .env(
                    "AF_PAYMENT__STRIPE_WEBHOOK_SECRET",
                    "whsec-environment-secret",
                )
                .env("AF_PAYMENT__STRIPE_SIGNATURE_TOLERANCE_SECS", "120")
                .env("AF_CHANNEL_PROBE__ENABLED", "true")
                .env("AF_CHANNEL_PROBE__BATCH_SIZE", "8")
                .env("AF_CHANNEL_PROBE__INTERVAL_SECS", "120")
                .env("AF_CHANNEL_PROBE__PROBE_TIMEOUT_SECS", "15")
                .env("AF_OAUTH__REFRESH_ENABLED", "false")
                .env("AF_OAUTH__REFRESH_INTERVAL_SECS", "90")
                .env("AF_OAUTH__REFRESH_BEFORE_EXPIRY_SECS", "420")
                .env("AF_OAUTH__REFRESH_BATCH_SIZE", "12")
                .env("AF_OAUTH__REFRESH_CONCURRENCY", "3")
                .env(
                    "AF_OAUTH__CLAUDE_CODE__CLIENT_ID",
                    "environment-claude-client",
                )
                .env("AF_CREDENTIAL_ENCRYPTION__KEY_ID", "primary")
                .env(
                    "AF_CREDENTIAL_ENCRYPTION__KEY",
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                )
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
    fn dotenv_values_are_loaded_and_process_environment_wins() {
        with_clean_env(|jail| {
            jail.create_file(
                DEFAULT_ENV_FILE,
                "AF_DATABASE__URL=sqlite::memory:\nAF_SERVER__BIND=127.0.0.2:7070\n",
            )
            .unwrap();
            jail.set_env("AF_SERVER__BIND", "127.0.0.3:7070");

            let config = load().unwrap();
            assert_eq!(config.database().url().expose(), "sqlite::memory:");
            assert_eq!(config.server().bind().to_string(), "127.0.0.3:7070");
        });
    }

    #[test]
    fn malformed_dotenv_is_rejected_without_echoing_contents() {
        with_clean_env(|jail| {
            let secret = "dotenv-secret-value";
            jail.create_file(DEFAULT_ENV_FILE, &format!("AF_DATABASE__URL=\"{secret}\n"))
                .unwrap();

            let error = load().unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            assert_eq!(error, ConfigError::Invalid);
            assert!(!rendered.contains(secret));
        });
    }

    #[test]
    fn conf_default_is_preferred_and_toml_remains_legacy_fallback() {
        with_clean_env(|_| {
            fs::write(DEFAULT_CONFIG_FILE, "[database]\nurl = 'sqlite::memory:'\n").unwrap();
            let config = load().unwrap();
            assert_eq!(config.database().url().expose(), "sqlite::memory:");
            fs::remove_file(DEFAULT_CONFIG_FILE).unwrap();

            fs::write(LEGACY_CONFIG_FILE, "[database]\nurl = 'sqlite::memory:'\n").unwrap();
            let config = load().unwrap();
            assert_eq!(config.database().url().expose(), "sqlite::memory:");
            fs::remove_file(LEGACY_CONFIG_FILE).unwrap();
        });
    }

    #[test]
    fn checked_in_conf_template_loads_without_environment_overrides() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../anyflows.conf");
        let result = load_from(path);
        assert!(
            result.is_ok(),
            "仓库 anyflows.conf 必须是可解析的完整配置: {result:?}"
        );
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
                    "billing.flush_interval_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nflush_interval_secs = 0\n",
                ),
                (
                    "billing.flush_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nflush_timeout_secs = 0\n",
                ),
                (
                    "billing.usage_record_queue_capacity",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nusage_record_queue_capacity = 0\n",
                ),
                (
                    "billing.usage_record_worker_count",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nusage_record_worker_count = 0\n",
                ),
                (
                    "subscription.batch_size",
                    "[database]\nurl = 'sqlite::memory:'\n[subscription]\nbatch_size = 0\n",
                ),
                (
                    "subscription.interval_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[subscription]\ninterval_secs = 0\n",
                ),
                (
                    "subscription.max_batches_per_run",
                    "[database]\nurl = 'sqlite::memory:'\n[subscription]\nmax_batches_per_run = 0\n",
                ),
                (
                    "payment.stripe_signature_tolerance_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[payment]\nstripe_signature_tolerance_secs = 0\n",
                ),
                (
                    "channel_probe.batch_size",
                    "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\nbatch_size = 0\n",
                ),
                (
                    "channel_probe.interval_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\ninterval_secs = 0\n",
                ),
                (
                    "channel_probe.probe_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\nprobe_timeout_secs = 0\n",
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
    fn validates_management_session_key_and_ttl_without_leaking_key() {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

        with_clean_env(|_| {
            let key = URL_SAFE_NO_PAD.encode([0x42; 32]);
            let path = temp_file(&format!(
                "[database]\nurl = 'sqlite::memory:'\n[auth]\nsession_signing_key = '{key}'\nsession_ttl_secs = 7200\n"
            ));
            let config = load_from(&path).unwrap();
            let _ = fs::remove_file(path);
            assert_eq!(config.auth().session_signing_key().unwrap().expose(), key);
            assert_eq!(config.auth().session_ttl_secs(), 7_200);
            assert!(!format!("{:?}", config.auth()).contains(&key));

            for invalid_key in [
                "short".to_owned(),
                format!("{key}="),
                key.replace('Q', "+"),
                URL_SAFE_NO_PAD.encode([0x42; 31]),
                URL_SAFE_NO_PAD.encode([0x42; 33]),
            ] {
                let path = temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[auth]\nsession_signing_key = '{invalid_key}'\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(
                    error,
                    ConfigError::InvalidField {
                        field: "auth.session_signing_key"
                    }
                );
                assert!(!format!("{error:?}\n{error}").contains(&invalid_key));
            }

            for (value, expected) in [
                (
                    0,
                    ConfigError::NonPositive {
                        field: "auth.session_ttl_secs",
                    },
                ),
                (
                    86_401,
                    ConfigError::OutOfRange {
                        field: "auth.session_ttl_secs",
                    },
                ),
            ] {
                let path = temp_file(&format!(
                    "[database]\nurl = 'sqlite::memory:'\n[auth]\nsession_ttl_secs = {value}\n"
                ));
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, expected);
            }
        });
    }

    #[test]
    fn rejects_invalid_billing_settings_without_echoing_wal_directory() {
        with_clean_env(|_| {
            for (field, body) in [
                (
                    "billing.flush_interval_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nflush_interval_secs = 3601\n",
                ),
                (
                    "billing.flush_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nflush_timeout_secs = 301\n",
                ),
                (
                    "billing.usage_record_queue_capacity",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nusage_record_queue_capacity = 1000001\n",
                ),
                (
                    "billing.usage_record_worker_count",
                    "[database]\nurl = 'sqlite::memory:'\n[billing]\nusage_record_worker_count = 65\n",
                ),
                (
                    "subscription.batch_size",
                    "[database]\nurl = 'sqlite::memory:'\n[subscription]\nbatch_size = 101\n",
                ),
                (
                    "subscription.interval_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[subscription]\ninterval_secs = 3601\n",
                ),
                (
                    "subscription.max_batches_per_run",
                    "[database]\nurl = 'sqlite::memory:'\n[subscription]\nmax_batches_per_run = 65\n",
                ),
                (
                    "payment.stripe_signature_tolerance_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[payment]\nstripe_signature_tolerance_secs = 86401\n",
                ),
                (
                    "channel_probe.batch_size",
                    "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\nbatch_size = 65\n",
                ),
                (
                    "channel_probe.interval_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\ninterval_secs = 3601\n",
                ),
                (
                    "channel_probe.probe_timeout_secs",
                    "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\nprobe_timeout_secs = 301\n",
                ),
            ] {
                let path = temp_file(body);
                let error = load_from(&path).unwrap_err();
                let _ = fs::remove_file(path);
                assert_eq!(error, ConfigError::OutOfRange { field });
            }

            let path =
                temp_file("[database]\nurl = 'sqlite::memory:'\n[billing]\nwal_directory = ''\n");
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::EmptyValue {
                    field: "billing.wal_directory"
                }
            );
            let canary = "billing-path-secret-canary";
            let path = temp_file(&format!(
                "[database]\nurl = 'sqlite::memory:'\n[billing]\nwal_directory = '{canary}'\nflush_timeout_secs = 'invalid'\n"
            ));
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::InvalidField {
                    field: "billing.flush_timeout_secs"
                }
            );
            assert!(!rendered.contains(canary));
        });
    }

    #[test]
    fn enabled_channel_probe_requires_a_valid_redacted_credential_key() {
        with_clean_env(|_| {
            let path =
                temp_file("[database]\nurl = 'sqlite::memory:'\n[channel_probe]\nenabled = true\n");
            let error = load_from(&path).unwrap_err();
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::EmptyValue {
                    field: "credential_encryption.key"
                }
            );

            let canary = "credential-key-secret-canary";
            let path = temp_file(&format!(
                "[database]\nurl = 'sqlite::memory:'\n[channel_probe]\nenabled = true\n[credential_encryption]\nkey_id = 'primary'\nkey = '{canary}'\n"
            ));
            let error = load_from(&path).unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            let _ = fs::remove_file(path);
            assert_eq!(
                error,
                ConfigError::InvalidField {
                    field: "credential_encryption.key"
                }
            );
            assert!(!rendered.contains(canary));
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
