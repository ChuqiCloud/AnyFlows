use std::{error::Error, time::Duration};

use af_domain::{GroupId, GroupPeakSchedule, GroupPeakWindow};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait, QueryFilter, Statement,
    TransactionTrait, sea_query::Expr,
};

use super::{
    AdminGroupDeleteOutcome, AdminGroupMutationOutcome, AdminGroupPeakWriteRecord,
    AdminGroupRepository, AdminGroupRepositoryError, AdminGroupWriteRecord, DatabaseOptions,
    MigrationOptions, PoolOptions,
};
use crate::entity::{
    abilities, authentication_settings, channel_groups, group_group_grants,
    group_model_ratio_overrides, group_model_ratios, groups, scheduler_outbox_events, tokens,
    users,
};

#[tokio::test]
async fn weekly_schedule_persists_omission_preserves_and_empty_explicitly_disables()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let schedule = GroupPeakSchedule::new(vec![GroupPeakWindow::new(
        vec![1, 3, 5],
        23 * 3_600,
        3_600,
        500_000,
        vec!["deepseek-chat".to_owned()],
        480,
    )?])?;
    let created = fixture
        .repository
        .create(
            write_record("weekly", "Weekly", None, true).with_peak_schedule(Some(schedule.clone())),
        )
        .await?;
    assert_eq!(created.peak_schedule(), Some(&schedule));
    let id = created.group_id();
    let AdminGroupMutationOutcome::Mutated(updated) = fixture
        .repository
        .update(id, write_record("weekly", "Omitted", None, true))
        .await?
    else {
        panic!("group must exist")
    };
    assert_eq!(updated.peak_schedule(), Some(&schedule));
    let AdminGroupMutationOutcome::Mutated(cleared) = fixture
        .repository
        .update(
            id,
            write_record("weekly", "Disabled", None, true)
                .with_peak_schedule(Some(GroupPeakSchedule::default())),
        )
        .await?
    else {
        panic!("group must exist")
    };
    assert!(cleared.peak_schedule().unwrap().windows().is_empty());
    assert!(cleared.peak().is_some());
    let model = groups::Entity::find_by_id(id.get())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(model.peak_windows, Some(serde_json::json!([])));
    Ok(())
}

