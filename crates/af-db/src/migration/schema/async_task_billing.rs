use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{async_task_billings, async_task_submission_claims, groups, users};

use super::{table, timestamp};

/// 创建异步任务冻结、结算和失败释放的持久化状态机。
pub(in crate::migration) async fn create_async_task_billings(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, async_task_billings::Entity);
    statement
        .col(
            ColumnDef::new(async_task_billings::Column::TaskKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::ReservationKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::TargetGroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::State)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(async_task_billings::Column::State).is_in([1_i16, 2, 3, 4, 5, 6, 7]),
                ),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::PriceCardVersion)
                .small_integer()
                .not_null()
                .check(Expr::col(async_task_billings::Column::PriceCardVersion).gte(1_i16)),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::BillingResolution)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(async_task_billings::Column::BillingResolution).is_in([1_i16, 2, 3]),
                ),
        )
        .col(non_negative(async_task_billings::Column::GroupRatioMicros))
        .col(non_negative(
            async_task_billings::Column::GroupModelRatioMicros,
        ))
        .col(non_negative(async_task_billings::Column::PeakRatioMicros))
        .col(positive(async_task_billings::Column::UpperBound))
        .col(optional_positive(async_task_billings::Column::RateMicrousd))
        .col(optional_non_negative(
            async_task_billings::Column::FallbackQuota,
        ))
        .col(optional_non_negative(
            async_task_billings::Column::ActualQuota,
        ))
        .col(
            ColumnDef::new(async_task_billings::Column::ActualDurationSeconds)
                .small_integer()
                .check(
                    Expr::col(async_task_billings::Column::ActualDurationSeconds)
                        .is_null()
                        .or(
                            Expr::col(async_task_billings::Column::ActualDurationSeconds)
                                .between(1_i16, 15_i16),
                        ),
                ),
        )
        .col(
            ColumnDef::new(async_task_billings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(async_task_billings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, async_task_billings::Column::CreatedAt))
        .col(timestamp(manager, async_task_billings::Column::UpdatedAt))
        .check(hex_check(manager.get_database_backend(), "reservation_key"))
        .check(
            Expr::col(async_task_billings::Column::UpdatedAt)
                .gte(Expr::col(async_task_billings::Column::CreatedAt)),
        )
        .check(state_shape())
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_billings_claim")
                .from(
                    async_task_billings::Entity,
                    async_task_billings::Column::TaskKey,
                )
                .to(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::TaskKey,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_billings_user")
                .from(
                    async_task_billings::Entity,
                    async_task_billings::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_billings_target_group")
                .from(
                    async_task_billings::Entity,
                    async_task_billings::Column::TargetGroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    // MySQL 8.4 禁止 CHECK 引用带级联更新的外键列；实体与仓储仍校验任务键格式。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(hex_check(manager.get_database_backend(), "task_key"));
    }
    manager.create_table(statement).await?;
    for index in [
        Index::create()
            .name("uq_async_task_billings_reservation")
            .table(async_task_billings::Entity)
            .col(async_task_billings::Column::ReservationKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_async_task_billings_state_updated")
            .table(async_task_billings::Entity)
            .col(async_task_billings::Column::State)
            .col(async_task_billings::Column::UpdatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn non_negative<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .not_null()
        .check(Expr::col(column).gte(0_i64));
    definition
}

fn positive<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .not_null()
        .check(Expr::col(column).gt(0_i64));
    definition
}

fn optional_positive<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .check(Expr::col(column).is_null().or(Expr::col(column).gt(0_i64)));
    definition
}

fn optional_non_negative<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .check(Expr::col(column).is_null().or(Expr::col(column).gte(0_i64)));
    definition
}

fn state_shape() -> SimpleExpr {
    let no_submission = Expr::col(async_task_billings::Column::RateMicrousd)
        .is_null()
        .and(Expr::col(async_task_billings::Column::FallbackQuota).is_null());
    let submission = Expr::col(async_task_billings::Column::RateMicrousd)
        .is_not_null()
        .and(Expr::col(async_task_billings::Column::FallbackQuota).is_not_null());
    let no_actual = Expr::col(async_task_billings::Column::ActualQuota)
        .is_null()
        .and(Expr::col(async_task_billings::Column::ActualDurationSeconds).is_null());
    let actual = Expr::col(async_task_billings::Column::ActualQuota)
        .is_not_null()
        .and(Expr::col(async_task_billings::Column::ActualDurationSeconds).is_not_null());
    Expr::col(async_task_billings::Column::State)
        .is_in([1_i16, 2])
        .and(no_submission.clone())
        .and(no_actual.clone())
        .or(Expr::col(async_task_billings::Column::State)
            .eq(3_i16)
            .and(submission.clone())
            .and(no_actual.clone()))
        .or(Expr::col(async_task_billings::Column::State)
            .is_in([4_i16, 5])
            .and(submission.clone())
            .and(actual))
        .or(Expr::col(async_task_billings::Column::State)
            .is_in([6_i16, 7])
            .and(no_actual)
            .and(no_submission.or(submission)))
}

fn hex_check(backend: DbBackend, column: &str) -> SimpleExpr {
    match backend {
        DbBackend::Postgres => Expr::cust(format!(
            r#""{column}" ~ '^[0-9a-f]{{32}}$' AND "{column}" <> '00000000000000000000000000000000'"#
        )),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{column}`) = 32 AND `{column}` REGEXP '^[0-9a-f]{{32}}$' AND `{column}` <> '00000000000000000000000000000000'"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            r#"length("{column}") = 32 AND "{column}" NOT GLOB '*[^0-9a-f]*' AND "{column}" <> '00000000000000000000000000000000'"#
        )),
    }
}
