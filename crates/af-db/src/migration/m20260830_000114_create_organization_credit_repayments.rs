use sea_orm_migration::prelude::*;

use super::schema::{create_organization_credit_repayments, drop_organization_credit_repayments};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_organization_credit_repayments(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_organization_credit_repayments(manager).await
    }
}
