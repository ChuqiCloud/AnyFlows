use sea_orm_migration::prelude::*;

use super::{iden::usage_logs, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_usage_logs(manager).await?;
        schema::create_usage_logs_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(usage_logs::Entity).to_owned())
            .await
    }
}
