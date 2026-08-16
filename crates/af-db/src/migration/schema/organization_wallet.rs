use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_wallet_ledger_entries, organization_wallets, organizations, topup_orders, users,
};

use super::{auto_id, payment, table, timestamp};

const TOPUP_ORGANIZATION_FOREIGN_KEY: &str = "fk_topup_orders_organization";
const TOPUP_ORGANIZATION_INDEX: &str = "idx_topup_orders_organization";
const WALLET_SCOPE_INDEX: &str = "uq_organization_wallets_organization";
const LEDGER_EVENT_INDEX: &str = "uq_organization_wallet_ledger_scope_event";
const LEDGER_SCOPE_INDEX: &str = "idx_organization_wallet_ledger_scope_id";

/// 创建企业共享钱包、只追加账本，并为历史企业补齐固定钱包行。
pub(in crate::migration) async fn create_organization_wallet_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    add_topup_organization_scope(manager).await?;
    create_wallets(manager).await?;
    create_ledger(manager).await?;
    backfill_wallets(manager).await
}

/// 删除企业钱包及充值订单组织归属，供迁移回滚使用。
pub(in crate::migration) async fn drop_organization_wallet_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_wallet_ledger_entries::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(Table::drop().table(organization_wallets::Entity).to_owned())
        .await?;
    remove_topup_organization_scope(manager).await
}

async fn add_topup_organization_scope(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        return rebuild_topup_orders(manager, true).await;
    }
    if !manager
        .has_column("topup_orders", "organization_id")
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(topup_orders::Entity)
                    .add_column(ColumnDef::new(topup_orders::Column::OrganizationId).big_integer())
                    .to_owned(),
            )
            .await?;
    }
    if !manager
        .has_index("topup_orders", TOPUP_ORGANIZATION_INDEX)
        .await?
    {
        manager
            .create_index(
                Index::create()
                    .name(TOPUP_ORGANIZATION_INDEX)
                    .table(topup_orders::Entity)
                    .col(topup_orders::Column::OrganizationId)
                    .col(topup_orders::Column::Id)
                    .to_owned(),
            )
            .await?;
    }
    if !has_topup_organization_foreign_key(manager).await? {
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name(TOPUP_ORGANIZATION_FOREIGN_KEY)
                    .from(topup_orders::Entity, topup_orders::Column::OrganizationId)
                    .to(organizations::Entity, organizations::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

async fn remove_topup_organization_scope(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        return rebuild_topup_orders(manager, false).await;
    }
    if has_topup_organization_foreign_key(manager).await? {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name(TOPUP_ORGANIZATION_FOREIGN_KEY)
                    .table(topup_orders::Entity)
                    .to_owned(),
            )
            .await?;
    }
    if manager
        .has_index("topup_orders", TOPUP_ORGANIZATION_INDEX)
        .await?
    {
        manager
            .drop_index(
                Index::drop()
                    .name(TOPUP_ORGANIZATION_INDEX)
                    .table(topup_orders::Entity)
                    .to_owned(),
            )
            .await?;
    }
    manager
        .alter_table(
            Table::alter()
                .table(topup_orders::Entity)
                .drop_column(topup_orders::Column::OrganizationId)
                .to_owned(),
        )
        .await
}

async fn has_topup_organization_foreign_key(manager: &SchemaManager<'_>) -> Result<bool, DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => Statement::from_string(
            manager.get_database_backend(),
            format!(
                "SELECT 1 FROM information_schema.table_constraints WHERE table_name = 'topup_orders' AND constraint_name = '{TOPUP_ORGANIZATION_FOREIGN_KEY}' LIMIT 1"
            ),
        ),
        DbBackend::Sqlite => return Ok(false),
    };
    Ok(manager
        .get_connection()
        .query_one(statement)
        .await?
        .is_some())
}

const TOPUP_ORDERS_NEXT_TABLE: &str = "topup_orders_organization_scope_next";

