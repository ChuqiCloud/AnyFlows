use sea_orm_migration::prelude::*;

use crate::migration::iden::payment_settings;

/// 为已有支付设置补齐退款能力和审批后自动提交策略，历史数据默认关闭。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(payment_settings::Entity)
                    .add_column(
                        ColumnDef::new(payment_settings::Column::EpayRefundEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(payment_settings::Entity)
                    .add_column(
                        ColumnDef::new(payment_settings::Column::RefundAutoSubmitEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(payment_settings::Entity)
                    .drop_column(payment_settings::Column::RefundAutoSubmitEnabled)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(payment_settings::Entity)
                    .drop_column(payment_settings::Column::EpayRefundEnabled)
                    .to_owned(),
            )
            .await
    }
}
