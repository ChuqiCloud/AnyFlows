use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_teams, organizations, usage_logs},
    schema,
};

const USAGE_ORGANIZATION_CURSOR_INDEX: &str = "idx_usage_logs_organization_id";
const USAGE_ORGANIZATION_USER_INDEX: &str = "idx_usage_logs_organization_user_id";
const USAGE_ORGANIZATION_TEAM_INDEX: &str = "idx_usage_logs_organization_team_id";
const USAGE_ORGANIZATION_SHAPE_CHECK: &str = "ck_usage_logs_organization_shape";
const USAGE_ORGANIZATION_FOREIGN_KEY: &str = "fk_usage_logs_organization";
const USAGE_ORGANIZATION_TEAM_FOREIGN_KEY: &str = "fk_usage_logs_organization_team";
const SQLITE_TRIGGERS: &[&str] = &[
    "trg_usage_logs_organization_insert",
    "trg_usage_logs_organization_update",
    "trg_organizations_usage_logs_restrict_delete",
    "trg_organization_teams_usage_logs_restrict_delete",
];

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column(
            manager,
            ColumnDef::new(usage_logs::Column::OrganizationId).big_integer(),
        )
        .await?;
        add_column(
            manager,
            ColumnDef::new(usage_logs::Column::OrganizationTeamId).big_integer(),
        )
        .await?;
        create_usage_shape(manager).await?;
        create_usage_references(manager).await?;
        for index in [
            Index::create()
                .name(USAGE_ORGANIZATION_CURSOR_INDEX)
                .table(usage_logs::Entity)
                .col(usage_logs::Column::OrganizationId)
                .col(usage_logs::Column::Id)
                .to_owned(),
            Index::create()
                .name(USAGE_ORGANIZATION_USER_INDEX)
                .table(usage_logs::Entity)
                .col(usage_logs::Column::OrganizationId)
                .col(usage_logs::Column::UserId)
                .col(usage_logs::Column::Id)
                .to_owned(),
            Index::create()
                .name(USAGE_ORGANIZATION_TEAM_INDEX)
                .table(usage_logs::Entity)
                .col(usage_logs::Column::OrganizationId)
                .col(usage_logs::Column::OrganizationTeamId)
                .col(usage_logs::Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        schema::create_organization_audit_storage(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::drop_organization_audit_storage(manager).await?;
        // MySQL 的外键会占用匹配索引，必须先解除引用再删除索引。
        drop_usage_references(manager).await?;
        for index in [
            USAGE_ORGANIZATION_TEAM_INDEX,
            USAGE_ORGANIZATION_USER_INDEX,
            USAGE_ORGANIZATION_CURSOR_INDEX,
        ] {
            manager
                .drop_index(
                    Index::drop()
                        .name(index)
                        .table(usage_logs::Entity)
                        .to_owned(),
                )
                .await?;
        }
        drop_usage_shape(manager).await?;
        drop_column(manager, usage_logs::Column::OrganizationTeamId).await?;
        drop_column(manager, usage_logs::Column::OrganizationId).await
    }
}

async fn create_usage_shape(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => Ok(()),
        DbBackend::Postgres => manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE \"usage_logs\" ADD CONSTRAINT \"ck_usage_logs_organization_shape\" CHECK ((\"organization_id\" IS NULL AND \"organization_team_id\" IS NULL) OR \"organization_id\" IS NOT NULL)",
            )
            .await
            .map(|_| ()),
        DbBackend::MySql => manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE `usage_logs` ADD CONSTRAINT `ck_usage_logs_organization_shape` CHECK ((`organization_id` IS NULL AND `organization_team_id` IS NULL) OR `organization_id` IS NOT NULL)",
            )
            .await
            .map(|_| ()),
    }
}

async fn drop_usage_shape(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => Ok(()),
        DbBackend::Postgres => manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE \"usage_logs\" DROP CONSTRAINT \"ck_usage_logs_organization_shape\"",
            )
            .await
            .map(|_| ()),
        DbBackend::MySql => manager
            .get_connection()
            .execute_unprepared(&format!(
                "ALTER TABLE `usage_logs` DROP CHECK `{USAGE_ORGANIZATION_SHAPE_CHECK}`"
            ))
            .await
            .map(|_| ()),
    }
}

async fn create_usage_references(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        return create_sqlite_usage_triggers(manager).await;
    }
    for foreign_key in [
        ForeignKey::create()
            .name(USAGE_ORGANIZATION_FOREIGN_KEY)
            .from(usage_logs::Entity, usage_logs::Column::OrganizationId)
            .to(organizations::Entity, organizations::Column::Id)
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
        ForeignKey::create()
            .name(USAGE_ORGANIZATION_TEAM_FOREIGN_KEY)
            .from(
                usage_logs::Entity,
                (
                    usage_logs::Column::OrganizationId,
                    usage_logs::Column::OrganizationTeamId,
                ),
            )
            .to(
                organization_teams::Entity,
                (
                    organization_teams::Column::OrganizationId,
                    organization_teams::Column::Id,
                ),
            )
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
    ] {
        manager.create_foreign_key(foreign_key).await?;
    }
    Ok(())
}

async fn drop_usage_references(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        for trigger in SQLITE_TRIGGERS {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                .await?;
        }
        return Ok(());
    }
    for name in [
        USAGE_ORGANIZATION_TEAM_FOREIGN_KEY,
        USAGE_ORGANIZATION_FOREIGN_KEY,
    ] {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name(name)
                    .table(usage_logs::Entity)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

async fn create_sqlite_usage_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for statement in [
        "CREATE TRIGGER trg_usage_logs_organization_insert BEFORE INSERT ON usage_logs \
         WHEN NOT ((NEW.organization_id IS NULL AND NEW.organization_team_id IS NULL) OR NEW.organization_id IS NOT NULL) OR \
         (NEW.organization_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organizations WHERE id = NEW.organization_id)) OR \
         (NEW.organization_team_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_teams WHERE id = NEW.organization_team_id AND organization_id = NEW.organization_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid usage log organization subject'); END",
        "CREATE TRIGGER trg_usage_logs_organization_update BEFORE UPDATE OF organization_id, organization_team_id ON usage_logs \
         BEGIN SELECT RAISE(ABORT, 'usage logs are append only'); END",
        "CREATE TRIGGER trg_organizations_usage_logs_restrict_delete BEFORE DELETE ON organizations \
         WHEN EXISTS (SELECT 1 FROM usage_logs WHERE organization_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'organization is referenced by usage logs'); END",
        "CREATE TRIGGER trg_organization_teams_usage_logs_restrict_delete BEFORE DELETE ON organization_teams \
         WHEN EXISTS (SELECT 1 FROM usage_logs WHERE organization_team_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'organization team is referenced by usage logs'); END",
    ] {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

async fn add_column(manager: &SchemaManager<'_>, column: &mut ColumnDef) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(column)
                .to_owned(),
        )
        .await
}

async fn drop_column(manager: &SchemaManager<'_>, column: usage_logs::Column) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .drop_column(column)
                .to_owned(),
        )
        .await
}
