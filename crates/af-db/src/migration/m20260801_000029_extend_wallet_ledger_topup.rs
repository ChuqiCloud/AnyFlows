use sea_orm_migration::prelude::*;

use super::schema;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::rebuild_wallet_ledger_for_topup(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::rebuild_wallet_ledger_for_topup(manager, false).await
    }
}
