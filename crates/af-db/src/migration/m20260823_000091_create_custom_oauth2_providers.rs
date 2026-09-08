use sea_orm_migration::prelude::*;

use super::{
    iden::custom_oauth2_providers,
    schema::{table, timestamp},
};

/// 创建自定义 OAuth2 Provider 的独立持久化表，不开放公开登录入口。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = table(manager, custom_oauth2_providers::Entity);
        statement
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::ProviderKey)
                    .string_len(32)
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::DisplayName)
                    .string_len(128)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::ClientId)
                    .string_len(255)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::AuthorizationEndpoint)
                    .string_len(2048)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::TokenEndpoint)
                    .string_len(2048)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::UserinfoEndpoint)
                    .string_len(2048)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::Scope)
                    .string_len(2048)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::SubjectField)
                    .string_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::Enabled)
                    .boolean()
                    .not_null()
                    .default(false),
            )
            .col(ColumnDef::new(custom_oauth2_providers::Column::ClientSecret).json_binary())
            .col(
                ColumnDef::new(custom_oauth2_providers::Column::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64)
                    .check(Expr::col(custom_oauth2_providers::Column::Version).gte(1_i64)),
            )
            .col(timestamp(
                manager,
                custom_oauth2_providers::Column::CreatedAt,
            ))
            .col(timestamp(
                manager,
                custom_oauth2_providers::Column::UpdatedAt,
            ))
            .check(
                Expr::col(custom_oauth2_providers::Column::Enabled)
                    .eq(false)
                    .or(Expr::col(custom_oauth2_providers::Column::ClientSecret).is_not_null()),
            );
        manager.create_table(statement).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(custom_oauth2_providers::Entity)
                    .to_owned(),
            )
            .await
    }
}
