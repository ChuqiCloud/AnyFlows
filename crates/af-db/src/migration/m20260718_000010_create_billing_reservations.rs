use sea_orm_migration::prelude::*;

use super::{iden::billing_reservations, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_billing_reservations(manager).await?;
        schema::create_billing_reservations_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(billing_reservations::Entity).to_owned())
            .await
    }
}
