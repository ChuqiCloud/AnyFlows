use sea_orm_migration::prelude::*;

use super::{
    iden::{balance_alert_settings, subscription_balance_alert_events},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_subscription_balance_alert_storage(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(subscription_balance_alert_events::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(balance_alert_settings::Entity)
                    .drop_column(balance_alert_settings::Column::SubscriptionRemainingPercent)
                    .to_owned(),
            )
            .await?;
        // SQLite 不支持一次 ALTER TABLE 删除多个字段，按升级的反向顺序独立回退。
        manager
            .alter_table(
                Table::alter()
                    .table(balance_alert_settings::Entity)
                    .drop_column(balance_alert_settings::Column::SubscriptionAlertEnabled)
                    .to_owned(),
            )
            .await
    }
}
