use std::{error::Error, time::Duration};

use af_domain::GroupId;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait, QueryFilter, Statement,
    sea_query::Expr,
};

use super::{
    AdminGroupDeleteOutcome, AdminGroupMutationOutcome, AdminGroupPeakWriteRecord,
    AdminGroupRepository, AdminGroupRepositoryError, AdminGroupWriteRecord, DatabaseOptions,
    MigrationOptions,
};
use crate::entity::{
    abilities, channel_groups, group_model_ratios, groups, scheduler_outbox_events,
};

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

    fixture.pool.close().await?;
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
    Ok(())
}
