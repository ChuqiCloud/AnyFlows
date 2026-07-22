use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::billing_batch_checkpoints;

use super::{table, timestamp};

/// 创建按 writer 保存最后已应用连续批次的 exactly-once 检查点表。
pub(in crate::migration) async fn create_billing_batch_checkpoints(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, billing_batch_checkpoints::Entity);
    statement
        .col(
            ColumnDef::new(billing_batch_checkpoints::Column::WriterKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(billing_batch_checkpoints::Column::LastStartSequence)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(billing_batch_checkpoints::Column::LastStartSequence).gt(0_i64),
                ),
        )
        .col(
            ColumnDef::new(billing_batch_checkpoints::Column::LastEndSequence)
                .big_integer()
                .not_null()
                .check(Expr::col(billing_batch_checkpoints::Column::LastEndSequence).gt(0_i64)),
        )
        .col(
            ColumnDef::new(billing_batch_checkpoints::Column::LastEventCount)
                .big_integer()
                .not_null()
                .check(Expr::col(billing_batch_checkpoints::Column::LastEventCount).gt(0_i64)),
        )
        .col(
            ColumnDef::new(billing_batch_checkpoints::Column::LastFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(timestamp(
            manager,
            billing_batch_checkpoints::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            billing_batch_checkpoints::Column::UpdatedAt,
        ))
        .check(
            Expr::col(billing_batch_checkpoints::Column::LastEndSequence)
                .gte(Expr::col(
                    billing_batch_checkpoints::Column::LastStartSequence,
                )),
        )
        // 每条 WAL 记录占用一个连续序号，事件数必须与保存的范围完全一致。
        .check(
            Expr::col(billing_batch_checkpoints::Column::LastEndSequence)
                .sub(Expr::col(
                    billing_batch_checkpoints::Column::LastStartSequence,
                ))
                .add(1_i64)
                .eq(Expr::col(
                    billing_batch_checkpoints::Column::LastEventCount,
                )),
        )
        .check(hex_format_check(
            manager.get_database_backend(),
            "writer_key",
            32,
        ))
        .check(
            Expr::col(billing_batch_checkpoints::Column::WriterKey)
                .ne("00000000000000000000000000000000"),
        )
        .check(hex_format_check(
            manager.get_database_backend(),
            "last_fingerprint",
            64,
        ));
    manager.create_table(statement).await
}

fn hex_format_check(database_backend: DbBackend, column: &str, length: usize) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(format!(r#""{column}" ~ '^[0-9a-f]{{{length}}}$'"#)),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{column}`) = {length} AND `{column}` REGEXP '^[0-9a-f]{{{length}}}$'"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            r#"length("{column}") = {length} AND "{column}" NOT GLOB '*[^0-9a-f]*'"#
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_hex_checks_do_not_use_binary_regex_operands() {
        for (column, length) in [("writer_key", 32), ("last_fingerprint", 64)] {
            let rendered = Query::select()
                .expr(hex_format_check(DbBackend::MySql, column, length))
                .to_owned()
                .to_string(MysqlQueryBuilder);

            assert!(rendered.contains("REGEXP '^[0-9a-f]"));
            assert!(!rendered.contains("BINARY"));
        }
    }
}
