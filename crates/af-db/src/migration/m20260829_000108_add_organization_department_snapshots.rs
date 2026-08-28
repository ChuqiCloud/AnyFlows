use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::iden::{organization_departments, organization_service_accounts, tokens};

const TOKEN_DEPARTMENT_INDEX: &str = "idx_tokens_organization_department_status";
const SERVICE_ACCOUNT_DEPARTMENT_INDEX: &str =
    "idx_organization_service_accounts_department_status";
const TOKEN_DEPARTMENT_FOREIGN_KEY: &str = "fk_tokens_organization_department";
const SERVICE_ACCOUNT_DEPARTMENT_FOREIGN_KEY: &str = "fk_organization_service_accounts_department";
const SQLITE_TRIGGERS: [&str; 6] = [
    "trg_tokens_organization_department_insert",
    "trg_tokens_organization_department_update",
    "trg_service_accounts_organization_department_insert",
    "trg_service_accounts_organization_department_update",
    "trg_organization_departments_tokens_restrict_delete",
    "trg_organization_departments_service_accounts_restrict_delete",
];

/// 为企业 Key 和服务账号增加不可变部门归属快照字段。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_snapshot_column(
            manager,
            tokens::Entity,
            tokens::Column::OrganizationDepartmentId,
            Some(
                Expr::col(tokens::Column::OrganizationDepartmentId)
                    .is_null()
                    .or(Expr::col(tokens::Column::OrganizationId).is_not_null()),
            ),
        )
        .await?;
        add_snapshot_column(
            manager,
            organization_service_accounts::Entity,
            organization_service_accounts::Column::OrganizationDepartmentId,
            Some(
                Expr::col(organization_service_accounts::Column::OrganizationDepartmentId)
                    .is_null()
                    .or(
                        Expr::col(organization_service_accounts::Column::OrganizationId)
                            .is_not_null(),
                    ),
            ),
        )
        .await?;

        if manager.get_database_backend() == DbBackend::MySql {
            for statement in [
                "ALTER TABLE `tokens` ADD CONSTRAINT `ck_tokens_organization_department` CHECK (`organization_department_id` IS NULL OR `organization_id` IS NOT NULL)",
                "ALTER TABLE `organization_service_accounts` ADD CONSTRAINT `ck_service_accounts_organization_department` CHECK (`organization_department_id` IS NULL OR `organization_id` IS NOT NULL)",
            ] {
                manager
                    .get_connection()
                    .execute_unprepared(statement)
                    .await?;
            }
        }

        manager
            .create_index(
                Index::create()
                    .name(TOKEN_DEPARTMENT_INDEX)
                    .table(tokens::Entity)
                    .col(tokens::Column::OrganizationId)
                    .col(tokens::Column::OrganizationDepartmentId)
                    .col(tokens::Column::Status)
                    .col(tokens::Column::Id)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(SERVICE_ACCOUNT_DEPARTMENT_INDEX)
                    .table(organization_service_accounts::Entity)
                    .col(organization_service_accounts::Column::OrganizationId)
                    .col(organization_service_accounts::Column::OrganizationDepartmentId)
                    .col(organization_service_accounts::Column::Status)
                    .col(organization_service_accounts::Column::Id)
                    .to_owned(),
            )
            .await?;

        if manager.get_database_backend() == DbBackend::Sqlite {
            create_sqlite_reference_triggers(manager).await
        } else {
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name(TOKEN_DEPARTMENT_FOREIGN_KEY)
                        .from(
                            tokens::Entity,
                            (
                                tokens::Column::OrganizationId,
                                tokens::Column::OrganizationDepartmentId,
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
                        .on_delete(ForeignKeyAction::Restrict)
                        .to_owned(),
                )
                .await?;
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name(SERVICE_ACCOUNT_DEPARTMENT_FOREIGN_KEY)
                        .from(
                            organization_service_accounts::Entity,
                            (
                                organization_service_accounts::Column::OrganizationId,
                                organization_service_accounts::Column::OrganizationDepartmentId,
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
                        .on_delete(ForeignKeyAction::Restrict)
                        .to_owned(),
                )
                .await
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            for trigger in SQLITE_TRIGGERS {
                manager
                    .get_connection()
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                    .await?;
            }
        } else {
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name(SERVICE_ACCOUNT_DEPARTMENT_FOREIGN_KEY)
                        .table(organization_service_accounts::Entity)
                        .to_owned(),
                )
                .await?;
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name(TOKEN_DEPARTMENT_FOREIGN_KEY)
                        .table(tokens::Entity)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_index(
                Index::drop()
                    .name(SERVICE_ACCOUNT_DEPARTMENT_INDEX)
                    .table(organization_service_accounts::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(TOKEN_DEPARTMENT_INDEX)
                    .table(tokens::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_service_accounts::Entity)
                    .drop_column(organization_service_accounts::Column::OrganizationDepartmentId)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(tokens::Entity)
                    .drop_column(tokens::Column::OrganizationDepartmentId)
                    .to_owned(),
            )
            .await
    }
}

async fn create_sqlite_reference_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statements = [
        "CREATE TRIGGER trg_tokens_organization_department_insert BEFORE INSERT ON tokens \
         WHEN NEW.organization_department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.organization_department_id) \
         BEGIN SELECT RAISE(ABORT, 'invalid token organization department'); END",
        "CREATE TRIGGER trg_tokens_organization_department_update BEFORE UPDATE OF organization_id, organization_department_id ON tokens \
         WHEN NEW.organization_department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.organization_department_id) \
         BEGIN SELECT RAISE(ABORT, 'invalid token organization department'); END",
        "CREATE TRIGGER trg_service_accounts_organization_department_insert BEFORE INSERT ON organization_service_accounts \
         WHEN NEW.organization_department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.organization_department_id) \
         BEGIN SELECT RAISE(ABORT, 'invalid service account organization department'); END",
        "CREATE TRIGGER trg_service_accounts_organization_department_update BEFORE UPDATE OF organization_id, organization_department_id ON organization_service_accounts \
         WHEN NEW.organization_department_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_departments WHERE organization_id = NEW.organization_id AND id = NEW.organization_department_id) \
         BEGIN SELECT RAISE(ABORT, 'invalid service account organization department'); END",
        "CREATE TRIGGER trg_organization_departments_tokens_restrict_delete BEFORE DELETE ON organization_departments \
         WHEN EXISTS (SELECT 1 FROM tokens WHERE organization_id = OLD.organization_id AND organization_department_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'organization department is referenced by tokens'); END",
        "CREATE TRIGGER trg_organization_departments_service_accounts_restrict_delete BEFORE DELETE ON organization_departments \
         WHEN EXISTS (SELECT 1 FROM organization_service_accounts WHERE organization_id = OLD.organization_id AND organization_department_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'organization department is referenced by service accounts'); END",
    ];
    for statement in statements {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

async fn add_snapshot_column(
    manager: &SchemaManager<'_>,
    table: impl IntoIden,
    column: impl IntoIden,
    check: Option<SimpleExpr>,
) -> Result<(), DbErr> {
    let mut definition = ColumnDef::new(column).big_integer();
    // MySQL 不允许列级 CHECK 引用同表其他列，统一在列就绪后追加表级约束。
    if manager.get_database_backend() != DbBackend::MySql {
        if let Some(check) = check {
            definition.check(check);
        }
    }
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .add_column(definition)
                .to_owned(),
        )
        .await
}
