use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{auth_challenges, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

const REGISTRATION_EMAIL_PURPOSE: i16 = 1;
const PASSWORD_RESET_PURPOSE: i16 = 2;
const MAX_ATTEMPTS: i32 = 10;

/// 创建跨实例认证挑战表；邮箱、验证码和重置令牌明文均不进入数据库。
pub(in crate::migration) async fn create_auth_challenges(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut issued_at = nullable_timestamp(manager, auth_challenges::Column::IssuedAt);
    issued_at.not_null();
    let mut expires_at = nullable_timestamp(manager, auth_challenges::Column::ExpiresAt);
    expires_at.not_null();
    let mut next_send_at = nullable_timestamp(manager, auth_challenges::Column::NextSendAt);
    next_send_at.not_null();

    let mut statement = table(manager, auth_challenges::Entity);
    statement
        .col(auto_id(auth_challenges::Column::Id))
        .col(
            ColumnDef::new(auth_challenges::Column::Purpose)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(auth_challenges::Column::SubjectFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(auth_challenges::Column::SecretDigest)
                .char_len(64)
                .not_null(),
        )
        .col(ColumnDef::new(auth_challenges::Column::TargetUserId).big_integer())
        .col(
            ColumnDef::new(auth_challenges::Column::Attempts)
                .integer()
                .not_null()
                .default(0_i32),
        )
        .col(
            ColumnDef::new(auth_challenges::Column::MaxAttempts)
                .integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(auth_challenges::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(issued_at)
        .col(expires_at)
        .col(next_send_at)
        .col(nullable_timestamp(
            manager,
            auth_challenges::Column::ConsumedAt,
        ))
        .col(timestamp(manager, auth_challenges::Column::CreatedAt))
        .col(timestamp(manager, auth_challenges::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_auth_challenges_target_user")
                .from(
                    auth_challenges::Entity,
                    auth_challenges::Column::TargetUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .check(
            Expr::col(auth_challenges::Column::Purpose)
                .is_in([REGISTRATION_EMAIL_PURPOSE, PASSWORD_RESET_PURPOSE]),
        )
        .check(Expr::col(auth_challenges::Column::Attempts).gte(0_i32))
        .check(
            Expr::col(auth_challenges::Column::Attempts)
                .lte(Expr::col(auth_challenges::Column::MaxAttempts)),
        )
        .check(Expr::col(auth_challenges::Column::MaxAttempts).between(1_i32, MAX_ATTEMPTS))
        .check(Expr::col(auth_challenges::Column::Version).gte(1_i64))
        .check(
            Expr::col(auth_challenges::Column::ExpiresAt)
                .gt(Expr::col(auth_challenges::Column::IssuedAt)),
        )
        .check(
            Expr::col(auth_challenges::Column::NextSendAt)
                .gte(Expr::col(auth_challenges::Column::IssuedAt))
                .and(
                    Expr::col(auth_challenges::Column::NextSendAt)
                        .lte(Expr::col(auth_challenges::Column::ExpiresAt)),
                ),
        )
        .check(
            Expr::col(auth_challenges::Column::ConsumedAt)
                .is_null()
                .or(Expr::col(auth_challenges::Column::ConsumedAt)
                    .gte(Expr::col(auth_challenges::Column::IssuedAt))
                    .and(
                        Expr::col(auth_challenges::Column::ConsumedAt)
                            .lt(Expr::col(auth_challenges::Column::ExpiresAt)),
                    )),
        )
        .check(hash_format_check(manager.get_database_backend()));
    // MySQL 8.4 禁止 CHECK 引用带级联动作的外键列；该方言由实体与仓储双重校验同一不变量。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(purpose_target_check());
    }
    manager.create_table(statement).await
}

fn purpose_target_check() -> SimpleExpr {
    Expr::col(auth_challenges::Column::Purpose)
        .eq(REGISTRATION_EMAIL_PURPOSE)
        .and(Expr::col(auth_challenges::Column::TargetUserId).is_null())
        .or(Expr::col(auth_challenges::Column::Purpose)
            .eq(PASSWORD_RESET_PURPOSE)
            .and(Expr::col(auth_challenges::Column::TargetUserId).is_not_null()))
}

/// 创建挑战定位、用户撤销和过期清理所需索引。
pub(in crate::migration) async fn create_auth_challenge_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_auth_challenges_purpose_subject")
            .table(auth_challenges::Entity)
            .col(auth_challenges::Column::Purpose)
            .col(auth_challenges::Column::SubjectFingerprint)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_auth_challenges_target_purpose")
            .table(auth_challenges::Entity)
            .col(auth_challenges::Column::TargetUserId)
            .col(auth_challenges::Column::Purpose)
            .to_owned(),
        Index::create()
            .name("idx_auth_challenges_expires_at")
            .table(auth_challenges::Entity)
            .col(auth_challenges::Column::ExpiresAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn hash_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""subject_fingerprint" ~ '^[0-9a-f]{64}$' AND "secret_digest" ~ '^[0-9a-f]{64}$'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`subject_fingerprint`) = 64 AND `subject_fingerprint` REGEXP '^[0-9a-f]{64}$' AND CHAR_LENGTH(`secret_digest`) = 64 AND `secret_digest` REGEXP '^[0-9a-f]{64}$'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("subject_fingerprint") = 64 AND "subject_fingerprint" NOT GLOB '*[^0-9a-f]*' AND length("secret_digest") = 64 AND "secret_digest" NOT GLOB '*[^0-9a-f]*'"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_hash_check_avoids_binary_regex_operands() {
        let rendered = Query::select()
            .expr(hash_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{64}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
