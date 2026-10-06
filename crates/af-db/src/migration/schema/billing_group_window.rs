use af_domain::{SubscriptionCycle, SubscriptionWindow};
use sea_orm::{DbBackend, entity::prelude::TimeDateTimeWithTimeZone};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{billing_group_window_reservations, billing_reservations, groups};

use super::{nullable_timestamp, table, timestamp};

/// 为分组补充 UTC 日、周、月窗口的已结算用量与当前起点。
pub(in crate::migration) async fn add_group_window_state(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    for column in [
        groups::Column::DailyUsage,
        groups::Column::WeeklyUsage,
        groups::Column::MonthlyUsage,
    ] {
        manager
            .alter_table(
                Table::alter()
                    .table(groups::Entity)
                    .add_column(
                        ColumnDef::new(column)
                            .big_integer()
                            .not_null()
                            .default(0_i64)
                            .check(Expr::col(column).gte(0_i64)),
                    )
                    .to_owned(),
            )
            .await?;
    }
    for column in [
        groups::Column::DailyWindowStart,
        groups::Column::WeeklyWindowStart,
        groups::Column::MonthlyWindowStart,
    ] {
        manager
            .alter_table(
                Table::alter()
                    .table(groups::Entity)
                    .add_column(group_window_start(manager, column))
                    .to_owned(),
            )
            .await?;
    }
    backfill_group_window_starts(manager).await
}

/// 使用三方言均可接受的常量初值新增窗口列，随后再统一回填真实 UTC 起点。
fn group_window_start(manager: &SchemaManager<'_>, column: groups::Column) -> ColumnDef {
    let mut definition = nullable_timestamp(manager, column);
    let canonical_anchor = match manager.get_database_backend() {
        DbBackend::MySql => "1970-06-01 00:00:00.000000",
        DbBackend::Postgres => "1970-06-01 00:00:00+00",
        DbBackend::Sqlite => "1970-06-01 00:00:00",
    };
    // SQLite 的 ADD COLUMN 禁止 CURRENT_TIMESTAMP 等非常量默认值。
    // 1970-06-01 同时是月初和周一，可作为日、周、月三种窗口的合法旧锚点。
    definition.not_null().default(canonical_anchor);
    definition
}

/// 将现有分组统一放入迁移时刻所属的 UTC 日历窗口。
async fn backfill_group_window_starts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let now_seconds = <u64 as std::convert::TryFrom<i64>>::try_from(now.unix_timestamp())
        .map_err(|_| DbErr::Migration("分组额度窗口当前时间无效".to_owned()))?;
    let cycles = [
        SubscriptionCycle::Daily,
        SubscriptionCycle::Weekly,
        SubscriptionCycle::Monthly,
    ];
    let mut starts = [TimeDateTimeWithTimeZone::UNIX_EPOCH; 3];
    for (index, cycle) in cycles.into_iter().enumerate() {
        let window = SubscriptionWindow::initial(cycle, now_seconds)
            .map_err(|_| DbErr::Migration("分组额度窗口边界无效".to_owned()))?;
        starts[index] = TimeDateTimeWithTimeZone::from_unix_timestamp(
            <i64 as std::convert::TryFrom<u64>>::try_from(window.started_at())
                .map_err(|_| DbErr::Migration("分组额度窗口边界无效".to_owned()))?,
        )
        .map_err(|_| DbErr::Migration("分组额度窗口边界无效".to_owned()))?;
    }
    let statement = Query::update()
        .table(groups::Entity)
        .value(groups::Column::DailyWindowStart, starts[0])
        .value(groups::Column::WeeklyWindowStart, starts[1])
        .value(groups::Column::MonthlyWindowStart, starts[2])
        .to_owned();
    manager
        .get_connection()
        .execute(manager.get_database_backend().build(&statement))
        .await?;
    Ok(())
}

