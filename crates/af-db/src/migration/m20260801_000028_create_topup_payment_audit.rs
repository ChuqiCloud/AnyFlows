use sea_orm_migration::prelude::*;

use super::{iden, schema};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_topup_payment_audit(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(iden::topup_payment_events::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(iden::topup_orders::Entity).to_owned())
            .await?;
        Ok(())
    }
}
