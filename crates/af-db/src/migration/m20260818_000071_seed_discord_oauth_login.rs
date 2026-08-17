use sea_orm_migration::prelude::*;

use super::iden::oauth_login_providers;

const DISCORD_PROVIDER: &str = "discord";

/// 为已有 OAuth 登录表补齐禁用态 Discord Provider 配置行。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(oauth_login_providers::Entity)
                    .columns([oauth_login_providers::Column::Provider])
                    .values_panic([DISCORD_PROVIDER.into()])
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .exec_stmt(
                Query::delete()
                    .from_table(oauth_login_providers::Entity)
                    .and_where(
                        Expr::col(oauth_login_providers::Column::Provider).eq(DISCORD_PROVIDER),
                    )
                    .to_owned(),
            )
            .await
    }
}