/// 创建请求预留与分组三段日历窗口之间的一对一起点快照。
pub(in crate::migration) async fn create_billing_group_window_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut statement = table(manager, billing_group_window_reservations::Entity);
    statement
        .col(
            ColumnDef::new(billing_group_window_reservations::Column::IdempotencyKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(timestamp(
            manager,
            billing_group_window_reservations::Column::DailyWindowStart,
        ))
        .col(timestamp(
            manager,
            billing_group_window_reservations::Column::WeeklyWindowStart,
        ))
        .col(timestamp(
            manager,
            billing_group_window_reservations::Column::MonthlyWindowStart,
        ))
        .col(
            ColumnDef::new(billing_group_window_reservations::Column::ReservedQuota)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(billing_group_window_reservations::Column::ReservedQuota).gt(0_i64),
                ),
        )
        .col(timestamp(
            manager,
            billing_group_window_reservations::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            billing_group_window_reservations::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_group_window_reservations_reservation")
                .from(
                    billing_group_window_reservations::Entity,
                    billing_group_window_reservations::Column::IdempotencyKey,
                )
                .to(
                    billing_reservations::Entity,
                    billing_reservations::Column::IdempotencyKey,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .check(timestamp_lte_check(
            backend,
            billing_group_window_reservations::Column::DailyWindowStart,
            billing_group_window_reservations::Column::CreatedAt,
        ))
        .check(timestamp_lte_check(
            backend,
            billing_group_window_reservations::Column::WeeklyWindowStart,
            billing_group_window_reservations::Column::CreatedAt,
        ))
        .check(timestamp_lte_check(
            backend,
            billing_group_window_reservations::Column::MonthlyWindowStart,
            billing_group_window_reservations::Column::CreatedAt,
        ))
        .check(timestamp_lte_check(
            backend,
            billing_group_window_reservations::Column::CreatedAt,
            billing_group_window_reservations::Column::UpdatedAt,
        ));
    manager.create_table(statement).await
}

/// 为每次窗口准入聚合提供分组与状态联合索引。
pub(in crate::migration) async fn create_billing_group_window_reservations_index(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_billing_reservations_group_status")
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::GroupId)
                .col(billing_reservations::Column::Status)
                .to_owned(),
        )
        .await
}

/// 为升级前仍在途的普通请求补兼容快照，使其可以安全结算但不占用新窗口。
pub(in crate::migration) async fn backfill_active_billing_group_window_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            r#"INSERT INTO `billing_group_window_reservations`
(`idempotency_key`, `daily_window_start`, `weekly_window_start`, `monthly_window_start`, `reserved_quota`, `created_at`, `updated_at`)
SELECT `idempotency_key`, `created_at`, `created_at`, `created_at`, `reserved_quota`, `created_at`, `created_at`
FROM `billing_reservations`
WHERE `reservation_kind` = 1 AND `status` IN (1, 2)"#
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            r#"INSERT INTO "billing_group_window_reservations"
("idempotency_key", "daily_window_start", "weekly_window_start", "monthly_window_start", "reserved_quota", "created_at", "updated_at")
SELECT "idempotency_key", "created_at", "created_at", "created_at", "reserved_quota", "created_at", "created_at"
FROM "billing_reservations"
WHERE "reservation_kind" = 1 AND "status" IN (1, 2)"#
        }
    };
    manager.get_connection().execute_unprepared(sql).await?;
    Ok(())
}

/// SQLite 的 RFC3339 时间文本含可选小数秒，跨列比较前必须转换为时间值。
fn timestamp_lte_check(
    backend: DbBackend,
    left: billing_group_window_reservations::Column,
    right: billing_group_window_reservations::Column,
) -> SimpleExpr {
    if backend != DbBackend::Sqlite {
        return Expr::col(left).lte(Expr::col(right));
    }
    let left: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(left))
        .into();
    let right: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(right))
        .into();
    left.clone()
        .is_not_null()
        .and(right.clone().is_not_null())
        .and(left.lte(right))
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{Query, SqliteQueryBuilder};

    use super::*;

    #[test]
    fn sqlite_timestamp_check_uses_chronological_comparison() {
        let query = Query::select()
            .expr(timestamp_lte_check(
                DbBackend::Sqlite,
                billing_group_window_reservations::Column::DailyWindowStart,
                billing_group_window_reservations::Column::CreatedAt,
            ))
            .to_owned();
        let sql = query.to_string(SqliteQueryBuilder);

        assert!(sql.contains("julianday(\"daily_window_start\")"));
        assert!(sql.contains("julianday(\"created_at\")"));
        assert_eq!(sql.matches("IS NOT NULL").count(), 2);
    }

    #[test]
    fn constant_default_anchor_is_valid_for_every_group_cycle() {
        const JUNE_1_1970: u64 = 151 * 24 * 60 * 60;

        for cycle in [
            SubscriptionCycle::Daily,
            SubscriptionCycle::Weekly,
            SubscriptionCycle::Monthly,
        ] {
            assert_eq!(
                SubscriptionWindow::initial(cycle, JUNE_1_1970)
                    .unwrap()
                    .started_at(),
                JUNE_1_1970
            );
        }
    }
}
