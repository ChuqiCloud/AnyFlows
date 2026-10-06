use sea_orm_migration::prelude::*;

use crate::migration::iden::{subscription_orders, subscription_payment_events};

use super::schema;

/// 补齐订阅订单支付事实字段并创建订阅专用支付审计表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 一次 ALTER TABLE 只能增加一列，拆开执行以保持三方言一致。
        manager
            .alter_table(
                Table::alter()
                    .table(subscription_orders::Entity)
                    .add_column(
                        ColumnDef::new(subscription_orders::Column::TradeNo).string_len(128),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(subscription_orders::Entity)
                    .add_column(
                        ColumnDef::new(subscription_orders::Column::PaymentMethod).string_len(32),
                    )
                    .to_owned(),
            )
            .await?;
        schema::create_subscription_payment_events(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(subscription_payment_events::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(subscription_orders::Entity)
                    .drop_column(subscription_orders::Column::PaymentMethod)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(subscription_orders::Entity)
                    .drop_column(subscription_orders::Column::TradeNo)
                    .to_owned(),
            )
            .await
    }
}
