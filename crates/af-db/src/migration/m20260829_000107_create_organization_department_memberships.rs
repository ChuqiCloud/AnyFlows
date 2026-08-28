use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_department_memberships as binding, organization_departments,
        organization_memberships, organizations,
    },
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

/// 创建企业成员与部门的多归属事实表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, binding::Entity)
                    .if_not_exists()
                    .col(auto_id(binding::Column::Id))
                    .col(
                        ColumnDef::new(binding::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(binding::Column::MembershipId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(binding::Column::DepartmentId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(binding::Column::IsPrimary)
                            .small_integer()
                            .not_null()
                            .default(0_i16)
                            .check(Expr::col(binding::Column::IsPrimary).between(0_i16, 1_i16)),
                    )
                    .col(
                        ColumnDef::new(binding::Column::Status)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(Expr::col(binding::Column::Status).between(1_i16, 2_i16)),
                    )
                    .col(
                        ColumnDef::new(binding::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(binding::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, binding::Column::CreatedAt))
                    .col(timestamp(manager, binding::Column::UpdatedAt))
                    .col(nullable_timestamp(manager, binding::Column::RemovedAt))
                    .check(lifecycle_shape())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_department_memberships_organization")
                            .from(binding::Entity, binding::Column::OrganizationId)
                            .to(organizations::Entity, organizations::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_department_memberships_membership_scope")
                            .from(
                                binding::Entity,
                                (
                                    binding::Column::OrganizationId,
                                    binding::Column::MembershipId,
                                ),
                            )
                            .to(
                                organization_memberships::Entity,
                                (
                                    organization_memberships::Column::OrganizationId,
                                    organization_memberships::Column::Id,
                                ),
                            )
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_department_memberships_department_scope")
                            .from(
                                binding::Entity,
                                (
                                    binding::Column::OrganizationId,
                                    binding::Column::DepartmentId,
                                ),
                            )
                            .to(
                                organization_departments::Entity,
                                (
                                    organization_departments::Column::OrganizationId,
                                    organization_departments::Column::Id,
                                ),
                            )
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        for index in [
            Index::create()
                .name("uq_org_dept_membership_scope")
                .table(binding::Entity)
                .col(binding::Column::OrganizationId)
                .col(binding::Column::MembershipId)
                .col(binding::Column::DepartmentId)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_organization_department_memberships_scope_membership_status")
                .table(binding::Entity)
                .col(binding::Column::OrganizationId)
                .col(binding::Column::MembershipId)
                .col(binding::Column::Status)
                .col(binding::Column::IsPrimary)
                .col(binding::Column::Id)
                .to_owned(),
            Index::create()
                .name("idx_organization_department_memberships_scope_department_status")
                .table(binding::Entity)
                .col(binding::Column::OrganizationId)
                .col(binding::Column::DepartmentId)
                .col(binding::Column::Status)
                .col(binding::Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(binding::Entity).to_owned())
            .await?;
        Ok(())
    }
}

fn lifecycle_shape() -> SimpleExpr {
    use binding::Column;
    Expr::col(Column::Status)
        .eq(1_i16)
        .and(Expr::col(Column::RemovedAt).is_null())
        .or(Expr::col(Column::Status)
            .eq(2_i16)
            .and(Expr::col(Column::RemovedAt).is_not_null())
            .and(Expr::col(Column::IsPrimary).eq(0_i16)))
}
