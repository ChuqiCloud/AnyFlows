use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{channels, request_outcome_logs};

use super::{auto_id, table, timestamp};

const REQUEST_ID_UNIQUE: &str = "uq_request_outcome_request_id";
const CREATED_CURSOR_INDEX: &str = "idx_request_outcome_created_id";
const OUTCOME_CREATED_INDEX: &str = "idx_request_outcome_outcome_created";
const CHANNEL_CREATED_INDEX: &str = "idx_request_outcome_channel_created";
const FLOW_CREATED_INDEX: &str = "idx_request_outcome_flow_created";
const APPEND_ONLY_TRIGGER: &str = "trg_request_outcome_append_only_update";
const POSTGRES_APPEND_ONLY_FUNCTION: &str = "reject_request_outcome_update";

/// 创建请求终态事实表、看板聚合索引和只追加保护。
pub(in crate::migration) async fn create_request_outcome_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut outcome = ColumnDef::new(request_outcome_logs::Column::Outcome);
    outcome
        .small_integer()
        .not_null()
        .check(Expr::col(request_outcome_logs::Column::Outcome).is_in([1_i16, 2]));
    let mut duration = ColumnDef::new(request_outcome_logs::Column::DurationMs);
    duration
        .big_integer()
        .not_null()
        .check(Expr::col(request_outcome_logs::Column::DurationMs).gte(0_i64));
    let mut statement = table(manager, request_outcome_logs::Entity);
    statement
        .col(auto_id(request_outcome_logs::Column::Id))
        .col(
            ColumnDef::new(request_outcome_logs::Column::RequestId)
                .string_len(128)
                .not_null(),
        )
        .col(
            ColumnDef::new(request_outcome_logs::Column::Protocol)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(request_outcome_logs::Column::Operation)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(request_outcome_logs::Column::Model)
                .string_len(255)
                .not_null(),
        )
        .col(&mut outcome)
        .col(ColumnDef::new(request_outcome_logs::Column::ErrorKind).string_len(32))
        .col(ColumnDef::new(request_outcome_logs::Column::ChannelId).big_integer())
        .col(&mut duration)
        .col(timestamp(manager, request_outcome_logs::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_request_outcome_channel")
                .from(
                    request_outcome_logs::Entity,
                    request_outcome_logs::Column::ChannelId,
                )
                .to(channels::Entity, channels::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement.to_owned()).await?;

    for index in [
        Index::create()
            .name(REQUEST_ID_UNIQUE)
            .table(request_outcome_logs::Entity)
            .col(request_outcome_logs::Column::RequestId)
            .unique()
            .to_owned(),
        Index::create()
            .name(CREATED_CURSOR_INDEX)
            .table(request_outcome_logs::Entity)
            .col(request_outcome_logs::Column::CreatedAt)
            .col(request_outcome_logs::Column::Id)
            .to_owned(),
        Index::create()
            .name(OUTCOME_CREATED_INDEX)
            .table(request_outcome_logs::Entity)
            .col(request_outcome_logs::Column::Outcome)
            .col(request_outcome_logs::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name(CHANNEL_CREATED_INDEX)
            .table(request_outcome_logs::Entity)
            .col(request_outcome_logs::Column::ChannelId)
            .col(request_outcome_logs::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name(FLOW_CREATED_INDEX)
            .table(request_outcome_logs::Entity)
            .col(request_outcome_logs::Column::Protocol)
            .col(request_outcome_logs::Column::ChannelId)
            .col(request_outcome_logs::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    create_append_only_guard(manager).await
}

/// 删除请求终态事实表及只追加保护。
pub(in crate::migration) async fn drop_request_outcome_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    drop_append_only_guard(manager).await?;
    manager
        .drop_table(Table::drop().table(request_outcome_logs::Entity).to_owned())
        .await
}

async fn create_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_request_outcome_append_only_update BEFORE UPDATE ON request_outcome_logs BEGIN SELECT RAISE(ABORT, 'request outcome logs are append only'); END",
            )
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(
                    "CREATE FUNCTION reject_request_outcome_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'request outcome logs are append only'; RETURN OLD; END $$",
                )
                .await?;
            connection
                .execute_unprepared(
                    "CREATE TRIGGER trg_request_outcome_append_only_update BEFORE UPDATE ON request_outcome_logs FOR EACH ROW EXECUTE FUNCTION reject_request_outcome_update()",
                )
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_request_outcome_append_only_update BEFORE UPDATE ON request_outcome_logs FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'request outcome logs are append only'",
            )
            .await
            .map(|_| ()),
    }
}

async fn drop_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {APPEND_ONLY_TRIGGER}"))
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {APPEND_ONLY_TRIGGER} ON request_outcome_logs"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_APPEND_ONLY_FUNCTION}()"
                ))
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {APPEND_ONLY_TRIGGER}"))
            .await
            .map(|_| ()),
    }
}
