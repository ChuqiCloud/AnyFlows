use sea_orm_migration::prelude::*;

use super::iden::credentials;

#[derive(DeriveIden)]
enum CredentialOAuthColumn {
    OauthRevision,
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
                        ColumnDef::new(CredentialOAuthColumn::OauthRevision)
                            .big_integer()
                            .not_null()
                            .default(0_i64)
                            .check(Expr::col(CredentialOAuthColumn::OauthRevision).gte(0_i64)),
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
                    .drop_column(CredentialOAuthColumn::OauthRevision)
                    .to_owned(),
            )
            .await
    }
}
