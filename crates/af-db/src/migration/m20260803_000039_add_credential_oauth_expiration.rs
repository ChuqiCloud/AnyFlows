use sea_orm_migration::prelude::*;

use super::iden::credentials;

const OAUTH_REFRESH_DUE_INDEX: &str = "idx_credentials_oauth_refresh_due";

#[derive(DeriveIden)]
enum CredentialOAuthColumn {
    OauthExpiresAtEpochSeconds,
}

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(credentials::Entity)
                    .add_column(
                        ColumnDef::new(CredentialOAuthColumn::OauthExpiresAtEpochSeconds)
                            .big_integer()
                            .check(
                                Expr::col(CredentialOAuthColumn::OauthExpiresAtEpochSeconds)
                                    .gte(0_i64),
                            ),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(OAUTH_REFRESH_DUE_INDEX)
                    .table(credentials::Entity)
                    .col(CredentialOAuthColumn::OauthExpiresAtEpochSeconds)
                    .col(credentials::Column::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(OAUTH_REFRESH_DUE_INDEX)
                    .table(credentials::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(credentials::Entity)
                    .drop_column(CredentialOAuthColumn::OauthExpiresAtEpochSeconds)
                    .to_owned(),
            )
            .await
    }
}
