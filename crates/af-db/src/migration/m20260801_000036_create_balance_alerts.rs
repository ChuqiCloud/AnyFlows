use sea_orm_migration::prelude::*;

use super::{
    iden::{balance_alert_events, balance_alert_settings, users},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_balance_alert_storage(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(balance_alert_events::Entity).to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(balance_alert_settings::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(users::Entity)
                    .drop_column(users::Column::BalanceAlertThreshold)
                    .to_owned(),
            )
            .await
    }
}
