use sea_orm_migration::prelude::*;

use super::{iden::wallet_ledger_entries, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_wallet_ledger(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(wallet_ledger_entries::Entity)
                    .to_owned(),
            )
            .await
    }
}
