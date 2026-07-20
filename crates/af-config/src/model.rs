use std::{fmt, net::SocketAddr, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use url::Url;

use crate::{ConfigError, CorsOriginError, SecretString};

/// 经过规范化的 HTTP(S) CORS Origin，不包含路径、凭据或通配语义。
#[derive(Clone, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CorsOrigin(String);

impl CorsOrigin {
    /// 返回可直接用于 `Origin` 精确比较的规范 ASCII 文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CorsOrigin {
    type Err = CorsOriginError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || value.trim() != value
            || matches!(value, "*" | "null")
            || !has_strict_origin_shape(value)
        {
            return Err(CorsOriginError);
        }
        let parsed = Url::parse(value).map_err(|_| CorsOriginError)?;
        let supported_scheme = matches!(parsed.scheme(), "http" | "https");
        let has_credentials = !parsed.username().is_empty() || parsed.password().is_some();
        if !supported_scheme
            || !parsed.has_host()
            || has_credentials
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(CorsOriginError);
        }

        let canonical = parsed.origin().ascii_serialization();
        if canonical == "null" {
            return Err(CorsOriginError);
        }
        Ok(Self(canonical))
    }
}

/// 在 URL 规范化前拒绝 userinfo、点路径等会被解析器消除的非 Origin 结构。
fn has_strict_origin_shape(value: &str) -> bool {
    let Some((_, scheme_relative)) = value.split_once("://") else {
        return false;
    };
    let authority_end = scheme_relative
        .find(['/', '?', '#'])
        .unwrap_or(scheme_relative.len());
    let authority = &scheme_relative[..authority_end];
    let suffix = &scheme_relative[authority_end..];

    !authority.is_empty()
        && !authority.contains('@')
        && !authority.contains('\\')
        && !authority.ends_with(':')
        && matches!(suffix, "" | "/")
}

impl<'de> Deserialize<'de> for CorsOrigin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

impl fmt::Debug for CorsOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CorsOrigin")
            .field(&"<已配置>")
            .finish()
    }
}

/// 服务监听配置。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    bind: SocketAddr,
    cors_allowed_origins: Vec<CorsOrigin>,
}

impl ServerConfig {
    /// 返回服务监听地址。
    #[must_use]
    pub const fn bind(&self) -> SocketAddr {
        self.bind
    }

    /// 返回允许访问业务路由的规范 CORS Origin；空列表表示关闭跨域。
    #[must_use]
    pub fn cors_allowed_origins(&self) -> &[CorsOrigin] {
        &self.cors_allowed_origins
    }

    /// 替换业务路由 CORS 白名单，供嵌入式启动与测试构造使用。
    #[must_use]
    pub fn with_cors_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = CorsOrigin>,
    ) -> Self {
        self.cors_allowed_origins = origins.into_iter().collect();
        self
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080"
                .parse()
                .expect("内建监听地址必须是有效的 SocketAddr"),
            cors_allowed_origins: Vec::new(),
        }
    }
}

/// AnyFlows 自身结构化日志的最高详细级别。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// 完全关闭 tracing 输出。
    Off,
    /// 仅输出错误。
    Error,
    /// 输出警告与错误。
    Warn,
    /// 输出生产默认的信息、警告与错误。
    #[default]
    Info,
    /// 输出调试及以上级别。
    Debug,
    /// 输出全部 tracing 事件。
    Trace,
}

/// tracing 启动设置；运行期调整通过 telemetry reload handle 完成。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetrySettings {
    level: LogLevel,
}

impl TelemetrySettings {
    /// 使用指定日志级别构造设置。
    #[must_use]
    pub const fn new(level: LogLevel) -> Self {
        Self { level }
    }

    /// 返回 AnyFlows 自身日志的最高详细级别。
    #[must_use]
    pub const fn level(&self) -> LogLevel {
        self.level
    }
}

/// 数据库启动配置。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    url: SecretString,
    migration_timeout_secs: Option<u64>,
}

impl DatabaseConfig {
    /// 返回受保护的数据库连接 URL；下层连接器必须显式读取明文。
    #[must_use]
    pub const fn url(&self) -> &SecretString {
        &self.url
    }

    /// 返回启动迁移截止时间覆盖（秒）；缺省时沿用数据库层默认值。
    #[must_use]
    pub const fn migration_timeout_secs(&self) -> Option<u64> {
        self.migration_timeout_secs
    }
}

/// 可选 Redis 连接配置；缺省时由缓存层运行在本地模式。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RedisSettings {
    url: Option<SecretString>,
}

impl RedisSettings {
    /// 返回受保护的 Redis URL；为 `None` 时不得尝试建立远程连接。
    #[must_use]
    pub fn url(&self) -> Option<&SecretString> {
        self.url.as_ref()
    }
}

