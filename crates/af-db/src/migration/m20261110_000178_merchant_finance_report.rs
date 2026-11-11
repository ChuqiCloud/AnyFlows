use sea_orm_migration::prelude::*;

use super::schema;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_merchant_finance_report_index(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::drop_merchant_finance_report_index(manager).await
    }
}
