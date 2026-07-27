use sea_orm_migration::prelude::*;

use super::{iden::channel_groups, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_channel_groups(manager).await?;
        schema::create_channel_groups_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(channel_groups::Entity).to_owned())
            .await
    }
}
