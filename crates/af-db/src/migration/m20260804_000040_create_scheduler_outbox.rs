use sea_orm_migration::prelude::*;

use super::{iden::scheduler_outbox_events, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_scheduler_outbox(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(scheduler_outbox_events::Entity)
                    .to_owned(),
            )
            .await
    }
}
