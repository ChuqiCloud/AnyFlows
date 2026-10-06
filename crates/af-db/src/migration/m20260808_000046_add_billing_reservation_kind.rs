use sea_orm_migration::prelude::*;

use super::iden::billing_reservations;

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
                        ColumnDef::new(billing_reservations::Column::ReservationKind)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(
                                Expr::col(billing_reservations::Column::ReservationKind)
                                    .is_in([1_i16, 2_i16]),
                            ),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(billing_reservations::Entity)
                    .drop_column(billing_reservations::Column::ReservationKind)
                    .to_owned(),
            )
            .await
    }
}
