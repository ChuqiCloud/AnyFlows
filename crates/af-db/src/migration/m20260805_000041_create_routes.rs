use sea_orm_migration::prelude::*;

use super::{
    iden::{route_channels, routes},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_routes(manager).await?;
        schema::create_route_channels(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(route_channels::Entity).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(routes::Entity).to_owned())
            .await
    }
}
