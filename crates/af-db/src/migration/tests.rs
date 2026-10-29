use std::env;
use std::error::Error;

use sea_orm_migration::prelude::*;

use super::{LEGACY_ENTERPRISE_MIGRATION_VERSIONS, Migrator};
use crate::{DatabaseOptions, PoolOptions, connect};

const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
const ALLOW_DESTRUCTIVE_MIGRATION_TEST: &str = "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST";
const LEGACY_PRICING_MODEL: &str = "migration-legacy-model-price";

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
    prepare_legacy_model_price(&pool).await?;

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
    check_model_group_controls_upgrade(&pool).await?;
    pool.close().await?;
    Ok(())
}

async fn prepare_legacy_model_price(pool: &crate::DatabasePool) -> Result<(), Box<dyn Error>> {
    use super::iden::{model_prices, models};
    use sea_orm::ConnectionTrait;

    let backend = pool.connection().get_database_backend();
    let insert_model = Query::insert()
        .into_table(models::Entity)
        .columns([
            models::Column::Model,
            models::Column::DisplayName,
            models::Column::Provider,
            models::Column::Tags,
            models::Column::SupportsTextInput,
            models::Column::SupportsTextOutput,
            models::Column::Visibility,
            models::Column::Lifecycle,
        ])
        .values_panic([
            LEGACY_PRICING_MODEL.into(),
            "Legacy model price".into(),
            "migration".into(),
            serde_json::json!([]).into(),
            true.into(),
            true.into(),
            1_i16.into(),
            1_i16.into(),
        ])
        .to_owned();
    pool.connection()
        .execute(backend.build(&insert_model))
        .await?;
    // 使用历史列写入，确保新增的可空计划列不会破坏已有价格和版本。
    let insert_price = Query::insert()
        .into_table(model_prices::Entity)
        .columns([
            model_prices::Column::Model,
            model_prices::Column::BillingMode,
            model_prices::Column::InputPrice,
            model_prices::Column::Version,
        ])
        .values_panic([
            LEGACY_PRICING_MODEL.into(),
            1_i16.into(),
            3_i64.into(),
            7_i64.into(),
        ])
        .to_owned();
    pool.connection()
        .execute(backend.build(&insert_price))
        .await?;
    Ok(())
}

