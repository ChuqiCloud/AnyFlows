use af_domain::OrganizationPermission;
use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_custom_role_permissions, organization_custom_roles, organization_memberships,
        organizations,
    },
    schema::{auto_id, table, timestamp},
};

const ROLE_KEY_INDEX: &str = "uq_organization_custom_roles_key";
const ROLE_SCOPE_NAME_INDEX: &str = "uq_organization_custom_roles_scope_name";
const ROLE_SCOPE_STATUS_INDEX: &str = "idx_organization_custom_roles_scope_status";
const ROLE_PERMISSION_INDEX: &str = "uq_organization_custom_role_permissions_role_code";

/// 创建企业自定义角色目录及逐条权限事实，不接入成员授权。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_roles(manager).await?;
        create_permissions(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            organization_custom_role_permissions::Entity.into_iden(),
            organization_custom_roles::Entity.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}

async fn create_roles(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_custom_roles::Entity);
    statement
        .col(auto_id(organization_custom_roles::Column::Id))
        .col(
            ColumnDef::new(organization_custom_roles::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_custom_roles::Column::RoleKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_custom_roles::Column::Name)
                .string_len(128)
                .not_null()
                .check(Expr::col(organization_custom_roles::Column::Name).ne("")),
        )
        .col(ColumnDef::new(organization_custom_roles::Column::Description).text())
        .col(
            ColumnDef::new(organization_custom_roles::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_custom_roles::Column::Status).between(1_i16, 2_i16)),
        )
        .col(
            ColumnDef::new(organization_custom_roles::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_custom_roles::Column::Version).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_custom_roles::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            organization_custom_roles::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_custom_roles::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_custom_roles_organization")
                .from(
                    organization_custom_roles::Entity,
                    organization_custom_roles::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_custom_roles_creator_scope")
                .from(
                    organization_custom_roles::Entity,
                    (
                        organization_custom_roles::Column::OrganizationId,
                        organization_custom_roles::Column::CreatedByUserId,
                    ),
                )
                .to(
                    organization_memberships::Entity,
                    (
                        organization_memberships::Column::OrganizationId,
                        organization_memberships::Column::UserId,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(role_key_format_check(manager.get_database_backend()));
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(ROLE_KEY_INDEX)
            .table(organization_custom_roles::Entity)
            .col(organization_custom_roles::Column::RoleKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(ROLE_SCOPE_NAME_INDEX)
            .table(organization_custom_roles::Entity)
            .col(organization_custom_roles::Column::OrganizationId)
            .col(organization_custom_roles::Column::Name)
            .unique()
            .to_owned(),
        Index::create()
            .name(ROLE_SCOPE_STATUS_INDEX)
            .table(organization_custom_roles::Entity)
            .col(organization_custom_roles::Column::OrganizationId)
            .col(organization_custom_roles::Column::Status)
            .col(organization_custom_roles::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_permissions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let allowed_codes = OrganizationPermission::custom_role_grantable_permissions()
        .iter()
        .map(|permission| permission.code())
        .collect::<Vec<_>>();
    let mut statement = table(manager, organization_custom_role_permissions::Entity);
    statement
        .col(auto_id(organization_custom_role_permissions::Column::Id))
        .col(
            ColumnDef::new(organization_custom_role_permissions::Column::CustomRoleId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_custom_role_permissions::Column::PermissionCode)
                .string_len(64)
                .not_null()
                .check(
                    Expr::col(organization_custom_role_permissions::Column::PermissionCode)
                        .is_in(allowed_codes),
                ),
        )
        .col(timestamp(
            manager,
            organization_custom_role_permissions::Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_custom_role_permissions_role")
                .from(
                    organization_custom_role_permissions::Entity,
                    organization_custom_role_permissions::Column::CustomRoleId,
                )
                .to(
                    organization_custom_roles::Entity,
                    organization_custom_roles::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(ROLE_PERMISSION_INDEX)
                .table(organization_custom_role_permissions::Entity)
                .col(organization_custom_role_permissions::Column::CustomRoleId)
                .col(organization_custom_role_permissions::Column::PermissionCode)
                .unique()
                .to_owned(),
        )
        .await
}

fn role_key_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""role_key" ~ '^[0-9a-f]{32}$' AND "role_key" <> '00000000000000000000000000000000'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`role_key`) = 32 AND `role_key` REGEXP '^[0-9a-f]{32}$' AND `role_key` <> '00000000000000000000000000000000'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("role_key") = 32 AND "role_key" NOT GLOB '*[^0-9a-f]*' AND "role_key" <> '00000000000000000000000000000000'"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_role_key_check_avoids_binary_regex_operands() {
        let rendered = Query::select()
            .expr(role_key_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
