use sea_orm_migration::prelude::*;

use super::{iden::auth_challenge_rate_limits, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_auth_challenge_rate_limits(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(auth_challenge_rate_limits::Entity)
                    .to_owned(),
            )
            .await
    }
}