async fn check_model_group_controls_upgrade(
    pool: &crate::DatabasePool,
) -> Result<(), Box<dyn Error>> {
    use std::time::Duration;

    use af_domain::{GroupId, GroupPeakWindow, ModelPeakSchedule, PricingWeekTime};
    use rust_decimal::Decimal;
    use sea_orm::{ConnectionTrait, Statement, TransactionTrait};

    use crate::{
        GroupModelPolicyRepository, GroupModelPolicyRepositoryError, ModelPriceBillingMode,
        ModelPriceRepository, ModelPriceWriteRecord,
    };

    let connection = pool.connection();
    let backend = connection.get_database_backend();
    let price_query = Query::select()
        .columns([Alias::new("peak_windows"), Alias::new("version")])
        .from(Alias::new("model_prices"))
        .and_where(Expr::col(Alias::new("model")).eq(LEGACY_PRICING_MODEL))
        .to_owned();
    let row = connection
        .query_one(backend.build(&price_query))
        .await?
        .ok_or("legacy model price must survive the upgrade")?;
    assert!(
        row.try_get::<Option<serde_json::Value>>("", "peak_windows")?
            .is_none()
    );
    assert_eq!(row.try_get::<i64>("", "version")?, 7);

    let prices = ModelPriceRepository::new(pool.clone());
    let legacy = prices
        .load_all()
        .await?
        .into_iter()
        .find(|price| price.model() == LEGACY_PRICING_MODEL)
        .ok_or("upgraded model price must remain readable")?;
    assert_eq!(legacy.billing_mode(), ModelPriceBillingMode::PerToken);
    assert_eq!(legacy.prices()[0], Decimal::from(3));
    assert!(legacy.peak_schedule().is_none());

    let group_id = GroupId::new(1)?;
    let policies = GroupModelPolicyRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert!(policies.get(group_id).await?.allowed_models.is_none());
    assert!(
        policies
            .policy_for_group(group_id)
            .await?
            .allows("any-model")
    );
    let allowed_models = vec![LEGACY_PRICING_MODEL.to_owned()];
    policies
        .replace(group_id, Some(allowed_models.clone()))
        .await?;
    assert_eq!(
        policies.get(group_id).await?.allowed_models,
        Some(allowed_models)
    );
    let policy = policies.policy_for_group(group_id).await?;
    assert!(policy.allows(LEGACY_PRICING_MODEL));
    assert!(!policy.allows("another-model"));
    assert_eq!(
        policies.replace(group_id, Some(vec![])).await,
        Err(GroupModelPolicyRepositoryError::InvalidInput)
    );

    // 主键、外键和非空约束必须在三种数据库上保留，失败语句各自使用自动提交。
    let insert_policy = |id: i64| {
        Query::insert()
            .into_table(Alias::new("group_model_policies"))
            .columns([Alias::new("group_id"), Alias::new("allowed_models")])
            .values_panic([id.into(), serde_json::json!([LEGACY_PRICING_MODEL]).into()])
            .to_owned()
    };
    assert!(
        connection
            .execute(backend.build(&insert_policy(1)))
            .await
            .is_err()
    );
    assert!(
        connection
            .execute(backend.build(&insert_policy(i64::MAX)))
            .await
            .is_err()
    );
    let null_policy = Query::update()
        .table(Alias::new("group_model_policies"))
        .value(
            Alias::new("allowed_models"),
            Option::<serde_json::Value>::None,
        )
        .and_where(Expr::col(Alias::new("group_id")).eq(1_i64))
        .to_owned();
    assert!(
        connection
            .execute(backend.build(&null_policy))
            .await
            .is_err()
    );
    assert!(
        policies
            .policy_for_group(group_id)
            .await?
            .allows(LEGACY_PRICING_MODEL)
    );

    let transaction = connection.begin().await?;
    let temporary_group_id = 9_900_160_i64;
    let insert_group = Query::insert()
        .into_table(super::iden::groups::Entity)
        .columns([
            super::iden::groups::Column::Id,
            super::iden::groups::Column::Name,
            super::iden::groups::Column::DisplayName,
            super::iden::groups::Column::Flags,
        ])
        .values_panic([
            temporary_group_id.into(),
            "model-policy-migration".into(),
            "Model policy migration".into(),
            serde_json::json!({}).into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&insert_group)).await?;
    transaction
        .execute(backend.build(&insert_policy(temporary_group_id)))
        .await?;
    let delete_group = Query::delete()
        .from_table(super::iden::groups::Entity)
        .and_where(Expr::col(super::iden::groups::Column::Id).eq(temporary_group_id))
        .to_owned();
    transaction.execute(backend.build(&delete_group)).await?;
    let remaining_policy = Query::select()
        .column(Alias::new("group_id"))
        .from(Alias::new("group_model_policies"))
        .and_where(Expr::col(Alias::new("group_id")).eq(temporary_group_id))
        .to_owned();
    assert!(
        transaction
            .query_one(backend.build(&remaining_policy))
            .await?
            .is_none()
    );
    transaction.rollback().await?;

    policies.replace(group_id, None).await?;
    assert!(policies.get(group_id).await?.allowed_models.is_none());
    let row = connection
        .query_one(Statement::from_string(
            backend,
            "SELECT COUNT(*) AS policy_count FROM group_model_policies".to_owned(),
        ))
        .await?
        .ok_or("group model policy table must exist")?;
    assert_eq!(row.try_get::<i64>("", "policy_count")?, 0);

    let schedule = ModelPeakSchedule::new(vec![GroupPeakWindow::new(
        vec![7],
        23 * 3_600,
        2 * 3_600,
        500_000,
        vec![],
        480,
    )?])?;
    let write = ModelPriceWriteRecord::new_with_schedule_patch(
        LEGACY_PRICING_MODEL.to_owned(),
        Some(7),
        None,
        legacy.billing_mode(),
        legacy.prices(),
        None,
        Some(Some(schedule.clone())),
    )?;
    prices.apply_batch(vec![write]).await?;
    let restored = prices
        .load_all()
        .await?
        .into_iter()
        .find(|price| price.model() == LEGACY_PRICING_MODEL)
        .ok_or("model peak schedule must remain readable")?;
    assert_eq!(restored.version(), 8);
    assert_eq!(restored.prices(), legacy.prices());
    assert_eq!(restored.peak_schedule(), Some(&schedule));
    assert_eq!(
        restored
            .peak_schedule()
            .unwrap()
            .ratio_at(PricingWeekTime::new(7, 16 * 3_600)?),
        Some(500_000)
    );
    assert_eq!(
        restored
            .peak_schedule()
            .unwrap()
            .ratio_at(PricingWeekTime::new(7, 18 * 3_600)?),
        None
    );
    // 旧版价格客户端省略计划字段时保留规则；显式清除后恢复可空状态。
    let unchanged = ModelPriceWriteRecord::new(
        LEGACY_PRICING_MODEL.to_owned(),
        Some(8),
        restored.billing_mode(),
        restored.prices(),
    )?;
    let saved = prices.apply_batch(vec![unchanged]).await?;
    assert_eq!(saved[0].version(), 9);
    assert_eq!(saved[0].peak_schedule(), Some(&schedule));
    let clear = ModelPriceWriteRecord::new_with_schedule_patch(
        LEGACY_PRICING_MODEL.to_owned(),
        Some(9),
        None,
        restored.billing_mode(),
        restored.prices(),
        None,
        Some(None),
    )?;
    let saved = prices.apply_batch(vec![clear]).await?;
    assert_eq!(saved[0].version(), 10);
    assert!(saved[0].peak_schedule().is_none());
    let row = connection
        .query_one(backend.build(&price_query))
        .await?
        .ok_or("cleared model schedule must remain readable")?;
    assert!(
        row.try_get::<Option<serde_json::Value>>("", "peak_windows")?
            .is_none()
    );
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
