use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{users, wallet_ledger_entries};

use super::{auto_id, table, timestamp};

const WALLET_LEDGER_TABLE: &str = "wallet_ledger_entries";
const WALLET_LEDGER_NEXT_TABLE: &str = "wallet_ledger_entries_next";

/// 创建钱包追加账本，并为升级前已有的非零余额写入可解释的 opening 基线。
pub(in crate::migration) async fn create_wallet_ledger(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_table(manager).await?;
    create_indexes(manager).await?;
    backfill_opening_balances(manager).await
}

async fn create_table(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_table_named(manager, WALLET_LEDGER_TABLE, 2, "current").await
}

async fn create_table_named(
    manager: &SchemaManager<'_>,
    table_name: &'static str,
    maximum_entry_type: i16,
    constraint_namespace: &'static str,
) -> Result<(), DbErr> {
    let mut statement = table(manager, Alias::new(table_name));
    statement
        .col(auto_id(wallet_ledger_entries::Column::Id))
        .col(
            ColumnDef::new(wallet_ledger_entries::Column::EventKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(wallet_ledger_entries::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(wallet_ledger_entries::Column::ActorUserId).big_integer())
        .col(
            ColumnDef::new(wallet_ledger_entries::Column::EntryType)
                .small_integer()
                .not_null()
                .check(entry_type_check(maximum_entry_type)),
        )
        .col(
            ColumnDef::new(wallet_ledger_entries::Column::QuotaDelta)
                .big_integer()
                .not_null()
                .check(Expr::col(wallet_ledger_entries::Column::QuotaDelta).ne(0_i64))
                .check(Expr::col(wallet_ledger_entries::Column::QuotaDelta).ne(i64::MIN)),
        )
        .col(
            ColumnDef::new(wallet_ledger_entries::Column::BalanceBefore)
                .big_integer()
                .not_null()
                .check(Expr::col(wallet_ledger_entries::Column::BalanceBefore).gte(0_i64)),
        )
        .col(
            ColumnDef::new(wallet_ledger_entries::Column::BalanceAfter)
                .big_integer()
                .not_null()
                .check(Expr::col(wallet_ledger_entries::Column::BalanceAfter).gte(0_i64)),
        )
        .col(ColumnDef::new(wallet_ledger_entries::Column::Reason).string_len(500))
        .col(timestamp(manager, wallet_ledger_entries::Column::CreatedAt))
        .check(event_key_format_check(manager.get_database_backend()))
        .check(
            Expr::col(wallet_ledger_entries::Column::EventKey)
                .ne("00000000000000000000000000000000"),
        )
        .check(
            Expr::col(wallet_ledger_entries::Column::BalanceAfter)
                .eq(Expr::col(wallet_ledger_entries::Column::BalanceBefore)
                    .add(Expr::col(wallet_ledger_entries::Column::QuotaDelta))),
        )
        .foreign_key(
            ForeignKey::create()
                .name(user_foreign_key_name(constraint_namespace))
                .from(
                    Alias::new(table_name),
                    wallet_ledger_entries::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name(actor_foreign_key_name(constraint_namespace))
                .from(
                    Alias::new(table_name),
                    wallet_ledger_entries::Column::ActorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );

    // MySQL 8.4 禁止 CHECK 引用带级联动作的外键列；该方言由实体与仓储校验事件形态。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(entry_shape_check(maximum_entry_type));
    }
    manager.create_table(statement).await
}

fn entry_type_check(maximum_entry_type: i16) -> SimpleExpr {
    Expr::col(wallet_ledger_entries::Column::EntryType)
        .is_in((1_i16..=maximum_entry_type).collect::<Vec<_>>())
}

fn entry_shape_check(maximum_entry_type: i16) -> SimpleExpr {
    let mut shape = Expr::col(wallet_ledger_entries::Column::EntryType)
        .eq(1_i16)
        .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_null())
        .and(Expr::col(wallet_ledger_entries::Column::QuotaDelta).gt(0_i64))
        .and(Expr::col(wallet_ledger_entries::Column::BalanceBefore).eq(0_i64))
        .and(Expr::col(wallet_ledger_entries::Column::Reason).is_null())
        .or(Expr::col(wallet_ledger_entries::Column::EntryType)
            .eq(2_i16)
            .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_not_null())
            .and(Expr::col(wallet_ledger_entries::Column::Reason).is_not_null())
            .and(Expr::col(wallet_ledger_entries::Column::Reason).ne("")));
    if maximum_entry_type >= 3 {
        shape = shape.or(Expr::col(wallet_ledger_entries::Column::EntryType)
            .eq(3_i16)
            .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_null())
            .and(Expr::col(wallet_ledger_entries::Column::QuotaDelta).gt(0_i64))
            .and(Expr::col(wallet_ledger_entries::Column::Reason).is_null()));
    }
    if maximum_entry_type >= 4 {
        shape = shape.or(Expr::col(wallet_ledger_entries::Column::EntryType)
            .eq(4_i16)
            .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_null())
            .and(Expr::col(wallet_ledger_entries::Column::QuotaDelta).gt(0_i64))
            .and(Expr::col(wallet_ledger_entries::Column::Reason).is_null()));
    }
    if maximum_entry_type >= 5 {
        shape = shape.or(Expr::col(wallet_ledger_entries::Column::EntryType)
            .eq(5_i16)
            .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_null())
            .and(Expr::col(wallet_ledger_entries::Column::QuotaDelta).gt(0_i64))
            .and(Expr::col(wallet_ledger_entries::Column::Reason).is_null()));
    }
    shape
}

