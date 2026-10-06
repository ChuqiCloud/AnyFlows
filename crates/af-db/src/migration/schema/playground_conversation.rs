use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{playground_conversations, users};

use super::{table, timestamp};

/// 创建 Playground 所有者私有、带乐观版本的会话历史表。
pub(in crate::migration) async fn create_playground_conversations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, playground_conversations::Entity);
    statement
        .col(
            ColumnDef::new(playground_conversations::Column::ConversationId)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(playground_conversations::Column::OwnerUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(playground_conversations::Column::Title)
                .string_len(120)
                .not_null(),
        )
        .col(
            ColumnDef::new(playground_conversations::Column::Models)
                .json_binary()
                .not_null(),
        )
        .col(
            ColumnDef::new(playground_conversations::Column::Snapshot)
                .json_binary()
                .not_null(),
        )
        .col(
            ColumnDef::new(playground_conversations::Column::Revision)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            playground_conversations::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            playground_conversations::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_playground_conversations_owner")
                .from(
                    playground_conversations::Entity,
                    playground_conversations::Column::OwnerUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .check(conversation_id_format_check(manager.get_database_backend()))
        .check(Expr::col(playground_conversations::Column::Title).ne(""))
        .check(Expr::col(playground_conversations::Column::Revision).gte(1_i64))
        .check(
            Expr::col(playground_conversations::Column::UpdatedAt)
                .gte(Expr::col(playground_conversations::Column::CreatedAt)),
        );
    manager.create_table(statement).await
}

/// 创建所有者最近更新排序所需的稳定复合索引。
pub(in crate::migration) async fn create_playground_conversation_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_playground_conversations_owner_updated")
                .table(playground_conversations::Entity)
                .col(playground_conversations::Column::OwnerUserId)
                .col(playground_conversations::Column::UpdatedAt)
                .col(playground_conversations::Column::ConversationId)
                .to_owned(),
        )
        .await
}

fn conversation_id_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""conversation_id" ~ '^[0-9a-f]{32}$' AND "conversation_id" <> '00000000000000000000000000000000'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`conversation_id`) = 32 AND `conversation_id` REGEXP '^[0-9a-f]{32}$' AND `conversation_id` <> '00000000000000000000000000000000'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("conversation_id") = 32 AND "conversation_id" NOT GLOB '*[^0-9a-f]*' AND "conversation_id" <> '00000000000000000000000000000000'"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_conversation_id_check_uses_valid_identifier_quotes() {
        let rendered = Query::select()
            .expr(conversation_id_format_check(DbBackend::Postgres))
            .to_owned()
            .to_string(PostgresQueryBuilder);

        assert_eq!(
            rendered,
            "SELECT \"conversation_id\" ~ '^[0-9a-f]{32}$' AND \"conversation_id\" <> '00000000000000000000000000000000'"
        );
    }

    #[test]
    fn mysql_conversation_id_check_avoids_binary_regex_operands() {
        let rendered = Query::select()
            .expr(conversation_id_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }

    #[test]
    fn sqlite_conversation_id_check_uses_valid_identifier_quotes() {
        let rendered = Query::select()
            .expr(conversation_id_format_check(DbBackend::Sqlite))
            .to_owned()
            .to_string(SqliteQueryBuilder);

        assert_eq!(
            rendered,
            "SELECT length(\"conversation_id\") = 32 AND \"conversation_id\" NOT GLOB '*[^0-9a-f]*' AND \"conversation_id\" <> '00000000000000000000000000000000'"
        );
    }
}
