use sea_orm_migration::prelude::*;

use super::iden::account_verification_settings as settings;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(settings::Entity)
                    .add_column(
                        ColumnDef::new(settings::Column::ManualEnabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(settings::Entity)
                    .drop_column(settings::Column::ManualEnabled)
                    .to_owned(),
            )
            .await
    }
}
