use sea_orm_migration::prelude::*;

use super::{
    iden::{debug_trace_attempts, debug_trace_settings, debug_traces},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_debug_trace_storage(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(debug_trace_attempts::Entity).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(debug_traces::Entity).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(debug_trace_settings::Entity).to_owned())
            .await
    }
}
