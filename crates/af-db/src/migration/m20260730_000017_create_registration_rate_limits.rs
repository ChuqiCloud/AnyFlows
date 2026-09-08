use sea_orm_migration::prelude::*;

use super::{iden::registration_rate_limits, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_registration_rate_limits(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(registration_rate_limits::Entity)
                    .to_owned(),
            )
            .await
    }
}