/// HTTP 客户端池与三段超时配置。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpClientSettings {
    max_cached_clients: Option<usize>,
    connect_timeout_secs: Option<u64>,
    read_timeout_secs: Option<u64>,
    request_timeout_secs: Option<u64>,
}

impl HttpClientSettings {
    /// 返回 Client LRU 最大容量覆盖；缺省时沿用 HTTP 客户端层默认值。
    #[must_use]
    pub const fn max_cached_clients(&self) -> Option<usize> {
        self.max_cached_clients
    }

    /// 返回连接阶段超时覆盖（秒）。
    #[must_use]
    pub const fn connect_timeout_secs(&self) -> Option<u64> {
        self.connect_timeout_secs
    }

    /// 返回读取阶段超时覆盖（秒）。
    #[must_use]
    pub const fn read_timeout_secs(&self) -> Option<u64> {
        self.read_timeout_secs
    }

    /// 返回单请求总超时覆盖（秒）。
    #[must_use]
    pub const fn request_timeout_secs(&self) -> Option<u64> {
        self.request_timeout_secs
    }
}

/// 应用启动期配置。运营期 Option 热更新不属于此类型。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    telemetry: TelemetrySettings,
    database: DatabaseConfig,
    #[serde(default)]
    redis: RedisSettings,
    #[serde(default)]
    http_client: HttpClientSettings,
}

impl AppConfig {
    /// 返回服务监听配置。
    #[must_use]
    pub const fn server(&self) -> &ServerConfig {
        &self.server
    }

    /// 返回 tracing 启动设置。
    #[must_use]
    pub const fn telemetry(&self) -> &TelemetrySettings {
        &self.telemetry
    }

    /// 返回数据库配置。
    #[must_use]
    pub const fn database(&self) -> &DatabaseConfig {
        &self.database
    }

    /// 返回可选 Redis 配置。
    #[must_use]
    pub const fn redis(&self) -> &RedisSettings {
        &self.redis
    }

    /// 返回 HTTP 客户端配置。
    #[must_use]
    pub const fn http_client(&self) -> &HttpClientSettings {
        &self.http_client
    }

    /// 校验敏感文本与所有受控数值，确保错误在启动期暴露。
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.database.url.expose().trim().is_empty() {
            return Err(ConfigError::EmptyValue {
                field: "database.url",
            });
        }
        if self.database.migration_timeout_secs == Some(0) {
            return Err(ConfigError::NonPositive {
                field: "database.migration_timeout_secs",
            });
        }
        if let Some(url) = self.redis.url()
            && url.expose().trim().is_empty()
        {
            return Err(ConfigError::EmptyValue { field: "redis.url" });
        }
        if self.http_client.max_cached_clients == Some(0) {
            return Err(ConfigError::NonPositive {
                field: "http_client.max_cached_clients",
            });
        }
        for (value, field) in [
            (
                self.http_client.connect_timeout_secs,
                "http_client.connect_timeout_secs",
            ),
            (
                self.http_client.read_timeout_secs,
                "http_client.read_timeout_secs",
            ),
            (
                self.http_client.request_timeout_secs,
                "http_client.request_timeout_secs",
            ),
        ] {
            if value == Some(0) {
                return Err(ConfigError::NonPositive { field });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origin_normalizes_safe_http_origins() {
        assert_eq!(
            "https://Console.Example:443/"
                .parse::<CorsOrigin>()
                .unwrap()
                .as_str(),
            "https://console.example"
        );
        assert_eq!(
            "http://127.0.0.1:5173"
                .parse::<CorsOrigin>()
                .unwrap()
                .as_str(),
            "http://127.0.0.1:5173"
        );
    }

    #[test]
    fn cors_origin_rejects_ambient_or_non_origin_values_without_echoing_them() {
        for value in [
            "*",
            "null",
            "file:///tmp/index.html",
            "https://user:secret@console.example",
            "https://@console.example",
            "https://console.example/path",
            "https://console.example/a/..",
            "https://console.example/%2e",
            "https://console.example?secret=value",
            "https://console.example#fragment",
            " https://console.example",
        ] {
            let error = value.parse::<CorsOrigin>().unwrap_err();
            let rendered = format!("{error:?}\n{error}");
            assert_eq!(error, CorsOriginError);
            assert!(!rendered.contains(value));
        }
    }

    #[test]
    fn cors_origin_debug_does_not_expose_internal_hosts() {
        let origin = "https://internal-console.example"
            .parse::<CorsOrigin>()
            .unwrap();
        let debug = format!("{origin:?}");
        assert!(!debug.contains("internal-console.example"));
    }
}
