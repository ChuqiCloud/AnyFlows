use std::env;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use af_domain::{
    BillingReservationId, ChannelId, GatewayPrincipal, GroupId, Quota, QuotaDelta, TokenId,
    TrustedClientIp, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rust_decimal::Decimal;
use sea_orm::entity::prelude::{Json, TimeDateTimeWithTimeZone, TimeTime};
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend,
    EntityTrait, IdenStatic, IntoActiveModel, Iterable, PaginatorTrait, Set, Statement,
};
use sea_orm_migration::prelude::*;
use url::Url;

use super::*;
use crate::entity::{
    BillingBatchFingerprint, BillingBatchWriterKey, BillingReservationKey, ChannelBaseUrl,
    EncryptedJson, HeaderOverrides, PasswordHash, PlaygroundConversationKey, SensitiveDecimal,
    SensitiveJson, SensitiveString, TokenHash, TokenIpAllowlist, abilities,
    auth_challenge_rate_limits, auth_challenges, authentication_settings,
    billing_batch_checkpoints, billing_reservations, channel_groups, channel_models, channels,
    credentials, email_settings, group_model_ratios, groups, model_prices, options,
    playground_conversations, playground_shares, registration_rate_limits, site_settings, tokens,
    usage_logs, users,
};
use crate::{
    AdminDashboardRepository, AuthChallengeConsume, AuthChallengeConsumeOutcome,
    AuthChallengeIssue, AuthChallengeIssueOutcome, AuthChallengePurpose, AuthChallengeRepository,
    BillingBatchRepository, BillingBatchWrite, BillingBatchWriteOutcome, ChannelBillingWrite,
    DatabaseOptions, EmailPasswordUpdate, EmailSettingsRepository, EmailSettingsWriteRecord,
    EmailTlsMode, EncryptedCredentialEnvelope, GroupPricingRepository, ModelPriceBillingMode,
    ModelPriceRepository, PlaygroundConversationRepository, PlaygroundConversationWrite,
    PlaygroundShareRepository, PlaygroundShareWrite, PoolOptions, QuotaMutationOutcome,
    QuotaRepository, QuotaReservationStatus, RegistrationAttemptOutcome,
    RegistrationPolicyWriteRecord, RegistrationRepository, SiteSettingsRepository,
    SiteSettingsWriteRecord, TokenAuthLookup, TokenAuthLookupOutcome, TokenAuthRepository,
    TokenBillingWrite, UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError,
    UsageLogSemantics, UsageLogSource, UsageLogUsage, UsageLogWrite, UsageLogWriteOutcome,
    UserBillingWrite, connect,
};

