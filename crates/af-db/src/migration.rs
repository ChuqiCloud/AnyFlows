use std::time::Duration;

use sea_orm_migration::{MigrationTrait, MigratorTrait};
use tokio::time::timeout;

use crate::{DatabaseError, DatabaseOptionsError, DatabasePool};

const DEFAULT_MIGRATION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// 待处理迁移的执行参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationOptions {
    timeout: Duration,
}

impl MigrationOptions {
    /// 创建迁移参数；零截止时间会被拒绝。
    pub fn new(timeout: Duration) -> Result<Self, DatabaseOptionsError> {
        if timeout.is_zero() {
            return Err(DatabaseOptionsError::ZeroTimeout {
                field: "migration_timeout",
            });
        }
        Ok(Self { timeout })
    }

    /// 返回完整迁移批次的截止时间。
    pub fn timeout(self) -> Duration {
        self.timeout
    }
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_MIGRATION_TIMEOUT,
        }
    }
}

struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Vec::new()
    }
}

/// 执行项目注册表中的全部待处理迁移。
///
/// SeaORM 不提供跨实例迁移锁，部署时必须保证只有一个 migration owner。PostgreSQL
/// 会为迁移批次开启事务，而 MySQL/SQLite 不具备同等 DDL 原子性；失败或超时后不会自动
/// 重试，调用方必须拒绝启动并检查数据库状态。
pub async fn run_pending_migrations(
    pool: &DatabasePool,
    options: MigrationOptions,
) -> Result<(), DatabaseError> {
    run_with::<Migrator>(pool, options).await
}

async fn run_with<M>(pool: &DatabasePool, options: MigrationOptions) -> Result<(), DatabaseError>
where
    M: MigratorTrait,
{
    match timeout(options.timeout(), M::up(pool.connection(), None)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(DatabaseError::Migration(error)),
        Err(source) => Err(DatabaseError::MigrationTimeout {
            timeout: options.timeout(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::error::Error;

    use sea_orm_migration::prelude::*;

    use super::*;
    use crate::{DatabaseOptions, PoolOptions, connect};

    const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
    const REQUIRE_LIVE_DATABASE: &str = "AF_REQUIRE_LIVE_DATABASE";

    struct SmokeMigrator;

    impl MigratorTrait for SmokeMigrator {
        fn migrations() -> Vec<Box<dyn MigrationTrait>> {
            vec![Box::new(SmokeMigration)]
        }
    }

    #[derive(DeriveMigrationName)]
    struct SmokeMigration;

    #[async_trait::async_trait]
    impl MigrationTrait for SmokeMigration {
        async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager
                .create_table(
                    Table::create()
                        .table(SmokeTable::Table)
                        .if_not_exists()
                        .col(
                            ColumnDef::new(SmokeTable::Id)
                                .big_integer()
                                .not_null()
                                .primary_key(),
                        )
                        .col(ColumnDef::new(SmokeTable::Marker).string_len(64).not_null())
                        .to_owned(),
                )
                .await
        }

        async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager
                .drop_table(Table::drop().table(SmokeTable::Table).to_owned())
                .await
        }
    }

    #[derive(DeriveIden)]
    enum SmokeTable {
        Table,
        Id,
        Marker,
    }

    struct SlowMigrator;

    impl MigratorTrait for SlowMigrator {
        fn migrations() -> Vec<Box<dyn MigrationTrait>> {
            vec![Box::new(SlowMigration)]
        }
    }

    #[derive(DeriveMigrationName)]
    struct SlowMigration;

    #[async_trait::async_trait]
    impl MigrationTrait for SlowMigration {
        async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn standard_startup_installs_migration_tracking_table() -> Result<(), Box<dyn Error>> {
        let database_options = DatabaseOptions::new("sqlite::memory:")?;
        let pool =
            crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;

        assert!(
            SchemaManager::new(pool.connection())
                .has_table("seaql_migrations")
                .await?
        );
        pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn migration_deadline_fails_without_retrying() -> Result<(), Box<dyn Error>> {
        let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
        let migration_options = MigrationOptions::new(Duration::from_millis(1))?;

        let error = run_with::<SlowMigrator>(&pool, migration_options)
            .await
            .unwrap_err();
        assert!(matches!(error, DatabaseError::MigrationTimeout { .. }));

        pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn migration_runner_smoke() -> Result<(), Box<dyn Error>> {
        let database_url = test_database_url();
        let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
            max_connections: 1,
            min_connections: 1,
            ..PoolOptions::default()
        });
        let pool = connect(&options).await?;

        run_with::<SmokeMigrator>(&pool, MigrationOptions::default()).await?;
        run_with::<SmokeMigrator>(&pool, MigrationOptions::default()).await?;

        {
            let manager = SchemaManager::new(pool.connection());
            assert!(manager.has_table("smoke_table").await?);
            assert!(
                SmokeMigrator::get_pending_migrations(pool.connection())
                    .await?
                    .is_empty()
            );
            assert_eq!(
                SmokeMigrator::get_applied_migrations(pool.connection())
                    .await?
                    .len(),
                1
            );

            SmokeMigrator::down(pool.connection(), Some(1)).await?;
            assert!(!manager.has_table("smoke_table").await?);
        }

        pool.close().await?;
        Ok(())
    }

    fn test_database_url() -> String {
        match env::var(TEST_DATABASE_URL) {
            Ok(url) if !url.trim().is_empty() => url,
            _ if env::var(REQUIRE_LIVE_DATABASE).as_deref() == Ok("1") => {
                panic!("强制数据库冒烟测试缺少 {TEST_DATABASE_URL}")
            }
            _ => "sqlite::memory:".to_owned(),
        }
    }
}
