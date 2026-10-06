use sea_orm_migration::prelude::*;

use super::iden::usage_logs;

const MAX_REQUEST_ID_BYTES: u32 = 128;
const MAX_MODEL_BYTES: u32 = 255;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_text_column(manager, usage_logs::Column::RequestId, MAX_REQUEST_ID_BYTES).await?;
        add_text_column(manager, usage_logs::Column::Model, MAX_MODEL_BYTES).await?;
        add_small_integer(manager, usage_logs::Column::Protocol).await?;
        add_small_integer(manager, usage_logs::Column::Operation).await?;
        add_boolean(manager, usage_logs::Column::IsStream).await?;
        add_small_integer(manager, usage_logs::Column::ReasoningEffort).await?;
        add_non_negative_big_integer(manager, usage_logs::Column::ReasoningBudgetTokens).await?;
        add_non_negative_big_integer(manager, usage_logs::Column::FirstTokenMs).await?;
        add_non_negative_big_integer(manager, usage_logs::Column::DurationMs).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            usage_logs::Column::DurationMs,
            usage_logs::Column::FirstTokenMs,
            usage_logs::Column::ReasoningBudgetTokens,
            usage_logs::Column::ReasoningEffort,
            usage_logs::Column::IsStream,
            usage_logs::Column::Operation,
            usage_logs::Column::Protocol,
            usage_logs::Column::Model,
            usage_logs::Column::RequestId,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(usage_logs::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

async fn add_text_column(
    manager: &SchemaManager<'_>,
    column: usage_logs::Column,
    max_len: u32,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(ColumnDef::new(column).string_len(max_len))
                .to_owned(),
        )
        .await
}

async fn add_small_integer(
    manager: &SchemaManager<'_>,
    column: usage_logs::Column,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(ColumnDef::new(column).small_integer())
                .to_owned(),
        )
        .await
}

async fn add_boolean(manager: &SchemaManager<'_>, column: usage_logs::Column) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(ColumnDef::new(column).boolean())
                .to_owned(),
        )
        .await
}

async fn add_non_negative_big_integer(
    manager: &SchemaManager<'_>,
    column: usage_logs::Column,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(usage_logs::Entity)
                .add_column(
                    ColumnDef::new(column)
                        .big_integer()
                        .check(Expr::col(column).is_null().or(Expr::col(column).gte(0_i64))),
                )
                .to_owned(),
        )
        .await
}
