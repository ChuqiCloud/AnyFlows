use sea_orm_migration::prelude::*;

use crate::migration::{iden::organization_invitations, schema};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_organization_invitations(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_invitations::Entity)
                    .to_owned(),
            )
            .await
    }
}
