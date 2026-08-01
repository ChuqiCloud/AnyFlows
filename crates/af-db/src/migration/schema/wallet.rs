use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{users, wallet_ledger_entries};

use super::{auto_id, table, timestamp};

/// 创建钱包追加账本，并为升级前已有的非零余额写入可解释的 opening 基线。
pub(in crate::migration) async fn create_wallet_ledger(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_table(manager).await?;
    create_indexes(manager).await?;
    backfill_opening_balances(manager).await
}

async fn create_table(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, wallet_ledger_entries::Entity);
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
                .check(Expr::col(wallet_ledger_entries::Column::EntryType).is_in([1_i16, 2_i16])),
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
                .name("fk_wallet_ledger_user")
                .from(
                    wallet_ledger_entries::Entity,
                    wallet_ledger_entries::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_wallet_ledger_actor")
                .from(
                    wallet_ledger_entries::Entity,
                    wallet_ledger_entries::Column::ActorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );

    // MySQL 8.4 禁止 CHECK 引用带级联动作的外键列；该方言由实体与仓储校验事件形态。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(entry_shape_check());
    }
    manager.create_table(statement).await
}

fn entry_shape_check() -> SimpleExpr {
    Expr::col(wallet_ledger_entries::Column::EntryType)
        .eq(1_i16)
        .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_null())
        .and(Expr::col(wallet_ledger_entries::Column::QuotaDelta).gt(0_i64))
        .and(Expr::col(wallet_ledger_entries::Column::BalanceBefore).eq(0_i64))
        .and(Expr::col(wallet_ledger_entries::Column::Reason).is_null())
        .or(Expr::col(wallet_ledger_entries::Column::EntryType)
            .eq(2_i16)
            .and(Expr::col(wallet_ledger_entries::Column::ActorUserId).is_not_null())
            .and(Expr::col(wallet_ledger_entries::Column::Reason).is_not_null())
            .and(Expr::col(wallet_ledger_entries::Column::Reason).ne("")))
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
    for index in [
        Index::create()
            .name("uq_wallet_ledger_event_key")
            .table(wallet_ledger_entries::Entity)
            .col(wallet_ledger_entries::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_wallet_ledger_user_id")
            .table(wallet_ledger_entries::Entity)
            .col(wallet_ledger_entries::Column::UserId)
            .col(wallet_ledger_entries::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
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
