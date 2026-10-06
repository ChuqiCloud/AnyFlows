use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{billing_reservations, billing_token_window_reservations};

use super::{table, timestamp};

/// 创建请求预留与令牌 5h/1d/7d 窗口之间的一对一起点快照。
pub(in crate::migration) async fn create_billing_token_window_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut statement = table(manager, billing_token_window_reservations::Entity);
    statement
        .col(
            ColumnDef::new(billing_token_window_reservations::Column::IdempotencyKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(timestamp(
            manager,
            billing_token_window_reservations::Column::Window5hStart,
        ))
        .col(timestamp(
            manager,
            billing_token_window_reservations::Column::Window1dStart,
        ))
        .col(timestamp(
            manager,
            billing_token_window_reservations::Column::Window7dStart,
        ))
        .col(
            ColumnDef::new(billing_token_window_reservations::Column::ReservedQuota)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(billing_token_window_reservations::Column::ReservedQuota).gt(0_i64),
                ),
        )
        .col(timestamp(
            manager,
            billing_token_window_reservations::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            billing_token_window_reservations::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_token_window_reservations_reservation")
                .from(
                    billing_token_window_reservations::Entity,
                    billing_token_window_reservations::Column::IdempotencyKey,
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
            billing_token_window_reservations::Column::Window5hStart,
            billing_token_window_reservations::Column::CreatedAt,
        ))
        .check(timestamp_lte_check(
            backend,
            billing_token_window_reservations::Column::Window1dStart,
            billing_token_window_reservations::Column::CreatedAt,
        ))
        .check(timestamp_lte_check(
            backend,
            billing_token_window_reservations::Column::Window7dStart,
            billing_token_window_reservations::Column::CreatedAt,
        ))
        .check(timestamp_lte_check(
            backend,
            billing_token_window_reservations::Column::CreatedAt,
            billing_token_window_reservations::Column::UpdatedAt,
        ));
    manager.create_table(statement).await
}

/// SQLite 的 RFC3339 时间文本含可选小数秒，跨列比较前必须转换为时间值。
fn timestamp_lte_check(
    backend: DbBackend,
    left: billing_token_window_reservations::Column,
    right: billing_token_window_reservations::Column,
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

/// 为每次窗口准入聚合提供令牌与状态联合索引。
pub(in crate::migration) async fn create_billing_token_window_reservations_index(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_billing_reservations_token_status")
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::TokenId)
                .col(billing_reservations::Column::Status)
                .to_owned(),
        )
        .await
}

/// 为升级前仍在途的普通请求补兼容快照，使其可以安全结算但不占用新窗口。
pub(in crate::migration) async fn backfill_active_billing_token_window_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            r#"INSERT INTO `billing_token_window_reservations`
(`idempotency_key`, `window_5h_start`, `window_1d_start`, `window_7d_start`, `reserved_quota`, `created_at`, `updated_at`)
SELECT `idempotency_key`, `created_at`, `created_at`, `created_at`, `reserved_quota`, `created_at`, `created_at`
FROM `billing_reservations`
WHERE `reservation_kind` = 1 AND `status` IN (1, 2)"#
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            r#"INSERT INTO "billing_token_window_reservations"
("idempotency_key", "window_5h_start", "window_1d_start", "window_7d_start", "reserved_quota", "created_at", "updated_at")
SELECT "idempotency_key", "created_at", "created_at", "created_at", "reserved_quota", "created_at", "created_at"
FROM "billing_reservations"
WHERE "reservation_kind" = 1 AND "status" IN (1, 2)"#
        }
    };
    manager.get_connection().execute_unprepared(sql).await?;
    Ok(())
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
                billing_token_window_reservations::Column::Window5hStart,
                billing_token_window_reservations::Column::CreatedAt,
            ))
            .to_owned();
        let sql = query.to_string(SqliteQueryBuilder);

        assert!(sql.contains("julianday(\"window_5h_start\")"));
        assert!(sql.contains("julianday(\"created_at\")"));
        assert_eq!(sql.matches("IS NOT NULL").count(), 2);
    }
}