const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
const REQUIRE_LIVE_DATABASE: &str = "AF_REQUIRE_LIVE_DATABASE";
const ALLOW_DESTRUCTIVE_MIGRATION_TEST: &str = "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST";
/// 当前必须完整应用的迁移数量；列变更不会增加核心表数量。
const EXPECTED_MIGRATION_COUNT: usize = 22;
const ISOLATED_DATABASE_PREFIX: &str = "anyflows_migration_test_";
const TEST_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";
const TOKEN_HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TOKEN_HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TOKEN_HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const TOKEN_HASH_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const TOKEN_HASH_E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const RESERVATION_KEY_A: &str = "11111111111111111111111111111111";
const RESERVATION_KEY_B: &str = "22222222222222222222222222222222";
const RESERVATION_KEY_C: &str = "33333333333333333333333333333333";
const RESERVATION_KEY_D: &str = "44444444444444444444444444444444";
const PLAYGROUND_CONVERSATION_KEY: &str = "55555555555555555555555555555555";
const CORE_TABLES: [&str; 22] = [
    "groups",
    "users",
    "channels",
    "credentials",
    "tokens",
    "channel_models",
    "channel_groups",
    "abilities",
    "options",
    "billing_reservations",
    "billing_batch_checkpoints",
    "model_prices",
    "group_model_ratios",
    "usage_logs",
    "playground_shares",
    "playground_conversations",
    "registration_rate_limits",
    "email_settings",
    "site_settings",
    "authentication_settings",
    "auth_challenges",
    "auth_challenge_rate_limits",
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
async fn site_authentication_migration_moves_legacy_registration_policy()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(19)).await?;
    let group = group_active_model("legacy-registration", "旧注册分组")
        .insert(pool.connection())
        .await?;
    option_active_model(
        "system.registration.policy.v1",
        &serde_json::json!({
            "version": 1,
            "enabled": true,
            "default_group_id": group.id,
            "initial_quota": 250,
            "email_required": true,
            "rate_limit_attempts": 7,
            "rate_limit_window_seconds": 1800
        })
        .to_string(),
    )
    .insert(pool.connection())
    .await?;

    Migrator::up(pool.connection(), None).await?;
    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(pool.connection())
        .await?
        .expect("认证设置固定记录必须存在");
    assert!(settings.password_login_enabled);
    assert!(settings.registration_enabled);
    assert_eq!(settings.registration_default_group_id, Some(group.id));
    assert_eq!(settings.registration_initial_quota, 250);
    assert!(settings.registration_email_required);
    assert_eq!(settings.registration_rate_limit_attempts, 7);
    assert_eq!(settings.registration_rate_limit_window_seconds, 1_800);
    assert!(
        options::Entity::find_by_id("system.registration.policy.v1".to_owned())
            .one(pool.connection())
            .await?
            .is_none()
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
async fn generated_active_models_redact_sensitive_payloads() {
    let credential = credential_active_model(1, "credential-ciphertext");
    let user = user_active_model(1, "user", "user@example.com", "invite-a");
    let mut token = token_active_model(1, 1, TOKEN_HASH_A, "token-a");
    token.allow_ips = Set(Some(
        TokenIpAllowlist::validate(Json::Array(vec![Json::String("192.0.2.0/24".to_owned())]))
            .unwrap(),
    ));
    let option = option_active_model("site.name", "option-value");
    let reservation = reservation_active_model(RESERVATION_KEY_A, 1, 1, 1, 10, 10);
    let checkpoint = billing_batch_checkpoints::ActiveModel {
        writer_key: Set(BillingBatchWriterKey::parse(&"c".repeat(32)).unwrap()),
        last_start_sequence: Set(1),
        last_end_sequence: Set(1),
        last_event_count: Set(1),
        last_fingerprint: Set(BillingBatchFingerprint::parse(&"d".repeat(64)).unwrap()),
        ..Default::default()
    };
    let model_price = model_prices::ActiveModel {
        model: Set(SensitiveString::from("debug-model-secret")),
        billing_mode: Set(1),
        input_price: Set(SensitiveDecimal::from(Decimal::from(123_456_789_i64))),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        version: Set(1),
        ..Default::default()
    };
    let playground_share = playground_shares::ActiveModel {
        owner_user_id: Set(1),
        token_hash: Set(TokenHash::parse(TOKEN_HASH_E).unwrap()),
        snapshot: Set(SensitiveJson::from(serde_json::json!({
            "content": "share-snapshot-secret"
        }))),
        created_at: Set(historical_audit_time()),
        expires_at: Set(future_expiration()),
        revoked_at: Set(None),
        ..Default::default()
    };
    let playground_conversation = playground_conversations::ActiveModel {
        conversation_id: Set(
            PlaygroundConversationKey::parse(PLAYGROUND_CONVERSATION_KEY).unwrap(),
        ),
        owner_user_id: Set(1),
        title: Set(SensitiveString::from("conversation-title-secret")),
        models: Set(SensitiveJson::from(serde_json::json!([
            "conversation-model-secret"
        ]))),
        snapshot: Set(SensitiveJson::from(serde_json::json!({
            "content": "conversation-snapshot-secret"
        }))),
        revision: Set(1),
        created_at: Set(historical_audit_time()),
        updated_at: Set(historical_audit_time()),
    };
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
        (format!("{reservation:?}"), RESERVATION_KEY_A),
        (format!("{checkpoint:?}"), &"c".repeat(32)),
        (format!("{checkpoint:?}"), &"d".repeat(64)),
        (format!("{model_price:?}"), "debug-model-secret"),
        (format!("{model_price:?}"), "123456789"),
        (format!("{playground_share:?}"), TOKEN_HASH_E),
        (format!("{playground_share:?}"), "share-snapshot-secret"),
        (
            format!("{playground_conversation:?}"),
            PLAYGROUND_CONVERSATION_KEY,
        ),
        (
            format!("{playground_conversation:?}"),
            "conversation-title-secret",
        ),
        (
            format!("{playground_conversation:?}"),
            "conversation-model-secret",
        ),
        (
            format!("{playground_conversation:?}"),
            "conversation-snapshot-secret",
        ),
    ] {
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(secret));
    }
    assert!(!format!("{token:?}").contains("192.0.2.0"));
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库迁移流水线执行"]
async fn migration_runner_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let is_sqlite = database_url.starts_with("sqlite:");
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        // SQLite 内存库必须固定单连接；网络数据库使用多连接覆盖真实锁竞争。
        max_connections: if is_sqlite { 1 } else { 4 },
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
        EXPECTED_MIGRATION_COUNT
    );

    assert_updated_at_behaviors(pool.connection()).await?;
    exercise_token_auth_repository(&pool).await?;
    exercise_quota_repository(&pool).await?;
    exercise_billing_batch_repository(&pool).await?;
    exercise_model_price_repository(&pool).await?;
    exercise_usage_log_repository(&pool).await?;
    exercise_playground_share_repository(&pool).await?;
    exercise_playground_conversation_repository(&pool).await?;
    exercise_registration_repository(&pool).await?;
    exercise_auth_challenge_repository(&pool).await?;
    exercise_site_settings_repository(&pool).await?;
    exercise_email_settings_repository(&pool).await?;
    exercise_admin_dashboard_repository(&pool).await?;
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

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库模型价格流水线执行"]
async fn model_price_migration_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let is_sqlite = database_url.starts_with("sqlite:");
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        max_connections: if is_sqlite { 1 } else { 4 },
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    let manager = SchemaManager::new(pool.connection());
    let migration = m20260722_000012_create_model_prices::Migration;

    migration.up(&manager).await?;
    assert!(manager.has_table("model_prices").await?);
    exercise_model_price_repository(&pool).await?;
    migration.down(&manager).await?;
    assert!(!manager.has_table("model_prices").await?);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库分组计费流水线执行"]
