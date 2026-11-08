use sea_orm_migration::prelude::*;

use super::iden::account_verification_settings as settings;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            settings::Column::RequireVerificationForTopup,
            settings::Column::RequireVerificationForApi,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(settings::Entity)
                        .add_column(ColumnDef::new(column).boolean().not_null().default(false))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            settings::Column::RequireVerificationForApi,
            settings::Column::RequireVerificationForTopup,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(settings::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
