use sea_orm_migration::prelude::*;

use super::{
    iden::{
        debug_trace_attempts, debug_trace_snapshot_access_audits, debug_trace_snapshots,
        debug_traces, users,
    },
    schema::{auto_id, table, timestamp},
};

const SNAPSHOT_POSITION_INDEX: &str = "uq_debug_trace_snapshots_position_kind";
const SNAPSHOT_TRACE_INDEX: &str = "idx_debug_trace_snapshots_trace";
const ACCESS_TRACE_CREATED_INDEX: &str = "idx_debug_trace_snapshot_access_trace_created";

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_snapshots(manager).await?;
        create_access_audits(manager).await?;
        // 数据库迁移拿不到启动主密钥；历史诊断正文宁可失效，也不能伪装成已加密数据。
        clear_legacy_plaintext(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(debug_trace_snapshot_access_audits::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(debug_trace_snapshots::Entity)
                    .to_owned(),
            )
            .await
    }
}

async fn create_snapshots(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, debug_trace_snapshots::Entity);
    // MySQL 8.4 禁止级联外键列参与 CHECK；父表自增键与强类型写入共同保证 ID 和快照层级有效。
    statement
        .col(auto_id(debug_trace_snapshots::Column::Id))
        .col(
            ColumnDef::new(debug_trace_snapshots::Column::TraceId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(debug_trace_snapshots::Column::AttemptId).big_integer())
        .col(
            ColumnDef::new(debug_trace_snapshots::Column::Kind)
                .small_integer()
                .not_null()
                .check(Expr::col(debug_trace_snapshots::Column::Kind).between(1_i16, 6_i16)),
        )
        .col(
            ColumnDef::new(debug_trace_snapshots::Column::EncryptedPayload)
                .json_binary()
                .not_null(),
        )
        .col(timestamp(manager, debug_trace_snapshots::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_trace_snapshots_trace")
                .from(
                    debug_trace_snapshots::Entity,
                    debug_trace_snapshots::Column::TraceId,
                )
                .to(debug_traces::Entity, debug_traces::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_trace_snapshots_attempt")
                .from(
                    debug_trace_snapshots::Entity,
                    debug_trace_snapshots::Column::AttemptId,
                )
                .to(
                    debug_trace_attempts::Entity,
                    debug_trace_attempts::Column::Id,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        );
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(SNAPSHOT_POSITION_INDEX)
                .table(debug_trace_snapshots::Entity)
                .col(debug_trace_snapshots::Column::TraceId)
                .col(debug_trace_snapshots::Column::AttemptId)
                .col(debug_trace_snapshots::Column::Kind)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(SNAPSHOT_TRACE_INDEX)
                .table(debug_trace_snapshots::Entity)
                .col(debug_trace_snapshots::Column::TraceId)
                .to_owned(),
        )
        .await
}

async fn create_access_audits(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, debug_trace_snapshot_access_audits::Entity);
    statement
        .col(auto_id(debug_trace_snapshot_access_audits::Column::Id))
        .col(
            ColumnDef::new(debug_trace_snapshot_access_audits::Column::TraceId)
                .big_integer()
                .not_null()
                .check(Expr::col(debug_trace_snapshot_access_audits::Column::TraceId).gt(0_i64)),
        )
        .col(
            ColumnDef::new(debug_trace_snapshot_access_audits::Column::ActorUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(debug_trace_snapshot_access_audits::Column::Scope)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(debug_trace_snapshot_access_audits::Column::Scope)
                        .between(1_i16, 2_i16),
                ),
        )
        .col(
            ColumnDef::new(debug_trace_snapshot_access_audits::Column::Outcome)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(debug_trace_snapshot_access_audits::Column::Outcome)
                        .between(1_i16, 3_i16),
                ),
        )
        .col(timestamp(
            manager,
            debug_trace_snapshot_access_audits::Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_trace_snapshot_access_actor")
                .from(
                    debug_trace_snapshot_access_audits::Entity,
                    debug_trace_snapshot_access_audits::Column::ActorUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(ACCESS_TRACE_CREATED_INDEX)
                .table(debug_trace_snapshot_access_audits::Entity)
                .col(debug_trace_snapshot_access_audits::Column::TraceId)
                .col(debug_trace_snapshot_access_audits::Column::CreatedAt)
                .to_owned(),
        )
        .await
}

async fn clear_legacy_plaintext(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .exec_stmt(
            Query::update()
                .table(debug_traces::Entity)
                .value(
                    debug_traces::Column::DownstreamHeadersJson,
                    Expr::value(Option::<String>::None),
                )
                .value(
                    debug_traces::Column::DownstreamBodyJson,
                    Expr::value(Option::<String>::None),
                )
                .to_owned(),
        )
        .await?;
    manager
        .exec_stmt(
            Query::update()
                .table(debug_trace_attempts::Entity)
                .value(
                    debug_trace_attempts::Column::RequestHeadersJson,
                    Expr::value(Option::<String>::None),
                )
                .value(
                    debug_trace_attempts::Column::RequestBodyJson,
                    Expr::value(Option::<String>::None),
                )
                .value(
                    debug_trace_attempts::Column::ResponseHeadersJson,
                    Expr::value(Option::<String>::None),
                )
                .value(
                    debug_trace_attempts::Column::ResponseBodyJson,
                    Expr::value(Option::<String>::None),
                )
                .to_owned(),
        )
        .await?;
    Ok(())
}
