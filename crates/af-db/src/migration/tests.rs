use std::env;
use std::error::Error;

use sea_orm_migration::prelude::*;

use super::Migrator;
use crate::{DatabaseOptions, PoolOptions, connect};

const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
const ALLOW_DESTRUCTIVE_MIGRATION_TEST: &str = "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST";

fn test_database_url() -> String {
    env::var(TEST_DATABASE_URL).unwrap_or_else(|_| "sqlite::memory:".to_owned())
}

fn validate_destructive_test_target(database_url: &str) -> Result<(), Box<dyn Error>> {
    if env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref() != Some("1") {
        return Err(format!(
            "迁移冒烟测试需要设置 {ALLOW_DESTRUCTIVE_MIGRATION_TEST}=1（目标：{database_url}）"
        )
        .into());
    }
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库迁移流水线执行"]
async fn migration_runner_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(&database_url)?;

    let is_sqlite = database_url.starts_with("sqlite:");
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        max_connections: if is_sqlite { 1 } else { 4 },
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;

    // 先执行历史节点，再执行剩余迁移，覆盖公共核心的升级路径。
    Migrator::up(pool.connection(), Some(54)).await?;
    assert!(
        !Migrator::get_pending_migrations(pool.connection())
            .await?
            .is_empty()
    );

    Migrator::up(pool.connection(), None).await?;
    assert!(
        Migrator::get_pending_migrations(pool.connection())
            .await?
            .is_empty()
    );

    pool.close().await?;
    Ok(())
}
