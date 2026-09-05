use sea_orm_migration::prelude::*;

use super::{iden::organization_plan_orders, iden::organization_plan_payment_events, schema};

/// 创建企业授权订单和支付事件事实；本迁移不接入 Provider 或授权发放。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_organization_plan_order_storage(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_plan_payment_events::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(organization_plan_orders::Entity)
                    .to_owned(),
            )
            .await
    }
}
