use sea_orm_migration::prelude::*;

use super::{
    iden::{billing_reservations, billing_subscription_reservations},
    schema,
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(billing_reservations::Entity)
                    .add_column(
                        ColumnDef::new(billing_reservations::Column::FundingSource)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(
                                Expr::col(billing_reservations::Column::FundingSource)
                                    .is_in([1_i16, 2_i16]),
                            ),
                    )
                    .to_owned(),
            )
            .await?;
        schema::create_billing_subscription_reservations(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(billing_subscription_reservations::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(billing_reservations::Entity)
                    .drop_column(billing_reservations::Column::FundingSource)
                    .to_owned(),
            )
            .await
    }
}
