use sea_orm_migration::prelude::*;

use crate::migration::iden::email_settings;

use super::{table, timestamp};

/// 创建固定主键的系统 SMTP 邮件设置表。
pub(in crate::migration) async fn create_email_settings(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, email_settings::Entity);
    statement
        .col(
            ColumnDef::new(email_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(email_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(email_settings::Column::Enabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(email_settings::Column::Host)
                .string_len(255)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(email_settings::Column::Port)
                .integer()
                .not_null()
                .default(587_i32)
                .check(Expr::col(email_settings::Column::Port).gt(0_i32))
                .check(Expr::col(email_settings::Column::Port).lte(65_535_i32)),
        )
        .col(
            ColumnDef::new(email_settings::Column::TlsMode)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(email_settings::Column::TlsMode).is_in([1_i16, 2_i16])),
        )
        .col(ColumnDef::new(email_settings::Column::Username).string_len(320))
        .col(ColumnDef::new(email_settings::Column::PasswordSecret).json_binary())
        .col(
            ColumnDef::new(email_settings::Column::FromAddress)
                .string_len(320)
                .not_null()
                .default(""),
        )
        .col(ColumnDef::new(email_settings::Column::FromName).string_len(128))
        .col(ColumnDef::new(email_settings::Column::ReplyTo).string_len(320))
        .col(
            ColumnDef::new(email_settings::Column::TimeoutSeconds)
                .integer()
                .not_null()
                .default(10_i32)
                .check(Expr::col(email_settings::Column::TimeoutSeconds).gte(1_i32))
                .check(Expr::col(email_settings::Column::TimeoutSeconds).lte(60_i32)),
        )
        .col(
            ColumnDef::new(email_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(email_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, email_settings::Column::CreatedAt))
        .col(timestamp(manager, email_settings::Column::UpdatedAt))
        .check(
            Expr::col(email_settings::Column::Username)
                .is_null()
                .and(Expr::col(email_settings::Column::PasswordSecret).is_null())
                .or(Expr::col(email_settings::Column::Username)
                    .is_not_null()
                    .and(Expr::col(email_settings::Column::PasswordSecret).is_not_null())),
        )
        .check(
            Expr::col(email_settings::Column::Enabled)
                .eq(false)
                .or(Expr::col(email_settings::Column::Host)
                    .ne("")
                    .and(Expr::col(email_settings::Column::FromAddress).ne(""))),
        );
    manager.create_table(statement).await?;

    // 固定行让所有更新都能取得同一数据库锁，避免首次写入并发产生版本冲突。
    let seed = Query::insert()
        .into_table(email_settings::Entity)
        .columns([
            email_settings::Column::Id,
            email_settings::Column::Enabled,
            email_settings::Column::Host,
            email_settings::Column::Port,
            email_settings::Column::TlsMode,
            email_settings::Column::FromAddress,
            email_settings::Column::TimeoutSeconds,
            email_settings::Column::Version,
        ])
        .values_panic([
            1_i16.into(),
            false.into(),
            "".into(),
            587_i32.into(),
            1_i16.into(),
            "".into(),
            10_i32.into(),
            1_i64.into(),
        ])
        .to_owned();
    manager.exec_stmt(seed).await
}
