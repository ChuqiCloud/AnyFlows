use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_service_account_keys as key_iden,
        organization_service_accounts as account_iden, organizations as organization_iden,
    },
    schema::{auto_id, table, timestamp},
};

/// 创建企业服务账号主体和密钥摘要/轮换事实表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, account_iden::Entity)
                    .if_not_exists()
                    .col(auto_id(account_iden::Column::Id))
                    .col(
                        ColumnDef::new(account_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(account_iden::Column::PublicKey)
                            .char_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(account_iden::Column::Name)
                            .string_len(128)
                            .not_null()
                            .check(Expr::col(account_iden::Column::Name).ne("")),
                    )
                    .col(
                        ColumnDef::new(account_iden::Column::Status)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(Expr::col(account_iden::Column::Status).between(1_i16, 3_i16)),
                    )
                    .col(
                        ColumnDef::new(account_iden::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(account_iden::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, account_iden::Column::CreatedAt))
                    .col(timestamp(manager, account_iden::Column::UpdatedAt))
                    .col(
                        ColumnDef::new(account_iden::Column::RevokedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_service_accounts_organization")
                            .from(account_iden::Entity, account_iden::Column::OrganizationId)
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("uq_organization_service_accounts_scope_public_key")
                .table(account_iden::Entity)
                .col(account_iden::Column::OrganizationId)
                .col(account_iden::Column::PublicKey)
                .unique()
                .to_owned(),
            Index::create()
                .name("uq_organization_service_accounts_scope_id")
                .table(account_iden::Entity)
                .col(account_iden::Column::OrganizationId)
                .col(account_iden::Column::Id)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_organization_service_accounts_scope_status_id")
                .table(account_iden::Entity)
                .col(account_iden::Column::OrganizationId)
                .col(account_iden::Column::Status)
                .col(account_iden::Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }

        manager
            .create_table(
                table(manager, key_iden::Entity)
                    .if_not_exists()
                    .col(auto_id(key_iden::Column::Id))
                    .col(
                        ColumnDef::new(key_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(key_iden::Column::ServiceAccountId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(key_iden::Column::KeyKey)
                            .char_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(key_iden::Column::KeyPrefix)
                            .string_len(3)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(key_iden::Column::KeyDigest)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(key_iden::Column::Status)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(Expr::col(key_iden::Column::Status).between(1_i16, 2_i16)),
                    )
                    .col(
                        ColumnDef::new(key_iden::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(key_iden::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, key_iden::Column::CreatedAt))
                    .col(
                        ColumnDef::new(key_iden::Column::RevokedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_service_account_keys_organization")
                            .from(key_iden::Entity, key_iden::Column::OrganizationId)
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_service_account_keys_account")
                            .from(key_iden::Entity, key_iden::Column::ServiceAccountId)
                            .to(account_iden::Entity, account_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("uq_organization_service_account_keys_key_key")
                .table(key_iden::Entity)
                .col(key_iden::Column::KeyKey)
                .unique()
                .to_owned(),
            Index::create()
                .name("uq_organization_service_account_keys_key_digest")
                .table(key_iden::Entity)
                .col(key_iden::Column::KeyDigest)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_organization_service_account_keys_scope_account_status")
                .table(key_iden::Entity)
                .col(key_iden::Column::OrganizationId)
                .col(key_iden::Column::ServiceAccountId)
                .col(key_iden::Column::Status)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(key_iden::Entity).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(account_iden::Entity).to_owned())
            .await
    }
}
