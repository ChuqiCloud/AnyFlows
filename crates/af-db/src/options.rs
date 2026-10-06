use std::fmt;
use std::time::Duration;

use sea_orm::ConnectOptions;
use url::Url;

use crate::DatabaseOptionsError;

const DEFAULT_NETWORK_MAX_CONNECTIONS: u32 = 32;
const DEFAULT_MIN_CONNECTIONS: u32 = 1;
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(30 * 60);
const DEFAULT_HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

/// AnyFlows 支持的数据库方言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseDialect {
    /// PostgreSQL。
    Postgres,
    /// MySQL。
    MySql,
    /// SQLite。
    Sqlite,
}

impl DatabaseDialect {
    fn from_url(url: &str) -> Result<Self, DatabaseOptionsError> {
        if url.trim().is_empty() {
            return Err(DatabaseOptionsError::EmptyUrl);
        }

        let parsed = Url::parse(url).map_err(|_| DatabaseOptionsError::InvalidUrl)?;
        match parsed.scheme() {
            "postgres" | "postgresql" => Ok(Self::Postgres),
            "mysql" => Ok(Self::MySql),
            "sqlite" => Ok(Self::Sqlite),
            scheme => Err(DatabaseOptionsError::UnsupportedScheme {
                scheme: scheme.to_owned(),
            }),
        }
    }
}

/// SeaORM/SQLx 连接池参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolOptions {
    /// 池内允许的最大连接数。
    pub max_connections: u32,
    /// 启动后保持的最小连接数。
    pub min_connections: u32,
    /// 创建连接池并建立首条连接的最长等待时间。
    pub connect_timeout: Duration,
    /// 从池中获取连接的最长等待时间。
    pub acquire_timeout: Duration,
    /// 空闲连接的最长保留时间，`None` 表示使用驱动默认值。
    pub idle_timeout: Option<Duration>,
    /// 单条连接的最长生命周期，`None` 表示使用驱动默认值。
    pub max_lifetime: Option<Duration>,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_NETWORK_MAX_CONNECTIONS,
            min_connections: DEFAULT_MIN_CONNECTIONS,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
            idle_timeout: Some(DEFAULT_IDLE_TIMEOUT),
            max_lifetime: Some(DEFAULT_MAX_LIFETIME),
        }
    }
}

impl PoolOptions {
    fn for_dialect(dialect: DatabaseDialect) -> Self {
        if dialect == DatabaseDialect::Sqlite {
            // SQLite 默认单连接，避免内存库隔离及文件库写锁竞争。
            return Self {
                max_connections: 1,
                min_connections: 1,
                ..Self::default()
            };
        }
        Self::default()
    }

    fn validate(&self) -> Result<(), DatabaseOptionsError> {
        if self.max_connections == 0 {
            return Err(DatabaseOptionsError::ZeroMaxConnections);
        }
        if self.min_connections > self.max_connections {
            return Err(DatabaseOptionsError::MinConnectionsExceedMax {
                min: self.min_connections,
                max: self.max_connections,
            });
        }
        validate_timeout("connect_timeout", self.connect_timeout)?;
        validate_timeout("acquire_timeout", self.acquire_timeout)?;
        validate_optional_timeout("idle_timeout", self.idle_timeout)?;
        validate_optional_timeout("max_lifetime", self.max_lifetime)
    }
}

/// 数据库启动参数。
#[derive(Clone)]
pub struct DatabaseOptions {
    url: String,
    dialect: DatabaseDialect,
    pool: PoolOptions,
    health_check_timeout: Duration,
    close_timeout: Duration,
    sqlx_logging: bool,
}

impl DatabaseOptions {
    /// 从数据库 URL 创建启动参数，并按方言选择安全默认池大小。
    pub fn new(url: impl Into<String>) -> Result<Self, DatabaseOptionsError> {
        let url = url.into();
        let dialect = DatabaseDialect::from_url(&url)?;
        Ok(Self {
            url,
            dialect,
            pool: PoolOptions::for_dialect(dialect),
            health_check_timeout: DEFAULT_HEALTH_CHECK_TIMEOUT,
            close_timeout: DEFAULT_CLOSE_TIMEOUT,
            sqlx_logging: false,
        })
    }

    /// 返回已识别的数据库方言。
    pub fn dialect(&self) -> DatabaseDialect {
        self.dialect
    }