#[tokio::test]
async fn create_and_update_enforce_name_fallback_and_complete_fields() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let created = fixture
        .repository
        .create(write_record("vip", "VIP", Some(fixture.default_id), true))
        .await?;
    let vip_id = created.group_id();
    assert_eq!(created.ratio_micros(), 1_250_000);
    assert_eq!(created.peak().unwrap().start_second(), 8 * 3_600);
    assert_eq!(created.fallback_group_id(), Some(fixture.default_id));
    assert_eq!(created.flags()["claude_code_only"], true);
    let window_starts = [
        created.daily_window().started_at(),
        created.weekly_window().started_at(),
        created.monthly_window().started_at(),
    ];

    assert_eq!(
        fixture
            .repository
            .create(write_record("vip", "Duplicate", None, false))
            .await
            .unwrap_err(),
        AdminGroupRepositoryError::Conflict
    );
    assert_eq!(
        fixture
            .repository
            .update(
                vip_id,
                write_record("vip-self", "Self", Some(vip_id), false),
            )
            .await
            .unwrap_err(),
        AdminGroupRepositoryError::InvalidReference
    );
    assert_eq!(
        fixture
            .repository
            .update(
                vip_id,
                write_record(
                    "vip-missing",
                    "Missing",
                    Some(GroupId::new(999_999)?),
                    false,
                ),
            )
            .await
            .unwrap_err(),
        AdminGroupRepositoryError::InvalidReference
    );

    groups::Entity::update_many()
        .filter(groups::Column::Id.eq(vip_id.get()))
        .col_expr(groups::Column::DailyUsage, Expr::value(10_i64))
        .col_expr(groups::Column::WeeklyUsage, Expr::value(20_i64))
        .col_expr(groups::Column::MonthlyUsage, Expr::value(30_i64))
        .exec(fixture.pool.connection())
        .await?;

    let AdminGroupMutationOutcome::Mutated(updated) = fixture
        .repository
        .update(
            vip_id,
            AdminGroupWriteRecord::new(
                "premium".to_owned(),
                "Premium".to_owned(),
                900_000,
                None,
                false,
                None,
                Some(50_000),
                None,
                None,
                None,
                serde_json::json!({"visible": true}),
            ),
        )
        .await?
    else {
        panic!("有效分组必须完成更新");
    };
    assert_eq!(updated.name(), "premium");
    assert_eq!(updated.display_name(), "Premium");
    assert_eq!(updated.ratio_micros(), 900_000);
    assert!(updated.peak().is_none());
    assert_eq!(updated.weekly_limit(), Some(50_000));
    assert_eq!(updated.fallback_group_id(), None);
    assert_eq!(
        [
            updated.daily_window().usage(),
            updated.weekly_window().usage(),
            updated.monthly_window().usage(),
        ],
        [10, 20, 30]
    );
    assert_eq!(
        [
            updated.daily_window().started_at(),
            updated.weekly_window().started_at(),
            updated.monthly_window().started_at(),
        ],
        window_starts
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn delete_rejects_live_owners_and_rolls_back_tombstone() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let protected = fixture
        .repository
        .create(write_record("protected", "Protected", None, false))
        .await?;
    insert_user(
        fixture.pool.connection(),
        501,
        "protected-user",
        protected.group_id().get(),
        false,
    )
    .await?;
    assert_eq!(
        fixture
            .repository
            .delete(protected.group_id())
            .await
            .unwrap_err(),
        AdminGroupRepositoryError::InUse
    );
    assert!(matches!(
        fixture.repository.get(protected.group_id()).await?,
        super::AdminGroupLookupOutcome::Found(_)
    ));

    let token_group = fixture
        .repository
        .create(write_record("token-group", "Token Group", None, false))
        .await?;
    insert_user(
        fixture.pool.connection(),
        502,
        "token-user",
        fixture.default_id.get(),
        false,
    )
    .await?;
    insert_token(fixture.pool.connection(), 502, token_group.group_id().get()).await?;
    assert_eq!(
        fixture
            .repository
            .delete(token_group.group_id())
            .await
            .unwrap_err(),
        AdminGroupRepositoryError::InUse
    );
    assert!(matches!(
        fixture.repository.get(token_group.group_id()).await?,
        super::AdminGroupLookupOutcome::Found(_)
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn delete_clears_safe_runtime_relationships_atomically() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let target = fixture
        .repository
        .create(write_record("target", "Target", None, false))
        .await?;
    let dependent = fixture
        .repository
        .create(write_record(
            "dependent",
            "Dependent",
            Some(target.group_id()),
            false,
        ))
        .await?;
    insert_runtime_relations(
        fixture.pool.connection(),
        target.group_id().get(),
        fixture.default_id.get(),
    )
    .await?;

    assert_eq!(
        fixture.repository.delete(target.group_id()).await?,
        AdminGroupDeleteOutcome::Deleted
    );
    assert_eq!(
        fixture.repository.delete(target.group_id()).await?,
        AdminGroupDeleteOutcome::NotFound
    );
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .filter(scheduler_outbox_events::Column::SubjectKind.eq(2_i16))
            .filter(scheduler_outbox_events::Column::SubjectId.eq(target.group_id().get()),)
            .count(fixture.pool.connection())
            .await?,
        1,
        "重复删除不得追加调度 outbox 事件"
    );
    let dependent_model = groups::Entity::find_by_id(dependent.group_id().get())
        .one(fixture.pool.connection())
        .await?
        .expect("依赖分组必须保留");
    assert_eq!(dependent_model.fallback_group_id, None);
    assert_eq!(
        channel_groups::Entity::find()
            .filter(channel_groups::Column::GroupId.eq(target.group_id().get()))
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        abilities::Entity::find()
            .filter(abilities::Column::GroupId.eq(target.group_id().get()))
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        group_model_ratios::Entity::find()
            .filter(
                group_model_ratios::Column::SourceGroupId
                    .eq(target.group_id().get())
                    .or(group_model_ratios::Column::TargetGroupId.eq(target.group_id().get())),
            )
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        group_group_grants::Entity::find()
            .filter(
                group_group_grants::Column::SourceGroupId
                    .eq(target.group_id().get())
                    .or(group_group_grants::Column::TargetGroupId.eq(target.group_id().get())),
            )
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        group_model_ratio_overrides::Entity::find()
            .filter(
                group_model_ratio_overrides::Column::SourceGroupId
                    .eq(target.group_id().get())
                    .or(group_model_ratio_overrides::Column::TargetGroupId
                        .eq(target.group_id().get()),),
            )
            .count(fixture.pool.connection())
            .await?,
        0
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn explicit_group_replacement_moves_disabled_users_and_revokes_direct_keys()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let source = fixture
        .repository
        .create(write_record("retired", "Retired", None, false))
        .await?
        .group_id();
    insert_user(
        fixture.pool.connection(),
        701,
        "disabled-owner",
        source.get(),
        false,
    )
    .await?;
    insert_token(fixture.pool.connection(), 701, source.get()).await?;
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO tokens (user_id, key_hash, key_prefix, name, status, group_id) VALUES (701, ?, 'sk-af-inherited', 'inherited', 1, NULL)",
            ["22".repeat(32).into()],
        ))
        .await?;
    fixture
        .pool
        .connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE users SET status = 2 WHERE id = 701;".to_owned(),
        ))
        .await?;
    fixture
        .pool
        .connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE tokens SET status = 2 WHERE user_id = 701 AND group_id IS NOT NULL;".to_owned(),
        ))
        .await?;
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE authentication_settings SET registration_default_group_id = ? WHERE id = 1",
            [source.get().into()],
        ))
        .await?;
    let settings_before = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(
        fixture.repository.delete(source).await.unwrap_err(),
        AdminGroupRepositoryError::InUse
    );
    assert_eq!(
        fixture
            .repository
            .delete_with_replacement(source, Some(fixture.default_id))
            .await?,
        AdminGroupDeleteOutcome::Deleted
    );
    let owner = users::Entity::find_by_id(701)
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(owner.default_group_id, fixture.default_id.get());
    assert_eq!(owner.status, 2);
    assert!(owner.deleted_at.is_none());
    let key = tokens::Entity::find()
        .filter(tokens::Column::UserId.eq(701_i64))
        .filter(tokens::Column::GroupId.eq(source.get()))
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(key.group_id, Some(source.get()));
    assert_eq!(key.status, 2);
    assert!(
        key.deleted_at.is_some(),
        "direct keys cannot fall back to the new default group"
    );
    let inherited_key = tokens::Entity::find()
        .filter(tokens::Column::UserId.eq(701_i64))
        .filter(tokens::Column::GroupId.is_null())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert!(inherited_key.deleted_at.is_none());
    assert_eq!(inherited_key.status, 1);
    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(
        settings.registration_default_group_id,
        Some(fixture.default_id.get())
    );
    assert_eq!(settings.version, settings_before.version + 1);
    fixture
        .pool
        .connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE users SET status = 1 WHERE id = 701;".to_owned(),
        ))
        .await?;
    let auth = crate::TokenAuthRepository::new(fixture.pool.clone(), Duration::from_secs(2))?;
    let client_ip = af_domain::TrustedClientIp::new("192.0.2.10".parse()?);
    assert_eq!(
        auth.lookup(&crate::TokenAuthLookup::new(&"11".repeat(32))?, client_ip)
            .await?,
        crate::TokenAuthLookupOutcome::Rejected
    );
    let inherited = auth
        .lookup(&crate::TokenAuthLookup::new(&"22".repeat(32))?, client_ip)
        .await?;
    let crate::TokenAuthLookupOutcome::Authenticated { principal, .. } = inherited else {
        panic!("inherited keys should use the explicitly selected replacement group")
    };
    assert_eq!(principal.group_id(), fixture.default_id);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_group_replacements_roll_back_the_source_and_key_state()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let source = fixture
        .repository
        .create(write_record("source", "Source", None, false))
        .await?
        .group_id();
    let deleted = fixture
        .repository
        .create(write_record(
            "deleted-target",
            "Deleted target",
            None,
            false,
        ))
        .await?
        .group_id();
    fixture.repository.delete(deleted).await?;
    let merchant_group = fixture
        .repository
        .create(write_record(
            "merchant-target",
            "Merchant target",
            None,
            false,
        ))
        .await?
        .group_id();
    insert_pool(fixture.pool.connection(), merchant_group.get()).await?;
    insert_user(
        fixture.pool.connection(),
        702,
        "source-owner",
        source.get(),
        false,
    )
    .await?;
    insert_token(fixture.pool.connection(), 702, source.get()).await?;
    for target in [source, GroupId::new(999_999)?, deleted, merchant_group] {
        assert_eq!(
            fixture
                .repository
                .delete_with_replacement(source, Some(target))
                .await
                .unwrap_err(),
            AdminGroupRepositoryError::InvalidReference
        );
        let source_row = groups::Entity::find_by_id(source.get())
            .one(fixture.pool.connection())
            .await?
            .unwrap();
        assert!(source_row.deleted_at.is_none());
        let owner = users::Entity::find_by_id(702)
            .one(fixture.pool.connection())
            .await?
            .unwrap();
        assert_eq!(owner.default_group_id, source.get());
        let key = tokens::Entity::find()
            .filter(tokens::Column::UserId.eq(702_i64))
            .one(fixture.pool.connection())
            .await?
            .unwrap();
        assert!(key.deleted_at.is_none());
    }
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_pool_group_requires_pool_deletion_first() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let group = fixture
        .repository
        .create(write_record(
            "merchant-source",
            "Merchant source",
            None,
            false,
        ))
        .await?
        .group_id();
    insert_pool(fixture.pool.connection(), group.get()).await?;
    for replacement in [None, Some(fixture.default_id)] {
        assert_eq!(
            fixture
                .repository
                .delete_with_replacement(group, replacement)
                .await
                .unwrap_err(),
            AdminGroupRepositoryError::InUse
        );
    }
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE merchant_pools SET deleted_at = CURRENT_TIMESTAMP WHERE group_id = ?",
            [group.get().into()],
        ))
        .await?;
    assert_eq!(
        fixture.repository.delete(group).await?,
        AdminGroupDeleteOutcome::Deleted
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn reciprocal_group_replacements_preserve_one_live_target_and_all_users()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture
        .repository
        .create(write_record("first-source", "First source", None, false))
        .await?
        .group_id();
    let second = fixture
        .repository
        .create(write_record("second-source", "Second source", None, false))
        .await?
        .group_id();
    insert_user(
        fixture.pool.connection(),
        711,
        "first-owner",
        first.get(),
        false,
    )
    .await?;
    insert_user(
        fixture.pool.connection(),
        712,
        "second-owner",
        second.get(),
        false,
    )
    .await?;
    let (first_result, second_result) = tokio::join!(
        fixture
            .repository
            .delete_with_replacement(first, Some(second)),
        fixture
            .repository
            .delete_with_replacement(second, Some(first)),
    );
    let surviving = match (first_result, second_result) {
        (
            Ok(AdminGroupDeleteOutcome::Deleted),
            Err(AdminGroupRepositoryError::InvalidReference),
        ) => second,
        (
            Err(AdminGroupRepositoryError::InvalidReference),
            Ok(AdminGroupDeleteOutcome::Deleted),
        ) => first,
        other => panic!("reciprocal deletion must retain one valid target: {other:?}"),
    };
    assert!(
        groups::Entity::find_by_id(surviving.get())
            .one(fixture.pool.connection())
            .await?
            .unwrap()
            .deleted_at
            .is_none()
    );
    for id in [711_i64, 712] {
        let user = users::Entity::find_by_id(id)
            .one(fixture.pool.connection())
            .await?
            .unwrap();
        assert_eq!(user.default_group_id, surviving.get());
        assert!(user.deleted_at.is_none());
    }
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn group_binding_write_guard_serializes_deletion_without_partial_migration()
-> Result<(), Box<dyn Error>> {
    let file = std::env::temp_dir().join(format!(
        "anyflows-group-deletion-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut sqlite_path = file.to_string_lossy().replace('\\', "/");
    if !sqlite_path.starts_with('/') {
        sqlite_path.insert(0, '/');
    }
    let options = DatabaseOptions::new(format!("sqlite://{sqlite_path}?mode=rwc"))?
        .with_pool_options(PoolOptions {
            max_connections: 2,
            min_connections: 2,
            ..PoolOptions::default()
        });
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let repository = AdminGroupRepository::new(pool.clone(), Duration::from_secs(2))?;
    let source = repository
        .create(write_record(
            "concurrent-source",
            "Concurrent source",
            None,
            false,
        ))
        .await?
        .group_id();
    let replacement = repository
        .create(write_record(
            "concurrent-target",
            "Concurrent target",
            None,
            false,
        ))
        .await?
        .group_id();
    let transaction = pool.connection().begin().await?;
    assert!(crate::group_key_access::lock_active_group(&transaction, source).await?);
    transaction
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, role, status, default_group_id, aff_code, settings) VALUES (703, 'concurrent-owner', 0, 1, ?, 'concurrent-aff', '{}')",
            [source.get().into()],
        ))
        .await?;
    let bounded = AdminGroupRepository::new(pool.clone(), Duration::from_millis(200))?;
    let outcome = bounded
        .delete_with_replacement(source, Some(replacement))
        .await;
    assert!(matches!(
        outcome,
        Err(AdminGroupRepositoryError::Timeout | AdminGroupRepositoryError::Query)
    ));
    transaction.commit().await?;
    let source_row = groups::Entity::find_by_id(source.get())
        .one(pool.connection())
        .await?
        .unwrap();
    assert!(source_row.deleted_at.is_none());
    repository
        .delete_with_replacement(source, Some(replacement))
        .await?;
    let owner = users::Entity::find_by_id(703)
        .one(pool.connection())
        .await?
        .unwrap();
    assert_eq!(owner.default_group_id, replacement.get());
    pool.close().await?;
    std::fs::remove_file(file)?;
    Ok(())
}

