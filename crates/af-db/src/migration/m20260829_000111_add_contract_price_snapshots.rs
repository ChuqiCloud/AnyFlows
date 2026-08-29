use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{billing_organization_reservations, billing_reservations};

/// 为请求预留增加企业合同价的不可变版本与五类单价快照。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_columns_billing(manager).await?;
        add_columns_organization(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_columns(manager, billing_organization_reservations::Entity).await?;
        drop_columns(manager, billing_reservations::Entity).await
    }
}

async fn add_columns_billing(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_reservations::Column;
    manager
        .alter_table(
            Table::alter()
                .table(billing_reservations::Entity)
                .add_column(ColumnDef::new(Column::ContractPriceId).big_integer())
                .add_column(ColumnDef::new(Column::ContractPriceVersion).big_integer())
                .add_column(price(manager, Column::ContractInputPrice))
                .add_column(price(manager, Column::ContractOutputPrice))
                .add_column(price(manager, Column::ContractCacheReadPrice))
                .add_column(price(manager, Column::ContractCacheCreation5mPrice))
                .add_column(price(manager, Column::ContractCacheCreation1hPrice))
                .to_owned(),
        )
        .await
}

async fn add_columns_organization(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_organization_reservations::Column;
    manager
        .alter_table(
            Table::alter()
                .table(billing_organization_reservations::Entity)
                .add_column(ColumnDef::new(Column::ContractPriceId).big_integer())
                .add_column(ColumnDef::new(Column::ContractPriceVersion).big_integer())
                .add_column(price(manager, Column::ContractInputPrice))
                .add_column(price(manager, Column::ContractOutputPrice))
                .add_column(price(manager, Column::ContractCacheReadPrice))
                .add_column(price(manager, Column::ContractCacheCreation5mPrice))
                .add_column(price(manager, Column::ContractCacheCreation1hPrice))
                .to_owned(),
        )
        .await
}

async fn drop_columns<T>(manager: &SchemaManager<'_>, table: T) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
{
    use billing_reservations::Column;
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .drop_column(Column::ContractCacheCreation1hPrice)
                .drop_column(Column::ContractCacheCreation5mPrice)
                .drop_column(Column::ContractCacheReadPrice)
                .drop_column(Column::ContractOutputPrice)
                .drop_column(Column::ContractInputPrice)
                .drop_column(Column::ContractPriceVersion)
                .drop_column(Column::ContractPriceId)
                .to_owned(),
        )
        .await
}

fn price<T>(manager: &SchemaManager<'_>, column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column);
    if manager.get_database_backend() == DbBackend::Sqlite {
        definition.decimal();
    } else {
        definition.decimal_len(38, 28);
    }
    definition
}
