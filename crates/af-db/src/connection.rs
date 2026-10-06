use std::fmt;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, TransactionTrait};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber, subscriber::NoSubscriber};

use crate::{DatabaseDialect, DatabaseError, DatabaseOptions};

/// 受 af-db 边界保护的 SeaORM 连接池。
///
/// 应用层通过后续 Repository API 访问数据，避免直接依赖 SeaORM 类型并绕过存储边界。
#[derive(Clone)]
pub struct DatabasePool {
    connection: DatabaseConnection,
    dialect: DatabaseDialect,
    health_check_timeout: std::time::Duration,
    close_timeout: std::time::Duration,
}

impl DatabasePool {
    /// 返回连接池使用的数据库方言。
    pub fn dialect(&self) -> DatabaseDialect {
        self.dialect
    }

    /// 在配置的截止时间内验证连接池可用性。
    pub async fn ping(&self) -> Result<(), DatabaseError> {
        match timeout(self.health_check_timeout, self.connection.ping()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(DatabaseError::HealthCheck(error)),
            Err(source) => Err(DatabaseError::HealthCheckTimeout {
                timeout: self.health_check_timeout,
                source,
            }),
        }
    }

    /// 在配置的截止时间内关闭连接池并等待底层连接释放。
    pub async fn close(self) -> Result<(), DatabaseError> {
        match timeout(self.close_timeout, self.connection.close()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(DatabaseError::Close(error)),
            Err(source) => Err(DatabaseError::CloseTimeout {
                timeout: self.close_timeout,
                source,
            }),
        }
    }

    pub(crate) fn connection(&self) -> &DatabaseConnection {
        &self.connection
    }

    /// Borrow the shared connection for extension-owned persistence code.
    ///
    /// Extensions own their queries, transactions, validation, and deadlines.
    /// Application services should continue to use repository APIs. The opaque
    /// reference cannot close or replace the core-managed connection pool.
    pub fn extension_connection(&self) -> &(impl ConnectionTrait + TransactionTrait) {
        &self.connection
    }
}

impl fmt::Debug for DatabasePool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabasePool")
            .field("dialect", &self.dialect)
            .finish_non_exhaustive()
    }
}

/// 创建连接池并立即执行一次有截止时间的健康检查。
pub(crate) async fn connect(options: &DatabaseOptions) -> Result<DatabasePool, DatabaseError> {
    let connect_options = options.to_connect_options()?;

    // SeaORM 1.1 的方言 connector 会在 TRACE span 中输出含完整 URL 的连接参数。
    let connect_future =
        Database::connect(connect_options).with_subscriber(NoSubscriber::default());
    let connection = match timeout(options.connect_timeout(), connect_future).await {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => return Err(DatabaseError::Connect(error)),
        Err(source) => {
            return Err(DatabaseError::ConnectTimeout {
                timeout: options.connect_timeout(),
                source,
            });
        }
    };
    let pool = DatabasePool {
        connection,
        dialect: options.dialect(),
        health_check_timeout: options.health_check_timeout(),
        close_timeout: options.close_timeout(),
    };
    pool.ping().await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use sea_orm::TransactionTrait;
    use tracing::Level;
    use tracing_subscriber::fmt::format::FmtSpan;

    use super::*;
    use crate::PoolOptions;

    #[derive(Clone)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedWriter {
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

    #[tokio::test]
    async fn connection_credentials_never_enter_trace_output() -> Result<(), Box<dyn Error>> {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer_buffer = Arc::clone(&captured);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_max_level(Level::TRACE)
            .with_span_events(FmtSpan::NEW)
            .with_writer(move || SharedWriter(Arc::clone(&writer_buffer)))
            .finish();
        let options = DatabaseOptions::new(
            "postgres://sensitive-user:sensitive-password@127.0.0.1:1/private-database",
        )?
        .with_pool_options(PoolOptions {
            connect_timeout: Duration::from_millis(100),
            acquire_timeout: Duration::from_millis(100),
            ..PoolOptions::default()
        });

        let error = connect(&options)
            .with_subscriber(subscriber)
            .await
            .unwrap_err();
        let trace_output =
            String::from_utf8(captured.lock().expect("日志捕获缓冲区锁不应中毒").clone())?;
        let error_output = format!("{error:?}\n{error}");

        for secret in ["sensitive-user", "sensitive-password", "private-database"] {
            assert!(!trace_output.contains(secret));
            assert!(!error_output.contains(secret));
        }
        Ok(())
    }

    #[tokio::test]
    async fn close_timeout_is_enforced_while_a_transaction_is_held() -> Result<(), Box<dyn Error>> {
        let options =
            DatabaseOptions::new("sqlite::memory:")?.with_close_timeout(Duration::from_millis(1));
        let mut pool = connect(&options).await?;
        let transaction = pool.connection().begin().await?;

        let error = pool.clone().close().await.unwrap_err();
        assert!(matches!(error, DatabaseError::CloseTimeout { .. }));

        transaction.rollback().await?;
        // 超时断言使用极短预算，最终清理需容纳并发测试下的连接归还调度。
        pool.close_timeout = Duration::from_secs(1);
        pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn health_check_timeout_is_enforced_while_pool_is_exhausted() -> Result<(), Box<dyn Error>>
    {
        let options = DatabaseOptions::new("sqlite::memory:")?;
        let mut pool = connect(&options).await?;
        pool.health_check_timeout = Duration::from_millis(1);
        let transaction = pool.connection().begin().await?;

        let error = pool.ping().await.unwrap_err();
        assert!(matches!(error, DatabaseError::HealthCheckTimeout { .. }));

        transaction.rollback().await?;
        pool.close().await?;
        Ok(())
    }
}