async fn insert_pool(
    connection: &sea_orm::DatabaseConnection,
    group_id: i64,
) -> Result<(), sea_orm::DbErr> {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO merchant_pools (group_id, name, description, commission_bps, min_ratio_micros, max_ratio_micros, settlement_delay_secs) VALUES (?, 'Group deletion fixture', '', 0, 100000, 10000000, 86400)",
            [group_id.into()],
        ))
        .await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminGroupRepository,
    default_id: GroupId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    pool.connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            [
                1_i64.into(),
                "default".into(),
                "Default".into(),
                "{}".into(),
            ],
        ))
        .await?;
    Ok(Fixture {
        repository: AdminGroupRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        default_id: GroupId::new(1)?,
    })
}

fn write_record(
    name: &str,
    display_name: &str,
    fallback_group_id: Option<GroupId>,
    with_limits: bool,
) -> AdminGroupWriteRecord {
    AdminGroupWriteRecord::new(
        name.to_owned(),
        display_name.to_owned(),
        if with_limits { 1_250_000 } else { 1_000_000 },
        with_limits.then(|| AdminGroupPeakWriteRecord::new(1_500_000, 8 * 3_600, 20 * 3_600)),
        with_limits,
        with_limits.then_some(10_000),
        with_limits.then_some(60_000),
        with_limits.then_some(200_000),
        with_limits.then_some(120),
        fallback_group_id,
        if with_limits {
            serde_json::json!({"claude_code_only": true})
        } else {
            serde_json::json!({})
        },
    )
}

