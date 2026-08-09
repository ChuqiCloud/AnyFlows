use sea_orm::DbBackend;
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
        ensure_mysql_group_foreign_key_index(manager).await?;
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

/// MySQL 可能让本迁移的联合索引接管分组外键，降级前必须先补替代索引。
async fn ensure_mysql_group_foreign_key_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    const INDEX_NAME: &str = "idx_billing_reservations_group_fk";
    if manager.get_database_backend() != DbBackend::MySql
        || manager
            .has_index("billing_reservations", INDEX_NAME)
            .await?
    {
        return Ok(());
    }
    manager
        .create_index(
            Index::create()
                .name(INDEX_NAME)
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::GroupId)
                .to_owned(),
        )
        .await
}
