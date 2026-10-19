use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("proxy_pools"))
                    .add_column(ColumnDef::new(Alias::new("last_refresh_attempt_at")).big_integer())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("proxy_pools"))
                    .drop_column(Alias::new("last_refresh_attempt_at"))
                    .to_owned(),
            )
            .await
    }
}
