use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{playground_shares, users};

use super::{nullable_timestamp, table, timestamp};

/// 创建 Playground 只读分享的有界不可变快照表。
pub(in crate::migration) async fn create_playground_shares(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut expires_at = nullable_timestamp(manager, playground_shares::Column::ExpiresAt);
    expires_at.not_null();

    let mut statement = table(manager, playground_shares::Entity);
    statement
        .col(
            ColumnDef::new(playground_shares::Column::Id)
                .big_integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
        .col(
            ColumnDef::new(playground_shares::Column::OwnerUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(playground_shares::Column::TokenHash)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(playground_shares::Column::Snapshot)
                .json_binary()
                .not_null(),
        )
        .col(timestamp(manager, playground_shares::Column::CreatedAt))
        .col(expires_at)
        .col(nullable_timestamp(
            manager,
            playground_shares::Column::RevokedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_playground_shares_owner")
                .from(
                    playground_shares::Entity,
                    playground_shares::Column::OwnerUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .check(token_hash_format_check(manager.get_database_backend()))
        .check(
            Expr::col(playground_shares::Column::ExpiresAt)
                .gt(Expr::col(playground_shares::Column::CreatedAt)),
        )
        .check(
            Expr::col(playground_shares::Column::RevokedAt)
                .is_null()
                .or(Expr::col(playground_shares::Column::RevokedAt)
                    .gte(Expr::col(playground_shares::Column::CreatedAt))),
        );
    manager.create_table(statement).await
}

/// 创建分享令牌定位、所有者容量和过期清理所需索引。
pub(in crate::migration) async fn create_playground_shares_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_playground_shares_token_hash")
            .table(playground_shares::Entity)
            .col(playground_shares::Column::TokenHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_playground_shares_owner_created")
            .table(playground_shares::Entity)
            .col(playground_shares::Column::OwnerUserId)
            .col(playground_shares::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_playground_shares_expires_at")
            .table(playground_shares::Entity)
            .col(playground_shares::Column::ExpiresAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn token_hash_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""token_hash" ~ '^[0-9a-f]{64}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`token_hash`) = 64 AND `token_hash` REGEXP '^[0-9a-f]{64}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("token_hash") = 64 AND "token_hash" NOT GLOB '*[^0-9a-f]*'"#)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_token_hash_check_avoids_binary_regex_operands() {
        let rendered = Query::select()
            .expr(token_hash_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{64}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
