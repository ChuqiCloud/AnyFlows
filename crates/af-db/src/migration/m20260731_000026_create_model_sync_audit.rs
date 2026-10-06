use sea_orm_migration::prelude::*;

use super::{
    iden::{model_sync_items, model_sync_runs},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_model_sync_audit(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(model_sync_items::Entity).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(model_sync_runs::Entity).to_owned())
            .await
    }
}
