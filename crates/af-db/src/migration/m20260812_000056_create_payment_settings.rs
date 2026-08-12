use sea_orm_migration::prelude::*;

use super::{iden, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_payment_settings(manager).await?;
        schema::extend_topup_payment_facts(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 每条 ALTER TABLE 只支持删除一列，逐列执行保证三种数据库行为一致。
        for column in [
            iden::topup_payment_events::Column::PaymentMethod,
            iden::topup_payment_events::Column::Currency,
            iden::topup_payment_events::Column::AmountMinor,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(iden::topup_payment_events::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(iden::topup_orders::Entity)
                    .drop_column(iden::topup_orders::Column::PaymentMethod)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(iden::payment_settings::Entity)
                    .to_owned(),
            )
            .await
    }
}
