//! 启动期分层配置加载与运行期动态 Option 契约。

mod dynamic_options;
mod error;
mod loader;
mod model;
mod secret;

pub use dynamic_options::{
    DynamicOptionError, DynamicOptionSignal, DynamicOptionSnapshot, DynamicOptionSource,
    DynamicOptionSubscription,
};
pub use error::{ConfigError, CorsOriginError};
pub use loader::{CONFIG_FILE_ENV, DEFAULT_CONFIG_FILE, ENV_PREFIX, load, load_from};
pub use model::{
    AppConfig, AuthSettings, BillingSettings, CREDENTIAL_ENCRYPTION_KEY_BYTES,
    ChannelProbeSettings, ClientIpSource, CorsOrigin, CredentialEncryptionSettings,
    DEFAULT_AUTH_LOOKUP_TIMEOUT_SECS, DEFAULT_BILLING_FLUSH_INTERVAL_SECS,
    DEFAULT_BILLING_FLUSH_TIMEOUT_SECS, DEFAULT_CHANNEL_PROBE_BATCH_SIZE,
    DEFAULT_CHANNEL_PROBE_ENABLED, DEFAULT_CHANNEL_PROBE_INTERVAL_SECS,
    DEFAULT_CHANNEL_PROBE_TIMEOUT_SECS, DEFAULT_SHUTDOWN_TIMEOUT_SECS, DatabaseConfig,
    HttpClientSettings, LogLevel, MAX_AUTH_LOOKUP_TIMEOUT_SECS, MAX_BILLING_FLUSH_INTERVAL_SECS,
    MAX_BILLING_FLUSH_TIMEOUT_SECS, MAX_CHANNEL_PROBE_BATCH_SIZE, MAX_CHANNEL_PROBE_INTERVAL_SECS,
    MAX_CHANNEL_PROBE_TIMEOUT_SECS, MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES,
    MAX_DATABASE_HEALTH_CHECK_TIMEOUT_SECS, MAX_OPENAI_UPSTREAM_API_KEY_BYTES,
    MAX_OPENAI_UPSTREAM_BASE_URL_BYTES, MAX_OPENAI_UPSTREAM_MODEL_BYTES, MAX_SHUTDOWN_TIMEOUT_SECS,
    MAX_TRUSTED_PROXY_CIDRS, OpenAiUpstreamConfig, RedisSettings, ServerConfig, TelemetrySettings,
};
pub use secret::SecretString;
