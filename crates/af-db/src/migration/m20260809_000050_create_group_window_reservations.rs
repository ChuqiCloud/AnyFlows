use sea_orm_migration::prelude::*;

use super::{
    iden::{billing_group_window_reservations, billing_reservations, groups},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::add_group_window_state(manager).await?;
        schema::create_billing_group_window_reservations(manager).await?;
        schema::create_billing_group_window_reservations_index(manager).await?;
        schema::backfill_active_billing_group_window_reservations(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(billing_group_window_reservations::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_billing_reservations_group_status")
                    .table(billing_reservations::Entity)
                    .to_owned(),
            )
            .await?;
        for column in [
            groups::Column::MonthlyWindowStart,
            groups::Column::WeeklyWindowStart,
            groups::Column::DailyWindowStart,
            groups::Column::MonthlyUsage,
            groups::Column::WeeklyUsage,
            groups::Column::DailyUsage,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(groups::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
