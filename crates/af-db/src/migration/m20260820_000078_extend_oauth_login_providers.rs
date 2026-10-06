use sea_orm_migration::prelude::*;

use super::iden::oauth_login_providers;

const OIDC_PROVIDER: &str = "oidc";
const LINUXDO_PROVIDER: &str = "linuxdo";
const LINUXDO_ISSUER: &str = "https://connect.linux.do/";

/// 为通用 OIDC 和 LinuxDO 预设增加受控 issuer 配置。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(oauth_login_providers::Entity)
                    .add_column(
                        ColumnDef::new(oauth_login_providers::Column::IssuerUrl)
                            .string_len(2048)
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        for provider in [OIDC_PROVIDER, LINUXDO_PROVIDER] {
            manager
                .exec_stmt(
                    Query::insert()
                        .into_table(oauth_login_providers::Entity)
                        .columns([oauth_login_providers::Column::Provider])
                        .values_panic([provider.into()])
                        .to_owned(),
                )
                .await?;
        }
        manager
            .exec_stmt(
                Query::update()
                    .table(oauth_login_providers::Entity)
                    .value(oauth_login_providers::Column::IssuerUrl, LINUXDO_ISSUER)
                    .and_where(
                        Expr::col(oauth_login_providers::Column::Provider).eq(LINUXDO_PROVIDER),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for provider in [OIDC_PROVIDER, LINUXDO_PROVIDER] {
            manager
                .exec_stmt(
                    Query::delete()
                        .from_table(oauth_login_providers::Entity)
                        .and_where(Expr::col(oauth_login_providers::Column::Provider).eq(provider))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(oauth_login_providers::Entity)
                    .drop_column(oauth_login_providers::Column::IssuerUrl)
                    .to_owned(),
            )
            .await
    }
}
