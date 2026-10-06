use sea_orm_migration::prelude::*;

use super::iden::payment_settings;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(payment_settings::Entity)
                    .add_column(
                        ColumnDef::new(payment_settings::Column::EpayQrEnabled)
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
                    .table(payment_settings::Entity)
                    .drop_column(payment_settings::Column::EpayQrEnabled)
                    .to_owned(),
            )
            .await
    }
}