fn event_key_format_check(database_backend: DbBackend) -> SimpleExpr {
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

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_indexes_for(manager, wallet_ledger_entries::Entity).await
}

async fn create_indexes_for<T>(manager: &SchemaManager<'_>, table_name: T) -> Result<(), DbErr>
where
    T: IntoTableRef + Clone,
{
    for index in [
        Index::create()
            .name("uq_wallet_ledger_event_key")
            .table(table_name.clone())
            .col(wallet_ledger_entries::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_wallet_ledger_user_id")
            .table(table_name.clone())
            .col(wallet_ledger_entries::Column::UserId)
            .col(wallet_ledger_entries::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

/// 重建钱包账本的闭合类型约束，以支持或移除充值到账事件。
///
/// SQLite 无法原地修改 CHECK，三方言统一采用建新表、复制、替换的路径；MySQL DDL
/// 失败后可能留下中间表，重试时会根据两张表的存在状态恢复，不删除唯一剩余副本。
pub(in crate::migration) async fn rebuild_wallet_ledger_for_topup(
    manager: &SchemaManager<'_>,
    allow_topup: bool,
) -> Result<(), DbErr> {
    if !allow_topup {
        ensure_no_entry_type(manager, 3, "存在充值到账账本，无法回退钱包账本类型").await?;
    }
    rebuild_wallet_ledger(
        manager,
        if allow_topup { 3 } else { 2 },
        if allow_topup { "topup" } else { "base" },
    )
    .await
}

/// 重建钱包账本约束，以支持或移除兑换码到账事件。
pub(in crate::migration) async fn rebuild_wallet_ledger_for_redemption(
    manager: &SchemaManager<'_>,
    allow_redemption: bool,
) -> Result<(), DbErr> {
    if !allow_redemption {
        ensure_no_entry_type(manager, 4, "存在兑换码到账账本，无法回退钱包账本类型").await?;
    }
    rebuild_wallet_ledger(
        manager,
        if allow_redemption { 4 } else { 3 },
        if allow_redemption {
            "redemption"
        } else {
            "topup"
        },
    )
    .await
}

/// 重建钱包账本约束，以支持或移除邀请返利到账事件。
pub(in crate::migration) async fn rebuild_wallet_ledger_for_invite_rebate(
    manager: &SchemaManager<'_>,
    allow_invite_rebate: bool,
) -> Result<(), DbErr> {
    if !allow_invite_rebate {
        ensure_no_entry_type(manager, 5, "存在邀请返利到账账本，无法回退钱包账本类型").await?;
    }
    rebuild_wallet_ledger(
        manager,
        if allow_invite_rebate { 5 } else { 4 },
        if allow_invite_rebate {
            "invite_rebate"
        } else {
            "redemption"
        },
    )
    .await
}

async fn rebuild_wallet_ledger(
    manager: &SchemaManager<'_>,
    maximum_entry_type: i16,
    namespace: &'static str,
) -> Result<(), DbErr> {
    let current_exists = manager.has_table(WALLET_LEDGER_TABLE).await?;
    let next_exists = manager.has_table(WALLET_LEDGER_NEXT_TABLE).await?;
    match (current_exists, next_exists) {
        (true, _) => {
            if next_exists {
                manager
                    .drop_table(
                        Table::drop()
                            .table(Alias::new(WALLET_LEDGER_NEXT_TABLE))
                            .to_owned(),
                    )
                    .await?;
            }
            create_table_named(
                manager,
                WALLET_LEDGER_NEXT_TABLE,
                maximum_entry_type,
                namespace,
            )
            .await?;
            copy_wallet_ledger(manager).await?;
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(WALLET_LEDGER_TABLE))
                        .to_owned(),
                )
                .await?;
            rename_wallet_ledger(manager).await?;
        }
        (false, true) => rename_wallet_ledger(manager).await?,
        (false, false) => {
            return Err(DbErr::Custom("钱包账本重建缺少源表和恢复表".to_owned()));
        }
    }
    ensure_wallet_indexes(manager).await?;
    reset_postgres_wallet_sequence(manager).await
}

async fn ensure_no_entry_type(
    manager: &SchemaManager<'_>,
    entry_type: i16,
    message: &'static str,
) -> Result<(), DbErr> {
    let current_exists = manager.has_table(WALLET_LEDGER_TABLE).await?;
    let table_name = if current_exists {
        WALLET_LEDGER_TABLE
    } else {
        WALLET_LEDGER_NEXT_TABLE
    };
    if !manager.has_table(table_name).await? {
        return Ok(());
    }
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("SELECT 1 FROM `{table_name}` WHERE `entry_type` = {entry_type} LIMIT 1")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!(r#"SELECT 1 FROM "{table_name}" WHERE "entry_type" = {entry_type} LIMIT 1"#)
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
        return Err(DbErr::Custom(message.to_owned()));
    }
    Ok(())
}

async fn copy_wallet_ledger(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => format!(
            "INSERT INTO `{WALLET_LEDGER_NEXT_TABLE}` \
(`id`, `event_key`, `user_id`, `actor_user_id`, `entry_type`, `quota_delta`, `balance_before`, `balance_after`, `reason`, `created_at`) \
SELECT `id`, `event_key`, `user_id`, `actor_user_id`, `entry_type`, `quota_delta`, `balance_before`, `balance_after`, `reason`, `created_at` \
FROM `{WALLET_LEDGER_TABLE}`"
        ),
        DbBackend::Postgres | DbBackend::Sqlite => format!(
            "INSERT INTO \"{WALLET_LEDGER_NEXT_TABLE}\" \
(\"id\", \"event_key\", \"user_id\", \"actor_user_id\", \"entry_type\", \"quota_delta\", \"balance_before\", \"balance_after\", \"reason\", \"created_at\") \
SELECT \"id\", \"event_key\", \"user_id\", \"actor_user_id\", \"entry_type\", \"quota_delta\", \"balance_before\", \"balance_after\", \"reason\", \"created_at\" \
FROM \"{WALLET_LEDGER_TABLE}\""
        ),
    };
    manager
        .get_connection()
        .execute_unprepared(&statement)
        .await?;
    Ok(())
}

async fn rename_wallet_ledger(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("RENAME TABLE `{WALLET_LEDGER_NEXT_TABLE}` TO `{WALLET_LEDGER_TABLE}`")
        }
        DbBackend::Postgres | DbBackend::Sqlite => format!(
            "ALTER TABLE \"{WALLET_LEDGER_NEXT_TABLE}\" RENAME TO \"{WALLET_LEDGER_TABLE}\""
        ),
    };
    manager
        .get_connection()
        .execute_unprepared(&statement)
        .await?;
    Ok(())
}

