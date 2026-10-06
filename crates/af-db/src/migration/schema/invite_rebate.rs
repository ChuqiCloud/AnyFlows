use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{invite_rebate_events, users};

use super::{auto_id, table, timestamp};

/// 创建邀请返利到账事件审计表。
pub(in crate::migration) async fn create_invite_rebate_events(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, invite_rebate_events::Entity);
    statement
        .col(auto_id(invite_rebate_events::Column::Id))
        .col(
            ColumnDef::new(invite_rebate_events::Column::EventKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(invite_rebate_events::Column::InviterUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(invite_rebate_events::Column::InviteeUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(invite_rebate_events::Column::QuotaAmount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(invite_rebate_events::Column::BalanceAfter)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(invite_rebate_events::Column::WalletLedgerEntryId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(manager, invite_rebate_events::Column::CreditedAt))
        .col(timestamp(manager, invite_rebate_events::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_invite_rebate_events_inviter")
                .from(
                    invite_rebate_events::Entity,
                    invite_rebate_events::Column::InviterUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_invite_rebate_events_invitee")
                .from(
                    invite_rebate_events::Entity,
                    invite_rebate_events::Column::InviteeUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(event_key_format_check(manager.get_database_backend()))
        .check(
            Expr::col(invite_rebate_events::Column::EventKey)
                .ne("00000000000000000000000000000000"),
        )
        .check(Expr::col(invite_rebate_events::Column::QuotaAmount).gt(0_i64))
        .check(Expr::col(invite_rebate_events::Column::BalanceAfter).gte(0_i64))
        .check(
            Expr::col(invite_rebate_events::Column::BalanceAfter)
                .gte(Expr::col(invite_rebate_events::Column::QuotaAmount)),
        )
        .check(Expr::col(invite_rebate_events::Column::WalletLedgerEntryId).gt(0_i64))
        .check(
            Expr::col(invite_rebate_events::Column::CreatedAt)
                .gte(Expr::col(invite_rebate_events::Column::CreditedAt)),
        );
    if manager.get_database_backend() != DbBackend::MySql {
        // MySQL 8.4 禁止 CHECK 引用带级联动作的外键列，实体与仓储会兜底拒绝自返利。
        statement.check(
            Expr::col(invite_rebate_events::Column::InviterUserId)
                .ne(Expr::col(invite_rebate_events::Column::InviteeUserId)),
        );
    }
    manager.create_table(statement).await?;
    create_indexes(manager).await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_invite_rebate_events_event_key")
            .table(invite_rebate_events::Entity)
            .col(invite_rebate_events::Column::EventKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_invite_rebate_events_invitee")
            .table(invite_rebate_events::Entity)
            .col(invite_rebate_events::Column::InviteeUserId)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_invite_rebate_events_wallet_entry")
            .table(invite_rebate_events::Entity)
            .col(invite_rebate_events::Column::WalletLedgerEntryId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_invite_rebate_events_inviter_created")
            .table(invite_rebate_events::Entity)
            .col(invite_rebate_events::Column::InviterUserId)
            .col(invite_rebate_events::Column::CreatedAt)
            .col(invite_rebate_events::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{MysqlQueryBuilder, Query};

    use super::*;

    #[test]
    fn mysql_event_key_check_avoids_binary_regexp() {
        let rendered = Query::select()
            .expr(event_key_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert!(rendered.contains("REGEXP '^[0-9a-f]"));
        assert!(!rendered.contains("BINARY"));
    }
}
