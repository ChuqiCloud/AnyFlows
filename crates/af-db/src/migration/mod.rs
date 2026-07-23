use std::time::Duration;

use sea_orm_migration::{MigrationTrait, MigratorTrait};
use tokio::time::timeout;

use crate::{DatabaseError, DatabaseOptionsError, DatabasePool};

mod iden;
mod m20260718_000001_create_groups;
mod m20260718_000002_create_users;
mod m20260718_000003_create_channels;
mod m20260718_000004_create_credentials;
mod m20260718_000005_create_tokens;
mod m20260718_000006_create_channel_models;
mod m20260718_000007_create_channel_groups;
mod m20260718_000008_create_abilities;
mod m20260718_000009_create_options;
mod m20260718_000010_create_billing_reservations;
mod m20260722_000011_create_billing_batch_checkpoints;
mod m20260722_000012_create_model_prices;
mod m20260723_000013_create_group_model_ratios;
mod schema;

#[cfg(test)]
mod tests;

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

pub(crate) struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260718_000001_create_groups::Migration),
            Box::new(m20260718_000002_create_users::Migration),
            Box::new(m20260718_000003_create_channels::Migration),
            Box::new(m20260718_000004_create_credentials::Migration),
            Box::new(m20260718_000005_create_tokens::Migration),
            Box::new(m20260718_000006_create_channel_models::Migration),
            Box::new(m20260718_000007_create_channel_groups::Migration),
            Box::new(m20260718_000008_create_abilities::Migration),
            Box::new(m20260718_000009_create_options::Migration),
            Box::new(m20260718_000010_create_billing_reservations::Migration),
            Box::new(m20260722_000011_create_billing_batch_checkpoints::Migration),
            Box::new(m20260722_000012_create_model_prices::Migration),
            Box::new(m20260723_000013_create_group_model_ratios::Migration),
        ]
    }
}

/// 执行项目注册表中的全部待处理迁移。
///
/// SeaORM 不提供跨实例迁移锁，部署时必须保证只有一个迁移执行者。PostgreSQL
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
