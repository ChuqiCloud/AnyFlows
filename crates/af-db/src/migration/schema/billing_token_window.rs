use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{billing_reservations, billing_token_window_reservations};

use super::{table, timestamp};

/// 创建请求预留与令牌 5h/1d/7d 窗口之间的一对一起点快照。
pub(in crate::migration) async fn create_billing_token_window_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
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
        .check(
            Expr::col(billing_token_window_reservations::Column::Window5hStart).lte(Expr::col(
                billing_token_window_reservations::Column::CreatedAt,
            )),
        )
        .check(
            Expr::col(billing_token_window_reservations::Column::Window1dStart).lte(Expr::col(
                billing_token_window_reservations::Column::CreatedAt,
            )),
        )
        .check(
            Expr::col(billing_token_window_reservations::Column::Window7dStart).lte(Expr::col(
                billing_token_window_reservations::Column::CreatedAt,
            )),
        )
        .check(
            Expr::col(billing_token_window_reservations::Column::UpdatedAt).gte(Expr::col(
                billing_token_window_reservations::Column::CreatedAt,
            )),
        );
    manager.create_table(statement).await
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
