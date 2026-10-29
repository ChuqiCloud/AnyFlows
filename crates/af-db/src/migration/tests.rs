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

    prepare_legacy_async_claim(&pool).await?;

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
    crate::smart_route_runtime_tests::check_admin_route_crud(&pool).await?;
    check_key_group_access_schema(&pool).await?;
    check_async_billing_source_upgrade(&pool).await?;
    pool.close().await?;
    Ok(())
}

async fn prepare_legacy_async_claim(pool: &crate::DatabasePool) -> Result<(), Box<dyn Error>> {
    use super::iden::{async_task_submission_claims, tokens, users};
    use sea_orm::ConnectionTrait;

    let backend = pool.connection().get_database_backend();
    let insert_user = Query::insert()
        .into_table(users::Entity)
        .columns([
            users::Column::Id,
            users::Column::Username,
            users::Column::DefaultGroupId,
            users::Column::AffCode,
            users::Column::Settings,
        ])
        .values_panic([
            9_900_159_i64.into(),
            "async-source-migration".into(),
            1_i64.into(),
            "async-source-migration".into(),
            serde_json::json!({}).into(),
        ])
        .to_owned();
    pool.connection()
        .execute(backend.build(&insert_user))
        .await?;
    let insert_token = Query::insert()
        .into_table(tokens::Entity)
        .columns([
            tokens::Column::Id,
            tokens::Column::UserId,
            tokens::Column::KeyHash,
            tokens::Column::KeyPrefix,
            tokens::Column::Name,
        ])
        .values_panic([
            9_900_159_i64.into(),
            9_900_159_i64.into(),
            "ac".repeat(32).into(),
            "sk-af-legacy-source".into(),
            "async-source-migration".into(),
        ])
        .to_owned();
    pool.connection()
        .execute(backend.build(&insert_token))
        .await?;
    let insert_claim = Query::insert()
        .into_table(async_task_submission_claims::Entity)
        .columns([
            async_task_submission_claims::Column::TaskKey,
            async_task_submission_claims::Column::UserId,
            async_task_submission_claims::Column::TokenId,
            async_task_submission_claims::Column::GroupId,
            async_task_submission_claims::Column::IdempotencyKey,
            async_task_submission_claims::Column::Protocol,
            async_task_submission_claims::Column::RequestedModel,
            async_task_submission_claims::Column::RequestFingerprint,
        ])
        .values_panic([
            "a1".repeat(16).into(),
            9_900_159_i64.into(),
            9_900_159_i64.into(),
            1_i64.into(),
            "b1".repeat(16).into(),
            "xai_video".into(),
            "grok-video".into(),
            "c1".repeat(32).into(),
        ])
        .to_owned();
    pool.connection()
        .execute(backend.build(&insert_claim))
        .await?;
    Ok(())
}

async fn check_async_billing_source_upgrade(
    pool: &crate::DatabasePool,
) -> Result<(), Box<dyn Error>> {
    use sea_orm::{ConnectionTrait, Statement};
    let backend = pool.connection().get_database_backend();
    let row = pool.connection().query_one(Statement::from_string(backend, "SELECT group_id, billing_source_group_id FROM async_task_submission_claims WHERE user_id = 9900159".to_owned())).await?.ok_or("legacy async claim must survive upgrade")?;
    assert_eq!(row.try_get::<i64>("", "group_id")?, 1);
    assert_eq!(
        row.try_get::<Option<i64>>("", "billing_source_group_id")?,
        None
    );
    let repository =
        crate::AsyncTaskSubmissionRepository::new(pool.clone(), std::time::Duration::from_secs(5))?;
    let request_id = af_domain::AsyncTaskRequestId::from_persistence_key(&"b1".repeat(16))?;
    let restored = repository
        .find_by_request(af_domain::UserId::new(9_900_159)?, request_id)
        .await?
        .ok_or("legacy async billing source must remain readable")?;
    assert_eq!(
        restored.principal().billing_source_group_id(),
        restored.principal().group_id()
    );
    Ok(())
}

async fn check_key_group_access_schema(pool: &crate::DatabasePool) -> Result<(), Box<dyn Error>> {
    use sea_orm::{ConnectionTrait, Statement, TransactionTrait};

    use super::iden::users;

    let connection = pool.connection();
    let backend = connection.get_database_backend();
    // 旧分组升级后不自动开放；缺少策略行保持仅默认组用户可选。
    let row = connection
        .query_one(Statement::from_string(
            backend,
            "SELECT COUNT(*) AS policy_count FROM group_key_policies".to_owned(),
        ))
        .await?
        .ok_or("key group policy table must exist")?;
    assert_eq!(row.try_get::<i64>("", "policy_count")?, 0);

    let transaction = connection.begin().await?;
    let user_id = 9_900_158_i64;
    let insert_user = Query::insert()
        .into_table(users::Entity)
        .columns([
            users::Column::Id,
            users::Column::Username,
            users::Column::DefaultGroupId,
            users::Column::AffCode,
            users::Column::Settings,
        ])
        .values_panic([
            user_id.into(),
            "key-group-migration".into(),
            1_i64.into(),
            "key-group-migration".into(),
            serde_json::json!({}).into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&insert_user)).await?;
    let insert_policy = Query::insert()
        .into_table(Alias::new("group_key_policies"))
        .columns([Alias::new("group_id"), Alias::new("selection_mode")])
        .values_panic([1_i64.into(), "selected_users".into()])
        .to_owned();
    transaction.execute(backend.build(&insert_policy)).await?;
    let insert_grant = Query::insert()
        .into_table(Alias::new("user_group_grants"))
        .columns([Alias::new("group_id"), Alias::new("user_id")])
        .values_panic([1_i64.into(), user_id.into()])
        .to_owned();
    transaction.execute(backend.build(&insert_grant)).await?;
    let row = transaction
        .query_one(Statement::from_string(
            backend,
            "SELECT COUNT(*) AS grant_count FROM user_group_grants".to_owned(),
        ))
        .await?
        .ok_or("key group grant table must exist")?;
    assert_eq!(row.try_get::<i64>("", "grant_count")?, 1);

    let delete_user = Query::delete()
        .from_table(users::Entity)
        .and_where(Expr::col(users::Column::Id).eq(user_id))
        .to_owned();
    transaction.execute(backend.build(&delete_user)).await?;
    let row = transaction
        .query_one(Statement::from_string(
            backend,
            "SELECT COUNT(*) AS grant_count FROM user_group_grants".to_owned(),
        ))
        .await?
        .ok_or("key group grant count must be readable")?;
    assert_eq!(row.try_get::<i64>("", "grant_count")?, 0);
    transaction.rollback().await?;
    Ok(())
}
