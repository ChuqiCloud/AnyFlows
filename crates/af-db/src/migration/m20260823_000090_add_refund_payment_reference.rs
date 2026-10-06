use sea_orm_migration::prelude::*;

use crate::migration::iden::refund_requests;

/// 保存 Provider 原支付流水号；历史请求为空时禁止自动或人工提交。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .add_column(
                        ColumnDef::new(refund_requests::Column::PaymentReference).string_len(128),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .drop_column(refund_requests::Column::PaymentReference)
                    .to_owned(),
            )
            .await
    }
}
