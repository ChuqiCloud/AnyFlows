use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_audit_logs, organizations, users};

use super::{auto_id, table, timestamp};

const AUDIT_REQUEST_ACTION_UNIQUE: &str = "uq_org_audit_request_operation";
const AUDIT_ORGANIZATION_CURSOR_INDEX: &str = "idx_org_audit_organization_id";
const AUDIT_ORGANIZATION_ACTOR_INDEX: &str = "idx_org_audit_organization_actor_id";
const AUDIT_ORGANIZATION_OPERATION_INDEX: &str = "idx_org_audit_organization_operation_id";
const AUDIT_ORGANIZATION_CREATED_INDEX: &str = "idx_org_audit_organization_created";
const AUDIT_PLATFORM_CURSOR_INDEX: &str = "idx_org_audit_platform_id";
const AUDIT_PLATFORM_SHAPE_CHECK: &str = "ck_org_audit_platform_shape";
const AUDIT_APPEND_ONLY_TRIGGER: &str = "trg_org_audit_append_only_update";
const POSTGRES_AUDIT_APPEND_ONLY_FUNCTION: &str = "reject_org_audit_update";

/// 创建企业管理审计表与稳定游标、筛选和保留期索引。
pub(in crate::migration) async fn create_organization_audit_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut actor_role = ColumnDef::new(organization_audit_logs::Column::ActorRole);
    actor_role.small_integer().check(
        Expr::col(organization_audit_logs::Column::ActorRole)
            .is_null()
            .or(Expr::col(organization_audit_logs::Column::ActorRole).is_in([1_i16, 2, 3, 4, 5])),
    );
    let mut outcome = ColumnDef::new(organization_audit_logs::Column::Outcome);
    outcome
        .small_integer()
        .not_null()
        .check(Expr::col(organization_audit_logs::Column::Outcome).is_in([1_i16, 2, 3]));
    let mut statement = table(manager, organization_audit_logs::Entity);
    statement
        .col(auto_id(organization_audit_logs::Column::Id))
        .col(
            ColumnDef::new(organization_audit_logs::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_audit_logs::Column::ActorUserId)
                .big_integer()
                .not_null(),
        )
        .col(&mut actor_role)
        .col(
            ColumnDef::new(organization_audit_logs::Column::PermissionCode)
                .string_len(96)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_audit_logs::Column::Operation)
                .string_len(96)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_audit_logs::Column::TargetType)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_audit_logs::Column::TargetIdentifier)
                .string_len(128)
                .not_null(),
        )
        .col(&mut outcome)
        .col(
            ColumnDef::new(organization_audit_logs::Column::PlatformProxy)
                .boolean()
                .not_null(),
        )
        .col(ColumnDef::new(organization_audit_logs::Column::PlatformReason).string_len(255))
        .col(
            ColumnDef::new(organization_audit_logs::Column::RequestId)
                .string_len(128)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_audit_logs::Column::ClientFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(ColumnDef::new(organization_audit_logs::Column::ChangeSummary).string_len(2048))
        .col(timestamp(
            manager,
            organization_audit_logs::Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_audit_organization")
                .from(
                    organization_audit_logs::Entity,
                    organization_audit_logs::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_audit_actor")
                .from(
                    organization_audit_logs::Entity,
                    organization_audit_logs::Column::ActorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(platform_shape());
    }
    manager.create_table(statement.to_owned()).await?;

    if manager.get_database_backend() == DbBackend::MySql {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE `organization_audit_logs` ADD CONSTRAINT `ck_org_audit_platform_shape` CHECK ((`platform_proxy` = TRUE AND `actor_role` IS NULL AND `platform_reason` IS NOT NULL) OR (`platform_proxy` = FALSE AND `actor_role` IS NOT NULL AND `platform_reason` IS NULL))",
            )
            .await?;
    }

    for index in [
        Index::create()
            .name(AUDIT_REQUEST_ACTION_UNIQUE)
            .table(organization_audit_logs::Entity)
            .col(organization_audit_logs::Column::OrganizationId)
            .col(organization_audit_logs::Column::RequestId)
            .col(organization_audit_logs::Column::Operation)
            .unique()
            .to_owned(),
        Index::create()
            .name(AUDIT_ORGANIZATION_CURSOR_INDEX)
            .table(organization_audit_logs::Entity)
            .col(organization_audit_logs::Column::OrganizationId)
            .col(organization_audit_logs::Column::Id)
            .to_owned(),
        Index::create()
            .name(AUDIT_ORGANIZATION_ACTOR_INDEX)
            .table(organization_audit_logs::Entity)
            .col(organization_audit_logs::Column::OrganizationId)
            .col(organization_audit_logs::Column::ActorUserId)
            .col(organization_audit_logs::Column::Id)
            .to_owned(),
        Index::create()
            .name(AUDIT_ORGANIZATION_OPERATION_INDEX)
            .table(organization_audit_logs::Entity)
            .col(organization_audit_logs::Column::OrganizationId)
            .col(organization_audit_logs::Column::Operation)
            .col(organization_audit_logs::Column::Id)
            .to_owned(),
        Index::create()
            .name(AUDIT_ORGANIZATION_CREATED_INDEX)
            .table(organization_audit_logs::Entity)
            .col(organization_audit_logs::Column::OrganizationId)
            .col(organization_audit_logs::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name(AUDIT_PLATFORM_CURSOR_INDEX)
            .table(organization_audit_logs::Entity)
            .col(organization_audit_logs::Column::PlatformProxy)
            .col(organization_audit_logs::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    create_append_only_guard(manager).await
}

/// 删除企业管理审计表。
pub(in crate::migration) async fn drop_organization_audit_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    drop_append_only_guard(manager).await?;
    if manager.get_database_backend() == DbBackend::MySql {
        manager
            .get_connection()
            .execute_unprepared(&format!(
                "ALTER TABLE `organization_audit_logs` DROP CHECK `{AUDIT_PLATFORM_SHAPE_CHECK}`"
            ))
            .await?;
    }
    manager
        .drop_table(
            Table::drop()
                .table(organization_audit_logs::Entity)
                .to_owned(),
        )
        .await
}

/// 数据库只拒绝修改，保留期任务仍可按企业删除过期审计。
async fn create_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_org_audit_append_only_update BEFORE UPDATE ON organization_audit_logs \
                 BEGIN SELECT RAISE(ABORT, 'organization audit logs are append only'); END",
            )
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(
                    "CREATE FUNCTION reject_org_audit_update() RETURNS trigger LANGUAGE plpgsql AS $$ \
                     BEGIN RAISE EXCEPTION 'organization audit logs are append only'; RETURN OLD; END $$",
                )
                .await?;
            connection
                .execute_unprepared(
                    "CREATE TRIGGER trg_org_audit_append_only_update BEFORE UPDATE ON organization_audit_logs \
                     FOR EACH ROW EXECUTE FUNCTION reject_org_audit_update()",
                )
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_org_audit_append_only_update BEFORE UPDATE ON organization_audit_logs \
                 FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'organization audit logs are append only'",
            )
            .await
            .map(|_| ()),
    }
}

async fn drop_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(&format!(
                "DROP TRIGGER IF EXISTS {AUDIT_APPEND_ONLY_TRIGGER}"
            ))
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {AUDIT_APPEND_ONLY_TRIGGER} ON organization_audit_logs"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_AUDIT_APPEND_ONLY_FUNCTION}()"
                ))
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(&format!(
                "DROP TRIGGER IF EXISTS {AUDIT_APPEND_ONLY_TRIGGER}"
            ))
            .await
            .map(|_| ()),
    }
}

fn platform_shape() -> SimpleExpr {
    let platform = Expr::col(organization_audit_logs::Column::PlatformProxy)
        .eq(true)
        .and(Expr::col(organization_audit_logs::Column::ActorRole).is_null())
        .and(Expr::col(organization_audit_logs::Column::PlatformReason).is_not_null());
    let member = Expr::col(organization_audit_logs::Column::PlatformProxy)
        .eq(false)
        .and(Expr::col(organization_audit_logs::Column::ActorRole).is_not_null())
        .and(Expr::col(organization_audit_logs::Column::PlatformReason).is_null());
    platform.or(member)
}
