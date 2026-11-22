use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_plugin_storage_expiry_at")
                    .table(Alias::new("plugin_storage"))
                    .col(Alias::new("expires_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_plugin_storage_expiry_at")
                    .table(Alias::new("plugin_storage"))
                    .to_owned(),
            )
            .await
    }
}
