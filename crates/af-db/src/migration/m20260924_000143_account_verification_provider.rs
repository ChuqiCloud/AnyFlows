use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("account_verifications"))
                    .add_column(ColumnDef::new(Alias::new("provider_reference")).string_len(128))
                    .add_column(ColumnDef::new(Alias::new("provider_action_url")).string_len(2_048))
                    .add_column(ColumnDef::new(Alias::new("provider_status")).string_len(32))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("account_verifications"))
                    .drop_column(Alias::new("provider_status"))
                    .drop_column(Alias::new("provider_action_url"))
                    .drop_column(Alias::new("provider_reference"))
                    .to_owned(),
            )
            .await
    }
}
