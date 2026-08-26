use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_scim_tokens as token_iden, organizations as organization_iden},
    schema::{auto_id, table, timestamp},
};

/// 创建企业 SCIM 令牌摘要表；不提供明文令牌列。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, token_iden::Entity)
                    .if_not_exists()
                    .col(auto_id(token_iden::Column::Id))
                    .col(
                        ColumnDef::new(token_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::TokenKey)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::TokenPrefix)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::TokenDigest)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::Name)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::ResourceScope)
                            .small_integer()
                            .not_null()
                            .check(
                                Expr::col(token_iden::Column::ResourceScope).between(1_i16, 3_i16),
                            ),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::Status)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(Expr::col(token_iden::Column::Status).between(1_i16, 2_i16)),
                    )
                    .col(
                        ColumnDef::new(token_iden::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(token_iden::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, token_iden::Column::CreatedAt))
                    .col(timestamp(manager, token_iden::Column::UpdatedAt))
                    .col(
                        ColumnDef::new(token_iden::Column::RevokedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_scim_tokens_organization")
                            .from(token_iden::Entity, token_iden::Column::OrganizationId)
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        // 索引单独创建，避免 PostgreSQL 将索引片段误拼进建表 DDL。
        manager
            .create_index(
                Index::create()
                    .name("uq_organization_scim_tokens_token_key")
                    .table(token_iden::Entity)
                    .col(token_iden::Column::TokenKey)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_organization_scim_tokens_token_digest")
                    .table(token_iden::Entity)
                    .col(token_iden::Column::TokenDigest)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_organization_scim_tokens_organization_status_id")
                    .table(token_iden::Entity)
                    .col(token_iden::Column::OrganizationId)
                    .col(token_iden::Column::Status)
                    .col(token_iden::Column::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(token_iden::Entity).to_owned())
            .await
    }
}
