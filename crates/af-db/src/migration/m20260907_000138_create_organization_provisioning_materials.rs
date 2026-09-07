use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_provisioning_materials, organization_provisioning_requests},
    schema::{auto_id, table, timestamp},
};

/// 企业开通申请材料正文；正文读取只通过管理员受控下载接口。
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use organization_provisioning_materials::Column;
        let table_statement = table(manager, organization_provisioning_materials::Entity)
            .col(auto_id(Column::Id))
            .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
            .col(ColumnDef::new(Column::Kind).string_len(64).not_null())
            .col(
                ColumnDef::new(Column::ObjectReference)
                    .string_len(256)
                    .not_null(),
            )
            .col(ColumnDef::new(Column::FileName).string_len(255).not_null())
            .col(
                ColumnDef::new(Column::ContentType)
                    .string_len(128)
                    .not_null(),
            )
            .col(ColumnDef::new(Column::SizeBytes).big_integer().not_null())
            .col(ColumnDef::new(Column::ContentBytes).blob())
            .col(timestamp(manager, Column::CreatedAt))
            .check(Expr::col(Column::SizeBytes).between(0_i64, 50_000_000_i64))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_provision_materials_request")
                    .from(
                        organization_provisioning_materials::Entity,
                        Column::RequestId,
                    )
                    .to(
                        organization_provisioning_requests::Entity,
                        organization_provisioning_requests::Column::Id,
                    )
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned();
        manager.create_table(table_statement).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_org_provision_materials_request")
                    .table(organization_provisioning_materials::Entity)
                    .col(Column::RequestId)
                    .col(Column::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_provisioning_materials::Entity)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
