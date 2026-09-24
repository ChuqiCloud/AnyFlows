use sea_orm_migration::prelude::*;

use super::{
    iden::model_provider_catalog,
    schema::{table, timestamp},
};

/// 保存管理员维护的模型厂商展示目录，不影响渠道和模型的实际 provider 字符串。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = table(manager, model_provider_catalog::Entity);
        statement
            .col(
                ColumnDef::new(model_provider_catalog::Column::ProviderKey)
                    .string_len(64)
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(model_provider_catalog::Column::DisplayName)
                    .string_len(128)
                    .not_null(),
            )
            .col(ColumnDef::new(model_provider_catalog::Column::Logo).string_len(128))
            .col(
                ColumnDef::new(model_provider_catalog::Column::Aliases)
                    .json_binary()
                    .not_null(),
            )
            .col(
                ColumnDef::new(model_provider_catalog::Column::Enabled)
                    .boolean()
                    .not_null()
                    .default(true),
            )
            .col(
                ColumnDef::new(model_provider_catalog::Column::SortOrder)
                    .integer()
                    .not_null()
                    .default(0),
            )
            .col(
                ColumnDef::new(model_provider_catalog::Column::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64)
                    .check(Expr::col(model_provider_catalog::Column::Version).gte(1_i64)),
            )
            .col(timestamp(
                manager,
                model_provider_catalog::Column::CreatedAt,
            ))
            .col(timestamp(
                manager,
                model_provider_catalog::Column::UpdatedAt,
            ));
        manager.create_table(statement).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(model_provider_catalog::Entity)
                    .to_owned(),
            )
            .await
    }
}
