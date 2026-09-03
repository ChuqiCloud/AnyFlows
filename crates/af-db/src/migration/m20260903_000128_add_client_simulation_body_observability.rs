use sea_orm_migration::prelude::*;

use super::iden::debug_trace_attempts;

/// 为调试追踪增加闭合正文仿真档案和补丁结果，不保存日期或正文。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_body_profile_column(manager).await?;
        add_body_result_column(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            debug_trace_attempts::Column::ClientSimulationBodyResult,
            debug_trace_attempts::Column::ClientSimulationBodyProfile,
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

async fn add_body_profile_column(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_attempts::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_attempts::Column::ClientSimulationBodyProfile)
                        .string_len(64)
                        .check(
                            Expr::col(debug_trace_attempts::Column::ClientSimulationBodyProfile)
                                .is_null()
                                .or(Expr::col(
                                    debug_trace_attempts::Column::ClientSimulationBodyProfile,
                                )
                                .eq("anthropic_cli_system_date_v1")),
                        ),
                )
                .to_owned(),
        )
        .await
}

async fn add_body_result_column(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_attempts::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_attempts::Column::ClientSimulationBodyResult)
                        .string_len(32)
                        .check(
                            Expr::col(debug_trace_attempts::Column::ClientSimulationBodyResult)
                                .is_null()
                                .or(Expr::col(
                                    debug_trace_attempts::Column::ClientSimulationBodyResult,
                                )
                                .is_in(["applied", "rejected"])),
                        ),
                )
                .to_owned(),
        )
        .await
}
