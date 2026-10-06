use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{async_tasks, channels, credentials, groups, tokens, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建供应商无关的异步任务状态机存储。
pub(in crate::migration) async fn create_async_tasks(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, async_tasks::Entity);
    statement
        .col(auto_id(async_tasks::Column::Id))
        .col(
            ColumnDef::new(async_tasks::Column::TaskKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::IdempotencyKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::Protocol)
                .string_len(64)
                .not_null()
                .check(Expr::col(async_tasks::Column::Protocol).ne("")),
        )
        .col(
            ColumnDef::new(async_tasks::Column::RequestedModel)
                .string_len(256)
                .not_null()
                .check(Expr::col(async_tasks::Column::RequestedModel).ne("")),
        )
        .col(
            ColumnDef::new(async_tasks::Column::UpstreamModel)
                .string_len(256)
                .not_null()
                .check(Expr::col(async_tasks::Column::UpstreamModel).ne("")),
        )
        .col(
            ColumnDef::new(async_tasks::Column::ChannelId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::CredentialId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::CredentialRevision)
                .char_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_tasks::Column::UpstreamTaskId)
                .string_len(512)
                .not_null()
                .check(Expr::col(async_tasks::Column::UpstreamTaskId).ne("")),
        )
        .col(
            ColumnDef::new(async_tasks::Column::Status)
                .small_integer()
                .not_null()
                .check(Expr::col(async_tasks::Column::Status).is_in([1_i16, 2, 3, 4, 5])),
        )
        .col(
            ColumnDef::new(async_tasks::Column::ProgressBasisPoints)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(async_tasks::Column::ProgressBasisPoints).between(0_i16, 10_000_i16),
                ),
        )
        .col(
            ColumnDef::new(async_tasks::Column::FailureKind)
                .small_integer()
                .check(
                    Expr::col(async_tasks::Column::FailureKind)
                        .is_null()
                        .or(Expr::col(async_tasks::Column::FailureKind).is_in([1_i16, 2, 3, 4])),
                ),
        )
        .col(
            ColumnDef::new(async_tasks::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(async_tasks::Column::Version).gte(1_i64)),
        )
        .col(nullable_timestamp(manager, async_tasks::Column::TerminalAt))
        .col(timestamp(manager, async_tasks::Column::CreatedAt))
        .col(timestamp(manager, async_tasks::Column::UpdatedAt))
        .check(hex_check(
            manager.get_database_backend(),
            "task_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "idempotency_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "credential_revision",
            16,
            false,
        ))
        .check(task_state_shape())
        .check(
            Expr::col(async_tasks::Column::UpdatedAt)
                .gte(Expr::col(async_tasks::Column::CreatedAt)),
        )
        .check(
            Expr::col(async_tasks::Column::TerminalAt)
                .is_null()
                .or(Expr::col(async_tasks::Column::TerminalAt)
                    .gte(Expr::col(async_tasks::Column::CreatedAt))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_tasks_user")
                .from(async_tasks::Entity, async_tasks::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_tasks_token")
                .from(async_tasks::Entity, async_tasks::Column::TokenId)
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_tasks_group")
                .from(async_tasks::Entity, async_tasks::Column::GroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_tasks_channel")
                .from(async_tasks::Entity, async_tasks::Column::ChannelId)
                .to(channels::Entity, channels::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_tasks_credential")
                .from(async_tasks::Entity, async_tasks::Column::CredentialId)
                .to(credentials::Entity, credentials::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name("uq_async_tasks_task_key")
            .table(async_tasks::Entity)
            .col(async_tasks::Column::TaskKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_async_tasks_owner_idempotency")
            .table(async_tasks::Entity)
            .col(async_tasks::Column::UserId)
            .col(async_tasks::Column::IdempotencyKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_async_tasks_owner_created")
            .table(async_tasks::Entity)
            .col(async_tasks::Column::UserId)
            .col(async_tasks::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_async_tasks_status_updated")
            .table(async_tasks::Entity)
            .col(async_tasks::Column::Status)
            .col(async_tasks::Column::UpdatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn task_state_shape() -> SimpleExpr {
    Expr::col(async_tasks::Column::Status)
        .is_in([1_i16, 2, 3])
        .and(Expr::col(async_tasks::Column::FailureKind).is_null())
        .and(Expr::col(async_tasks::Column::TerminalAt).is_null())
        .or(Expr::col(async_tasks::Column::Status)
            .eq(4_i16)
            .and(Expr::col(async_tasks::Column::ProgressBasisPoints).eq(10_000_i16))
            .and(Expr::col(async_tasks::Column::FailureKind).is_null())
            .and(Expr::col(async_tasks::Column::TerminalAt).is_not_null()))
        .or(Expr::col(async_tasks::Column::Status)
            .eq(5_i16)
            .and(Expr::col(async_tasks::Column::ProgressBasisPoints).eq(0_i16))
            .and(Expr::col(async_tasks::Column::FailureKind).is_not_null())
            .and(Expr::col(async_tasks::Column::TerminalAt).is_not_null()))
}

pub(super) fn hex_check(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_hex_checks_do_not_use_binary_regex_operands() {
        let rendered = Query::select()
            .expr(hex_check(DbBackend::MySql, "task_key", 32, true))
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
