use sea_orm_migration::prelude::*;

use super::{iden::async_tasks, schema::create_async_tasks};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_async_tasks(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(async_tasks::Entity).to_owned())
            .await
    }
}
