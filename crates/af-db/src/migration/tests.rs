use std::env;
use std::error::Error;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::entity::prelude::{Json, TimeDateTimeWithTimeZone, TimeTime};
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend,
    EntityTrait, IdenStatic, IntoActiveModel, Iterable, PaginatorTrait, Set, Statement,
};
use sea_orm_migration::prelude::*;
use url::Url;

use super::*;
use crate::entity::{
    ChannelBaseUrl, EncryptedJson, HeaderOverrides, PasswordHash, SensitiveJson, SensitiveString,
    TokenHash, abilities, channel_groups, channel_models, channels, credentials, groups, options,
    tokens, users,
};
use crate::{DatabaseOptions, PoolOptions, connect};

const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
const REQUIRE_LIVE_DATABASE: &str = "AF_REQUIRE_LIVE_DATABASE";
const ALLOW_DESTRUCTIVE_MIGRATION_TEST: &str = "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST";
const ISOLATED_DATABASE_PREFIX: &str = "anyflows_migration_test_";
const TEST_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";
const TOKEN_HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TOKEN_HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TOKEN_HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const TOKEN_HASH_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const CORE_TABLES: [&str; 9] = [
    "groups",
    "users",
    "channels",
    "credentials",
    "tokens",
    "channel_models",
    "channel_groups",
    "abilities",
    "options",
];

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
async fn standard_startup_installs_core_schema() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    assert!(manager.has_table("seaql_migrations").await?);
    assert!(manager.has_table("users").await?);
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
async fn generated_active_models_redact_sensitive_payloads() {
    let credential = credential_active_model(1, "credential-ciphertext");
    let user = user_active_model(1, "user", "user@example.com", "invite-a");
    let token = token_active_model(1, 1, TOKEN_HASH_A, "token-a");
    let option = option_active_model("site.name", "option-value");
    let mut channel = channel_active_model("debug-channel");
    channel.base_url = Set(Some(
        ChannelBaseUrl::parse("https://api.example.com/channel-secret").unwrap(),
    ));
    channel.header_override = Set(HeaderOverrides::validate(Json::Object(
        [(
            "x-trace".to_owned(),
            Json::String("channel-secret".to_owned()),
        )]
        .into_iter()
        .collect(),
    ))
    .unwrap());

    for (rendered, secret) in [
        (format!("{credential:?}"), "credential-ciphertext"),
        (format!("{user:?}"), TEST_PASSWORD_HASH),
        (format!("{token:?}"), TOKEN_HASH_A),
        (format!("{option:?}"), "option-value"),
        (format!("{channel:?}"), "channel-secret"),
    ] {
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(secret));
    }
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库迁移流水线执行"]
async fn migration_runner_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        max_connections: 1,
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    assert_database_session(pool.connection()).await?;

    run_with::<Migrator>(&pool, MigrationOptions::default()).await?;
    run_with::<Migrator>(&pool, MigrationOptions::default()).await?;

    assert_core_schema(pool.connection()).await?;
    assert!(
        Migrator::get_pending_migrations(pool.connection())
            .await?
            .is_empty()
    );
    assert_eq!(
        Migrator::get_applied_migrations(pool.connection())
            .await?
            .len(),
        CORE_TABLES.len()
    );

    assert_updated_at_behaviors(pool.connection()).await?;
    exercise_core_schema(pool.connection()).await?;

    Migrator::down(pool.connection(), None).await?;
    assert_core_tables_absent(pool.connection()).await?;

    // 回滚后的完整重建可发现 down 顺序或残留索引导致的二次部署问题。
    Migrator::up(pool.connection(), None).await?;
    assert_core_schema(pool.connection()).await?;
    Migrator::down(pool.connection(), None).await?;
    assert_core_tables_absent(pool.connection()).await?;

    pool.close().await?;
    Ok(())
}

#[test]
fn destructive_migration_test_requires_gate_and_isolated_target() {
    assert!(validate_destructive_test_target("sqlite::memory:", None).is_err());
    assert!(validate_destructive_test_target("postgres://localhost/anyflows", Some("1")).is_err());
    assert!(
        validate_destructive_test_target(
            "postgres://localhost/anyflows_migration_test_ci?dbname=anyflows",
            Some("1"),
        )
        .is_err()
    );
    assert!(
        validate_destructive_test_target(
            "postgres://localhost/anyflows_migration_test_ci?options=-csearch_path%3Dpublic",
            Some("1"),
        )
        .is_err()
    );
    assert!(
        validate_destructive_test_target(
            "mysql://localhost/anyflows_migration_test_ci?timezone=%2B08:00",
            Some("1"),
        )
        .is_ok()
    );
    assert!(
        validate_destructive_test_target(
            "mysql://localhost/anyflows_migration_test_ci?database=anyflows",
            Some("1"),
        )
        .is_err()
    );
    assert!(validate_destructive_test_target("sqlite::memory:", Some("1")).is_ok());
}