async fn group_pricing_migration_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        min_connections: 1,
        max_connections: 4,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    let manager = SchemaManager::new(pool.connection());
    let groups_migration = m20260718_000001_create_groups::Migration;
    let group_pricing_migration = m20260723_000013_create_group_model_ratios::Migration;

    groups_migration.up(&manager).await?;
    group_pricing_migration.up(&manager).await?;
    assert!(manager.has_table("groups").await?);
    assert!(manager.has_table("group_model_ratios").await?);
    exercise_group_pricing_repository(&pool).await?;
    group_pricing_migration.down(&manager).await?;
    assert!(!manager.has_table("group_model_ratios").await?);
    groups_migration.down(&manager).await?;
    assert!(!manager.has_table("groups").await?);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库用量日志流水线执行"]
async fn usage_log_migration_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        min_connections: 1,
        max_connections: 4,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    let manager = SchemaManager::new(pool.connection());
    let groups_migration = m20260718_000001_create_groups::Migration;
    let users_migration = m20260718_000002_create_users::Migration;
    let tokens_migration = m20260718_000005_create_tokens::Migration;
    let usage_migration = m20260723_000014_create_usage_logs::Migration;

    // 仅迁移 usage_logs 的外键依赖，允许本机旧 MySQL 独立验证新表。
    groups_migration.up(&manager).await?;
    users_migration.up(&manager).await?;
    tokens_migration.up(&manager).await?;
    usage_migration.up(&manager).await?;
    assert!(manager.has_table("usage_logs").await?);
    exercise_usage_log_repository(&pool).await?;

    usage_migration.down(&manager).await?;
    tokens_migration.down(&manager).await?;
    users_migration.down(&manager).await?;
    groups_migration.down(&manager).await?;
    assert!(!manager.has_table("usage_logs").await?);

    pool.close().await?;
    Ok(())
}

/// 在三方言隔离数据库中验证高精度价格与显式免费模式读取一致。
async fn exercise_model_price_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    model_prices::ActiveModel {
        model: Set(SensitiveString::from("smoke-priced-model")),
        billing_mode: Set(1),
        input_price: Set(SensitiveDecimal::from(Decimal::new(1, 28))),
        output_price: Set(SensitiveDecimal::from(Decimal::new(25, 1))),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::new(3, 2))),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::new(375, 2))),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::from(6))),
        version: Set(9),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    model_prices::ActiveModel {
        model: Set(SensitiveString::from("smoke-free-model")),
        billing_mode: Set(2),
        input_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        version: Set(4),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let records = ModelPriceRepository::new(pool.clone()).load_all().await?;
    let priced = records
        .iter()
        .find(|record| record.model() == "smoke-priced-model")
        .ok_or("定价模型必须存在")?;
    assert_eq!(priced.billing_mode(), ModelPriceBillingMode::PerToken);
    assert_eq!(
        priced.prices(),
        [
            Decimal::new(1, 28),
            Decimal::new(25, 1),
            Decimal::new(3, 2),
            Decimal::new(375, 2),
            Decimal::from(6),
        ]
    );
    assert_eq!(priced.version(), 9);
    let free = records
        .iter()
        .find(|record| record.model() == "smoke-free-model")
        .ok_or("免费模型必须存在")?;
    assert_eq!(free.billing_mode(), ModelPriceBillingMode::Free);
    assert_eq!(free.prices(), [Decimal::ZERO; 5]);
    Ok(())
}

/// 在三方言实库中验证用量事实的幂等追加与冲突拒绝。
async fn exercise_usage_log_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("usage-smoke-group", "用量日志冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "usage-smoke-user",
        "usage-smoke@example.com",
        "invite-usage-smoke",
    );
    user.status = Set(1);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_C, "usage-smoke-token");
    token.status = Set(1);
    let token = token.insert(pool.connection()).await?;
    let principal = GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    );
    let repository = UsageLogRepository::new(pool.clone());
    let event_id = BillingReservationId::new([0xc1; 16])?;
    let usage = UsageLogUsage::new(10, 2, 3, 4, 5, 1, 0, 0)?;
    let write = UsageLogWrite::new(
        event_id,
        principal,
        UsageLogBillingMode::PerToken,
        usage,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(7)?,
    );

    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Existing
    );
    assert_eq!(
        usage_logs::Entity::find().count(pool.connection()).await?,
        1
    );
    let conflicting_write = UsageLogWrite::new(
        event_id,
        principal,
        UsageLogBillingMode::PerToken,
        usage,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(8)?,
    );
    assert_eq!(
        repository.record(&conflicting_write).await,
        Err(UsageLogRepositoryError::Conflict)
    );

    usage_logs::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言实库中验证分享快照写入、读取和用户删除级联。
