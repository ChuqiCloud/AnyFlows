use sea_orm_migration::prelude::*;

use super::{
    iden::{billing_reservations, billing_token_window_reservations},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_billing_token_window_reservations(manager).await?;
        schema::create_billing_token_window_reservations_index(manager).await?;
        schema::backfill_active_billing_token_window_reservations(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(billing_token_window_reservations::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_billing_reservations_token_status")
                    .table(billing_reservations::Entity)
                    .to_owned(),
            )
            .await
    }
}
