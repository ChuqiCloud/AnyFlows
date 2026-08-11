use sea_orm_migration::prelude::*;

use super::{
    iden::{async_task_billings, async_task_submission_claims},
    schema::{create_async_task_billings, rebuild_usage_logs_for_per_call},
};

/// 增加跨重启视频任务计费绑定，并扩展按次用量模式。
#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column("async_task_submission_claims", "video_duration_seconds")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(async_task_submission_claims::Entity)
                        .add_column(
                            ColumnDef::new(
                                async_task_submission_claims::Column::VideoDurationSeconds,
                            )
                            .small_integer()
                            .check(
                                Expr::col(
                                    async_task_submission_claims::Column::VideoDurationSeconds,
                                )
                                .is_null()
                                .or(Expr::col(
                                    async_task_submission_claims::Column::VideoDurationSeconds,
                                )
                                .between(1_i16, 15_i16)),
                            ),
                        )
                        .to_owned(),
                )
                .await?;
        }
        if !manager.has_table("async_task_billings").await? {
            create_async_task_billings(manager).await?;
        }
        rebuild_usage_logs_for_per_call(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_usage_logs_for_per_call(manager, false).await?;
        if manager.has_table("async_task_billings").await? {
            manager
                .drop_table(Table::drop().table(async_task_billings::Entity).to_owned())
                .await?;
        }
        if manager
            .has_column("async_task_submission_claims", "video_duration_seconds")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(async_task_submission_claims::Entity)
                        .drop_column(async_task_submission_claims::Column::VideoDurationSeconds)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
