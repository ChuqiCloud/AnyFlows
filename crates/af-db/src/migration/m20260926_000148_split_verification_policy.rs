use sea_orm_migration::prelude::*;

use super::iden::account_verification_settings as settings;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            settings::Column::IndividualManualEnabled,
            settings::Column::EnterpriseManualEnabled,
            settings::Column::IndividualReasonRequired,
            settings::Column::EnterpriseReasonRequired,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(settings::Entity)
                        .add_column(ColumnDef::new(column).boolean().not_null().default(true))
                        .to_owned(),
                )
                .await?;
        }
        // Preserve the existing manual-review setting for installations upgrading in place.
        manager
            .exec_stmt(
                Query::update()
                    .table(settings::Entity)
                    .values([
                        (
                            settings::Column::IndividualManualEnabled,
                            Expr::col(settings::Column::ManualEnabled).into(),
                        ),
                        (
                            settings::Column::EnterpriseManualEnabled,
                            Expr::col(settings::Column::ManualEnabled).into(),
                        ),
                    ])
                    .and_where(Expr::col(settings::Column::Id).eq(1_i16))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            settings::Column::EnterpriseReasonRequired,
            settings::Column::IndividualReasonRequired,
            settings::Column::EnterpriseManualEnabled,
            settings::Column::IndividualManualEnabled,
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
