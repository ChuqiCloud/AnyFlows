use sea_orm_migration::prelude::*;

use super::{iden::billing_batch_checkpoints, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_billing_batch_checkpoints(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(billing_batch_checkpoints::Entity)
                    .to_owned(),
            )
            .await
    }
}
