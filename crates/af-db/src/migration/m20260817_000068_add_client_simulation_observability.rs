use sea_orm_migration::prelude::*;

use super::iden::debug_trace_attempts;

/// 为脱敏 Attempt 增加闭合仿真档案和应用结果。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_profile_column(manager).await?;
        add_result_column(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            debug_trace_attempts::Column::ClientSimulationResult,
            debug_trace_attempts::Column::ClientSimulationProfile,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(debug_trace_attempts::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

async fn add_profile_column(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_attempts::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_attempts::Column::ClientSimulationProfile)
                        .string_len(64)
                        .check(
                            Expr::col(debug_trace_attempts::Column::ClientSimulationProfile)
                                .is_null()
                                .or(Expr::col(
                                    debug_trace_attempts::Column::ClientSimulationProfile,
                                )
                                .eq("anthropic_cli_headers_v1")),
                        ),
                )
                .to_owned(),
        )
        .await
}

async fn add_result_column(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_attempts::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_attempts::Column::ClientSimulationResult)
                        .string_len(32)
                        .check(
                            Expr::col(debug_trace_attempts::Column::ClientSimulationResult)
                                .is_null()
                                .or(Expr::col(
                                    debug_trace_attempts::Column::ClientSimulationResult,
                                )
                                .is_in([
                                    "not_applied",
                                    "applied",
                                    "failed",
                                ])),
                        ),
                )
                .to_owned(),
        )
        .await
}