async fn exercise_playground_share_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("share-smoke-group", "分享冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut owner = user_active_model(
        group.id,
        "share-smoke-owner",
        "share-smoke@example.com",
        "invite-share-smoke",
    );
    owner.status = Set(1);
    let owner = owner.insert(pool.connection()).await?;
    let owner_user_id = UserId::new(owner.id)?;
    let repository = PlaygroundShareRepository::new(pool.clone(), Duration::from_secs(2))?;
    let expires_at = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(24 * 60 * 60)
        .ok_or_else(|| std::io::Error::other("分享冒烟过期时间溢出"))?;
    repository
        .create(PlaygroundShareWrite::new(
            owner_user_id,
            TOKEN_HASH_E.to_owned(),
            serde_json::json!({
                "version": 1,
                "sessions": [{
                    "model": "smoke-model",
                    "messages": [
                        {"role": "user", "content": "ping"},
                        {"role": "assistant", "content": "pong"}
                    ]
                }]
            }),
            expires_at,
        ))
        .await?;
    assert!(repository.find_active(TOKEN_HASH_E).await?.is_some());

    users::Entity::delete_by_id(owner.id)
        .exec(pool.connection())
        .await?;
    assert_eq!(
        playground_shares::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言实库中验证私有历史写入、列表、版本更新和用户删除级联。
async fn exercise_playground_conversation_repository(
    pool: &DatabasePool,
) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("history-smoke-group", "历史冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut owner = user_active_model(
        group.id,
        "history-smoke-owner",
        "history-smoke@example.com",
        "invite-history-smoke",
    );
    owner.status = Set(1);
    let owner = owner.insert(pool.connection()).await?;
    let owner_user_id = UserId::new(owner.id)?;
    let repository = PlaygroundConversationRepository::new(pool.clone(), Duration::from_secs(2))?;
    let snapshot = serde_json::json!({
        "version": 1,
        "sessions": [{
            "model": "smoke-model",
            "messages": [
                {"role": "user", "content": "ping"},
                {"role": "assistant", "content": "pong"}
            ]
        }]
    });
    repository
        .save(
            owner_user_id,
            PlaygroundConversationWrite::new(
                PLAYGROUND_CONVERSATION_KEY.to_owned(),
                "ping".to_owned(),
                vec!["smoke-model".to_owned()],
                snapshot.clone(),
                None,
            ),
        )
        .await?;
    assert_eq!(repository.list(owner_user_id).await?.len(), 1);
    assert!(
        repository
            .find(owner_user_id, PLAYGROUND_CONVERSATION_KEY)
            .await?
            .is_some()
    );
    assert_eq!(
        repository
            .save(
                owner_user_id,
                PlaygroundConversationWrite::new(
                    PLAYGROUND_CONVERSATION_KEY.to_owned(),
                    "ping".to_owned(),
                    vec!["smoke-model".to_owned()],
                    snapshot,
                    Some(1),
                ),
            )
            .await?
            .revision(),
        1
    );

    users::Entity::delete_by_id(owner.id)
        .exec(pool.connection())
        .await?;
    assert_eq!(
        playground_conversations::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言实库中执行空看板聚合，覆盖 COUNT 与 SUM 的真实返回类型。
async fn exercise_admin_dashboard_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let period_end = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("管理看板冒烟时间溢出"))?;
    let dashboard = AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2))?
        .snapshot(0, period_end)
        .await?;
    assert_eq!(dashboard.request_count(), 0);
    assert_eq!(dashboard.quota_consumed(), 0);
    assert_eq!(dashboard.upstream_usage_count(), 0);
    assert_eq!(dashboard.estimated_usage_count(), 0);
    assert_eq!(dashboard.per_token_request_count(), 0);
    assert_eq!(dashboard.free_request_count(), 0);
    Ok(())
}

/// 在三方言隔离数据库中验证分组基础、高峰与跨分组附加倍率读取一致。
async fn exercise_group_pricing_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let source = group_active_model("smoke-source", "冒烟来源分组")
        .insert(pool.connection())
        .await?;
    let mut target = group_active_model("smoke-target", "冒烟目标分组");
    target.ratio_micros = Set(1_200_000);
    target.peak_ratio_micros = Set(Some(2_000_000));
    target.peak_start = Set(Some(fixed_peak_time()));
    target.peak_end = Set(Some(fixed_peak_end_time()));
    let target = target.insert(pool.connection()).await?;
    group_model_ratios::ActiveModel {
        source_group_id: Set(source.id),
        target_group_id: Set(target.id),
        ratio_micros: Set(750_000),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let catalog = GroupPricingRepository::new(pool.clone())
        .load_snapshot()
        .await?;
    assert_eq!(catalog.groups().len(), 2);
    assert_eq!(catalog.group_model_ratios().len(), 1);
    assert_eq!(catalog.group_model_ratios()[0].ratio_micros(), 750_000);
    Ok(())
}

