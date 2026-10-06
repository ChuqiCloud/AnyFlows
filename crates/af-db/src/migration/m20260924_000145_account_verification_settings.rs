use sea_orm_migration::prelude::*;

use super::{
    iden::account_verification_settings as settings,
    schema::{table, timestamp},
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = table(manager, settings::Entity);
        statement
            .col(
                ColumnDef::new(settings::Column::Id)
                    .small_integer()
                    .not_null()
                    .primary_key()
                    .check(Expr::col(settings::Column::Id).eq(1_i16)),
            )
            .col(
                ColumnDef::new(settings::Column::Initialized)
                    .boolean()
                    .not_null()
                    .default(false),
            )
            .col(
                ColumnDef::new(settings::Column::AlipayEnabled)
                    .boolean()
                    .not_null()
                    .default(false),
            )
            .col(ColumnDef::new(settings::Column::AlipayAppId).string_len(128))
            .col(ColumnDef::new(settings::Column::AlipayCredentials).json_binary())
            .col(
                ColumnDef::new(settings::Column::AlipayGatewayUrl)
                    .string_len(2048)
                    .not_null(),
            )
            .col(
                ColumnDef::new(settings::Column::AlipayBizCode)
                    .string_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(settings::Column::AlipayTimeoutSecs)
                    .integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(settings::Column::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(timestamp(manager, settings::Column::CreatedAt))
            .col(timestamp(manager, settings::Column::UpdatedAt));
        manager.create_table(statement).await?;
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(settings::Entity)
                    .columns([
                        settings::Column::Id,
                        settings::Column::Initialized,
                        settings::Column::AlipayEnabled,
                        settings::Column::AlipayGatewayUrl,
                        settings::Column::AlipayBizCode,
                        settings::Column::AlipayTimeoutSecs,
                        settings::Column::Version,
                    ])
                    .values_panic([
                        1_i16.into(),
                        false.into(),
                        false.into(),
                        "https://openapi.alipay.com/gateway.do".into(),
                        "FACE".into(),
                        8_i32.into(),
                        1_i64.into(),
                    ])
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(settings::Entity).to_owned())
            .await
    }
}
