use std::env;
use std::error::Error;

use sea_orm_migration::prelude::*;

use super::{LEGACY_ENTERPRISE_MIGRATION_VERSIONS, Migrator};
use crate::{DatabaseOptions, PoolOptions, connect};

const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
const ALLOW_DESTRUCTIVE_MIGRATION_TEST: &str = "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST";

#[test]
fn legacy_enterprise_history_allowlist_is_exact_and_unique() {
    let unique = LEGACY_ENTERPRISE_MIGRATION_VERSIONS
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let public_migrations = Migrator::migrations();
    let public = public_migrations
        .iter()
        .map(|migration| migration.name())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(LEGACY_ENTERPRISE_MIGRATION_VERSIONS.len(), 47);
    assert_eq!(unique.len(), LEGACY_ENTERPRISE_MIGRATION_VERSIONS.len());
    assert!(unique.is_disjoint(&public));
    assert!(unique.contains("m20260815_000061_create_organization_foundation"));
    assert!(unique.contains("m20260907_000138_create_organization_provisioning_materials"));
    assert!(!unique.contains("m20260831_000123_create_refund_reconciliations"));
    assert!(!unique.contains("m20260903_000126_add_manual_refund_completion"));
}

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

    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let backend = pool.connection().get_database_backend();
    let sql = match backend {
        DbBackend::MySql => {
            "SELECT `invitation_invitee_quota` FROM `authentication_settings` WHERE `id` = 1"
        }
        _ => "SELECT invitation_invitee_quota FROM authentication_settings WHERE id = 1",
    };
    let row = pool
        .connection()
        .query_one(Statement::from_string(backend, sql.to_owned()))
        .await?
        .ok_or("authentication settings must survive the upgrade")?;
    assert_eq!(row.try_get::<i64>("", "invitation_invitee_quota")?, 0);
    pool.close().await?;
    Ok(())
}
