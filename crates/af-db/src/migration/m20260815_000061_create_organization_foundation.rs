use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_memberships, organization_teams, organizations},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_organization_foundation(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 成员先于团队、团队先于企业回滚，保持所有 RESTRICT 外键方向可验证。
        for table in [
            organization_memberships::Entity.into_iden(),
            organization_teams::Entity.into_iden(),
            organizations::Entity.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}
