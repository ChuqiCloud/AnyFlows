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
    AppConfig, CorsOrigin, DatabaseConfig, HttpClientSettings, LogLevel, RedisSettings,
    ServerConfig, TelemetrySettings,
};
pub use secret::SecretString;
