use sea_orm_migration::prelude::*;

use super::{iden::organization_credit_terms, schema::create_organization_credit_terms};

/// 创建独立于钱包和账单的企业授信/账期策略事实表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        create_organization_credit_terms(manager).await
    }

    async fn down(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_credit_terms::Entity)
                    .to_owned(),
            )
            .await
    }
}