/// 在三方言隔离数据库中验证 checkpoint、主体增量与并发重放处于同一事务边界。
async fn exercise_billing_batch_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("batch-smoke-group", "批量落盘冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "batch-smoke-user",
        "batch-smoke@example.com",
        "invite-batch-smoke",
    );
    user.status = Set(1);
    user.quota = Set(200);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_C, "batch-smoke-token");
    token.status = Set(1);
    token.remain_quota = Set(200);
    let token = token.insert(pool.connection()).await?;
    let channel = channel_active_model("batch-smoke-channel")
        .insert(pool.connection())
        .await?;
    let repository = BillingBatchRepository::new(pool.clone());
    let writer_id = [0xe1; 16];
    let first = billing_batch_write(writer_id, 1, 2, &user, &token, &channel, 10, 2)?;

    assert_eq!(
        repository.apply(&first).await?,
        BillingBatchWriteOutcome::Applied
    );
    assert_eq!(
        repository.apply(&first).await?,
        BillingBatchWriteOutcome::Existing
    );

    let next = Arc::new(billing_batch_write(
        writer_id, 3, 3, &user, &token, &channel, 5, 1,
    )?);
    let barrier = Arc::new(tokio::sync::Barrier::new(5));
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let repository = repository.clone();
        let batch = Arc::clone(&next);
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            repository.apply(&batch).await
        }));
    }
    barrier.wait().await;
    let mut applied = 0;
    let mut existing = 0;
    for task in tasks {
        match task.await?? {
            BillingBatchWriteOutcome::Applied => applied += 1,
            BillingBatchWriteOutcome::Existing => existing += 1,
        }
    }
    assert_eq!((applied, existing), (1, 3));

    let user = users::Entity::find_by_id(user.id)
        .one(pool.connection())
        .await?
        .expect("批量落盘冒烟用户必须存在");
    let token = tokens::Entity::find_by_id(token.id)
        .one(pool.connection())
        .await?
        .expect("批量落盘冒烟令牌必须存在");
    let channel = channels::Entity::find_by_id(channel.id)
        .one(pool.connection())
        .await?
        .expect("批量落盘冒烟渠道必须存在");
    assert_eq!(
        (user.quota, user.used_quota, user.request_count),
        (185, 15, 3)
    );
    assert_eq!((token.remain_quota, token.used_quota), (185, 15));
    assert_eq!(channel.used_quota, 15);

    billing_batch_checkpoints::Entity::delete_by_id(BillingBatchWriterKey::parse(
        &first.writer_key(),
    )?)
    .exec(pool.connection())
    .await?;
    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    channels::Entity::delete_by_id(channel.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

fn billing_batch_write(
    writer_id: [u8; 16],
    start_sequence: u64,
    end_sequence: u64,
    user: &users::Model,
    token: &tokens::Model,
    channel: &channels::Model,
    used_quota: i64,
    request_count: i64,
) -> Result<BillingBatchWrite, Box<dyn Error>> {
    let event_ids = (start_sequence..=end_sequence)
        .map(|sequence| {
            let marker = <u8 as TryFrom<u64>>::try_from(sequence).unwrap_or(0xfe);
            BillingReservationId::new([marker; 16])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BillingBatchWrite::new(
        writer_id,
        start_sequence,
        end_sequence,
        event_ids,
        vec![UserBillingWrite::new(
            UserId::new(user.id)?,
            QuotaDelta::new(-used_quota)?,
            QuotaDelta::new(used_quota)?,
            request_count,
        )?],
        vec![TokenBillingWrite::new(
            TokenId::new(token.id)?,
            QuotaDelta::new(-used_quota)?,
            QuotaDelta::new(used_quota)?,
        )?],
        vec![ChannelBillingWrite::new(
            ChannelId::new(channel.id)?,
            QuotaDelta::new(used_quota)?,
        )?],
    )?)
}

/// 在三方言隔离数据库中执行并发预扣、结算、退款及终态重放。
async fn exercise_quota_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("quota-smoke-group", "额度仓储冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "quota-smoke-user",
        "quota-smoke@example.com",
        "invite-quota-smoke",
    );
    user.status = Set(1);
    user.quota = Set(200);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_D, "quota-smoke-token");
    token.status = Set(1);
    token.remain_quota = Set(200);
    let token = token.insert(pool.connection()).await?;
    let principal = GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    );
    let settled_id = BillingReservationId::new([0xd1; 16])?;
    let refunded_id = BillingReservationId::new([0xd2; 16])?;
    let repository = QuotaRepository::new(pool.clone());

    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let first_repository = repository.clone();
    let first_barrier = Arc::clone(&barrier);
    let first = tokio::spawn(async move {
        first_barrier.wait().await;
        first_repository
            .precharge(settled_id, principal, Quota::new(40).unwrap())
            .await
    });
    let second_repository = repository.clone();
    let second_barrier = Arc::clone(&barrier);
    let second = tokio::spawn(async move {
        second_barrier.wait().await;
        second_repository
            .precharge(refunded_id, principal, Quota::new(40).unwrap())
            .await
    });
    barrier.wait().await;
    assert_eq!(first.await??, QuotaMutationOutcome::Applied);
    assert_eq!(second.await??, QuotaMutationOutcome::Applied);

    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let settle_repository = repository.clone();
    let settle_barrier = Arc::clone(&barrier);
    let settle = tokio::spawn(async move {
        settle_barrier.wait().await;
        settle_repository
            .settle(settled_id, Quota::new(50).unwrap())
            .await
    });
    let refund_repository = repository.clone();
    let refund_barrier = Arc::clone(&barrier);
    let refund = tokio::spawn(async move {
        refund_barrier.wait().await;
        refund_repository.refund(refunded_id).await
    });
    barrier.wait().await;
    assert_eq!(settle.await??, QuotaMutationOutcome::Applied);
    assert_eq!(refund.await??, QuotaMutationOutcome::Applied);
    assert_eq!(
        repository
            .settle(settled_id, Quota::new(50).unwrap())
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        repository.refund(refunded_id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );

    let user = users::Entity::find_by_id(user.id)
        .one(pool.connection())
        .await?
        .expect("额度冒烟用户必须存在");
    let token = tokens::Entity::find_by_id(token.id)
        .one(pool.connection())
        .await?
        .expect("额度冒烟令牌必须存在");
    assert_eq!(
        (
            user.quota,
            user.used_quota,
            user.frozen_quota,
            user.request_count
        ),
        (150, 50, 0, 1)
    );
    assert_eq!((token.remain_quota, token.used_quota), (150, 50));

    for (id, expected_status) in [
        (settled_id, QuotaReservationStatus::Settled),
        (refunded_id, QuotaReservationStatus::Refunded),
    ] {
        let key = BillingReservationKey::parse(&id.persistence_key())?;
        let reservation = billing_reservations::Entity::find_by_id(key.clone())
            .one(pool.connection())
            .await?
            .expect("额度冒烟预留记录必须存在");
        let finalized_at = reservation
            .finalized_at
            .expect("额度冒烟终态必须记录完成时间");
        assert_eq!(reservation.status, expected_status.code());
        assert!(reservation.updated_at >= reservation.created_at);
        assert!(finalized_at >= reservation.created_at);
        assert_eq!(finalized_at, reservation.updated_at);
        billing_reservations::Entity::delete_by_id(key)
            .exec(pool.connection())
            .await?;
    }
    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在现有三方言迁移流水线中执行一次真实联表鉴权，防止方言差异只在生产暴露。
async fn exercise_token_auth_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("auth-smoke-group", "鉴权冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "auth-smoke-user",
        "auth-smoke@example.com",
        "invite-auth-smoke",
    );
    user.status = Set(1);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_E, "auth-smoke-token");
    token.status = Set(1);
    token.allow_ips = Set(Some(TokenIpAllowlist::validate(Json::Array(vec![
        Json::String("192.0.2.0/24".to_owned()),
        Json::String("2001:db8::/32".to_owned()),
    ]))?));
    let token = token.insert(pool.connection()).await?;

    let repository = TokenAuthRepository::new(pool.clone(), Duration::from_secs(2))?;
    let outcome = repository
        .lookup(
            &TokenAuthLookup::new(TOKEN_HASH_E)?,
            TrustedClientIp::new("192.0.2.10".parse()?),
        )
        .await?;
    let TokenAuthLookupOutcome::Authenticated {
        principal,
        model_policy,
    } = outcome
    else {
        return Err("三方言令牌鉴权冒烟未返回已认证主体".into());
    };
    assert_eq!(principal.token_id().get(), token.id);
    assert_eq!(principal.user_id().get(), user.id);
    assert_eq!(principal.group_id().get(), group.id);
    assert!(model_policy.allows("auth-smoke-model"));
    assert_eq!(
        repository
            .lookup(
                &TokenAuthLookup::new(TOKEN_HASH_E)?,
                TrustedClientIp::new("198.51.100.10".parse()?),
            )
            .await?,
        TokenAuthLookupOutcome::Rejected
    );

    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
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

