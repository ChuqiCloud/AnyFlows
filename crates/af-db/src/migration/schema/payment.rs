use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    payment_settings, subscription_orders, subscription_payment_events, topup_orders,
    topup_payment_events, users,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建固定主键的在线支付 Provider 设置表。
pub(in crate::migration) async fn create_payment_settings(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, payment_settings::Entity);
    statement
        .col(
            ColumnDef::new(payment_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(payment_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(payment_settings::Column::Initialized)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(payment_settings::Column::StripeEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(ColumnDef::new(payment_settings::Column::StripePublishableKey).string_len(512))
        .col(ColumnDef::new(payment_settings::Column::StripeSecretKey).json_binary())
        .col(ColumnDef::new(payment_settings::Column::StripeWebhookSecret).json_binary())
        .col(
            ColumnDef::new(payment_settings::Column::StripeSignatureToleranceSeconds)
                .integer()
                .not_null()
                .default(300_i32)
                .check(
                    Expr::col(payment_settings::Column::StripeSignatureToleranceSeconds)
                        .between(30_i32, 900_i32),
                ),
        )
        .col(
            ColumnDef::new(payment_settings::Column::EpayEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(ColumnDef::new(payment_settings::Column::EpayGatewayUrl).string_len(2048))
        .col(ColumnDef::new(payment_settings::Column::EpayMerchantId).string_len(128))
        .col(ColumnDef::new(payment_settings::Column::EpayMerchantKey).json_binary())
        .col(
            ColumnDef::new(payment_settings::Column::EpayAlipayEnabled)
                .boolean()
                .not_null()
                .default(true),
        )
        .col(
            ColumnDef::new(payment_settings::Column::EpayWxpayEnabled)
                .boolean()
                .not_null()
                .default(true),
        )
        .col(
            ColumnDef::new(payment_settings::Column::EpayQuotaPerCny)
                .big_integer()
                .not_null()
                .default(500_000_i64)
                .check(Expr::col(payment_settings::Column::EpayQuotaPerCny).gt(0_i64)),
        )
        .col(
            ColumnDef::new(payment_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(payment_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, payment_settings::Column::CreatedAt))
        .col(timestamp(manager, payment_settings::Column::UpdatedAt))
        .check(
            Expr::col(payment_settings::Column::StripeEnabled)
                .eq(false)
                .or(Expr::col(payment_settings::Column::StripePublishableKey)
                    .is_not_null()
                    .and(Expr::col(payment_settings::Column::StripeSecretKey).is_not_null())
                    .and(Expr::col(payment_settings::Column::StripeWebhookSecret).is_not_null())),
        )
        .check(
            Expr::col(payment_settings::Column::EpayEnabled)
                .eq(false)
                .or(Expr::col(payment_settings::Column::EpayGatewayUrl)
                    .is_not_null()
                    .and(Expr::col(payment_settings::Column::EpayMerchantId).is_not_null())
                    .and(Expr::col(payment_settings::Column::EpayMerchantKey).is_not_null())
                    .and(
                        Expr::col(payment_settings::Column::EpayAlipayEnabled)
                            .eq(true)
                            .or(Expr::col(payment_settings::Column::EpayWxpayEnabled).eq(true)),
                    )),
        );
    manager.create_table(statement).await?;

    // 固定行作为跨数据库并发更新锁锚，同时区分尚未接管旧 TOML 的初始状态。
    manager
        .exec_stmt(
            Query::insert()
                .into_table(payment_settings::Entity)
                .columns([
                    payment_settings::Column::Id,
                    payment_settings::Column::Initialized,
                    payment_settings::Column::StripeEnabled,
                    payment_settings::Column::StripeSignatureToleranceSeconds,
                    payment_settings::Column::EpayEnabled,
                    payment_settings::Column::EpayAlipayEnabled,
                    payment_settings::Column::EpayWxpayEnabled,
                    payment_settings::Column::EpayQuotaPerCny,
                    payment_settings::Column::Version,
                ])
                .values_panic([
                    1_i16.into(),
                    false.into(),
                    false.into(),
                    300_i32.into(),
                    false.into(),
                    true.into(),
                    true.into(),
                    500_000_i64.into(),
                    1_i64.into(),
                ])
                .to_owned(),
        )
        .await
}

/// 为充值订单和支付事件补齐支付方式、金额与币种审计事实。
pub(in crate::migration) async fn extend_topup_payment_facts(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(topup_orders::Entity)
                .add_column(ColumnDef::new(topup_orders::Column::PaymentMethod).string_len(32))
                .to_owned(),
        )
        .await?;
    // SQLite 每条 ALTER TABLE 只支持新增一列，逐列执行避免迁移在本地预览库失败。
    manager
        .alter_table(
            Table::alter()
                .table(topup_payment_events::Entity)
                .add_column(ColumnDef::new(topup_payment_events::Column::AmountMinor).big_integer())
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(topup_payment_events::Entity)
                .add_column(ColumnDef::new(topup_payment_events::Column::Currency).char_len(3))
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(topup_payment_events::Entity)
                .add_column(
                    ColumnDef::new(topup_payment_events::Column::PaymentMethod).string_len(32),
                )
                .to_owned(),
        )
        .await?;
    Ok(())
}

/// 创建充值订单与支付 webhook 审计表；本迁移只固化状态机和幂等边界，不接任何 Provider。
pub(in crate::migration) async fn create_topup_payment_audit(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_orders(manager).await?;
    create_payment_events(manager).await?;
    create_indexes(manager).await
}

async fn create_orders(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_topup_orders_table(manager, topup_orders::Entity, false, false).await
}

/// 创建指定表名的当前充值订单结构，供 SQLite 迁移重建复用。
pub(super) async fn create_topup_orders_table<T>(
    manager: &SchemaManager<'_>,
    table_name: T,
    include_payment_method: bool,
    include_organization: bool,
) -> Result<(), DbErr>
where
    T: IntoTableRef + Clone,
{
    let mut statement = table(manager, table_name.clone());
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
        );
    // 企业归属由扩展层解释；公共核心始终保留可空上下文列，避免共享实体查询缺列。
    statement.col(ColumnDef::new(topup_orders::Column::OrganizationId).big_integer());
    statement.col(
        ColumnDef::new(topup_orders::Column::Provider)
            .string_len(64)
            .not_null()
            .check(Expr::col(topup_orders::Column::Provider).ne("")),
    );
    if include_payment_method {
        statement.col(ColumnDef::new(topup_orders::Column::PaymentMethod).string_len(32));
    }
    statement
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
                .from(table_name.clone(), topup_orders::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    if include_organization {
        statement.foreign_key(
            ForeignKey::create()
                .name("fk_topup_orders_organization")
                .from(table_name, topup_orders::Column::OrganizationId)
                .to(
                    crate::migration::iden::organizations::Entity,
                    crate::migration::iden::organizations::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    }
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

/// 创建订阅订单专用的只追加支付审计事件表。
pub(in crate::migration) async fn create_subscription_payment_events(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, subscription_payment_events::Entity);
    statement
        .col(auto_id(subscription_payment_events::Column::Id))
        .col(
            ColumnDef::new(subscription_payment_events::Column::EventKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_payment_events::Column::OrderId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_payment_events::Column::Provider)
                .string_len(64)
                .not_null()
                .check(Expr::col(subscription_payment_events::Column::Provider).ne("")),
        )
        .col(
            ColumnDef::new(subscription_payment_events::Column::ProviderEventId)
                .string_len(128)
                .not_null()
                .check(Expr::col(subscription_payment_events::Column::ProviderEventId).ne("")),
        )
        .col(ColumnDef::new(subscription_payment_events::Column::TradeNo).string_len(128))
        .col(ColumnDef::new(subscription_payment_events::Column::AmountMinor).big_integer())
        .col(ColumnDef::new(subscription_payment_events::Column::Currency).char_len(3))
        .col(ColumnDef::new(subscription_payment_events::Column::PaymentMethod).string_len(32))
        .col(
            ColumnDef::new(subscription_payment_events::Column::EventType)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(subscription_payment_events::Column::EventType).is_in([1_i16, 2, 3]),
                ),
        )
        .col(
            ColumnDef::new(subscription_payment_events::Column::SignatureKeyFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_payment_events::Column::PayloadSha256)
                .char_len(64)
                .not_null(),
        )
        .col(timestamp(
            manager,
            subscription_payment_events::Column::ReceivedAt,
        ))
        .col(nullable_timestamp(
            manager,
            subscription_payment_events::Column::ProcessedAt,
        ))
        .col(timestamp(
            manager,
            subscription_payment_events::Column::CreatedAt,
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
        .foreign_key(
            ForeignKey::create()
                .name("fk_subscription_payment_events_order")
                .from(
                    subscription_payment_events::Entity,
                    subscription_payment_events::Column::OrderId,
                )
                .to(subscription_orders::Entity, subscription_orders::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    for index in [
        Index::create()
            .name("uq_subscription_payment_events_event_key")
            .table(subscription_payment_events::Entity)
            .col(subscription_payment_events::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_subscription_payment_events_provider_event")
            .table(subscription_payment_events::Entity)
            .col(subscription_payment_events::Column::Provider)
            .col(subscription_payment_events::Column::ProviderEventId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_subscription_payment_events_order_created")
            .table(subscription_payment_events::Entity)
            .col(subscription_payment_events::Column::OrderId)
            .col(subscription_payment_events::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
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
    create_topup_order_indexes(manager, topup_orders::Entity).await?;
    for index in [
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

pub(super) async fn create_topup_order_indexes<T>(
    manager: &SchemaManager<'_>,
    table_name: T,
) -> Result<(), DbErr>
where
    T: IntoTableRef + Clone,
{
    for index in [
        Index::create()
            .name("uq_topup_orders_order_key")
            .table(table_name.clone())
            .col(topup_orders::Column::OrderKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_topup_orders_idempotency_key")
            .table(table_name.clone())
            .col(topup_orders::Column::IdempotencyKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_topup_orders_provider_trade_no")
            .table(table_name.clone())
            .col(topup_orders::Column::Provider)
            .col(topup_orders::Column::TradeNo)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_topup_orders_user_created")
            .table(table_name.clone())
            .col(topup_orders::Column::UserId)
            .col(topup_orders::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_topup_orders_status_expires")
            .table(table_name)
            .col(topup_orders::Column::Status)
            .col(topup_orders::Column::ExpiresAt)
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
