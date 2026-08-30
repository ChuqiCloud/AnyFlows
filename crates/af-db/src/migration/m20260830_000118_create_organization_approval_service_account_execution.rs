use sea_orm_migration::prelude::*;

use super::schema::{
    create_organization_approval_service_account_executions,
    drop_organization_approval_service_account_executions,
};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        create_organization_approval_service_account_executions(manager).await
    }

    async fn down(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        drop_organization_approval_service_account_executions(manager).await
    }
}