    /// 覆盖连接池参数；参数会在连接前统一校验。
    pub fn with_pool_options(mut self, pool: PoolOptions) -> Self {
        self.pool = pool;
        self
    }

    /// 覆盖健康检查截止时间；零值会在连接前被拒绝。
    pub fn with_health_check_timeout(mut self, timeout: Duration) -> Self {
        self.health_check_timeout = timeout;
        self
    }

    /// 覆盖连接池关闭的截止时间；零值会在连接前被拒绝。
    pub fn with_close_timeout(mut self, timeout: Duration) -> Self {
        self.close_timeout = timeout;
        self
    }

    /// 控制 SQLx 语句日志；默认关闭，避免无意记录敏感 SQL。
    pub fn with_sqlx_logging(mut self, enabled: bool) -> Self {
        self.sqlx_logging = enabled;
        self
    }

    /// 在实际连接前校验全部连接池与超时参数。
    pub fn validate(&self) -> Result<(), DatabaseOptionsError> {
        self.pool.validate()?;
        validate_timeout("health_check_timeout", self.health_check_timeout)?;
        validate_timeout("close_timeout", self.close_timeout)
    }

    pub(crate) fn connect_timeout(&self) -> Duration {
        self.pool.connect_timeout
    }

    pub(crate) fn health_check_timeout(&self) -> Duration {
        self.health_check_timeout
    }

    pub(crate) fn close_timeout(&self) -> Duration {
        self.close_timeout
    }

    /// 文件 SQLite 的并发池必须先通过独立单连接完成迁移，避免多步表重建跨连接执行。
    pub(crate) fn dedicated_migration_options(&self) -> Option<Self> {
        if self.dialect != DatabaseDialect::Sqlite
            || self.pool.max_connections <= 1
            || self.sqlite_is_memory()
        {
            return None;
        }
        let mut options = self.clone();
        options.pool.max_connections = 1;
        options.pool.min_connections = 1;
        Some(options)
    }

    fn sqlite_is_memory(&self) -> bool {
        Url::parse(&self.url).is_ok_and(|url| {
            url.path() == ":memory:"
                || url
                    .query_pairs()
                    .any(|(key, value)| key == "mode" && value.eq_ignore_ascii_case("memory"))
        })
    }

    pub(crate) fn to_connect_options(&self) -> Result<ConnectOptions, DatabaseOptionsError> {
        self.validate()?;

        // SeaORM 的 Debug 会包含完整 URL，该值只能交给 connection 模块的无日志连接路径。
        let mut options = ConnectOptions::new(self.url.clone());
        options
            .max_connections(self.pool.max_connections)
            .min_connections(self.pool.min_connections)
            .acquire_timeout(self.pool.acquire_timeout)
            .test_before_acquire(true)
            .connect_lazy(false)
            .sqlx_logging(self.sqlx_logging);
        if let Some(timeout) = self.pool.idle_timeout {
            options.idle_timeout(timeout);
        }
        if let Some(timeout) = self.pool.max_lifetime {
            options.max_lifetime(timeout);
        }
        if self.dialect == DatabaseDialect::MySql {
            // DATETIME 不携带时区，必须覆盖 URL 参数并统一为 UTC 会话。
            options.map_sqlx_mysql_opts(|options| options.timezone(Some("+00:00".to_owned())));
        }
        Ok(options)
    }
}

impl fmt::Debug for DatabaseOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseOptions")
            .field("url", &"<redacted>")
            .field("dialect", &self.dialect)
            .field("pool", &self.pool)
            .field("health_check_timeout", &self.health_check_timeout)
            .field("close_timeout", &self.close_timeout)
            .field("sqlx_logging", &self.sqlx_logging)
            .finish()
    }
}

fn validate_timeout(field: &'static str, timeout: Duration) -> Result<(), DatabaseOptionsError> {
    if timeout.is_zero() {
        return Err(DatabaseOptionsError::ZeroTimeout { field });
    }
    Ok(())
}

