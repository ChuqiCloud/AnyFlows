use sea_orm_migration::prelude::*;

use super::schema::{create_organization_credit_invoices, drop_organization_credit_invoices};

/// 创建企业账期账单事实表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        create_organization_credit_invoices(manager).await
    }

    async fn down(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        drop_organization_credit_invoices(manager).await
    }
}
