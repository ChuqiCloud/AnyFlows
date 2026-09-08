use sea_orm_migration::prelude::*;

use super::iden::credentials;

#[derive(DeriveIden)]
enum CredentialOAuthColumn {
    OauthTokenPending,
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
                        ColumnDef::new(CredentialOAuthColumn::OauthTokenPending)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(credentials::Entity)
                    .drop_column(CredentialOAuthColumn::OauthTokenPending)
                    .to_owned(),
            )
            .await
    }
}
