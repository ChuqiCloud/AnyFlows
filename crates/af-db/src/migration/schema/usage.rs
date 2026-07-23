use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{groups, tokens, usage_logs, users};

use super::{auto_id, table, timestamp};

/// 创建当前计费生命周期能够可靠提供的最小用量日志事实。
pub(in crate::migration) async fn create_usage_logs(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, usage_logs::Entity);
    statement
        .col(auto_id(usage_logs::Column::Id))
        .col(
            ColumnDef::new(usage_logs::Column::EventId)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::EventType)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(usage_logs::Column::EventType).eq(1_i16)),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::BillingMode)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::BillingMode).is_in([1_i16, 2_i16])),
        )
        .col(non_negative(usage_logs::Column::InputTokens))
        .col(non_negative(usage_logs::Column::OutputTokens))
        .col(non_negative(usage_logs::Column::CacheRead))
        .col(non_negative(usage_logs::Column::CacheCreation5m))
        .col(non_negative(usage_logs::Column::CacheCreation1h))
        .col(non_negative(usage_logs::Column::ReasoningTokens))
        .col(non_negative(usage_logs::Column::AudioInputTokens))
        .col(non_negative(usage_logs::Column::AudioOutputTokens))
        .col(
            ColumnDef::new(usage_logs::Column::UsageSource)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::UsageSource).is_in([1_i16, 2_i16])),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UsageSemantics)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::UsageSemantics).is_in([1_i16, 2_i16])),
        )
        .col(non_negative(usage_logs::Column::Quota))
        .col(timestamp(manager, usage_logs::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_usage_logs_user")
                .from(usage_logs::Entity, usage_logs::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_usage_logs_token")
                .from(usage_logs::Entity, usage_logs::Column::TokenId)
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_usage_logs_group")
                .from(usage_logs::Entity, usage_logs::Column::GroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_format_check(manager.get_database_backend()))
        .check(Expr::col(usage_logs::Column::EventId).ne("00000000000000000000000000000000"));
    manager.create_table(statement).await
}

/// 创建用量日志的幂等与看板基础索引。
pub(in crate::migration) async fn create_usage_logs_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_usage_logs_event_id")
            .table(usage_logs::Entity)
            .col(usage_logs::Column::EventId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_usage_logs_user_created")
            .table(usage_logs::Entity)
            .col(usage_logs::Column::UserId)
            .col(usage_logs::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_usage_logs_token_created")
            .table(usage_logs::Entity)
            .col(usage_logs::Column::TokenId)
            .col(usage_logs::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn non_negative<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .not_null()
        .check(Expr::col(column).gte(0_i64));
    definition
}

fn hex_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""event_id" ~ '^[0-9a-f]{32}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`event_id`) = 32 AND `event_id` REGEXP '^[0-9a-f]{32}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("event_id") = 32 AND "event_id" NOT GLOB '*[^0-9a-f]*'"#)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_event_id_check_uses_identifier_quotes_without_backslashes() {
        let rendered = Query::select()
            .expr(hex_format_check(DbBackend::Postgres))
            .to_owned()
            .to_string(PostgresQueryBuilder);
        assert!(rendered.contains(r#""event_id" ~ '^[0-9a-f]{32}$'"#));
        assert!(!rendered.contains('\\'));
    }

    #[test]
    fn mysql_event_id_check_does_not_use_binary_regex_operands() {
        let rendered = Query::select()
            .expr(hex_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