/// 完整回滚只允许显式门禁下的内存库或约定前缀测试库。
fn validate_destructive_test_target(database_url: &str, gate: Option<&str>) -> Result<(), DbErr> {
    if gate != Some("1") {
        return Err(DbErr::Custom(
            "完整迁移回滚缺少显式破坏性测试门禁".to_owned(),
        ));
    }
    if database_url == "sqlite::memory:" {
        return Ok(());
    }

    let parsed = Url::parse(database_url)
        .map_err(|_| DbErr::Custom("迁移冒烟测试数据库地址无效".to_owned()))?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql" | "mysql") {
        return Err(DbErr::Custom(
            "完整迁移回滚仅支持隔离的 PostgreSQL、MySQL 或内存 SQLite".to_owned(),
        ));
    }
    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| DbErr::Custom("迁移冒烟测试数据库名称无效".to_owned()))?;
    let database_name = segments
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| DbErr::Custom("迁移冒烟测试数据库名称无效".to_owned()))?;
    let isolated_name = database_name.starts_with(ISOLATED_DATABASE_PREFIX)
        && database_name.len() > ISOLATED_DATABASE_PREFIX.len()
        && database_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && segments.next().is_none();
    if !isolated_name {
        return Err(DbErr::Custom(
            "完整迁移回滚目标必须使用专用测试数据库名称".to_owned(),
        ));
    }
    let query_is_safe = match parsed.scheme() {
        "postgres" | "postgresql" => parsed.query().is_none(),
        "mysql" => {
            let mut pairs = parsed.query_pairs();
            pairs
                .next()
                .is_none_or(|(name, _)| name == "timezone" && pairs.next().is_none())
        }
        _ => false,
    };
    if !query_is_safe {
        return Err(DbErr::Custom(
            "完整迁移回滚目标禁止使用数据库重定向参数".to_owned(),
        ));
    }
    Ok(())
}

async fn assert_database_session(connection: &DatabaseConnection) -> Result<(), DbErr> {
    if connection.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }

    let row = connection
        .query_one(Statement::from_string(
            DbBackend::MySql,
            "SELECT @@session.time_zone AS session_time_zone",
        ))
        .await?
        .expect("MySQL 会话时区查询必须返回一行");
    let timezone: String = row.try_get("", "session_time_zone")?;
    assert_eq!(timezone, "+00:00");
    Ok(())
}

async fn assert_core_schema(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let manager = SchemaManager::new(connection);
    for table in CORE_TABLES {
        assert!(manager.has_table(table).await?, "缺少数据表 {table}");
    }

    assert_entity_columns::<groups::Column>(&manager, "groups").await?;
    assert_entity_columns::<users::Column>(&manager, "users").await?;
    assert_entity_columns::<channels::Column>(&manager, "channels").await?;
    assert_entity_columns::<credentials::Column>(&manager, "credentials").await?;
    assert_entity_columns::<tokens::Column>(&manager, "tokens").await?;
    assert_entity_columns::<channel_models::Column>(&manager, "channel_models").await?;
    assert_entity_columns::<channel_groups::Column>(&manager, "channel_groups").await?;
    assert_entity_columns::<abilities::Column>(&manager, "abilities").await?;
    assert_entity_columns::<options::Column>(&manager, "options").await?;

    assert!(manager.has_column("tokens", "key_hash").await?);
    assert!(manager.has_column("tokens", "key_prefix").await?);
    assert!(!manager.has_column("tokens", "key").await?);

    for (table, index) in [
        ("users", "uq_users_active_username"),
        ("users", "uq_users_active_email"),
        ("users", "uq_users_aff_code"),
        ("groups", "uq_groups_active_name"),
        ("tokens", "uq_tokens_key_hash"),
        ("credentials", "idx_credentials_channel_status_schedulable"),
        ("abilities", "idx_abilities_group_model_enabled"),
    ] {
        assert!(
            manager.has_index(table, index).await?,
            "数据表 {table} 缺少索引 {index}"
        );
    }
    Ok(())
}