/// SQLite 无法原地补充外键，使用同一事务重建充值订单并保留支付事件引用。
async fn rebuild_topup_orders(
    manager: &SchemaManager<'_>,
    include_organization: bool,
) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute_unprepared("PRAGMA defer_foreign_keys = ON")
        .await?;
    if manager.has_table(TOPUP_ORDERS_NEXT_TABLE).await? {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(TOPUP_ORDERS_NEXT_TABLE))
                    .to_owned(),
            )
            .await?;
    }
    payment::create_topup_orders_table(
        manager,
        Alias::new(TOPUP_ORDERS_NEXT_TABLE),
        true,
        include_organization,
    )
    .await?;
    copy_topup_orders(manager, include_organization).await?;
    manager
        .drop_table(Table::drop().table(topup_orders::Entity).to_owned())
        .await?;
    manager
        .get_connection()
        .execute_unprepared(&format!(
            "ALTER TABLE \"{TOPUP_ORDERS_NEXT_TABLE}\" RENAME TO \"topup_orders\""
        ))
        .await?;
    payment::create_topup_order_indexes(manager, topup_orders::Entity).await?;
    if include_organization {
        manager
            .create_index(
                Index::create()
                    .name(TOPUP_ORGANIZATION_INDEX)
                    .table(topup_orders::Entity)
                    .col(topup_orders::Column::OrganizationId)
                    .col(topup_orders::Column::Id)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

async fn copy_topup_orders(
    manager: &SchemaManager<'_>,
    include_organization: bool,
) -> Result<(), DbErr> {
    let (target_scope, source_scope) = if include_organization {
        let source_scope = if manager
            .has_column("topup_orders", "organization_id")
            .await?
        {
            "\"organization_id\""
        } else {
            "NULL"
        };
        (", \"organization_id\"", format!(", {source_scope}"))
    } else {
        ("", String::new())
    };
    let statement = format!(
        "INSERT INTO \"{TOPUP_ORDERS_NEXT_TABLE}\" (\"id\", \"order_key\", \"user_id\"{target_scope}, \"provider\", \"payment_method\", \"provider_order_id\", \"trade_no\", \"status\", \"amount_minor\", \"currency\", \"quota_amount\", \"idempotency_key\", \"version\", \"expires_at\", \"paid_at\", \"closed_at\", \"created_at\", \"updated_at\") SELECT \"id\", \"order_key\", \"user_id\"{source_scope}, \"provider\", \"payment_method\", \"provider_order_id\", \"trade_no\", \"status\", \"amount_minor\", \"currency\", \"quota_amount\", \"idempotency_key\", \"version\", \"expires_at\", \"paid_at\", \"closed_at\", \"created_at\", \"updated_at\" FROM \"topup_orders\""
    );
    manager
        .get_connection()
        .execute_unprepared(&statement)
        .await?;
    Ok(())
}

async fn create_wallets(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_wallets::Entity);
    statement
        .col(auto_id(organization_wallets::Column::Id))
        .col(
            ColumnDef::new(organization_wallets::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_wallets::Column::Quota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(Expr::col(organization_wallets::Column::Quota).gte(0_i64)),
        )
        .col(
            ColumnDef::new(organization_wallets::Column::UsedQuota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(Expr::col(organization_wallets::Column::UsedQuota).gte(0_i64)),
        )
        .col(
            ColumnDef::new(organization_wallets::Column::FrozenQuota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(Expr::col(organization_wallets::Column::FrozenQuota).gte(0_i64)),
        )
        .col(
            ColumnDef::new(organization_wallets::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_wallets::Column::Version).gt(0_i64)),
        )
        .col(timestamp(manager, organization_wallets::Column::CreatedAt))
        .col(timestamp(manager, organization_wallets::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_wallets_organization")
                .from(
                    organization_wallets::Entity,
                    organization_wallets::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(WALLET_SCOPE_INDEX)
                .table(organization_wallets::Entity)
                .col(organization_wallets::Column::OrganizationId)
                .unique()
                .to_owned(),
        )
        .await
}

async fn create_ledger(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_wallet_ledger_entries::Entity);
    statement
        .col(auto_id(organization_wallet_ledger_entries::Column::Id))
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::EventKey)
                .char_len(32)
                .not_null(),
        )
        .col(ColumnDef::new(organization_wallet_ledger_entries::Column::ActorUserId).big_integer())
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::EntryType)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_wallet_ledger_entries::Column::EntryType)
                        .is_in([1_i16, 2_i16, 3_i16]),
                ),
        )
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::QuotaDelta)
                .big_integer()
                .not_null()
                .check(Expr::col(organization_wallet_ledger_entries::Column::QuotaDelta).ne(0_i64))
                .check(
                    Expr::col(organization_wallet_ledger_entries::Column::QuotaDelta).ne(i64::MIN),
                ),
        )
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::BalanceBefore)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(organization_wallet_ledger_entries::Column::BalanceBefore).gte(0_i64),
                ),
        )
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::BalanceAfter)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(organization_wallet_ledger_entries::Column::BalanceAfter).gte(0_i64),
                ),
        )
        .col(
            ColumnDef::new(organization_wallet_ledger_entries::Column::Source)
                .string_len(64)
                .not_null()
                .check(Expr::col(organization_wallet_ledger_entries::Column::Source).ne("")),
        )
        .col(ColumnDef::new(organization_wallet_ledger_entries::Column::Reason).string_len(500))
        .col(timestamp(
            manager,
            organization_wallet_ledger_entries::Column::CreatedAt,
        ))
        .check(event_key_check(manager.get_database_backend()))
        .check(
            Expr::col(organization_wallet_ledger_entries::Column::EventKey)
                .ne("00000000000000000000000000000000"),
        )
        .check(
            Expr::col(organization_wallet_ledger_entries::Column::BalanceAfter).eq(Expr::col(
                organization_wallet_ledger_entries::Column::BalanceBefore,
            )
            .add(Expr::col(
                organization_wallet_ledger_entries::Column::QuotaDelta,
            ))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_wallet_ledger_organization")
                .from(
                    organization_wallet_ledger_entries::Entity,
                    organization_wallet_ledger_entries::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_wallet_ledger_actor")
                .from(
                    organization_wallet_ledger_entries::Entity,
                    organization_wallet_ledger_entries::Column::ActorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    for index in [
        Index::create()
            .name(LEDGER_EVENT_INDEX)
            .table(organization_wallet_ledger_entries::Entity)
            .col(organization_wallet_ledger_entries::Column::OrganizationId)
            .col(organization_wallet_ledger_entries::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(LEDGER_SCOPE_INDEX)
            .table(organization_wallet_ledger_entries::Entity)
            .col(organization_wallet_ledger_entries::Column::OrganizationId)
            .col(organization_wallet_ledger_entries::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn event_key_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""event_key" ~ '^[0-9a-f]{32}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`event_key`) = 32 AND `event_key` REGEXP '^[0-9a-f]{32}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("event_key") = 32 AND "event_key" NOT GLOB '*[^0-9a-f]*'"#)
        }
    }
}

async fn backfill_wallets(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => {
            "INSERT INTO `organization_wallets` (`organization_id`) SELECT `id` FROM `organizations`"
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            "INSERT INTO \"organization_wallets\" (\"organization_id\") SELECT \"id\" FROM \"organizations\""
        }
    };
    manager
        .get_connection()
        .execute_unprepared(statement)
        .await
        .map(|_| ())
}
