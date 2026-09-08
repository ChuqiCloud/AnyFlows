use sea_orm_migration::prelude::*;

use super::iden::oauth_login_providers;

const WECHAT_PROVIDER: &str = "wechat";

/// 预置默认关闭的微信开放平台登录 Provider。
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
                    .values_panic([WECHAT_PROVIDER.into()])
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
                        Expr::col(oauth_login_providers::Column::Provider).eq(WECHAT_PROVIDER),
                    )
                    .to_owned(),
            )
            .await
    }
}