async fn insert_user(
    connection: &sea_orm::DatabaseConnection,
    id: i64,
    username: &str,
    group_id: i64,
    deleted: bool,
) -> Result<(), sea_orm::DbErr> {
    let deleted_at = deleted.then_some("2026-01-01T00:00:00Z");
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, settings, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                id.into(),
                username.into(),
                0_i16.into(),
                1_i16.into(),
                group_id.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                format!("{username}-aff").into(),
                "{}".into(),
                deleted_at.into(),
            ],
        ))
        .await?;
    Ok(())
}

async fn insert_token(
    connection: &sea_orm::DatabaseConnection,
    user_id: i64,
    group_id: i64,
) -> Result<(), sea_orm::DbErr> {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO tokens (user_id, key_hash, key_prefix, name, group_id) VALUES (?, ?, ?, ?, ?)",
            [
                user_id.into(),
                "11".repeat(32).into(),
                "sk-af-token".into(),
                "protected-token".into(),
                group_id.into(),
            ],
        ))
        .await?;
    Ok(())
}

async fn insert_runtime_relations(
    connection: &sea_orm::DatabaseConnection,
    target_group_id: i64,
    default_group_id: i64,
) -> Result<(), sea_orm::DbErr> {
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO channels (id, name, type, protocol, status, weight, priority, auto_ban, model_mapping, param_override, header_override, used_quota, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                601_i64.into(),
                "test-channel".into(),
                "openai".into(),
                "openai_chat".into(),
                1_i16.into(),
                0_i32.into(),
                0_i32.into(),
                false.into(),
                "{}".into(),
                "{}".into(),
                "{}".into(),
                0_i64.into(),
                "{}".into(),
            ],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
            [601_i64.into(), "group-delete-model".into()],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
            [601_i64.into(), target_group_id.into()],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO abilities (group_id, model, channel_id, enabled, priority, weight) VALUES (?, ?, ?, ?, ?, ?)",
            [
                target_group_id.into(),
                "group-delete-model".into(),
                601_i64.into(),
                true.into(),
                0_i32.into(),
                0_i32.into(),
            ],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO group_model_ratios (source_group_id, target_group_id, ratio_micros) VALUES (?, ?, ?), (?, ?, ?)",
            [
                target_group_id.into(),
                default_group_id.into(),
                1_100_000_i64.into(),
                default_group_id.into(),
                target_group_id.into(),
                1_200_000_i64.into(),
            ],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO group_group_grants (target_group_id, source_group_id) VALUES (?, ?), (?, ?)",
            [
                target_group_id.into(),
                default_group_id.into(),
                default_group_id.into(),
                target_group_id.into(),
            ],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO group_model_ratio_overrides (source_group_id, target_group_id, model, ratio_micros) VALUES (?, ?, ?, ?), (?, ?, ?, ?)",
            [
                target_group_id.into(),
                default_group_id.into(),
                "delete-model".into(),
                1_300_000_i64.into(),
                default_group_id.into(),
                target_group_id.into(),
                "delete-model".into(),
                1_400_000_i64.into(),
            ],
        ))
        .await?;
    Ok(())
}
