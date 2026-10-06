use sea_orm_migration::prelude::*;

use crate::migration::iden::network_settings;

use super::{table, timestamp};

/// 创建固定主键的全局出站网络设置表。
pub(in crate::migration) async fn create_network_settings(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, network_settings::Entity);
    statement
        .col(
            ColumnDef::new(network_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(network_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(network_settings::Column::Mode)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(network_settings::Column::Mode).is_in([1_i16, 2, 3, 4, 5, 6])),
        )
        .col(ColumnDef::new(network_settings::Column::ProxyHost).string_len(255))
        .col(
            ColumnDef::new(network_settings::Column::ProxyPort)
                .integer()
                .check(
                    Expr::col(network_settings::Column::ProxyPort)
                        .is_null()
                        .or(Expr::col(network_settings::Column::ProxyPort)
                            .between(1_i32, 65_535_i32)),
                ),
        )
        .col(ColumnDef::new(network_settings::Column::Username).string_len(320))
        .col(ColumnDef::new(network_settings::Column::PasswordSecret).json_binary())
        .col(
            ColumnDef::new(network_settings::Column::TrustProxyDns)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(network_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(network_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, network_settings::Column::CreatedAt))
        .col(timestamp(manager, network_settings::Column::UpdatedAt))
        .check(
            Expr::col(network_settings::Column::Mode)
                .is_in([1_i16, 2_i16])
                .and(Expr::col(network_settings::Column::ProxyHost).is_null())
                .and(Expr::col(network_settings::Column::ProxyPort).is_null())
                .and(Expr::col(network_settings::Column::Username).is_null())
                .and(Expr::col(network_settings::Column::PasswordSecret).is_null())
                .and(Expr::col(network_settings::Column::TrustProxyDns).eq(false))
                .or(Expr::col(network_settings::Column::Mode)
                    .is_in([3_i16, 4, 5, 6])
                    .and(Expr::col(network_settings::Column::ProxyHost).is_not_null())
                    .and(Expr::col(network_settings::Column::ProxyPort).is_not_null())),
        )
        .check(
            Expr::col(network_settings::Column::Username)
                .is_null()
                .and(Expr::col(network_settings::Column::PasswordSecret).is_null())
                .or(Expr::col(network_settings::Column::Username)
                    .is_not_null()
                    .and(Expr::col(network_settings::Column::PasswordSecret).is_not_null())),
        );
    manager.create_table(statement).await?;

    // 固定行保证首次保存与并发保存都共享同一行锁，并默认继承启动配置。
    manager
        .exec_stmt(
            Query::insert()
                .into_table(network_settings::Entity)
                .columns([
                    network_settings::Column::Id,
                    network_settings::Column::Mode,
                    network_settings::Column::TrustProxyDns,
                    network_settings::Column::Version,
                ])
                .values_panic([1_i16.into(), 1_i16.into(), false.into(), 1_i64.into()])
                .to_owned(),
        )
        .await
}
