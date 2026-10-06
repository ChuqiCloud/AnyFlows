use sea_orm_migration::prelude::*;

use super::{iden::playground_conversations, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_playground_conversations(manager).await?;
        schema::create_playground_conversation_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(playground_conversations::Entity)
                    .to_owned(),
            )
            .await
    }
}
