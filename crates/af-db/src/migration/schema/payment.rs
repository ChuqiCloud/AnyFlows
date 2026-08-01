use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{topup_orders, topup_payment_events, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建充值订单与支付 webhook 审计表；本迁移只固化状态机和幂等边界，不接任何 Provider。
pub(in crate::migration) async fn create_topup_payment_audit(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_orders(manager).await?;
    create_payment_events(manager).await?;
    create_indexes(manager).await
}

async fn create_orders(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, topup_orders::Entity);
    statement
        .col(auto_id(topup_orders::Column::Id))
        .col(
            ColumnDef::new(topup_orders::Column::OrderKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_orders::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_orders::Column::Provider)
                .string_len(64)
                .not_null()
                .check(Expr::col(topup_orders::Column::Provider).ne("")),
        )
        .col(ColumnDef::new(topup_orders::Column::ProviderOrderId).string_len(128))
        .col(ColumnDef::new(topup_orders::Column::TradeNo).string_len(128))
        .col(
            ColumnDef::new(topup_orders::Column::Status)
                .small_integer()
                .not_null()
                .check(Expr::col(topup_orders::Column::Status).is_in([1_i16, 2, 3, 4, 5, 6])),
        )
        .col(
            ColumnDef::new(topup_orders::Column::AmountMinor)
                .big_integer()
                .not_null()
                .check(Expr::col(topup_orders::Column::AmountMinor).gt(0_i64)),
        )
        .col(
            ColumnDef::new(topup_orders::Column::Currency)
                .char_len(3)
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_orders::Column::QuotaAmount)
                .big_integer()
                .not_null()
                .check(Expr::col(topup_orders::Column::QuotaAmount).gt(0_i64)),
        )
        .col(
            ColumnDef::new(topup_orders::Column::IdempotencyKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_orders::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(topup_orders::Column::Version).gte(1_i64)),
        )
        .col(nullable_timestamp(manager, topup_orders::Column::ExpiresAt))
        .col(nullable_timestamp(manager, topup_orders::Column::PaidAt))
        .col(nullable_timestamp(manager, topup_orders::Column::ClosedAt))
        .col(timestamp(manager, topup_orders::Column::CreatedAt))
        .col(timestamp(manager, topup_orders::Column::UpdatedAt))
        .check(hex_check(
            manager.get_database_backend(),
            "order_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "idempotency_key",
            32,
            true,
        ))
        .check(currency_check(manager.get_database_backend()))
        .check(order_state_shape())
        .check(
            Expr::col(topup_orders::Column::ExpiresAt)
                .is_null()
                .or(Expr::col(topup_orders::Column::ExpiresAt)
                    .gt(Expr::col(topup_orders::Column::CreatedAt))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_topup_orders_user")
                .from(topup_orders::Entity, topup_orders::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

async fn create_payment_events(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, topup_payment_events::Entity);
    statement
        .col(auto_id(topup_payment_events::Column::Id))
        .col(
            ColumnDef::new(topup_payment_events::Column::EventKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_payment_events::Column::OrderId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_payment_events::Column::Provider)
                .string_len(64)
                .not_null()
                .check(Expr::col(topup_payment_events::Column::Provider).ne("")),
        )
        .col(
            ColumnDef::new(topup_payment_events::Column::ProviderEventId)
                .string_len(128)
                .not_null()
                .check(Expr::col(topup_payment_events::Column::ProviderEventId).ne("")),
        )
        .col(ColumnDef::new(topup_payment_events::Column::TradeNo).string_len(128))
        .col(
            ColumnDef::new(topup_payment_events::Column::EventType)
                .small_integer()
                .not_null()
                .check(Expr::col(topup_payment_events::Column::EventType).is_in([1_i16, 2, 3])),
        )
        .col(
            ColumnDef::new(topup_payment_events::Column::SignatureKeyFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(topup_payment_events::Column::PayloadSha256)
                .char_len(64)
                .not_null(),
        )
        .col(timestamp(manager, topup_payment_events::Column::ReceivedAt))
        .col(nullable_timestamp(
            manager,
            topup_payment_events::Column::ProcessedAt,
        ))
        .col(timestamp(manager, topup_payment_events::Column::CreatedAt))
        .check(hex_check(
            manager.get_database_backend(),
            "event_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "signature_key_fingerprint",
            64,
            false,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "payload_sha256",
            64,
            false,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_topup_payment_events_order")
                .from(
                    topup_payment_events::Entity,
                    topup_payment_events::Column::OrderId,
                )
                .to(topup_orders::Entity, topup_orders::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

fn order_state_shape() -> SimpleExpr {
    Expr::col(topup_orders::Column::Status)
        .is_in([1_i16, 2])
        .and(Expr::col(topup_orders::Column::PaidAt).is_null())
        .and(Expr::col(topup_orders::Column::ClosedAt).is_null())
        .or(Expr::col(topup_orders::Column::Status)
            .eq(3_i16)
            .and(Expr::col(topup_orders::Column::PaidAt).is_not_null())
            .and(Expr::col(topup_orders::Column::ClosedAt).is_null()))
        .or(Expr::col(topup_orders::Column::Status)
            .is_in([4_i16, 5, 6])
            .and(Expr::col(topup_orders::Column::PaidAt).is_null())
            .and(Expr::col(topup_orders::Column::ClosedAt).is_not_null()))
}

fn currency_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""currency" ~ '^[A-Z]{3}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`currency`) = 3 AND `currency` REGEXP '^[A-Z]{3}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("currency") = 3 AND "currency" NOT GLOB '*[^A-Z]*'"#)
        }
    }
}

fn hex_check(
    database_backend: DbBackend,
    column: &str,
    length: usize,
    non_zero: bool,
) -> SimpleExpr {
    let zero_clause = if non_zero {
        match database_backend {
            DbBackend::Postgres => format!(r#" AND "{column}" <> '{}'"#, "0".repeat(length)),
            DbBackend::MySql => format!(" AND `{column}` <> '{}'", "0".repeat(length)),
            DbBackend::Sqlite => format!(r#" AND "{column}" <> '{}'"#, "0".repeat(length)),
        }
    } else {
        String::new()
    };
    let sql = match database_backend {
        DbBackend::Postgres => format!(r#""{column}" ~ '^[0-9a-f]{{{length}}}$'{zero_clause}"#),
        DbBackend::MySql => format!(
            "CHAR_LENGTH(`{column}`) = {length} AND `{column}` REGEXP '^[0-9a-f]{{{length}}}$'{zero_clause}"
        ),
        DbBackend::Sqlite => format!(
            r#"length("{column}") = {length} AND "{column}" NOT GLOB '*[^0-9a-f]*'{zero_clause}"#
        ),
    };
    Expr::cust(sql)
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_topup_orders_order_key")
            .table(topup_orders::Entity)
            .col(topup_orders::Column::OrderKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_topup_orders_idempotency_key")
            .table(topup_orders::Entity)
            .col(topup_orders::Column::IdempotencyKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_topup_orders_provider_trade_no")
            .table(topup_orders::Entity)
            .col(topup_orders::Column::Provider)
            .col(topup_orders::Column::TradeNo)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_topup_orders_user_created")
            .table(topup_orders::Entity)
            .col(topup_orders::Column::UserId)
            .col(topup_orders::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_topup_orders_status_expires")
            .table(topup_orders::Entity)
            .col(topup_orders::Column::Status)
            .col(topup_orders::Column::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("uq_topup_payment_events_event_key")
            .table(topup_payment_events::Entity)
            .col(topup_payment_events::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_topup_payment_events_provider_event")
            .table(topup_payment_events::Entity)
            .col(topup_payment_events::Column::Provider)
            .col(topup_payment_events::Column::ProviderEventId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_topup_payment_events_order_created")
            .table(topup_payment_events::Entity)
            .col(topup_payment_events::Column::OrderId)
            .col(topup_payment_events::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_hex_checks_use_collation_sensitive_regex_without_binary_modifier() {
        let rendered = Query::select()
            .expr(hex_check(DbBackend::MySql, "event_key", 32, true))
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert_eq!(
            rendered,
            "SELECT CHAR_LENGTH(`event_key`) = 32 AND `event_key` REGEXP '^[0-9a-f]{32}$' AND `event_key` <> '00000000000000000000000000000000'"
        );
    }
}
