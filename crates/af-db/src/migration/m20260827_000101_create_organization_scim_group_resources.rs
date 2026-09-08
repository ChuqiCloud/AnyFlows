use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_scim_group_resources as resource_iden, organization_teams as team_iden,
        organizations as organization_iden,
    },
    schema::{auto_id, table, timestamp},
};

/// 创建企业 SCIM 组资源与团队绑定表，删除事实保留以禁止恢复。
#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, resource_iden::Entity)
                    .if_not_exists()
                    .col(auto_id(resource_iden::Column::Id))
                    .col(
                        ColumnDef::new(resource_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(resource_iden::Column::ResourceId)
                            .string_len(256)
                            .not_null()
                            .check(Expr::col(resource_iden::Column::ResourceId).ne("")),
                    )
                    .col(
                        ColumnDef::new(resource_iden::Column::TeamId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(resource_iden::Column::ResourceVersion)
                            .big_integer()
                            .not_null()
                            .check(Expr::col(resource_iden::Column::ResourceVersion).gt(0_i64)),
                    )
                    .col(
                        ColumnDef::new(resource_iden::Column::Status)
                            .small_integer()
                            .not_null()
                            .check(Expr::col(resource_iden::Column::Status).is_in([1_i16, 2_i16])),
                    )
                    .col(timestamp(manager, resource_iden::Column::CreatedAt))
                    .col(timestamp(manager, resource_iden::Column::UpdatedAt))
                    .col(
                        ColumnDef::new(resource_iden::Column::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_scim_group_resources_organization")
                            .from(resource_iden::Entity, resource_iden::Column::OrganizationId)
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_scim_group_resources_team")
                            .from(resource_iden::Entity, resource_iden::Column::TeamId)
                            .to(team_iden::Entity, team_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("uq_organization_scim_group_resources_scope_resource")
                .table(resource_iden::Entity)
                .col(resource_iden::Column::OrganizationId)
                .col(resource_iden::Column::ResourceId)
                .unique()
                .to_owned(),
            Index::create()
                .name("uq_organization_scim_group_resources_scope_team")
                .table(resource_iden::Entity)
                .col(resource_iden::Column::OrganizationId)
                .col(resource_iden::Column::TeamId)
                .unique()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(resource_iden::Entity).to_owned())
            .await
    }
}
