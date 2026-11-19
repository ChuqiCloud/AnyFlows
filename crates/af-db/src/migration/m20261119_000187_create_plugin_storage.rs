use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("plugin_storage"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("plugin_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("storage_key"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("value_json")).json().not_null())
                    .col(
                        ColumnDef::new(Alias::new("expires_at"))
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .name("pk_plugin_storage")
                            .col(Alias::new("plugin_id"))
                            .col(Alias::new("storage_key")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_plugin_storage_expiry")
                    .table(Alias::new("plugin_storage"))
                    .col(Alias::new("plugin_id"))
                    .col(Alias::new("expires_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("plugin_storage")).to_owned())
            .await
    }
}
