use sea_orm_migration::prelude::*;

use super::{iden::organization_plan_prices, iden::organization_plans, schema};

/// 创建企业套餐目录及不可变价格快照。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_organization_plan_storage(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_plan_prices::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(organization_plans::Entity).to_owned())
            .await
    }
}
