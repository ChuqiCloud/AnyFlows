//! 数据库连接池、迁移、实体与仓储边界。

mod auth;
#[cfg(test)]
mod auth_tests;
mod connection;
mod entity;
mod error;
mod migration;
mod options;
mod quota;
#[cfg(test)]
mod quota_tests;

pub use auth::{
    TokenAuthLookup, TokenAuthLookupError, TokenAuthLookupOutcome, TokenAuthRepository,
    TokenAuthRepositoryConfigError, TokenAuthRepositoryError,
};
pub use connection::DatabasePool;
pub(crate) use connection::connect;
pub use error::{DatabaseError, DatabaseOptionsError};
pub use migration::{MigrationOptions, run_pending_migrations};
pub use options::{DatabaseDialect, DatabaseOptions, PoolOptions};
pub use quota::{
    QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus,
};

/// 连接数据库并执行全部待处理迁移。
///
/// 这是服务启动的标准入口。迁移失败时不会返回连接池，调用方必须拒绝启动。
pub async fn connect_and_migrate(
    database_options: &DatabaseOptions,
    migration_options: MigrationOptions,
) -> Result<DatabasePool, DatabaseError> {
    let pool = connect(database_options).await?;
    if let Err(error) = run_pending_migrations(&pool, migration_options).await {
        // 迁移失败时主动执行有界关闭，同时保留原始迁移错误供启动层判定。
        let _ = pool.close().await;
        return Err(error);
    }
    Ok(pool)
}
