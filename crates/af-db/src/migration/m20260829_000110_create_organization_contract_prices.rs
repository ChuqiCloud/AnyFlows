use sea_orm_migration::prelude::*;

use super::schema::{create_organization_contract_prices, drop_organization_contract_prices};

/// 创建企业合同价版本事实表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_organization_contract_prices(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_organization_contract_prices(manager).await
    }
}