async fn assert_entity_columns<C>(manager: &SchemaManager<'_>, table: &str) -> Result<(), DbErr>
where
    C: Iterable + IdenStatic,
{
    for column in C::iter() {
        assert!(
            manager.has_column(table, column.as_str()).await?,
            "数据表 {table} 缺少实体字段 {}",
            column.as_str()
        );
    }
    Ok(())
}

async fn assert_core_tables_absent(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let manager = SchemaManager::new(connection);
    for table in CORE_TABLES {
        assert!(
            !manager.has_table(table).await?,
            "回滚后仍存在数据表 {table}"
        );
    }
    Ok(())
}

async fn assert_updated_at_behaviors(connection: &DatabaseConnection) -> Result<(), DbErr> {
    macro_rules! assert_behavior {
        ($active_model:path) => {{
            let insert_model = <$active_model as ActiveModelBehavior>::before_save(
                <$active_model as Default>::default(),
                connection,
                true,
            )
            .await?;
            assert!(insert_model.updated_at.is_not_set());

            let update_model = <$active_model as ActiveModelBehavior>::before_save(
                <$active_model as Default>::default(),
                connection,
                false,
            )
            .await?;
            assert!(update_model.updated_at.is_set());
        }};
    }

    assert_behavior!(abilities::ActiveModel);
    assert_behavior!(channel_groups::ActiveModel);
    assert_behavior!(channel_models::ActiveModel);
    assert_behavior!(channels::ActiveModel);
    assert_behavior!(credentials::ActiveModel);
    assert_behavior!(groups::ActiveModel);
    assert_behavior!(options::ActiveModel);
    assert_behavior!(tokens::ActiveModel);
    assert_behavior!(users::ActiveModel);
    Ok(())
}

async fn exercise_core_schema(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let group = group_active_model("default", "默认分组")
        .insert(connection)
        .await?;
    assert_eq!(group.ratio_micros, 1_000_000);
    assert!(
        group_active_model("default", "重复分组")
            .insert(connection)
            .await
            .is_err()
    );

    let user = user_active_model(group.id, "alice", "alice@example.com", "invite-a")
        .insert(connection)
        .await?;
    assert_eq!(user.status, 2);
    assert_debug_redacted(&user, "password-hash");
    assert!(
        user_active_model(
            group.id,
            "alice",
            "alice-other@example.com",
            "invite-duplicate-username",
        )
        .insert(connection)
        .await
        .is_err()
    );
    assert!(
        user_active_model(
            group.id,
            "alice-other",
            "alice@example.com",
            "invite-duplicate-email",
        )
        .insert(connection)
        .await
        .is_err()
    );

    let channel = channel_active_model("primary").insert(connection).await?;
    assert_eq!(channel.status, 2);
    assert_debug_redacted(&channel, "https://api.example.com");
    let credential = credential_active_model(channel.id, "credential-ciphertext")
        .insert(connection)
        .await?;
    assert_eq!(credential.status, 2);
    assert!(!credential.schedulable);
    assert_debug_redacted(&credential, "credential-ciphertext");

    let mut child_credential = credential_active_model(channel.id, "child-ciphertext");
    child_credential.parent_id = Set(Some(credential.id));
    child_credential.insert(connection).await?;

    let mut self_parent = credential.clone().into_active_model();
    self_parent.parent_id = Set(Some(credential.id));
    assert!(self_parent.update(connection).await.is_err());

    let direct_parent = credential_active_model(channel.id, "direct-parent")
        .insert(connection)
        .await?;
    let mut direct_child = credential_active_model(channel.id, "direct-child");
    direct_child.parent_id = Set(Some(direct_parent.id));
    let direct_child = direct_child.insert(connection).await?;
    credentials::Entity::delete_by_id(direct_parent.id)
        .exec(connection)
        .await?;
    assert!(
        credentials::Entity::find_by_id(direct_child.id)
            .one(connection)
            .await?
            .is_none()
    );
    let token = token_active_model(user.id, group.id, TOKEN_HASH_A, "token-a")
        .insert(connection)
        .await?;
    assert_eq!(token.status, 2);
    assert_eq!(token.expired_at, Some(future_expiration()));
    assert_debug_redacted(&token, TOKEN_HASH_A);

    channel_models::ActiveModel {
        channel_id: Set(channel.id),
        model: Set("gpt-5".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channel_groups::ActiveModel {
        channel_id: Set(channel.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let option = option_active_model("site.name", "AnyFlows")
        .insert(connection)
        .await?;
    assert_debug_redacted(&option, "AnyFlows");
    let old_audit_time = historical_audit_time();
    let mut updated_option = option.into_active_model();
    updated_option.value = Set(SensitiveString::from("AnyFlows updated"));
    updated_option.updated_at = Set(old_audit_time);
    let updated_option = updated_option.update(connection).await?;
    assert!(updated_option.updated_at > old_audit_time);
    assert_persistence_constraints(
        connection,
        group.id,
        user.id,
        channel.id,
        credential.id,
        token.id,
    )
    .await?;
    assert_foreign_key_actions(connection, group.id).await?;

    let cascade_channel = channel_active_model("cascade").insert(connection).await?;
    let mut cross_channel_child =
        credential_active_model(cascade_channel.id, "cross-channel-child");
    cross_channel_child.parent_id = Set(Some(credential.id));
    assert!(cross_channel_child.insert(connection).await.is_err());
    let raw_cross_channel_child =
        credential_active_model(cascade_channel.id, "raw-cross-channel-child")
            .insert(connection)
            .await?;
    assert!(
        execute_raw_update(
            connection,
            Query::update()
                .table(credentials::Entity)
                .value(credentials::Column::ParentId, credential.id)
                .and_where(Expr::col(credentials::Column::Id).eq(raw_cross_channel_child.id))
                .to_owned(),
        )
        .await
        .is_err()
    );
    credentials::Entity::delete_by_id(raw_cross_channel_child.id)
        .exec(connection)
        .await?;
    let cascade_parent = credential_active_model(cascade_channel.id, "cascade-ciphertext")
        .insert(connection)
        .await?;
    let mut cascade_child = credential_active_model(cascade_channel.id, "cascade-child");
    cascade_child.parent_id = Set(Some(cascade_parent.id));
    cascade_child.insert(connection).await?;
    channel_models::ActiveModel {
        channel_id: Set(cascade_channel.id),
        model: Set("gpt-5".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channel_groups::ActiveModel {
        channel_id: Set(cascade_channel.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(cascade_channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channels::Entity::delete_by_id(cascade_channel.id)
        .exec(connection)
        .await?;

    assert_eq!(credentials::Entity::find().count(connection).await?, 2);
    assert_eq!(abilities::Entity::find().count(connection).await?, 1);
    assert_eq!(channel_models::Entity::find().count(connection).await?, 1);
    assert_eq!(channel_groups::Entity::find().count(connection).await?, 1);
    assert_eq!(options::Entity::find().count(connection).await?, 1);

    assert!(
        channel_models::ActiveModel {
            channel_id: Set(channel.id),
            model: Set("gpt-5".to_owned()),
            ..Default::default()
        }
        .insert(connection)
        .await
        .is_err()
    );
    channel_models::Entity::delete_by_id((channel.id, "gpt-5".to_owned()))
        .exec(connection)
        .await?;
    assert_eq!(abilities::Entity::find().count(connection).await?, 0);
    channel_models::ActiveModel {
        channel_id: Set(channel.id),
        model: Set("gpt-5".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channel_groups::Entity::delete_by_id((channel.id, group.id))
        .exec(connection)
        .await?;
    assert_eq!(abilities::Entity::find().count(connection).await?, 0);
    channel_groups::ActiveModel {
        channel_id: Set(channel.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    assert!(
        token_active_model(user.id, i64::MAX, TOKEN_HASH_B, "invalid-group")
            .insert(connection)
            .await
            .is_err()
    );
    assert!(
        token_active_model(user.id, group.id, TOKEN_HASH_A, "duplicate-hash")
            .insert(connection)
            .await
            .is_err()
    );

    let mut deleted_token = token.into_active_model();
    deleted_token.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_token.update(connection).await?;
    assert!(
        token_active_model(user.id, group.id, TOKEN_HASH_A, "reused-hash")
            .insert(connection)
            .await
            .is_err()
    );
    assert!(
        groups::Entity::delete_by_id(group.id)
            .exec(connection)
            .await
            .is_err()
    );

    let reusable_group = group_active_model("reusable", "可复用分组")
        .insert(connection)
        .await?;
    let mut deleted_group = reusable_group.into_active_model();
    deleted_group.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_group.update(connection).await?;
    group_active_model("reusable", "新可复用分组")
        .insert(connection)
        .await?;

    let reusable_user = user_active_model(
        group.id,
        "reusable-user",
        "reusable@example.com",
        "invite-reusable",
    )
    .insert(connection)
    .await?;
    let mut deleted_user = reusable_user.into_active_model();
    deleted_user.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_user.update(connection).await?;
    assert!(
        user_active_model(group.id, "bob", "bob@example.com", "invite-reusable")
            .insert(connection)
            .await
            .is_err()
    );
    user_active_model(
        group.id,
        "reusable-user",
        "reusable@example.com",
        "invite-replacement",
    )
    .insert(connection)
    .await?;

    assert_invalid_sensitive_rows_fail_closed(connection, group.id).await?;

    Ok(())
}

async fn assert_persistence_constraints(
    connection: &DatabaseConnection,
    group_id: i64,
    user_id: i64,
    channel_id: i64,
    credential_id: i64,
    token_id: i64,
) -> Result<(), DbErr> {
    let mut invalid_ratio = group_active_model("invalid-ratio", "非法倍率");
    invalid_ratio.ratio_micros = Set(-1);
    assert!(invalid_ratio.insert(connection).await.is_err());

    let mut invalid_limit = group_active_model("invalid-limit", "非法限额");
    invalid_limit.daily_limit = Set(Some(-1));
    assert!(invalid_limit.insert(connection).await.is_err());

    assert!(
        execute_raw_update(
            connection,
            Query::update()
                .table(tokens::Entity)
                .value(tokens::Column::KeyHash, "C".repeat(64))
                .and_where(Expr::col(tokens::Column::Id).eq(token_id))
                .to_owned(),
        )
        .await
        .is_err()
    );

    let mut incomplete_peak = group_active_model("incomplete-peak", "不完整高峰窗口");
    incomplete_peak.peak_start = Set(Some(fixed_peak_time()));
    assert!(incomplete_peak.insert(connection).await.is_err());

    let mut ratio_without_peak = group_active_model("ratio-without-peak", "缺少高峰窗口");
    ratio_without_peak.peak_ratio_micros = Set(Some(2_000_000));
    assert!(ratio_without_peak.insert(connection).await.is_err());

    let mut peak_without_ratio = group_active_model("peak-without-ratio", "缺少高峰倍率");
    peak_without_ratio.peak_start = Set(Some(fixed_peak_time()));
    peak_without_ratio.peak_end = Set(Some(fixed_peak_end_time()));
    assert!(peak_without_ratio.insert(connection).await.is_err());

    let self_fallback = group_active_model("self-fallback", "自引用分组")
        .insert(connection)
        .await?;
    let mut self_fallback_update = self_fallback.clone().into_active_model();
    self_fallback_update.fallback_group_id = Set(Some(self_fallback.id));
    assert!(self_fallback_update.update(connection).await.is_err());
    if connection.get_database_backend() != DbBackend::MySql {
        assert!(
            execute_raw_update(
                connection,
                Query::update()
                    .table(groups::Entity)
                    .value(groups::Column::FallbackGroupId, self_fallback.id)
                    .and_where(Expr::col(groups::Column::Id).eq(self_fallback.id))
                    .to_owned(),
            )
            .await
            .is_err()
        );
    }
    groups::Entity::delete_by_id(self_fallback.id)
        .exec(connection)
        .await?;

    let mut invalid_role = user_active_model(
        group_id,
        "invalid-role",
        "invalid-role@example.com",
        "invite-invalid-role",
    );
    invalid_role.role = Set(2);
    assert!(invalid_role.insert(connection).await.is_err());

    let mut invalid_status = user_active_model(
        group_id,
        "invalid-status",
        "invalid-status@example.com",
        "invite-invalid-status",
    );
    invalid_status.status = Set(0);
    assert!(invalid_status.insert(connection).await.is_err());

    let mut invalid_concurrency = user_active_model(
        group_id,
        "invalid-concurrency",
        "invalid-concurrency@example.com",
        "invite-invalid-concurrency",
    );
    invalid_concurrency.concurrency = Set(Some(-1));
    assert!(invalid_concurrency.insert(connection).await.is_err());

    let self_inviter = user_active_model(
        group_id,
        "self-inviter",
        "self-inviter@example.com",
        "invite-self-inviter",
    )
    .insert(connection)
    .await?;
    let mut self_inviter_update = self_inviter.clone().into_active_model();
    self_inviter_update.inviter_id = Set(Some(self_inviter.id));
    assert!(self_inviter_update.update(connection).await.is_err());
    if connection.get_database_backend() != DbBackend::MySql {
        assert!(
            execute_raw_update(
                connection,
                Query::update()
                    .table(users::Entity)
                    .value(users::Column::InviterId, self_inviter.id)
                    .and_where(Expr::col(users::Column::Id).eq(self_inviter.id))
                    .to_owned(),
            )
            .await
            .is_err()
        );
    }
    users::Entity::delete_by_id(self_inviter.id)
        .exec(connection)
        .await?;

    let mut invalid_token_status =
        token_active_model(user_id, group_id, TOKEN_HASH_B, "invalid-token-status");
    invalid_token_status.status = Set(0);
    assert!(invalid_token_status.insert(connection).await.is_err());

    let mut invalid_token_quota =
        token_active_model(user_id, group_id, TOKEN_HASH_B, "invalid-token-quota");
    invalid_token_quota.remain_quota = Set(-1);
    assert!(invalid_token_quota.insert(connection).await.is_err());

    let mut invalid_channel_status = channel_active_model("invalid-channel-status");
    invalid_channel_status.status = Set(0);
    assert!(invalid_channel_status.insert(connection).await.is_err());

    let mut invalid_channel_weight = channel_active_model("invalid-channel-weight");
    invalid_channel_weight.weight = Set(-1);
    assert!(invalid_channel_weight.insert(connection).await.is_err());

    let mut invalid_credential_status = credential_active_model(channel_id, "invalid-status");
    invalid_credential_status.status = Set(0);
    assert!(invalid_credential_status.insert(connection).await.is_err());

    let mut invalid_credential_weight = credential_active_model(channel_id, "invalid-weight");
    invalid_credential_weight.weight = Set(-1);
    assert!(invalid_credential_weight.insert(connection).await.is_err());

    let mut invalid_credential_concurrency =
        credential_active_model(channel_id, "invalid-concurrency");
    invalid_credential_concurrency.concurrency = Set(Some(-1));
    assert!(
        invalid_credential_concurrency
            .insert(connection)
            .await
            .is_err()
    );

    let mut invalid_load_factor = credential_active_model(channel_id, "invalid-load-factor");
    invalid_load_factor.load_factor_micros = Set(Some(-1));
    assert!(invalid_load_factor.insert(connection).await.is_err());

    let mut invalid_rate_multiplier =
        credential_active_model(channel_id, "invalid-rate-multiplier");
    invalid_rate_multiplier.rate_multiplier_micros = Set(Some(-1));
    assert!(invalid_rate_multiplier.insert(connection).await.is_err());

    let mut invalid_quota_dimension =
        credential_active_model(channel_id, "invalid-quota-dimension");
    invalid_quota_dimension.quota_dimension = Set("unknown".to_owned());
    assert!(invalid_quota_dimension.insert(connection).await.is_err());

    let mut missing_parent = credential_active_model(channel_id, "missing-parent");
    missing_parent.parent_id = Set(Some(i64::MAX));
    assert!(missing_parent.insert(connection).await.is_err());

    if connection.get_database_backend() != DbBackend::MySql {
        assert!(
            execute_raw_update(
                connection,
                Query::update()
                    .table(credentials::Entity)
                    .value(credentials::Column::ParentId, credential_id)
                    .and_where(Expr::col(credentials::Column::Id).eq(credential_id))
                    .to_owned(),
            )
            .await
            .is_err()
        );
    }

    let mut incomplete_session_window =
        credential_active_model(channel_id, "incomplete-session-window");
    incomplete_session_window.session_window_start = Set(Some(historical_audit_time()));
    assert!(incomplete_session_window.insert(connection).await.is_err());

    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set("negative-weight-model".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let invalid_ability = abilities::ActiveModel {
        group_id: Set(group_id),
        model: Set("negative-weight-model".to_owned()),
        channel_id: Set(channel_id),
        weight: Set(-1),
        ..Default::default()
    };
    assert!(invalid_ability.insert(connection).await.is_err());
    channel_models::Entity::delete_by_id((channel_id, "negative-weight-model".to_owned()))
        .exec(connection)
        .await?;

    Ok(())
}

async fn assert_invalid_sensitive_rows_fail_closed(
    connection: &DatabaseConnection,
    group_id: i64,
) -> Result<(), DbErr> {
    let raw_user = user_active_model(
        group_id,
        "raw-sensitive-user",
        "raw-sensitive@example.com",
        "invite-raw-sensitive",
    )
    .insert(connection)
    .await?;
    let raw_token = token_active_model(raw_user.id, group_id, TOKEN_HASH_B, "raw-sensitive-token")
        .insert(connection)
        .await?;
    let invalid_password_hash =
        "$argon2id$v=19$m=19456,t=2,p=1,x=raw-password-secret$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";
    execute_raw_update(
        connection,
        Query::update()
            .table(users::Entity)
            .value(users::Column::PasswordHash, invalid_password_hash)
            .and_where(Expr::col(users::Column::Id).eq(raw_user.id))
            .to_owned(),
    )
    .await?;
    let error = users::Entity::find_by_id(raw_user.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-password-secret");

    let invalid_hash = "z".repeat(64);
    execute_raw_update(
        connection,
        Query::update()
            .table(tokens::Entity)
            .value(tokens::Column::KeyHash, invalid_hash.clone())
            .and_where(Expr::col(tokens::Column::Id).eq(raw_token.id))
            .to_owned(),
    )
    .await?;
    let error = tokens::Entity::find_by_id(raw_token.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, &invalid_hash);

    let raw_url_channel = channel_active_model("raw-url-channel")
        .insert(connection)
        .await?;
    let invalid_url = "https://user:raw-url-secret@example.com";
    execute_raw_update(
        connection,
        Query::update()
            .table(channels::Entity)
            .value(channels::Column::BaseUrl, invalid_url)
            .and_where(Expr::col(channels::Column::Id).eq(raw_url_channel.id))
            .to_owned(),
    )
    .await?;
    let error = channels::Entity::find_by_id(raw_url_channel.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-url-secret");

    let raw_header_channel = channel_active_model("raw-header-channel")
        .insert(connection)
        .await?;
    let invalid_headers = Json::Object(
        [("x-raw-header-secret".to_owned(), Json::from(1))]
            .into_iter()
            .collect(),
    );
    execute_raw_update(
        connection,
        Query::update()
            .table(channels::Entity)
            .value(channels::Column::HeaderOverride, invalid_headers)
            .and_where(Expr::col(channels::Column::Id).eq(raw_header_channel.id))
            .to_owned(),
    )
    .await?;
    let error = channels::Entity::find_by_id(raw_header_channel.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-header-secret");

    let raw_secret_channel = channel_active_model("raw-secret-channel")
        .insert(connection)
        .await?;
    let raw_credential = credential_active_model(raw_secret_channel.id, "valid-envelope")
        .insert(connection)
        .await?;
    let invalid_envelope = Json::Object(
        [(
            "ciphertext".to_owned(),
            Json::String("raw-envelope-secret".to_owned()),
        )]
        .into_iter()
        .collect(),
    );
    execute_raw_update(
        connection,
        Query::update()
            .table(credentials::Entity)
            .value(credentials::Column::Secret, invalid_envelope)
            .and_where(Expr::col(credentials::Column::Id).eq(raw_credential.id))
            .to_owned(),
    )
    .await?;
    let error = credentials::Entity::find_by_id(raw_credential.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-envelope-secret");

    Ok(())
}

async fn assert_foreign_key_actions(
    connection: &DatabaseConnection,
    default_group_id: i64,
) -> Result<(), DbErr> {
    let token_group = group_active_model("token-restrict", "令牌限制分组")
        .insert(connection)
        .await?;
    let token_user = user_active_model(
        default_group_id,
        "token-restrict-user",
        "token-restrict@example.com",
        "invite-token-restrict",
    )
    .insert(connection)
    .await?;
    let restricted_token = token_active_model(
        token_user.id,
        token_group.id,
        TOKEN_HASH_C,
        "restricted-token",
    )
    .insert(connection)
    .await?;
    assert!(
        groups::Entity::delete_by_id(token_group.id)
            .exec(connection)
            .await
            .is_err()
    );
    tokens::Entity::delete_by_id(restricted_token.id)
        .exec(connection)
        .await?;
    groups::Entity::delete_by_id(token_group.id)
        .exec(connection)
        .await?;
    users::Entity::delete_by_id(token_user.id)
        .exec(connection)
        .await?;

    let user_group = group_active_model("user-restrict", "用户限制分组")
        .insert(connection)
        .await?;
    let restricted_user = user_active_model(
        user_group.id,
        "group-restrict-user",
        "group-restrict@example.com",
        "invite-group-restrict",
    )
    .insert(connection)
    .await?;
    assert!(
        groups::Entity::delete_by_id(user_group.id)
            .exec(connection)
            .await
            .is_err()
    );
    users::Entity::delete_by_id(restricted_user.id)
        .exec(connection)
        .await?;
    groups::Entity::delete_by_id(user_group.id)
        .exec(connection)
        .await?;

    let fallback_target = group_active_model("fallback-target", "降级目标")
        .insert(connection)
        .await?;
    let mut fallback_source = group_active_model("fallback-source", "降级来源");
    fallback_source.fallback_group_id = Set(Some(fallback_target.id));
    let fallback_source = fallback_source.insert(connection).await?;
    groups::Entity::delete_by_id(fallback_target.id)
        .exec(connection)
        .await?;
    assert_eq!(
        groups::Entity::find_by_id(fallback_source.id)
            .one(connection)
            .await?
            .expect("降级来源分组必须存在")
            .fallback_group_id,
        None
    );
    groups::Entity::delete_by_id(fallback_source.id)
        .exec(connection)
        .await?;

    let inviter = user_active_model(
        default_group_id,
        "set-null-inviter",
        "set-null-inviter@example.com",
        "invite-set-null-inviter",
    )
    .insert(connection)
    .await?;
    let mut invitee = user_active_model(
        default_group_id,
        "set-null-invitee",
        "set-null-invitee@example.com",
        "invite-set-null-invitee",
    );
    invitee.inviter_id = Set(Some(inviter.id));
    let invitee = invitee.insert(connection).await?;
    users::Entity::delete_by_id(inviter.id)
        .exec(connection)
        .await?;
    assert_eq!(
        users::Entity::find_by_id(invitee.id)
            .one(connection)
            .await?
            .expect("被邀请用户必须存在")
            .inviter_id,
        None
    );
    users::Entity::delete_by_id(invitee.id)
        .exec(connection)
        .await?;

    let cascade_user = user_active_model(
        default_group_id,
        "cascade-token-user",
        "cascade-token@example.com",
        "invite-cascade-token",
    )
    .insert(connection)
    .await?;
    let cascade_token = token_active_model(
        cascade_user.id,
        default_group_id,
        TOKEN_HASH_D,
        "cascade-token",
    )
    .insert(connection)
    .await?;
    users::Entity::delete_by_id(cascade_user.id)
        .exec(connection)
        .await?;
    assert!(
        tokens::Entity::find_by_id(cascade_token.id)
            .one(connection)
            .await?
            .is_none()
    );

    Ok(())
}

async fn execute_raw_update(
    connection: &DatabaseConnection,
    statement: UpdateStatement,
) -> Result<(), DbErr> {
    connection
        .execute(connection.get_database_backend().build(&statement))
        .await?;
    Ok(())
}

fn assert_redacted_type_error(error: &DbErr, secret: &str) {
    assert!(matches!(error, DbErr::TryIntoErr { .. }));
    assert!(!error.to_string().contains(secret));
    assert!(!format!("{error:?}").contains(secret));
}

fn group_active_model(name: &str, display_name: &str) -> groups::ActiveModel {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(display_name.to_owned()),
        flags: Set(empty_json_object()),
        ..Default::default()
    }
}

fn user_active_model(
    group_id: i64,
    username: &str,
    email: &str,
    aff_code: &str,
) -> users::ActiveModel {
    users::ActiveModel {
        username: Set(username.to_owned()),
        email: Set(Some(email.to_owned())),
        password_hash: Set(Some(PasswordHash::parse(TEST_PASSWORD_HASH).unwrap())),
        default_group_id: Set(group_id),
        aff_code: Set(aff_code.to_owned()),
        settings: Set(empty_json_object()),
        ..Default::default()
    }
}

fn channel_active_model(name: &str) -> channels::ActiveModel {
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://api.example.com").unwrap(),
        )),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(empty_json_object()).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

fn credential_active_model(channel_id: i64, marker: &str) -> credentials::ActiveModel {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set("api_key".to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(marker)).unwrap()),
        ..Default::default()
    }
}

fn token_active_model(
    user_id: i64,
    group_id: i64,
    key_hash: &str,
    name: &str,
) -> tokens::ActiveModel {
    tokens::ActiveModel {
        user_id: Set(user_id),
        key_hash: Set(TokenHash::parse(key_hash).unwrap()),
        key_prefix: Set("sk-af-test".to_owned()),
        name: Set(name.to_owned()),
        group_id: Set(Some(group_id)),
        expired_at: Set(Some(future_expiration())),
        ..Default::default()
    }
}

fn option_active_model(key: &str, value: &str) -> options::ActiveModel {
    options::ActiveModel {
        key: Set(key.to_owned()),
        value: Set(SensitiveString::from(value)),
        ..Default::default()
    }
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}

fn encrypted_envelope(marker: &str) -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String(format!("test-{marker}"))),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0x42; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0xa5; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn future_expiration() -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(2_208_988_800)
        .expect("固定的 2040-01-01 时间戳必须有效")
}

fn historical_audit_time() -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(946_684_800)
        .expect("固定的 2000-01-01 时间戳必须有效")
}

fn fixed_peak_time() -> TimeTime {
    TimeTime::from_hms(9, 0, 0).expect("固定的 09:00:00 时间必须有效")
}

fn fixed_peak_end_time() -> TimeTime {
    TimeTime::from_hms(17, 0, 0).expect("固定的 17:00:00 时间必须有效")
}

fn assert_debug_redacted(value: &impl std::fmt::Debug, secret: &str) {
    let rendered = format!("{value:?}");
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains(secret));
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
