use sea_orm_migration::prelude::*;

use super::iden::{async_task_submission_claims, usage_logs};

const MAX_VIDEO_DURATION_SECONDS: i64 = 24 * 60 * 60;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_video_duration(manager).await?;
        add_usage_video_resolution(manager).await?;
        add_submission_video_resolution(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_column(
            manager,
            async_task_submission_claims::Entity,
            async_task_submission_claims::Column::VideoResolution,
        )
        .await?;
        drop_column(
            manager,
            usage_logs::Entity,
            usage_logs::Column::VideoResolution,
        )
        .await?;
        drop_column(
            manager,
            usage_logs::Entity,
            usage_logs::Column::VideoDurationSeconds,
        )
        .await
    }
}

async fn add_video_duration(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(
                    ColumnDef::new(usage_logs::Column::VideoDurationSeconds)
                        .big_integer()
                        .check(
                            Expr::col(usage_logs::Column::VideoDurationSeconds)
                                .is_null()
                                .or(Expr::col(usage_logs::Column::VideoDurationSeconds)
                                    .between(1_i64, MAX_VIDEO_DURATION_SECONDS)),
                        ),
                )
                .to_owned(),
        )
        .await
}

async fn add_usage_video_resolution(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(video_resolution_column(usage_logs::Column::VideoResolution))
                .to_owned(),
        )
        .await
}

async fn add_submission_video_resolution(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(async_task_submission_claims::Entity)
                .add_column(video_resolution_column(
                    async_task_submission_claims::Column::VideoResolution,
                ))
                .to_owned(),
        )
        .await
}

fn video_resolution_column<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition.small_integer().check(
        Expr::col(column)
            .is_null()
            .or(Expr::col(column).is_in([1_i16, 2_i16, 3_i16])),
    );
    definition
}

async fn drop_column<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + Copy + 'static,
    C: IntoIden + Copy + 'static,
{
    manager
        .alter_table(Table::alter().table(table).drop_column(column).to_owned())
        .await
}