/// 在三方言真实事务中验证注册策略与固定窗口限流的原子判定。
async fn exercise_registration_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("registration-smoke", "注册冒烟分组")
        .insert(pool.connection())
        .await?;
    let group_id = GroupId::new(group.id)?;
    let repository = RegistrationRepository::new(pool.clone(), Duration::from_secs(5))?;
    repository
        .update_policy(RegistrationPolicyWriteRecord::new(
            true, true, group_id, 100, true, 1, 60,
        ))
        .await
        .expect("注册冒烟必须成功写入策略");

    let first = repository
        .claim_attempt([0x5a; 32], 120)
        .await
        .expect("注册冒烟必须成功占用首次尝试");
    assert!(matches!(first, RegistrationAttemptOutcome::Allowed(_)));
    assert_eq!(
        repository
            .claim_attempt([0x5a; 32], 121)
            .await
            .expect("注册冒烟必须完成限流判定"),
        RegistrationAttemptOutcome::RateLimited {
            retry_after_seconds: 59
        }
    );

    // 冒烟数据不得改变后续核心表计数与外键行为基线。
    registration_rate_limits::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    authentication_settings::ActiveModel {
        id: Set(1),
        password_login_enabled: Set(true),
        registration_enabled: Set(false),
        registration_default_group_id: Set(None),
        registration_initial_quota: Set(0),
        registration_email_required: Set(false),
        registration_rate_limit_attempts: Set(5),
        registration_rate_limit_window_seconds: Set(3_600),
        ..Default::default()
    }
    .update(pool.connection())
    .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言真实事务中验证挑战签发、冷却、摘要替换与单次消费。
async fn exercise_auth_challenge_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = AuthChallengeRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = repository
        .issue(AuthChallengeIssue::new(
            AuthChallengePurpose::RegistrationEmail,
            [0x6a; 32],
            [0x6b; 32],
            None,
            10_000,
            600,
            60,
            3,
        )?)
        .await?;
    assert!(matches!(
        issued,
        AuthChallengeIssueOutcome::Issued(record)
            if record.version() == 1 && record.next_send_at() == 10_060
    ));
    let consumed = repository
        .consume(AuthChallengeConsume::new(
            AuthChallengePurpose::RegistrationEmail,
            [0x6a; 32],
            [0x6b; 32],
            10_001,
        )?)
        .await?;
    assert!(matches!(
        consumed,
        AuthChallengeConsumeOutcome::Consumed(record)
            if record.target_user_id().is_none() && record.version() == 2
    ));

    // 冒烟数据只验证短生命周期状态，不进入后续核心实体计数。
    auth_challenges::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言真实事务中验证站点固定记录和单调版本。
async fn exercise_site_settings_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = SiteSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let initial = repository.settings().await?;
    assert_eq!(initial.site_name(), "AnyFlows");
    assert_eq!(initial.version(), 1);

    let saved = repository
        .update(SiteSettingsWriteRecord::new(
            "AnyFlows Smoke".to_owned(),
            Some("https://example.com/gateway".to_owned()),
            Some("/brand.svg".to_owned()),
            Some("统一访问模型".to_owned()),
            Some("三方言站点设置冒烟。".to_owned()),
        ))
        .await?;
    assert_eq!(saved.version(), 2);
    assert_eq!(saved.site_name(), "AnyFlows Smoke");
    Ok(())
}

