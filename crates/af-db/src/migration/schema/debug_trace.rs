use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    debug_trace_attempts, debug_trace_settings, debug_traces, groups, tokens, users,
};

use super::{auto_id, table, timestamp};

const TRACE_REQUEST_ID_INDEX: &str = "uq_debug_traces_request_id";
const TRACE_CREATED_INDEX: &str = "idx_debug_traces_created";
const TRACE_OUTCOME_CREATED_INDEX: &str = "idx_debug_traces_outcome_created";
const ATTEMPT_TRACE_INDEX: &str = "uq_debug_trace_attempts_trace_candidate";

/// 创建默认关闭的固定设置、脱敏追踪主表和候选尝试明细表。
pub(in crate::migration) async fn create_debug_trace_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_settings(manager).await?;
    create_traces(manager).await?;
    create_attempts(manager).await
}

async fn create_settings(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, debug_trace_settings::Entity);
    statement
        .col(
            ColumnDef::new(debug_trace_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(debug_trace_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(debug_trace_settings::Column::Enabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(debug_trace_settings::Column::SamplePerMillion)
                .big_integer()
                .not_null()
                .default(10_000_i64)
                .check(
                    Expr::col(debug_trace_settings::Column::SamplePerMillion)
                        .between(0_i64, 1_000_000_i64),
                ),
        )
        .col(
            ColumnDef::new(debug_trace_settings::Column::RetentionHours)
                .integer()
                .not_null()
                .default(24_i32)
                .check(
                    Expr::col(debug_trace_settings::Column::RetentionHours).between(1_i32, 720_i32),
                ),
        )
        .col(
            ColumnDef::new(debug_trace_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(debug_trace_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, debug_trace_settings::Column::CreatedAt))
        .col(timestamp(manager, debug_trace_settings::Column::UpdatedAt));
    manager.create_table(statement).await?;
    // 固定行让首次读取、并发更新和运行时版本边界共享同一数据库事实。
    manager
        .exec_stmt(
            Query::insert()
                .into_table(debug_trace_settings::Entity)
                .columns([
                    debug_trace_settings::Column::Id,
                    debug_trace_settings::Column::Enabled,
                    debug_trace_settings::Column::SamplePerMillion,
                    debug_trace_settings::Column::RetentionHours,
                    debug_trace_settings::Column::Version,
                ])
                .values_panic([
                    1_i16.into(),
                    false.into(),
                    10_000_i64.into(),
                    24_i32.into(),
                    1_i64.into(),
                ])
                .to_owned(),
        )
        .await
}

async fn create_traces(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, debug_traces::Entity);
    statement
        .col(auto_id(debug_traces::Column::Id))
        .col(
            ColumnDef::new(debug_traces::Column::RequestId)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(debug_traces::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(debug_traces::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(debug_traces::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(debug_traces::Column::RequestedModel)
                .string_len(255)
                .not_null(),
        )
        .col(closed_small_integer(
            debug_traces::Column::DownstreamProtocol,
            1_i16,
            4_i16,
        ))
        .col(closed_small_integer(
            debug_traces::Column::UpstreamProtocol,
            1_i16,
            4_i16,
        ))
        .col(closed_small_integer(
            debug_traces::Column::Operation,
            1_i16,
            2_i16,
        ))
        .col(closed_small_integer(
            debug_traces::Column::Outcome,
            1_i16,
            2_i16,
        ))
        .col(positive_nullable(debug_traces::Column::SelectedChannelId))
        .col(positive_nullable(
            debug_traces::Column::SelectedCredentialId,
        ))
        .col(non_negative(debug_traces::Column::RoutingElapsedMs))
        .col(
            ColumnDef::new(debug_traces::Column::AttemptCount)
                .integer()
                .not_null()
                .check(Expr::col(debug_traces::Column::AttemptCount).between(0_i32, 64_i32)),
        )
        .col(timestamp(manager, debug_traces::Column::CreatedAt))
        .check(
            Expr::col(debug_traces::Column::Outcome)
                .eq(1_i16)
                .and(Expr::col(debug_traces::Column::SelectedChannelId).is_not_null())
                .and(Expr::col(debug_traces::Column::SelectedCredentialId).is_not_null())
                .or(Expr::col(debug_traces::Column::Outcome)
                    .eq(2_i16)
                    .and(Expr::col(debug_traces::Column::SelectedChannelId).is_null())
                    .and(Expr::col(debug_traces::Column::SelectedCredentialId).is_null())),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_traces_user")
                .from(debug_traces::Entity, debug_traces::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_traces_token")
                .from(debug_traces::Entity, debug_traces::Column::TokenId)
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_traces_group")
                .from(debug_traces::Entity, debug_traces::Column::GroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    create_index(
        manager,
        TRACE_REQUEST_ID_INDEX,
        [debug_traces::Column::RequestId],
        true,
    )
    .await?;
    create_index(
        manager,
        TRACE_CREATED_INDEX,
        [debug_traces::Column::CreatedAt],
        false,
    )
    .await?;
    manager
        .create_index(
            Index::create()
                .name(TRACE_OUTCOME_CREATED_INDEX)
                .table(debug_traces::Entity)
                .col(debug_traces::Column::Outcome)
                .col(debug_traces::Column::CreatedAt)
                .to_owned(),
        )
        .await
}

async fn create_attempts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, debug_trace_attempts::Entity);
    statement
        .col(auto_id(debug_trace_attempts::Column::Id))
        .col(
            ColumnDef::new(debug_trace_attempts::Column::TraceId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(debug_trace_attempts::Column::CandidateIndex)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(debug_trace_attempts::Column::CandidateIndex).between(0_i16, 63_i16),
                ),
        )
        .col(positive(debug_trace_attempts::Column::ChannelId))
        .col(positive(debug_trace_attempts::Column::CredentialId))
        .col(closed_small_integer(
            debug_trace_attempts::Column::Outcome,
            1_i16,
            2_i16,
        ))
        .col(
            ColumnDef::new(debug_trace_attempts::Column::FailureKind)
                .small_integer()
                .check(
                    Expr::col(debug_trace_attempts::Column::FailureKind)
                        .is_null()
                        .or(Expr::col(debug_trace_attempts::Column::FailureKind)
                            .between(1_i16, 11_i16)),
                ),
        )
        .col(
            ColumnDef::new(debug_trace_attempts::Column::UpstreamStatus)
                .small_integer()
                .check(
                    Expr::col(debug_trace_attempts::Column::UpstreamStatus)
                        .is_null()
                        .or(Expr::col(debug_trace_attempts::Column::UpstreamStatus)
                            .between(500_i16, 599_i16)),
                ),
        )
        .col(
            ColumnDef::new(debug_trace_attempts::Column::RetryDecision)
                .boolean()
                .not_null(),
        )
        .col(non_negative(debug_trace_attempts::Column::ElapsedMs))
        .col(timestamp(manager, debug_trace_attempts::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_debug_trace_attempts_trace")
                .from(
                    debug_trace_attempts::Entity,
                    debug_trace_attempts::Column::TraceId,
                )
                .to(debug_traces::Entity, debug_traces::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .check(
            Expr::col(debug_trace_attempts::Column::Outcome)
                .eq(1_i16)
                .and(Expr::col(debug_trace_attempts::Column::FailureKind).is_null())
                .and(Expr::col(debug_trace_attempts::Column::UpstreamStatus).is_null())
                .and(Expr::col(debug_trace_attempts::Column::RetryDecision).eq(false))
                .or(Expr::col(debug_trace_attempts::Column::Outcome)
                    .eq(2_i16)
                    .and(Expr::col(debug_trace_attempts::Column::FailureKind).is_not_null())),
        );
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(ATTEMPT_TRACE_INDEX)
                .table(debug_trace_attempts::Entity)
                .col(debug_trace_attempts::Column::TraceId)
                .col(debug_trace_attempts::Column::CandidateIndex)
                .unique()
                .to_owned(),
        )
        .await
}

fn closed_small_integer<T>(column: T, minimum: i16, maximum: i16) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    definition
        .small_integer()
        .not_null()
        .check(Expr::col(column).between(minimum, maximum));
    definition
}

fn non_negative<T>(column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    definition
        .big_integer()
        .not_null()
        .check(Expr::col(column).gte(0_i64));
    definition
}

fn positive<T>(column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    definition
        .big_integer()
        .not_null()
        .check(Expr::col(column).gt(0_i64));
    definition
}

fn positive_nullable<T>(column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    definition.big_integer().check(
        Expr::col(column.clone())
            .is_null()
            .or(Expr::col(column).gt(0_i64)),
    );
    definition
}

async fn create_index<T, const N: usize>(
    manager: &SchemaManager<'_>,
    name: &str,
    columns: [T; N],
    unique: bool,
) -> Result<(), DbErr>
where
    T: IntoIden,
{
    let mut index = Index::create();
    index.name(name).table(debug_traces::Entity);
    for column in columns {
        index.col(column);
    }
    if unique {
        index.unique();
    }
    manager.create_index(index.to_owned()).await
}
