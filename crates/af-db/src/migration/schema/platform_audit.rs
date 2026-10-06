use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{platform_audit_logs, users};

use super::{auto_id, table, timestamp};

const AUDIT_REQUEST_ACTION_UNIQUE: &str = "uq_platform_audit_request_action";
const AUDIT_OPERATOR_CURSOR_INDEX: &str = "idx_platform_audit_operator_id";
const AUDIT_ROUTE_CURSOR_INDEX: &str = "idx_platform_audit_route_id";
const AUDIT_CREATED_CURSOR_INDEX: &str = "idx_platform_audit_created_id";
const AUDIT_APPEND_ONLY_TRIGGER: &str = "trg_platform_audit_append_only_update";
const POSTGRES_AUDIT_APPEND_ONLY_FUNCTION: &str = "reject_platform_audit_update";

/// 创建平台管理审计表、权限查询索引和只追加保护。
pub(in crate::migration) async fn create_platform_audit_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut outcome = ColumnDef::new(platform_audit_logs::Column::Outcome);
    outcome
        .small_integer()
        .not_null()
        .check(Expr::col(platform_audit_logs::Column::Outcome).is_in([1_i16, 2, 3]));
    let mut statement = table(manager, platform_audit_logs::Entity);
    statement
        .col(auto_id(platform_audit_logs::Column::Id))
        .col(
            ColumnDef::new(platform_audit_logs::Column::OperatorUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(platform_audit_logs::Column::PermissionCode)
                .string_len(96)
                .not_null(),
        )
        .col(
            ColumnDef::new(platform_audit_logs::Column::Route)
                .string_len(128)
                .not_null(),
        )
        .col(
            ColumnDef::new(platform_audit_logs::Column::Operation)
                .string_len(96)
                .not_null(),
        )
        .col(
            ColumnDef::new(platform_audit_logs::Column::Resource)
                .string_len(64)
                .not_null(),
        )
        .col(ColumnDef::new(platform_audit_logs::Column::ResourceId).string_len(128))
        .col(&mut outcome)
        .col(ColumnDef::new(platform_audit_logs::Column::BeforeValue).string_len(2_048))
        .col(ColumnDef::new(platform_audit_logs::Column::AfterValue).string_len(2_048))
        .col(ColumnDef::new(platform_audit_logs::Column::AuditInfo).string_len(2_048))
        .col(
            ColumnDef::new(platform_audit_logs::Column::RequestId)
                .string_len(128)
                .not_null(),
        )
        .col(timestamp(manager, platform_audit_logs::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_platform_audit_operator")
                .from(
                    platform_audit_logs::Entity,
                    platform_audit_logs::Column::OperatorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement.to_owned()).await?;

    for index in [
        Index::create()
            .name(AUDIT_REQUEST_ACTION_UNIQUE)
            .table(platform_audit_logs::Entity)
            .col(platform_audit_logs::Column::RequestId)
            .col(platform_audit_logs::Column::Route)
            .col(platform_audit_logs::Column::Operation)
            .unique()
            .to_owned(),
        Index::create()
            .name(AUDIT_OPERATOR_CURSOR_INDEX)
            .table(platform_audit_logs::Entity)
            .col(platform_audit_logs::Column::OperatorUserId)
            .col(platform_audit_logs::Column::Id)
            .to_owned(),
        Index::create()
            .name(AUDIT_ROUTE_CURSOR_INDEX)
            .table(platform_audit_logs::Entity)
            .col(platform_audit_logs::Column::Route)
            .col(platform_audit_logs::Column::Id)
            .to_owned(),
        Index::create()
            .name(AUDIT_CREATED_CURSOR_INDEX)
            .table(platform_audit_logs::Entity)
            .col(platform_audit_logs::Column::CreatedAt)
            .col(platform_audit_logs::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    create_append_only_guard(manager).await
}

/// 删除平台管理审计表及只追加保护。
pub(in crate::migration) async fn drop_platform_audit_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    drop_append_only_guard(manager).await?;
    manager
        .drop_table(Table::drop().table(platform_audit_logs::Entity).to_owned())
        .await
}

async fn create_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_platform_audit_append_only_update BEFORE UPDATE ON platform_audit_logs BEGIN SELECT RAISE(ABORT, 'platform audit logs are append only'); END",
            )
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(
                    "CREATE FUNCTION reject_platform_audit_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'platform audit logs are append only'; RETURN OLD; END $$",
                )
                .await?;
            connection
                .execute_unprepared(
                    "CREATE TRIGGER trg_platform_audit_append_only_update BEFORE UPDATE ON platform_audit_logs FOR EACH ROW EXECUTE FUNCTION reject_platform_audit_update()",
                )
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_platform_audit_append_only_update BEFORE UPDATE ON platform_audit_logs FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform audit logs are append only'",
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
                    "DROP TRIGGER IF EXISTS {AUDIT_APPEND_ONLY_TRIGGER} ON platform_audit_logs"
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
