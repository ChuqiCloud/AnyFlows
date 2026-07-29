use sea_orm_migration::prelude::*;

use super::{iden::playground_shares, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_playground_shares(manager).await?;
        schema::create_playground_shares_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(playground_shares::Entity).to_owned())
            .await
    }
}
