use sea_orm_migration::prelude::*;

use super::schema::{
    create_organization_approval_budget_executions, drop_organization_approval_budget_executions,
};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        create_organization_approval_budget_executions(manager).await
    }

    async fn down(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        drop_organization_approval_budget_executions(manager).await
    }
}
