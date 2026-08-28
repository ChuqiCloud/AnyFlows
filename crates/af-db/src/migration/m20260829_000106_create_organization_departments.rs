use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_departments as department_iden, organizations as organization_iden},
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

/// 创建企业部门邻接表，并以复合外键固定父部门企业边界。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, department_iden::Entity)
                    .if_not_exists()
                    .col(auto_id(department_iden::Column::Id))
                    .col(
                        ColumnDef::new(department_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(department_iden::Column::DepartmentKey)
                            .char_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(department_iden::Column::ParentDepartmentId).big_integer())
                    .col(
                        ColumnDef::new(department_iden::Column::Name)
                            .string_len(128)
                            .not_null()
                            .check(Expr::col(department_iden::Column::Name).ne("")),
                    )
                    .col(
                        ColumnDef::new(department_iden::Column::SortOrder)
                            .big_integer()
                            .not_null()
                            .default(0_i64),
                    )
                    .col(
                        ColumnDef::new(department_iden::Column::Depth)
                            .small_integer()
                            .not_null()
                            .check(Expr::col(department_iden::Column::Depth).between(1_i16, 8_i16)),
                    )
                    .col(
                        ColumnDef::new(department_iden::Column::Status)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(
                                Expr::col(department_iden::Column::Status).between(1_i16, 2_i16),
                            ),
                    )
                    .col(
                        ColumnDef::new(department_iden::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(department_iden::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, department_iden::Column::CreatedAt))
                    .col(timestamp(manager, department_iden::Column::UpdatedAt))
                    .col(nullable_timestamp(
                        manager,
                        department_iden::Column::DisabledAt,
                    ))
                    .index(
                        Index::create()
                            .name("uq_organization_departments_scope_id")
                            .col(department_iden::Column::OrganizationId)
                            .col(department_iden::Column::Id)
                            .unique(),
                    )
                    .index(
                        Index::create()
                            .name("uq_organization_departments_scope_key")
                            .col(department_iden::Column::OrganizationId)
                            .col(department_iden::Column::DepartmentKey)
                            .unique(),
                    )
                    .index(
                        Index::create()
                            .name("idx_organization_departments_scope_sort")
                            .col(department_iden::Column::OrganizationId)
                            .col(department_iden::Column::SortOrder)
                            .col(department_iden::Column::Id),
                    )
                    .index(
                        Index::create()
                            .name("idx_organization_departments_scope_parent_status")
                            .col(department_iden::Column::OrganizationId)
                            .col(department_iden::Column::ParentDepartmentId)
                            .col(department_iden::Column::Status)
                            .col(department_iden::Column::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_departments_organization")
                            .from(
                                department_iden::Entity,
                                department_iden::Column::OrganizationId,
                            )
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_departments_parent_scope")
                            .from(
                                department_iden::Entity,
                                (
                                    department_iden::Column::OrganizationId,
                                    department_iden::Column::ParentDepartmentId,
                                ),
                            )
                            .to(
                                department_iden::Entity,
                                (
                                    department_iden::Column::OrganizationId,
                                    department_iden::Column::Id,
                                ),
                            )
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .check(
                        Expr::col(department_iden::Column::ParentDepartmentId)
                            .is_null()
                            .or(Expr::col(department_iden::Column::ParentDepartmentId)
                                .ne(Expr::col(department_iden::Column::Id))),
                    )
                    .check(
                        Expr::col(department_iden::Column::DisabledAt)
                            .is_null()
                            .and(Expr::col(department_iden::Column::Status).eq(1_i16))
                            .or(Expr::col(department_iden::Column::DisabledAt)
                                .is_not_null()
                                .and(Expr::col(department_iden::Column::Status).eq(2_i16))),
                    )
                    .check(department_key_format_check(manager.get_database_backend()))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(department_iden::Entity).to_owned())
            .await
    }
}

fn department_key_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""department_key" ~ '^[0-9a-f]{32}$' AND "department_key" <> '00000000000000000000000000000000'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`department_key`) = 32 AND `department_key` REGEXP '^[0-9a-f]{32}$' AND `department_key` <> '00000000000000000000000000000000'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("department_key") = 32 AND "department_key" NOT GLOB '*[^0-9a-f]*' AND "department_key" <> '00000000000000000000000000000000'"#,
        ),
    }
}
