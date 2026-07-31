use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{channels, model_sync_items, model_sync_runs, models, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建上游模型枚举预览及逐项脱敏证据的审计表。
pub(in crate::migration) async fn create_model_sync_audit(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_runs(manager).await?;
    create_items(manager).await?;
    create_indexes(manager).await
}

async fn create_runs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut expires_at = nullable_timestamp(manager, model_sync_runs::Column::ExpiresAt);
    expires_at.not_null();
    manager
        .create_table(
            table(manager, model_sync_runs::Entity)
                .col(auto_id(model_sync_runs::Column::Id))
                .col(
                    ColumnDef::new(model_sync_runs::Column::PreviewId)
                        .char_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(model_sync_runs::Column::ChannelId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(model_sync_runs::Column::ActorUserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(model_sync_runs::Column::ChannelType)
                        .string_len(64)
                        .not_null()
                        .check(Expr::col(model_sync_runs::Column::ChannelType).ne("")),
                )
                .col(
                    ColumnDef::new(model_sync_runs::Column::Protocol)
                        .string_len(64)
                        .not_null()
                        .check(Expr::col(model_sync_runs::Column::Protocol).ne("")),
                )
                .col(
                    ColumnDef::new(model_sync_runs::Column::State)
                        .small_integer()
                        .not_null()
                        .check(Expr::col(model_sync_runs::Column::State).is_in([1_i16, 2_i16])),
                )
                .col(
                    ColumnDef::new(model_sync_runs::Column::CandidateCount)
                        .integer()
                        .not_null()
                        .check(
                            Expr::col(model_sync_runs::Column::CandidateCount)
                                .between(0_i32, 1_000_i32),
                        ),
                )
                .col(expires_at)
                .col(nullable_timestamp(
                    manager,
                    model_sync_runs::Column::AppliedAt,
                ))
                .col(timestamp(manager, model_sync_runs::Column::CreatedAt))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_model_sync_runs_channel")
                        .from(model_sync_runs::Entity, model_sync_runs::Column::ChannelId)
                        .to(channels::Entity, channels::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_model_sync_runs_actor")
                        .from(
                            model_sync_runs::Entity,
                            model_sync_runs::Column::ActorUserId,
                        )
                        .to(users::Entity, users::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .check(
                    Expr::col(model_sync_runs::Column::ExpiresAt)
                        .gt(Expr::col(model_sync_runs::Column::CreatedAt)),
                )
                .check(
                    Expr::col(model_sync_runs::Column::State)
                        .eq(1_i16)
                        .and(Expr::col(model_sync_runs::Column::AppliedAt).is_null())
                        .or(Expr::col(model_sync_runs::Column::State)
                            .eq(2_i16)
                            .and(Expr::col(model_sync_runs::Column::AppliedAt).is_not_null())),
                )
                .to_owned(),
        )
        .await
}

async fn create_items(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, model_sync_items::Entity);
    statement
        .col(auto_id(model_sync_items::Column::Id))
        .col(
            ColumnDef::new(model_sync_items::Column::RunId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(model_sync_items::Column::Ordinal)
                .integer()
                .not_null()
                .check(Expr::col(model_sync_items::Column::Ordinal).gte(0_i32)),
        )
        .col(
            ColumnDef::new(model_sync_items::Column::CanonicalModel)
                .string_len(256)
                .not_null()
                .check(Expr::col(model_sync_items::Column::CanonicalModel).ne("")),
        )
        .col(ColumnDef::new(model_sync_items::Column::UpstreamModel).string_len(256))
        .col(
            ColumnDef::new(model_sync_items::Column::Relation)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(model_sync_items::Column::Relation)
                        .is_in([1_i16, 2_i16, 3_i16, 4_i16]),
                ),
        )
        .col(ColumnDef::new(model_sync_items::Column::DisplayNameHint).string_len(128))
        .col(ColumnDef::new(model_sync_items::Column::DescriptionHint).text())
        .col(positive_optional(
            model_sync_items::Column::ContextWindowHint,
        ))
        .col(positive_optional(
            model_sync_items::Column::InputTokenLimitHint,
        ))
        .col(positive_optional(
            model_sync_items::Column::OutputTokenLimitHint,
        ))
        .col(
            ColumnDef::new(model_sync_items::Column::SupportedMethods)
                .json_binary()
                .not_null(),
        )
        .col(ColumnDef::new(model_sync_items::Column::AppliedModelId).big_integer())
        .foreign_key(
            ForeignKey::create()
                .name("fk_model_sync_items_run")
                .from(model_sync_items::Entity, model_sync_items::Column::RunId)
                .to(model_sync_runs::Entity, model_sync_runs::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_model_sync_items_applied_model")
                .from(
                    model_sync_items::Entity,
                    model_sync_items::Column::AppliedModelId,
                )
                .to(models::Entity, models::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    // MySQL 8.4 禁止 CHECK 引用带级联动作的外键列；该方言由实体与仓储双重校验同一不变量。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(
            Expr::col(model_sync_items::Column::AppliedModelId)
                .is_null()
                .or(Expr::col(model_sync_items::Column::Relation).is_in([1_i16, 2_i16])),
        );
    }
    manager.create_table(statement).await
}

fn positive_optional<T>(column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    definition.big_integer().check(Expr::col(column).gt(0_i64));
    definition
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_model_sync_runs_preview_id")
            .table(model_sync_runs::Entity)
            .col(model_sync_runs::Column::PreviewId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_model_sync_runs_channel_created")
            .table(model_sync_runs::Entity)
            .col(model_sync_runs::Column::ChannelId)
            .col(model_sync_runs::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("uq_model_sync_items_run_ordinal")
            .table(model_sync_items::Entity)
            .col(model_sync_items::Column::RunId)
            .col(model_sync_items::Column::Ordinal)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_model_sync_items_run_relation")
            .table(model_sync_items::Entity)
            .col(model_sync_items::Column::RunId)
            .col(model_sync_items::Column::Relation)
            .col(model_sync_items::Column::Ordinal)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}
