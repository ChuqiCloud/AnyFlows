use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::{ConfigError, SecretString};

/// 服务监听配置。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    bind: SocketAddr,
}

impl ServerConfig {
    /// 返回服务监听地址。
    #[must_use]
    pub const fn bind(&self) -> SocketAddr {
        self.bind
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080"
                .parse()
                .expect("内建监听地址必须是有效的 SocketAddr"),
        }
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
