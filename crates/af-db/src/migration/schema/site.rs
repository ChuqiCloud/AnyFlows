use sea_orm_migration::prelude::*;

use crate::migration::iden::{authentication_settings, groups, site_settings};

use super::{table, timestamp};

/// 创建固定主键的站点身份与公开品牌设置表。
pub(in crate::migration) async fn create_site_settings(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, site_settings::Entity);
    statement
        .col(
            ColumnDef::new(site_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(site_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(site_settings::Column::SiteName)
                .string_len(80)
                .not_null()
                .default("AnyFlows")
                .check(Expr::col(site_settings::Column::SiteName).ne("")),
        )
        .col(ColumnDef::new(site_settings::Column::PublicBaseUrl).string_len(2048))
        .col(ColumnDef::new(site_settings::Column::BrandLogoUrl).string_len(2048))
        .col(ColumnDef::new(site_settings::Column::BrandTagline).string_len(160))
        .col(ColumnDef::new(site_settings::Column::BrandDescription).string_len(500))
        .col(
            ColumnDef::new(site_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(site_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, site_settings::Column::CreatedAt))
        .col(timestamp(manager, site_settings::Column::UpdatedAt));
    manager.create_table(statement).await?;

    // 固定行让跨实例更新竞争同一数据库锁，并为未配置实例提供稳定品牌回退。
    manager
        .exec_stmt(
            Query::insert()
                .into_table(site_settings::Entity)
                .columns([
                    site_settings::Column::Id,
                    site_settings::Column::SiteName,
                    site_settings::Column::Version,
                ])
                .values_panic([1_i16.into(), "AnyFlows".into(), 1_i64.into()])
                .to_owned(),
        )
        .await
}

/// 创建固定主键的密码登录与公开注册设置表。
pub(in crate::migration) async fn create_authentication_settings(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, authentication_settings::Entity);
    statement
        .col(
            ColumnDef::new(authentication_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(authentication_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::PasswordLoginEnabled)
                .boolean()
                .not_null()
                .default(true),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::RegistrationEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::RegistrationDefaultGroupId)
                .big_integer(),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::RegistrationInitialQuota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(
                    Expr::col(authentication_settings::Column::RegistrationInitialQuota).gte(0_i64),
                ),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::RegistrationEmailRequired)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::RegistrationRateLimitAttempts)
                .integer()
                .not_null()
                .default(5_i32)
                .check(
                    Expr::col(authentication_settings::Column::RegistrationRateLimitAttempts)
                        .gte(1_i32),
                )
                .check(
                    Expr::col(authentication_settings::Column::RegistrationRateLimitAttempts)
                        .lte(100_i32),
                ),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::RegistrationRateLimitWindowSeconds)
                .big_integer()
                .not_null()
                .default(3_600_i64)
                .check(
                    Expr::col(authentication_settings::Column::RegistrationRateLimitWindowSeconds)
                        .gte(60_i64),
                )
                .check(
                    Expr::col(authentication_settings::Column::RegistrationRateLimitWindowSeconds)
                        .lte(86_400_i64),
                ),
        )
        .col(
            ColumnDef::new(authentication_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(authentication_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(
            manager,
            authentication_settings::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            authentication_settings::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .from(
                    authentication_settings::Entity,
                    authentication_settings::Column::RegistrationDefaultGroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(
            Expr::col(authentication_settings::Column::RegistrationEnabled)
                .eq(false)
                .or(Expr::col(authentication_settings::Column::PasswordLoginEnabled).eq(true)),
        )
        .check(
            Expr::col(authentication_settings::Column::RegistrationEnabled)
                .eq(false)
                .or(
                    Expr::col(authentication_settings::Column::RegistrationDefaultGroupId)
                        .is_not_null(),
                ),
        );
    manager.create_table(statement).await?;

    // 认证设置先以登录开启、注册关闭落地，旧注册策略由紧随其后的迁移步骤搬运。
    manager
        .exec_stmt(
            Query::insert()
                .into_table(authentication_settings::Entity)
                .columns([
                    authentication_settings::Column::Id,
                    authentication_settings::Column::PasswordLoginEnabled,
                    authentication_settings::Column::RegistrationEnabled,
                    authentication_settings::Column::RegistrationInitialQuota,
                    authentication_settings::Column::RegistrationEmailRequired,
                    authentication_settings::Column::RegistrationRateLimitAttempts,
                    authentication_settings::Column::RegistrationRateLimitWindowSeconds,
                    authentication_settings::Column::Version,
                ])
                .values_panic([
                    1_i16.into(),
                    true.into(),
                    false.into(),
                    0_i64.into(),
                    false.into(),
                    5_i32.into(),
                    3_600_i64.into(),
                    1_i64.into(),
                ])
                .to_owned(),
        )
        .await
}