async fn ensure_wallet_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in ["uq_wallet_ledger_event_key", "idx_wallet_ledger_user_id"] {
        if manager.has_index(WALLET_LEDGER_TABLE, index).await? {
            continue;
        }
        let statement = match index {
            "uq_wallet_ledger_event_key" => Index::create()
                .name(index)
                .table(wallet_ledger_entries::Entity)
                .col(wallet_ledger_entries::Column::EventKey)
                .unique()
                .to_owned(),
            _ => Index::create()
                .name(index)
                .table(wallet_ledger_entries::Entity)
                .col(wallet_ledger_entries::Column::UserId)
                .col(wallet_ledger_entries::Column::Id)
                .to_owned(),
        };
        manager.create_index(statement).await?;
    }
    Ok(())
}

async fn reset_postgres_wallet_sequence(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }
    manager
        .get_connection()
        .execute_unprepared(
            r#"
SELECT setval(
    pg_get_serial_sequence('"wallet_ledger_entries"', 'id'),
    COALESCE((SELECT MAX("id") FROM "wallet_ledger_entries"), 1),
    EXISTS (SELECT 1 FROM "wallet_ledger_entries")
)"#,
        )
        .await?;
    Ok(())
}

fn user_foreign_key_name(namespace: &str) -> &'static str {
    match namespace {
        "invite_rebate" => "fk_wallet_ledger_invite_rebate_user",
        "redemption" => "fk_wallet_ledger_redemption_user",
        "topup" => "fk_wallet_ledger_topup_user",
        "base" => "fk_wallet_ledger_base_user",
        _ => "fk_wallet_ledger_user",
    }
}

