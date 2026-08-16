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
const LEDGER_TABLE: &str = "organization_wallet_ledger_entries";
const LEDGER_NEXT_TABLE: &str = "organization_wallet_ledger_entries_next";

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
const TOPUP_ORDERS_OLD_TABLE: &str = "topup_orders_organization_scope_old";

/// SQLite 无法原地补充外键，使用同一事务重建充值订单并保留支付事件引用。
async fn rebuild_topup_orders(
    manager: &SchemaManager<'_>,
    include_organization: bool,
) -> Result<(), DbErr> {
    if manager.has_table(TOPUP_ORDERS_NEXT_TABLE).await? {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(TOPUP_ORDERS_NEXT_TABLE))
                    .to_owned(),
            )
            .await?;
    }
    if manager.has_table(TOPUP_ORDERS_OLD_TABLE).await? {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(TOPUP_ORDERS_OLD_TABLE))
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
    // 保留子表的正式外键名称，避免旧表带 RESTRICT 子记录时 DROP TABLE 立即失败。
    manager
        .get_connection()
        .execute_unprepared("PRAGMA legacy_alter_table = ON")
        .await?;
    manager
        .get_connection()
        .execute_unprepared(&format!(
            "ALTER TABLE \"topup_orders\" RENAME TO \"{TOPUP_ORDERS_OLD_TABLE}\""
        ))
        .await?;
    copy_topup_orders(manager, TOPUP_ORDERS_OLD_TABLE, include_organization).await?;
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(TOPUP_ORDERS_OLD_TABLE))
                .to_owned(),
        )
        .await?;
    manager
        .get_connection()
        .execute_unprepared(&format!(
            "ALTER TABLE \"{TOPUP_ORDERS_NEXT_TABLE}\" RENAME TO \"topup_orders\""
        ))
        .await?;
    manager
        .get_connection()
        .execute_unprepared("PRAGMA legacy_alter_table = OFF")
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
    source_table: &str,
    include_organization: bool,
) -> Result<(), DbErr> {
    let (target_scope, source_scope) = if include_organization {
        let source_scope = if manager.has_column(source_table, "organization_id").await? {
            "\"organization_id\""
        } else {
            "NULL"
        };
        (", \"organization_id\"", format!(", {source_scope}"))
    } else {
        ("", String::new())
    };
    let statement = format!(
        "INSERT INTO \"{TOPUP_ORDERS_NEXT_TABLE}\" (\"id\", \"order_key\", \"user_id\"{target_scope}, \"provider\", \"payment_method\", \"provider_order_id\", \"trade_no\", \"status\", \"amount_minor\", \"currency\", \"quota_amount\", \"idempotency_key\", \"version\", \"expires_at\", \"paid_at\", \"closed_at\", \"created_at\", \"updated_at\") SELECT \"id\", \"order_key\", \"user_id\"{source_scope}, \"provider\", \"payment_method\", \"provider_order_id\", \"trade_no\", \"status\", \"amount_minor\", \"currency\", \"quota_amount\", \"idempotency_key\", \"version\", \"expires_at\", \"paid_at\", \"closed_at\", \"created_at\", \"updated_at\" FROM \"{source_table}\""
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
    create_ledger_table_named(manager, LEDGER_TABLE, 3, "current").await?;
    ensure_ledger_indexes(manager).await
}

async fn create_ledger_table_named(
    manager: &SchemaManager<'_>,
    table_name: &'static str,
    maximum_entry_type: i16,
    constraint_namespace: &'static str,
) -> Result<(), DbErr> {
    let mut statement = table(manager, Alias::new(table_name));
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
                        .is_in((1_i16..=maximum_entry_type).collect::<Vec<_>>()),
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
                .name(ledger_organization_foreign_key_name(constraint_namespace))
                .from(
                    Alias::new(table_name),
                    organization_wallet_ledger_entries::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name(ledger_actor_foreign_key_name(constraint_namespace))
                .from(
                    Alias::new(table_name),
                    organization_wallet_ledger_entries::Column::ActorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

async fn ensure_ledger_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_index(LEDGER_TABLE, LEDGER_EVENT_INDEX).await? {
        manager
            .create_index(
                Index::create()
                    .name(LEDGER_EVENT_INDEX)
                    .table(organization_wallet_ledger_entries::Entity)
                    .col(organization_wallet_ledger_entries::Column::OrganizationId)
                    .col(organization_wallet_ledger_entries::Column::EventKey)
                    .unique()
                    .to_owned(),
            )
            .await?;
    }
    if !manager.has_index(LEDGER_TABLE, LEDGER_SCOPE_INDEX).await? {
        manager
            .create_index(
                Index::create()
                    .name(LEDGER_SCOPE_INDEX)
                    .table(organization_wallet_ledger_entries::Entity)
                    .col(organization_wallet_ledger_entries::Column::OrganizationId)
                    .col(organization_wallet_ledger_entries::Column::Id)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

/// 重建企业钱包账本闭合类型约束，以支持或移除调用结算事件。
///
/// SQLite 无法原地修改 CHECK，三方言统一复制替换；中断后保留的 next 表可在重试时恢复。
pub(in crate::migration) async fn rebuild_organization_wallet_ledger_for_usage(
    manager: &SchemaManager<'_>,
    allow_usage_settlement: bool,
) -> Result<(), DbErr> {
    if !allow_usage_settlement {
        ensure_no_usage_settlement(manager).await?;
    }
    let current_exists = manager.has_table(LEDGER_TABLE).await?;
    let next_exists = manager.has_table(LEDGER_NEXT_TABLE).await?;
    match (current_exists, next_exists) {
        (true, _) => {
            if next_exists {
                manager
                    .drop_table(
                        Table::drop()
                            .table(Alias::new(LEDGER_NEXT_TABLE))
                            .to_owned(),
                    )
                    .await?;
            }
            create_ledger_table_named(
                manager,
                LEDGER_NEXT_TABLE,
                if allow_usage_settlement { 4 } else { 3 },
                if allow_usage_settlement {
                    "usage"
                } else {
                    "base"
                },
            )
            .await?;
            copy_ledger(manager).await?;
            manager
                .drop_table(Table::drop().table(Alias::new(LEDGER_TABLE)).to_owned())
                .await?;
            rename_ledger(manager).await?;
        }
        (false, true) => rename_ledger(manager).await?,
        (false, false) => {
            return Err(DbErr::Custom("企业钱包账本重建缺少源表和恢复表".to_owned()));
        }
    }
    ensure_ledger_indexes(manager).await?;
    reset_postgres_ledger_sequence(manager).await
}

async fn ensure_no_usage_settlement(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let table_name = if manager.has_table(LEDGER_TABLE).await? {
        LEDGER_TABLE
    } else {
        LEDGER_NEXT_TABLE
    };
    if !manager.has_table(table_name).await? {
        return Ok(());
    }
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("SELECT 1 FROM `{table_name}` WHERE `entry_type` = 4 LIMIT 1")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!(r#"SELECT 1 FROM "{table_name}" WHERE "entry_type" = 4 LIMIT 1"#)
        }
    };
    if manager
        .get_connection()
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            statement,
        ))
        .await?
        .is_some()
    {
        return Err(DbErr::Custom(
            "存在企业调用结算账本，无法回退企业钱包账本类型".to_owned(),
        ));
    }
    Ok(())
}

async fn copy_ledger(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => format!(
            "INSERT INTO `{LEDGER_NEXT_TABLE}` \
(`id`, `organization_id`, `event_key`, `actor_user_id`, `entry_type`, `quota_delta`, `balance_before`, `balance_after`, `source`, `reason`, `created_at`) \
SELECT `id`, `organization_id`, `event_key`, `actor_user_id`, `entry_type`, `quota_delta`, `balance_before`, `balance_after`, `source`, `reason`, `created_at` \
FROM `{LEDGER_TABLE}`"
        ),
        DbBackend::Postgres | DbBackend::Sqlite => format!(
            "INSERT INTO \"{LEDGER_NEXT_TABLE}\" \
(\"id\", \"organization_id\", \"event_key\", \"actor_user_id\", \"entry_type\", \"quota_delta\", \"balance_before\", \"balance_after\", \"source\", \"reason\", \"created_at\") \
SELECT \"id\", \"organization_id\", \"event_key\", \"actor_user_id\", \"entry_type\", \"quota_delta\", \"balance_before\", \"balance_after\", \"source\", \"reason\", \"created_at\" \
FROM \"{LEDGER_TABLE}\""
        ),
    };
    manager
        .get_connection()
        .execute_unprepared(&statement)
        .await?;
    Ok(())
}

async fn rename_ledger(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("RENAME TABLE `{LEDGER_NEXT_TABLE}` TO `{LEDGER_TABLE}`")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("ALTER TABLE \"{LEDGER_NEXT_TABLE}\" RENAME TO \"{LEDGER_TABLE}\"")
        }
    };
    manager
        .get_connection()
        .execute_unprepared(&statement)
        .await?;
    Ok(())
}

async fn reset_postgres_ledger_sequence(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }
    manager
        .get_connection()
        .execute_unprepared(
            r#"
SELECT setval(
    pg_get_serial_sequence('"organization_wallet_ledger_entries"', 'id'),
    COALESCE((SELECT MAX("id") FROM "organization_wallet_ledger_entries"), 1),
    EXISTS (SELECT 1 FROM "organization_wallet_ledger_entries")
)"#,
        )
        .await?;
    Ok(())
}

fn ledger_organization_foreign_key_name(namespace: &str) -> &'static str {
    match namespace {
        "usage" => "fk_organization_wallet_ledger_usage_organization",
        "base" => "fk_organization_wallet_ledger_base_organization",
        _ => "fk_organization_wallet_ledger_organization",
    }
}

fn ledger_actor_foreign_key_name(namespace: &str) -> &'static str {
    match namespace {
        "usage" => "fk_organization_wallet_ledger_usage_actor",
        "base" => "fk_organization_wallet_ledger_base_actor",
        _ => "fk_organization_wallet_ledger_actor",
    }
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
