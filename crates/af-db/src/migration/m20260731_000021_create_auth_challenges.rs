use sea_orm_migration::prelude::*;

use super::{iden::auth_challenges, schema};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_auth_challenges(manager).await?;
        schema::create_auth_challenge_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(auth_challenges::Entity).to_owned())
            .await
    }
}