fn validate_optional_timeout(
    field: &'static str,
    timeout: Option<Duration>,
) -> Result<(), DatabaseOptionsError> {
    if let Some(timeout) = timeout {
        validate_timeout(field, timeout)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_database_dialects() {
        let cases = [
            ("postgres://localhost/db", DatabaseDialect::Postgres),
            ("postgresql://localhost/db", DatabaseDialect::Postgres),
            ("mysql://localhost/db", DatabaseDialect::MySql),
            ("sqlite::memory:", DatabaseDialect::Sqlite),
        ];

        for (url, expected) in cases {
            assert_eq!(DatabaseOptions::new(url).unwrap().dialect(), expected);
        }
    }

    #[test]
    fn rejects_invalid_or_unsupported_urls() {
        assert_eq!(
            DatabaseOptions::new(" ").unwrap_err(),
            DatabaseOptionsError::EmptyUrl
        );
        assert_eq!(
            DatabaseOptions::new("not a url").unwrap_err(),
            DatabaseOptionsError::InvalidUrl
        );
        assert_eq!(
            DatabaseOptions::new("redis://localhost").unwrap_err(),
            DatabaseOptionsError::UnsupportedScheme {
                scheme: "redis".to_owned()
            }
        );
    }

    #[test]
    fn rejects_invalid_pool_and_timeout_values() -> Result<(), DatabaseOptionsError> {
        let mut pool = PoolOptions {
            max_connections: 0,
            ..PoolOptions::default()
        };
        let options =
            DatabaseOptions::new("postgres://localhost/db")?.with_pool_options(pool.clone());
        assert_eq!(
            options.validate(),
            Err(DatabaseOptionsError::ZeroMaxConnections)
        );

        pool.max_connections = 1;
        pool.min_connections = 2;
        let options = DatabaseOptions::new("postgres://localhost/db")?.with_pool_options(pool);
        assert_eq!(
            options.validate(),
            Err(DatabaseOptionsError::MinConnectionsExceedMax { min: 2, max: 1 })
        );

        let options = DatabaseOptions::new("postgres://localhost/db")?
            .with_health_check_timeout(Duration::ZERO);
        assert_eq!(
            options.validate(),
            Err(DatabaseOptionsError::ZeroTimeout {
                field: "health_check_timeout"
            })
        );

        let options =
            DatabaseOptions::new("postgres://localhost/db")?.with_close_timeout(Duration::ZERO);
        assert_eq!(
            options.validate(),
            Err(DatabaseOptionsError::ZeroTimeout {
                field: "close_timeout"
            })
        );

        Ok(())
    }

    #[test]
    fn keeps_startup_timeout_outside_sea_orm_pool_options() {
        let options = DatabaseOptions::new("postgres://localhost/db").unwrap();
        let sea_orm_options = options.to_connect_options().unwrap();

        assert_eq!(sea_orm_options.get_connect_timeout(), None);
        assert_eq!(
            sea_orm_options.get_acquire_timeout(),
            Some(options.pool.acquire_timeout)
        );
    }

    #[test]
    fn file_sqlite_uses_one_connection_for_migrations_before_reopening_parallel_pool() {
        let parallel_pool = PoolOptions {
            max_connections: 8,
            min_connections: 2,
            ..PoolOptions::default()
        };
        let file = DatabaseOptions::new("sqlite:///tmp/anyflows.db?mode=rwc")
            .unwrap()
            .with_pool_options(parallel_pool.clone());
        let dedicated = file
            .dedicated_migration_options()
            .expect("文件 SQLite 并发池必须使用独立迁移连接");
        assert_eq!(
            (
                dedicated.pool.max_connections,
                dedicated.pool.min_connections
            ),
            (1, 1)
        );
        assert_eq!(
            (file.pool.max_connections, file.pool.min_connections),
            (8, 2)
        );

        for url in ["sqlite::memory:", "sqlite:///memory.db?mode=memory"] {
            let memory = DatabaseOptions::new(url)
                .unwrap()
                .with_pool_options(parallel_pool.clone());
            assert!(memory.dedicated_migration_options().is_none());
        }
        let postgres = DatabaseOptions::new("postgres://localhost/db")
            .unwrap()
            .with_pool_options(parallel_pool);
        assert!(postgres.dedicated_migration_options().is_none());
    }

    #[test]
    fn redacts_database_url_from_debug_output() {
        let options =
            DatabaseOptions::new("postgres://sensitive-user:sensitive-password@localhost/private")
                .unwrap();

        let rendered = format!("{options:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("sensitive-user"));
        assert!(!rendered.contains("sensitive-password"));
        assert!(!rendered.contains("private"));
    }
}
