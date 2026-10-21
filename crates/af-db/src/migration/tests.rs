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
    use sea_orm::{ConnectionTrait, DbBackend, Statement, prelude::TimeTime};

    use super::iden::groups;

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

    // 默认分组由业务初始化创建，迁移本身不会插入；在历史结构中显式准备升级数据。
    let backend = pool.connection().get_database_backend();
    let peak_start = TimeTime::from_hms(8, 0, 0)?;
    let peak_end = TimeTime::from_hms(20, 0, 0)?;
    let insert = Query::insert()
        .into_table(groups::Entity)
        .columns([
            groups::Column::Id,
            groups::Column::Name,
            groups::Column::DisplayName,
            groups::Column::RatioMicros,
            groups::Column::PeakRatioMicros,
            groups::Column::PeakStart,
            groups::Column::PeakEnd,
            groups::Column::Flags,
        ])
        .values_panic([
            1_i64.into(),
            "legacy-peak".into(),
            "Legacy peak".into(),
            1_500_000_i64.into(),
            2_000_000_i64.into(),
            peak_start.into(),
            peak_end.into(),
            serde_json::json!({}).into(),
        ])
        .to_owned();
    pool.connection().execute(backend.build(&insert)).await?;

    Migrator::up(pool.connection(), None).await?;
    assert!(
        Migrator::get_pending_migrations(pool.connection())
            .await?
            .is_empty()
    );

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
    let sql = match backend {
        DbBackend::MySql => {
            "SELECT `ratio_micros`, `peak_ratio_micros`, `peak_start`, `peak_end`, `peak_windows` FROM `groups` WHERE `id` = 1"
        }
        _ => {
            "SELECT ratio_micros, peak_ratio_micros, peak_start, peak_end, peak_windows FROM groups WHERE id = 1"
        }
    };
    let row = pool
        .connection()
        .query_one(Statement::from_string(backend, sql.to_owned()))
        .await?
        .ok_or("legacy peak group must survive the upgrade")?;
    assert_eq!(row.try_get::<i64>("", "ratio_micros")?, 1_500_000);
    assert_eq!(
        row.try_get::<Option<i64>>("", "peak_ratio_micros")?,
        Some(2_000_000)
    );
    assert_eq!(
        row.try_get::<Option<TimeTime>>("", "peak_start")?,
        Some(peak_start)
    );
    assert_eq!(
        row.try_get::<Option<TimeTime>>("", "peak_end")?,
        Some(peak_end)
    );
    assert!(
        row.try_get::<Option<serde_json::Value>>("", "peak_windows")?
            .is_none()
    );
    let profiles = crate::ClientProfileRepository::new(pool.clone());
    assert_eq!(
        profiles
            .list(crate::ClientProfileQuery::default())
            .await?
            .items
            .len(),
        6
    );
    for profile in af_domain::built_in_client_profiles() {
        assert_eq!(
            profiles
                .revision(&profile.binding.key, profile.binding.revision)
                .await?,
            profile
        );
    }
    pool.close().await?;
    Ok(())
}
