use sea_orm_migration::prelude::*;

use super::iden::oauth_login_providers;

const TELEGRAM_PROVIDER: &str = "telegram";
const TELEGRAM_ISSUER: &str = "https://oauth.telegram.org";

/// 预置默认关闭且 issuer 固定的 Telegram OIDC 登录 Provider。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(oauth_login_providers::Entity)
                    .columns([
                        oauth_login_providers::Column::Provider,
                        oauth_login_providers::Column::IssuerUrl,
                    ])
                    .values_panic([TELEGRAM_PROVIDER.into(), TELEGRAM_ISSUER.into()])
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
                        Expr::col(oauth_login_providers::Column::Provider).eq(TELEGRAM_PROVIDER),
                    )
                    .to_owned(),
            )
            .await
    }
}