/// 在三方言真实事务中验证固定邮件设置行、密文保留和单调版本。
async fn exercise_email_settings_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = EmailSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let initial = repository.settings().await?;
    assert!(!initial.enabled());
    assert_eq!(initial.port(), 587);
    assert_eq!(initial.timeout_seconds(), 10);
    assert_eq!(initial.version(), 1);
    assert!(!initial.password_configured());

    let envelope = EncryptedCredentialEnvelope::new("email-smoke-key", [0x2a; 24], vec![0x5a; 32])?;
    let replaced = repository
        .update(EmailSettingsWriteRecord::new(
            true,
            "smtp.example.com".to_owned(),
            587,
            EmailTlsMode::StartTls,
            Some("mailer@example.com".to_owned()),
            EmailPasswordUpdate::Replace(envelope.clone()),
            "from@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            Some("reply@example.com".to_owned()),
            15,
        ))
        .await?;
    assert_eq!(replaced.version(), 2);
    assert_eq!(replaced.password_secret(), Some(&envelope));

    let kept = repository
        .update(EmailSettingsWriteRecord::new(
            true,
            "smtp.example.com".to_owned(),
            465,
            EmailTlsMode::Tls,
            Some("mailer@example.com".to_owned()),
            EmailPasswordUpdate::Keep,
            "from@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            None,
            20,
        ))
        .await?;
    assert_eq!(kept.version(), 3);
    assert_eq!(kept.password_secret(), Some(&envelope));
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
    assert_entity_columns::<billing_reservations::Column>(&manager, "billing_reservations").await?;
    assert_entity_columns::<billing_batch_checkpoints::Column>(
        &manager,
        "billing_batch_checkpoints",
    )
    .await?;
    assert_entity_columns::<group_model_ratios::Column>(&manager, "group_model_ratios").await?;
    assert_entity_columns::<playground_shares::Column>(&manager, "playground_shares").await?;
    assert_entity_columns::<playground_conversations::Column>(&manager, "playground_conversations")
        .await?;
    assert_entity_columns::<registration_rate_limits::Column>(&manager, "registration_rate_limits")
        .await?;
    assert_entity_columns::<email_settings::Column>(&manager, "email_settings").await?;
    assert_entity_columns::<site_settings::Column>(&manager, "site_settings").await?;
    assert_entity_columns::<authentication_settings::Column>(&manager, "authentication_settings")
        .await?;
    assert_entity_columns::<auth_challenges::Column>(&manager, "auth_challenges").await?;
    assert_entity_columns::<auth_challenge_rate_limits::Column>(
        &manager,
        "auth_challenge_rate_limits",
    )
    .await?;

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
        (
            "billing_reservations",
            "idx_billing_reservations_status_expires",
        ),
        (
            "billing_reservations",
            "idx_billing_reservations_user_created",
        ),
        (
            "billing_reservations",
            "idx_billing_reservations_token_created",
        ),
        ("group_model_ratios", "idx_group_model_ratios_target"),
        ("playground_shares", "uq_playground_shares_token_hash"),
        ("playground_shares", "idx_playground_shares_owner_created"),
        ("playground_shares", "idx_playground_shares_expires_at"),
        (
            "playground_conversations",
            "idx_playground_conversations_owner_updated",
        ),
        ("auth_challenges", "uq_auth_challenges_purpose_subject"),
        ("auth_challenges", "idx_auth_challenges_target_purpose"),
        ("auth_challenges", "idx_auth_challenges_expires_at"),
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
    assert_behavior!(authentication_settings::ActiveModel);
    assert_behavior!(billing_batch_checkpoints::ActiveModel);
    assert_behavior!(billing_reservations::ActiveModel);
    assert_behavior!(channel_groups::ActiveModel);
    assert_behavior!(channel_models::ActiveModel);
    assert_behavior!(channels::ActiveModel);
    assert_behavior!(credentials::ActiveModel);
    assert_behavior!(group_model_ratios::ActiveModel);
    assert_behavior!(groups::ActiveModel);
    assert_behavior!(options::ActiveModel);
    assert_behavior!(registration_rate_limits::ActiveModel);
    assert_behavior!(site_settings::ActiveModel);
    assert_behavior!(email_settings::ActiveModel);
    assert_behavior!(tokens::ActiveModel);
    assert_behavior!(users::ActiveModel);
    Ok(())
}

