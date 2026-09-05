use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_plan_orders, organization_plan_payment_events, organization_plans, organizations,
    users,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建企业授权订单及支付事件的三方言事实表。
pub(in crate::migration) async fn create_organization_plan_order_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_orders(manager).await?;
    create_payment_events(manager).await?;
    for index in [
        Index::create()
            .name("uq_organization_plan_orders_order_key")
            .table(organization_plan_orders::Entity)
            .col(organization_plan_orders::Column::OrderKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_organization_plan_orders_org_idempotency")
            .table(organization_plan_orders::Entity)
            .col(organization_plan_orders::Column::OrganizationId)
            .col(organization_plan_orders::Column::IdempotencyKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_organization_plan_orders_provider_trade_no")
            .table(organization_plan_orders::Entity)
            .col(organization_plan_orders::Column::Provider)
            .col(organization_plan_orders::Column::TradeNo)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_organization_plan_orders_org_created")
            .table(organization_plan_orders::Entity)
            .col(organization_plan_orders::Column::OrganizationId)
            .col(organization_plan_orders::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_organization_plan_orders_status_expires")
            .table(organization_plan_orders::Entity)
            .col(organization_plan_orders::Column::Status)
            .col(organization_plan_orders::Column::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_organization_plan_orders_due")
            .table(organization_plan_orders::Entity)
            .col(organization_plan_orders::Column::OrganizationId)
            .col(organization_plan_orders::Column::Status)
            .col(organization_plan_orders::Column::EffectiveAt)
            .col(organization_plan_orders::Column::AppliedAt)
            .to_owned(),
        Index::create()
            .name("uq_organization_plan_payment_events_event_key")
            .table(organization_plan_payment_events::Entity)
            .col(organization_plan_payment_events::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_organization_plan_payment_events_provider_event")
            .table(organization_plan_payment_events::Entity)
            .col(organization_plan_payment_events::Column::Provider)
            .col(organization_plan_payment_events::Column::ProviderEventId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_organization_plan_payment_events_order_created")
            .table(organization_plan_payment_events::Entity)
            .col(organization_plan_payment_events::Column::OrderId)
            .col(organization_plan_payment_events::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_orders(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_plan_orders::Entity);
    statement
        .col(auto_id(organization_plan_orders::Column::Id))
        .col(
            ColumnDef::new(organization_plan_orders::Column::OrderKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::PayerUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::PlanId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::PlanKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::PlanVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::Provider)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::Currency)
                .char_len(3)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::AmountMinor)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::Kind)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(nullable_timestamp(
            manager,
            organization_plan_orders::Column::EffectiveAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_plan_orders::Column::AppliedAt,
        ))
        .col(
            ColumnDef::new(organization_plan_orders::Column::Status)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::IdempotencyKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_orders::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(ColumnDef::new(organization_plan_orders::Column::ProviderOrderId).string_len(128))
        .col(ColumnDef::new(organization_plan_orders::Column::TradeNo).string_len(128))
        .col(ColumnDef::new(organization_plan_orders::Column::PaymentMethod).string_len(32))
        .col(nullable_timestamp(
            manager,
            organization_plan_orders::Column::ExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_plan_orders::Column::PaidAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_plan_orders::Column::ClosedAt,
        ))
        .col(timestamp(
            manager,
            organization_plan_orders::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_plan_orders::Column::UpdatedAt,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "order_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "plan_key",
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
        .check(Expr::col(organization_plan_orders::Column::PlanVersion).gt(0_i64))
        .check(Expr::col(organization_plan_orders::Column::AmountMinor).gt(0_i64))
        .check(Expr::col(organization_plan_orders::Column::Kind).is_in([1_i16, 2, 3, 4]))
        .check(Expr::col(organization_plan_orders::Column::Status).is_in([1_i16, 2, 3, 4, 5, 6]))
        .check(Expr::col(organization_plan_orders::Column::Version).gt(0_i64))
        .check(order_state_shape())
        .check(
            Expr::col(organization_plan_orders::Column::ExpiresAt)
                .is_null()
                .or(Expr::col(organization_plan_orders::Column::ExpiresAt)
                    .gt(Expr::col(organization_plan_orders::Column::CreatedAt))),
        )
        .check(
            Expr::col(organization_plan_orders::Column::EffectiveAt)
                .is_null()
                .or(Expr::col(organization_plan_orders::Column::EffectiveAt)
                    .gte(Expr::col(organization_plan_orders::Column::CreatedAt))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_plan_orders_organization")
                .from(
                    organization_plan_orders::Entity,
                    organization_plan_orders::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_plan_orders_payer")
                .from(
                    organization_plan_orders::Entity,
                    organization_plan_orders::Column::PayerUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_plan_orders_plan")
                .from(
                    organization_plan_orders::Entity,
                    organization_plan_orders::Column::PlanId,
                )
                .to(organization_plans::Entity, organization_plans::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

async fn create_payment_events(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_plan_payment_events::Entity);
    statement
        .col(auto_id(organization_plan_payment_events::Column::Id))
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::EventKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::OrderId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::Provider)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::ProviderEventId)
                .string_len(128)
                .not_null(),
        )
        .col(ColumnDef::new(organization_plan_payment_events::Column::TradeNo).string_len(128))
        .col(ColumnDef::new(organization_plan_payment_events::Column::AmountMinor).big_integer())
        .col(ColumnDef::new(organization_plan_payment_events::Column::Currency).char_len(3))
        .col(ColumnDef::new(organization_plan_payment_events::Column::PaymentMethod).string_len(32))
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::EventType)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::SignatureKeyFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_payment_events::Column::PayloadSha256)
                .char_len(64)
                .not_null(),
        )
        .col(timestamp(
            manager,
            organization_plan_payment_events::Column::ReceivedAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_plan_payment_events::Column::ProcessedAt,
        ))
        .col(timestamp(
            manager,
            organization_plan_payment_events::Column::CreatedAt,
        ))
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
        .check(Expr::col(organization_plan_payment_events::Column::EventType).is_in([1_i16, 2, 3]))
        .check(
            Expr::col(organization_plan_payment_events::Column::AmountMinor)
                .is_null()
                .or(Expr::col(organization_plan_payment_events::Column::AmountMinor).gt(0_i64)),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_plan_payment_events_order")
                .from(
                    organization_plan_payment_events::Entity,
                    organization_plan_payment_events::Column::OrderId,
                )
                .to(
                    organization_plan_orders::Entity,
                    organization_plan_orders::Column::Id,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

fn order_state_shape() -> SimpleExpr {
    Expr::col(organization_plan_orders::Column::Status)
        .is_in([1_i16, 2])
        .and(Expr::col(organization_plan_orders::Column::PaidAt).is_null())
        .and(Expr::col(organization_plan_orders::Column::ClosedAt).is_null())
        .or(Expr::col(organization_plan_orders::Column::Status)
            .eq(3_i16)
            .and(Expr::col(organization_plan_orders::Column::PaidAt).is_not_null())
            .and(Expr::col(organization_plan_orders::Column::ClosedAt).is_null()))
        .or(Expr::col(organization_plan_orders::Column::Status)
            .is_in([4_i16, 5, 6])
            .and(Expr::col(organization_plan_orders::Column::PaidAt).is_null())
            .and(Expr::col(organization_plan_orders::Column::ClosedAt).is_not_null()))
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
