use sea_orm_migration::prelude::*;

use super::iden::usage_logs;

const MAX_AUDIO_DURATION_NANOSECONDS: i64 = 24 * 60 * 60 * 1_000_000_000;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(usage_logs::Entity)
                    .add_column(
                        ColumnDef::new(usage_logs::Column::AudioDurationNanoseconds)
                            .big_integer()
                            .check(
                                Expr::col(usage_logs::Column::AudioDurationNanoseconds)
                                    .between(0_i64, MAX_AUDIO_DURATION_NANOSECONDS),
                            ),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(usage_logs::Entity)
                    .drop_column(usage_logs::Column::AudioDurationNanoseconds)
                    .to_owned(),
            )
            .await
    }
}