async fn exercise_core_schema(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let group = group_active_model("default", "默认分组")
        .insert(connection)
        .await?;
    assert_eq!(group.ratio_micros, 1_000_000);
    group_model_ratios::ActiveModel {
        source_group_id: Set(group.id),
        target_group_id: Set(group.id),
        ratio_micros: Set(1_000_000),
        ..Default::default()
    }
    .insert(connection)
    .await?;
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

    let reservation =
        reservation_active_model(RESERVATION_KEY_A, user.id, token.id, group.id, 100, 100)
            .insert(connection)
            .await?;
    assert_eq!(reservation.status, 1);
    assert_eq!(reservation.actual_quota, None);
    assert_eq!(reservation.finalized_at, None);
    assert_debug_redacted(&reservation, RESERVATION_KEY_A);
    assert_billing_reservation_constraints(connection, user.id, token.id, group.id).await?;

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

    for timeout_secs in [0, 901] {
        let mut invalid_channel_timeout =
            channel_active_model(&format!("invalid-channel-timeout-{timeout_secs}"));
        invalid_channel_timeout.timeout_secs = Set(Some(timeout_secs));
        assert!(invalid_channel_timeout.insert(connection).await.is_err());
    }

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

async fn assert_billing_reservation_constraints(
    connection: &DatabaseConnection,
    user_id: i64,
    token_id: i64,
    group_id: i64,
) -> Result<(), DbErr> {
    for invalid_key in [
        "00000000000000000000000000000000",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "gggggggggggggggggggggggggggggggg",
        "1111111111111111111111111111111",
        "111111111111111111111111111111111",
    ] {
        // 绕过实体值对象，确认数据库本身仍会拒绝非规范幂等键。
        let statement = Query::insert()
            .into_table(billing_reservations::Entity)
            .columns([
                billing_reservations::Column::IdempotencyKey,
                billing_reservations::Column::UserId,
                billing_reservations::Column::TokenId,
                billing_reservations::Column::GroupId,
                billing_reservations::Column::ReservedQuota,
                billing_reservations::Column::ExpiresAt,
            ])
            .values_panic([
                invalid_key.into(),
                user_id.into(),
                token_id.into(),
                group_id.into(),
                10_i64.into(),
                future_expiration().into(),
            ])
            .to_owned();
        assert!(
            connection
                .execute(connection.get_database_backend().build(&statement))
                .await
                .is_err()
        );
    }

    assert!(
        reservation_active_model(RESERVATION_KEY_A, user_id, token_id, group_id, 100, 100,)
            .insert(connection)
            .await
            .is_err()
    );

    let pending = reservation_active_model(RESERVATION_KEY_B, user_id, token_id, group_id, 100, 0)
        .insert(connection)
        .await?;
    let mut pending_update = pending.into_active_model();
    pending_update.status = Set(2);
    pending_update.actual_quota = Set(Some(80));
    let pending = pending_update.update(connection).await?;
    let mut settled_update = pending.into_active_model();
    settled_update.status = Set(3);
    settled_update.finalized_at = Set(Some(fixed_finalization_time()));
    settled_update.update(connection).await?;

    let mut invalid_reserved =
        reservation_active_model(RESERVATION_KEY_C, user_id, token_id, group_id, 1, 0);
    invalid_reserved.reserved_quota = Set(0);
    assert!(invalid_reserved.insert(connection).await.is_err());

    let invalid_token_reserved =
        reservation_active_model(RESERVATION_KEY_D, user_id, token_id, group_id, 100, 50);
    assert!(invalid_token_reserved.insert(connection).await.is_err());

    let mut missing_actual = reservation_active_model(
        "55555555555555555555555555555555",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    missing_actual.status = Set(2);
    assert!(missing_actual.insert(connection).await.is_err());

    let mut invalid_refund = reservation_active_model(
        "66666666666666666666666666666666",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    invalid_refund.status = Set(4);
    invalid_refund.actual_quota = Set(Some(0));
    invalid_refund.finalized_at = Set(Some(fixed_finalization_time()));
    assert!(invalid_refund.insert(connection).await.is_err());

    let missing_user = reservation_active_model(
        "77777777777777777777777777777777",
        i64::MAX,
        token_id,
        group_id,
        100,
        100,
    );
    assert!(missing_user.insert(connection).await.is_err());

    let missing_token = reservation_active_model(
        "88888888888888888888888888888888",
        user_id,
        i64::MAX,
        group_id,
        100,
        100,
    );
    assert!(missing_token.insert(connection).await.is_err());

    let missing_group = reservation_active_model(
        "99999999999999999999999999999999",
        user_id,
        token_id,
        i64::MAX,
        100,
        100,
    );
    assert!(missing_group.insert(connection).await.is_err());

    let mut negative_actual = reservation_active_model(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    negative_actual.status = Set(2);
    negative_actual.actual_quota = Set(Some(-1));
    assert!(negative_actual.insert(connection).await.is_err());

    let mut negative_token_reserved = reservation_active_model(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    negative_token_reserved.token_reserved_quota = Set(-1);
    assert!(negative_token_reserved.insert(connection).await.is_err());

    let mut invalid_status = reservation_active_model(
        "cccccccccccccccccccccccccccccccc",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    invalid_status.status = Set(5);
    assert!(invalid_status.insert(connection).await.is_err());

    // 计费记录必须阻止主体被硬删除，否则迟到重放将失去退款和审计依据。
    assert!(
        tokens::Entity::delete_by_id(token_id)
            .exec(connection)
            .await
            .is_err()
    );
    assert!(
        users::Entity::delete_by_id(user_id)
            .exec(connection)
            .await
            .is_err()
    );
    assert!(
        groups::Entity::delete_by_id(group_id)
            .exec(connection)
            .await
            .is_err()
    );
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

fn reservation_active_model(
    idempotency_key: &str,
    user_id: i64,
    token_id: i64,
    group_id: i64,
    reserved_quota: i64,
    token_reserved_quota: i64,
) -> billing_reservations::ActiveModel {
    billing_reservations::ActiveModel {
        idempotency_key: Set(BillingReservationKey::parse(idempotency_key).unwrap()),
        user_id: Set(user_id),
        token_id: Set(token_id),
        group_id: Set(group_id),
        reserved_quota: Set(reserved_quota),
        token_reserved_quota: Set(token_reserved_quota),
        expires_at: Set(future_expiration()),
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

fn fixed_finalization_time() -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(1_893_456_000)
        .expect("固定的 2030-01-01 时间戳必须有效")
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
