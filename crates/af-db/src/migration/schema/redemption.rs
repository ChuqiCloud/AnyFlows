use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{redemption_batches, redemption_codes, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 单批兑换码允许生成的数据库硬上限。
const MAX_BATCH_CODES: i32 = 1_000;

/// 创建兑换码批次与仅摘要码表。
pub(in crate::migration) async fn create_redemption_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_batches(manager).await?;
    create_codes(manager).await?;
    create_indexes(manager).await
}

async fn create_batches(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, redemption_batches::Entity);
    statement
        .col(auto_id(redemption_batches::Column::Id))
        .col(
            ColumnDef::new(redemption_batches::Column::BatchKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_batches::Column::Name)
                .string_len(80)
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_batches::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_batches::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(
            ColumnDef::new(redemption_batches::Column::QuotaAmount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_batches::Column::CodeCount)
                .integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_batches::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(nullable_timestamp(
            manager,
            redemption_batches::Column::ExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            redemption_batches::Column::DisabledAt,
        ))
        .col(timestamp(manager, redemption_batches::Column::CreatedAt))
        .col(timestamp(manager, redemption_batches::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_redemption_batches_creator")
                .from(
                    redemption_batches::Entity,
                    redemption_batches::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_check(
            manager.get_database_backend(),
            "batch_key",
            32,
            true,
        ))
        .check(Expr::col(redemption_batches::Column::Name).ne(""))
        .check(Expr::col(redemption_batches::Column::Status).is_in([1_i16, 2_i16]))
        .check(Expr::col(redemption_batches::Column::QuotaAmount).gt(0_i64))
        .check(Expr::col(redemption_batches::Column::CodeCount).between(1_i32, MAX_BATCH_CODES))
        .check(Expr::col(redemption_batches::Column::Version).gt(0_i64))
        .check(
            Expr::col(redemption_batches::Column::ExpiresAt)
                .is_null()
                .or(Expr::col(redemption_batches::Column::ExpiresAt)
                    .gt(Expr::col(redemption_batches::Column::CreatedAt))),
        )
        .check(
            Expr::col(redemption_batches::Column::Status)
                .eq(1_i16)
                .and(Expr::col(redemption_batches::Column::DisabledAt).is_null())
                .or(Expr::col(redemption_batches::Column::Status)
                    .eq(2_i16)
                    .and(Expr::col(redemption_batches::Column::DisabledAt).is_not_null())),
        )
        .check(
            Expr::col(redemption_batches::Column::DisabledAt)
                .is_null()
                .or(Expr::col(redemption_batches::Column::DisabledAt)
                    .gte(Expr::col(redemption_batches::Column::CreatedAt))),
        )
        .check(
            Expr::col(redemption_batches::Column::UpdatedAt)
                .gte(Expr::col(redemption_batches::Column::CreatedAt)),
        )
        .check(
            Expr::col(redemption_batches::Column::DisabledAt)
                .is_null()
                .or(Expr::col(redemption_batches::Column::UpdatedAt)
                    .gte(Expr::col(redemption_batches::Column::DisabledAt))),
        );
    manager.create_table(statement).await
}

async fn create_codes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, redemption_codes::Entity);
    statement
        .col(auto_id(redemption_codes::Column::Id))
        .col(
            ColumnDef::new(redemption_codes::Column::CodeKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_codes::Column::BatchId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_codes::Column::CodeSha256)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(redemption_codes::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(ColumnDef::new(redemption_codes::Column::UsedByUserId).big_integer())
        .col(nullable_timestamp(
            manager,
            redemption_codes::Column::RedeemedAt,
        ))
        .col(timestamp(manager, redemption_codes::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_redemption_codes_batch")
                .from(redemption_codes::Entity, redemption_codes::Column::BatchId)
                .to(redemption_batches::Entity, redemption_batches::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_redemption_codes_used_by")
                .from(
                    redemption_codes::Entity,
                    redemption_codes::Column::UsedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_check(
            manager.get_database_backend(),
            "code_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "code_sha256",
            64,
            false,
        ))
        .check(Expr::col(redemption_codes::Column::Status).is_in([1_i16, 2_i16]))
        .check(
            Expr::col(redemption_codes::Column::Status)
                .eq(1_i16)
                .and(Expr::col(redemption_codes::Column::RedeemedAt).is_null())
                .or(Expr::col(redemption_codes::Column::Status)
                    .eq(2_i16)
                    .and(Expr::col(redemption_codes::Column::RedeemedAt).is_not_null())),
        )
        .check(
            Expr::col(redemption_codes::Column::RedeemedAt)
                .is_null()
                .or(Expr::col(redemption_codes::Column::RedeemedAt)
                    .gte(Expr::col(redemption_codes::Column::CreatedAt))),
        );

    // MySQL 8.4 禁止 CHECK 引用带级联动作的外键列；该方言只把使用者绑定交给仓储读取边界校验。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(
            Expr::col(redemption_codes::Column::Status)
                .eq(1_i16)
                .and(Expr::col(redemption_codes::Column::UsedByUserId).is_null())
                .or(Expr::col(redemption_codes::Column::Status)
                    .eq(2_i16)
                    .and(Expr::col(redemption_codes::Column::UsedByUserId).is_not_null())),
        );
    }
    manager.create_table(statement).await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_redemption_batches_batch_key")
            .table(redemption_batches::Entity)
            .col(redemption_batches::Column::BatchKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_redemption_batches_status_expires")
            .table(redemption_batches::Entity)
            .col(redemption_batches::Column::Status)
            .col(redemption_batches::Column::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("uq_redemption_codes_code_key")
            .table(redemption_codes::Entity)
            .col(redemption_codes::Column::CodeKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_redemption_codes_sha256")
            .table(redemption_codes::Entity)
            .col(redemption_codes::Column::CodeSha256)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_redemption_codes_batch_status")
            .table(redemption_codes::Entity)
            .col(redemption_codes::Column::BatchId)
            .col(redemption_codes::Column::Status)
            .col(redemption_codes::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_redemption_codes_user_redeemed")
            .table(redemption_codes::Entity)
            .col(redemption_codes::Column::UsedByUserId)
            .col(redemption_codes::Column::RedeemedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{MysqlQueryBuilder, Query};

    use super::*;

    #[test]
    fn mysql_hash_checks_do_not_use_binary_regex_operands() {
        for (column, length, non_zero) in [("batch_key", 32, true), ("code_sha256", 64, false)] {
            let rendered = Query::select()
                .expr(hex_check(DbBackend::MySql, column, length, non_zero))
                .to_owned()
                .to_string(MysqlQueryBuilder);
            assert!(rendered.contains("REGEXP '^[0-9a-f]"));
            assert!(!rendered.contains("BINARY"));
        }
    }
}
