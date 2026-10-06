use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::registration_rate_limits;

use super::{table, timestamp};

/// 创建公开注册跨实例固定窗口限流表。
pub(in crate::migration) async fn create_registration_rate_limits(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, registration_rate_limits::Entity);
    statement
        .col(
            ColumnDef::new(registration_rate_limits::Column::IpFingerprint)
                .char_len(64)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(registration_rate_limits::Column::WindowStartedAt)
                .big_integer()
                .not_null()
                .check(Expr::col(registration_rate_limits::Column::WindowStartedAt).gte(0_i64)),
        )
        .col(
            ColumnDef::new(registration_rate_limits::Column::Attempts)
                .integer()
                .not_null()
                .default(0_i32)
                .check(Expr::col(registration_rate_limits::Column::Attempts).gte(0_i32))
                .check(Expr::col(registration_rate_limits::Column::Attempts).lte(100_i32)),
        )
        .col(timestamp(
            manager,
            registration_rate_limits::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            registration_rate_limits::Column::UpdatedAt,
        ))
        .check(fingerprint_format_check(manager.get_database_backend()));
    manager.create_table(statement).await
}

fn fingerprint_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""ip_fingerprint" ~ '^[0-9a-f]{64}$'"#),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`ip_fingerprint`) = 64 AND `ip_fingerprint` REGEXP '^[0-9a-f]{64}$'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("ip_fingerprint") = 64 AND "ip_fingerprint" NOT GLOB '*[^0-9a-f]*'"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_fingerprint_check_avoids_binary_regex_operands() {
        let rendered = Query::select()
            .expr(fingerprint_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{64}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
