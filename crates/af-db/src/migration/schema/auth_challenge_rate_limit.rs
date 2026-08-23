use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::auth_challenge_rate_limits;

use super::{auto_id, table, timestamp};

const REGISTRATION_EMAIL_PURPOSE: i16 = 1;
const PASSWORD_RESET_PURPOSE: i16 = 2;
const PASSKEY_AUTHENTICATION_PURPOSE: i16 = 3;
const SUBJECT_SCOPE: i16 = 1;
const CLIENT_IP_SCOPE: i16 = 2;
const MAX_ATTEMPTS: i32 = 100;

#[derive(Clone, Copy, DeriveIden)]
enum TemporaryTable {
    #[sea_orm(iden = "auth_challenge_rate_limits_passkey")]
    Passkey,
    #[sea_orm(iden = "auth_challenge_rate_limits_legacy")]
    Legacy,
}

/// 创建认证挑战发送的主体与客户端 IP 双作用域固定窗口限流表。
pub(in crate::migration) async fn create_auth_challenge_rate_limits(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_rate_limit_table(manager, auth_challenge_rate_limits::Entity, false).await?;
    create_rate_limit_index(manager).await
}

/// 无损重建短期限流表，扩展或收回 Passkey 登录用途约束。
pub(in crate::migration) async fn rebuild_auth_challenge_rate_limits_for_passkey(
    manager: &SchemaManager<'_>,
    include_passkey: bool,
) -> Result<(), DbErr> {
    let (temporary_table, temporary_table_name) = if include_passkey {
        (
            TemporaryTable::Passkey,
            "auth_challenge_rate_limits_passkey",
        )
    } else {
        (TemporaryTable::Legacy, "auth_challenge_rate_limits_legacy")
    };
    create_rate_limit_table(manager, temporary_table, include_passkey).await?;
    let quoted = match manager.get_database_backend() {
        DbBackend::MySql => "`",
        DbBackend::Postgres | DbBackend::Sqlite => "\"",
    };
    let filter = if include_passkey {
        String::new()
    } else {
        format!(" WHERE {quoted}purpose{quoted} <> {PASSKEY_AUTHENTICATION_PURPOSE}")
    };
    manager
        .get_connection()
        .execute_unprepared(&format!(
            "INSERT INTO {quoted}{temporary_table_name}{quoted} \
             ({quoted}purpose{quoted}, {quoted}scope{quoted}, {quoted}fingerprint{quoted}, \
              {quoted}window_started_at{quoted}, {quoted}attempts{quoted}, \
              {quoted}created_at{quoted}, {quoted}updated_at{quoted}) \
             SELECT {quoted}purpose{quoted}, {quoted}scope{quoted}, {quoted}fingerprint{quoted}, \
                    {quoted}window_started_at{quoted}, {quoted}attempts{quoted}, \
                    {quoted}created_at{quoted}, {quoted}updated_at{quoted} \
             FROM {quoted}auth_challenge_rate_limits{quoted}{filter}"
        ))
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(auth_challenge_rate_limits::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .rename_table(
            Table::rename()
                .table(temporary_table, auth_challenge_rate_limits::Entity)
                .to_owned(),
        )
        .await?;
    create_rate_limit_index(manager).await
}

async fn create_rate_limit_table<T>(
    manager: &SchemaManager<'_>,
    table_name: T,
    include_passkey: bool,
) -> Result<(), DbErr>
where
    T: IntoTableRef,
{
    let allowed_purposes = if include_passkey {
        vec![
            REGISTRATION_EMAIL_PURPOSE,
            PASSWORD_RESET_PURPOSE,
            PASSKEY_AUTHENTICATION_PURPOSE,
        ]
    } else {
        vec![REGISTRATION_EMAIL_PURPOSE, PASSWORD_RESET_PURPOSE]
    };
    let statement = table(manager, table_name)
        .col(auto_id(auth_challenge_rate_limits::Column::Id))
        .col(
            ColumnDef::new(auth_challenge_rate_limits::Column::Purpose)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(auth_challenge_rate_limits::Column::Scope)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(auth_challenge_rate_limits::Column::Fingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(auth_challenge_rate_limits::Column::WindowStartedAt)
                .big_integer()
                .not_null()
                .check(Expr::col(auth_challenge_rate_limits::Column::WindowStartedAt).gte(0_i64)),
        )
        .col(
            ColumnDef::new(auth_challenge_rate_limits::Column::Attempts)
                .integer()
                .not_null()
                .default(0_i32)
                .check(Expr::col(auth_challenge_rate_limits::Column::Attempts).gte(0_i32))
                .check(Expr::col(auth_challenge_rate_limits::Column::Attempts).lte(MAX_ATTEMPTS)),
        )
        .col(timestamp(
            manager,
            auth_challenge_rate_limits::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            auth_challenge_rate_limits::Column::UpdatedAt,
        ))
        .check(Expr::col(auth_challenge_rate_limits::Column::Purpose).is_in(allowed_purposes))
        .check(
            Expr::col(auth_challenge_rate_limits::Column::Scope)
                .is_in([SUBJECT_SCOPE, CLIENT_IP_SCOPE]),
        )
        .check(fingerprint_format_check(manager.get_database_backend()))
        .to_owned();
    manager.create_table(statement).await
}

async fn create_rate_limit_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("uq_auth_challenge_rate_limits_scope")
                .table(auth_challenge_rate_limits::Entity)
                .col(auth_challenge_rate_limits::Column::Purpose)
                .col(auth_challenge_rate_limits::Column::Scope)
                .col(auth_challenge_rate_limits::Column::Fingerprint)
                .unique()
                .to_owned(),
        )
        .await
}

fn fingerprint_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""fingerprint" ~ '^[0-9a-f]{64}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`fingerprint`) = 64 AND `fingerprint` REGEXP '^[0-9a-f]{64}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("fingerprint") = 64 AND "fingerprint" NOT GLOB '*[^0-9a-f]*'"#)
        }
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