fn actor_foreign_key_name(namespace: &str) -> &'static str {
    match namespace {
        "invite_rebate" => "fk_wallet_ledger_invite_rebate_actor",
        "redemption" => "fk_wallet_ledger_redemption_actor",
        "topup" => "fk_wallet_ledger_topup_actor",
        "base" => "fk_wallet_ledger_base_actor",
        _ => "fk_wallet_ledger_actor",
    }
}

async fn backfill_opening_balances(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::Postgres => {
            r#"
INSERT INTO "wallet_ledger_entries"
    ("event_key", "user_id", "actor_user_id", "entry_type", "quota_delta", "balance_before", "balance_after", "reason", "created_at")
SELECT '0000000000000001' || lpad(to_hex("id"), 16, '0'), "id", NULL, 1, "quota", 0, "quota", NULL, "created_at"
FROM "users" WHERE "quota" > 0"#
        }
        DbBackend::MySql => {
            r#"
INSERT INTO `wallet_ledger_entries`
    (`event_key`, `user_id`, `actor_user_id`, `entry_type`, `quota_delta`, `balance_before`, `balance_after`, `reason`, `created_at`)
SELECT CONCAT('0000000000000001', LPAD(LOWER(HEX(`id`)), 16, '0')), `id`, NULL, 1, `quota`, 0, `quota`, NULL, `created_at`
FROM `users` WHERE `quota` > 0"#
        }
        DbBackend::Sqlite => {
            r#"
INSERT INTO "wallet_ledger_entries"
    ("event_key", "user_id", "actor_user_id", "entry_type", "quota_delta", "balance_before", "balance_after", "reason", "created_at")
SELECT printf('0000000000000001%016x', "id"), "id", NULL, 1, "quota", 0, "quota", NULL, "created_at"
FROM "users" WHERE "quota" > 0"#
        }
    };
    manager
        .get_connection()
        .execute_unprepared(statement)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_event_key_check_uses_non_binary_case_sensitive_regex() {
        let rendered = Query::select()
            .expr(event_key_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert_eq!(
            rendered,
            "SELECT CHAR_LENGTH(`event_key`) = 32 AND `event_key` REGEXP '^[0-9a-f]{32}$'"
        );
    }
}
