use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use super::iden::{organization_custom_roles, organization_memberships};

const ROLE_SCOPE_ID_INDEX: &str = "uq_organization_custom_roles_scope_id";
const MEMBERSHIP_ROLE_INDEX: &str = "idx_organization_memberships_custom_role";
const MEMBERSHIP_ROLE_FOREIGN_KEY: &str = "fk_org_memberships_custom_role_scope";
const SQLITE_TRIGGERS: &[&str] = &[
    "trg_org_memberships_custom_role_insert",
    "trg_org_memberships_custom_role_update",
    "trg_org_custom_roles_memberships_restrict_delete",
    "trg_org_custom_roles_memberships_restrict_update",
];

/// 为固定 Member 增加同企业自定义角色绑定事实，不开放管理接口。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name(ROLE_SCOPE_ID_INDEX)
                    .table(organization_custom_roles::Entity)
                    .col(organization_custom_roles::Column::OrganizationId)
                    .col(organization_custom_roles::Column::Id)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_memberships::Entity)
                    .add_column(
                        ColumnDef::new(organization_memberships::Column::CustomRoleId)
                            .big_integer(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(MEMBERSHIP_ROLE_INDEX)
                    .table(organization_memberships::Entity)
                    .col(organization_memberships::Column::OrganizationId)
                    .col(organization_memberships::Column::CustomRoleId)
                    .col(organization_memberships::Column::Status)
                    .col(organization_memberships::Column::Id)
                    .to_owned(),
            )
            .await?;

        create_role_check(manager).await?;
        create_role_reference(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_role_reference(manager).await?;
        drop_role_check(manager).await?;
        manager
            .drop_index(
                Index::drop()
                    .name(MEMBERSHIP_ROLE_INDEX)
                    .table(organization_memberships::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_memberships::Entity)
                    .drop_column(organization_memberships::Column::CustomRoleId)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(ROLE_SCOPE_ID_INDEX)
                    .table(organization_custom_roles::Entity)
                    .to_owned(),
            )
            .await
    }
}

async fn create_role_check(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::Postgres => Some(
            r#"ALTER TABLE "organization_memberships" ADD CONSTRAINT "ck_org_memberships_custom_role" CHECK ("custom_role_id" IS NULL OR "role" = 4)"#,
        ),
        DbBackend::MySql => Some(
            "ALTER TABLE `organization_memberships` ADD CONSTRAINT `ck_org_memberships_custom_role` CHECK (`custom_role_id` IS NULL OR `role` = 4)",
        ),
        DbBackend::Sqlite => None,
    };
    if let Some(statement) = statement {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

async fn drop_role_check(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::Postgres => Some(
            r#"ALTER TABLE "organization_memberships" DROP CONSTRAINT "ck_org_memberships_custom_role""#,
        ),
        DbBackend::MySql => Some(
            "ALTER TABLE `organization_memberships` DROP CHECK `ck_org_memberships_custom_role`",
        ),
        DbBackend::Sqlite => None,
    };
    if let Some(statement) = statement {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

async fn create_role_reference(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        return create_sqlite_role_triggers(manager).await;
    }
    manager
        .create_foreign_key(
            ForeignKey::create()
                .name(MEMBERSHIP_ROLE_FOREIGN_KEY)
                .from(
                    organization_memberships::Entity,
                    (
                        organization_memberships::Column::OrganizationId,
                        organization_memberships::Column::CustomRoleId,
                    ),
                )
                .to(
                    organization_custom_roles::Entity,
                    (
                        organization_custom_roles::Column::OrganizationId,
                        organization_custom_roles::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict)
                .to_owned(),
        )
        .await
}

async fn drop_role_reference(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        for trigger in SQLITE_TRIGGERS {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                .await?;
        }
        return Ok(());
    }
    manager
        .drop_foreign_key(
            ForeignKey::drop()
                .name(MEMBERSHIP_ROLE_FOREIGN_KEY)
                .table(organization_memberships::Entity)
                .to_owned(),
        )
        .await
}

async fn create_sqlite_role_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    // SQLite 无法向既有表补复合外键，用触发器保持与其他方言相同的边界。
    let statements = [
        "CREATE TRIGGER trg_org_memberships_custom_role_insert BEFORE INSERT ON organization_memberships \
         WHEN NEW.custom_role_id IS NOT NULL AND (NEW.role <> 4 OR NOT EXISTS (SELECT 1 FROM organization_custom_roles WHERE id = NEW.custom_role_id AND organization_id = NEW.organization_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid organization custom role binding'); END",
        "CREATE TRIGGER trg_org_memberships_custom_role_update BEFORE UPDATE OF organization_id, role, custom_role_id ON organization_memberships \
         WHEN NEW.custom_role_id IS NOT NULL AND (NEW.role <> 4 OR NOT EXISTS (SELECT 1 FROM organization_custom_roles WHERE id = NEW.custom_role_id AND organization_id = NEW.organization_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid organization custom role binding'); END",
        "CREATE TRIGGER trg_org_custom_roles_memberships_restrict_delete BEFORE DELETE ON organization_custom_roles \
         WHEN EXISTS (SELECT 1 FROM organization_memberships WHERE custom_role_id = OLD.id AND organization_id = OLD.organization_id) \
         BEGIN SELECT RAISE(ABORT, 'custom role is referenced by memberships'); END",
        "CREATE TRIGGER trg_org_custom_roles_memberships_restrict_update BEFORE UPDATE OF id, organization_id ON organization_custom_roles \
         WHEN (NEW.id <> OLD.id OR NEW.organization_id <> OLD.organization_id) AND EXISTS (SELECT 1 FROM organization_memberships WHERE custom_role_id = OLD.id AND organization_id = OLD.organization_id) \
         BEGIN SELECT RAISE(ABORT, 'custom role is referenced by memberships'); END",
    ];
    for statement in statements {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}
