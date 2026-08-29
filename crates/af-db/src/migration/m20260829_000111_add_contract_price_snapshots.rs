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
        drop_organization_columns(manager).await?;
        drop_billing_columns(manager).await
    }
}

async fn add_columns_billing(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_reservations::Column;
    add_column(
        manager,
        billing_reservations::Entity,
        ColumnDef::new(Column::ContractPriceId).big_integer(),
    )
    .await?;
    add_column(
        manager,
        billing_reservations::Entity,
        ColumnDef::new(Column::ContractPriceVersion).big_integer(),
    )
    .await?;
    add_column(
        manager,
        billing_reservations::Entity,
        price(manager, Column::ContractInputPrice),
    )
    .await?;
    add_column(
        manager,
        billing_reservations::Entity,
        price(manager, Column::ContractOutputPrice),
    )
    .await?;
    add_column(
        manager,
        billing_reservations::Entity,
        price(manager, Column::ContractCacheReadPrice),
    )
    .await?;
    add_column(
        manager,
        billing_reservations::Entity,
        price(manager, Column::ContractCacheCreation5mPrice),
    )
    .await?;
    add_column(
        manager,
        billing_reservations::Entity,
        price(manager, Column::ContractCacheCreation1hPrice),
    )
    .await
}

async fn add_columns_organization(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_organization_reservations::Column;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        ColumnDef::new(Column::ContractPriceId).big_integer(),
    )
    .await?;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        ColumnDef::new(Column::ContractPriceVersion).big_integer(),
    )
    .await?;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        price(manager, Column::ContractInputPrice),
    )
    .await?;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        price(manager, Column::ContractOutputPrice),
    )
    .await?;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        price(manager, Column::ContractCacheReadPrice),
    )
    .await?;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        price(manager, Column::ContractCacheCreation5mPrice),
    )
    .await?;
    add_column(
        manager,
        billing_organization_reservations::Entity,
        price(manager, Column::ContractCacheCreation1hPrice),
    )
    .await
}

async fn drop_billing_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_reservations::Column;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractCacheCreation1hPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractCacheCreation5mPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractCacheReadPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractOutputPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractInputPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractPriceVersion,
    )
    .await?;
    drop_column(
        manager,
        billing_reservations::Entity,
        Column::ContractPriceId,
    )
    .await
}

async fn drop_organization_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_organization_reservations::Column;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractCacheCreation1hPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractCacheCreation5mPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractCacheReadPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractOutputPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractInputPrice,
    )
    .await?;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractPriceVersion,
    )
    .await?;
    drop_column(
        manager,
        billing_organization_reservations::Entity,
        Column::ContractPriceId,
    )
    .await
}

/// SQLite 仅允许一次 ALTER TABLE 变更一个列，所有方言统一走单列操作。
async fn add_column<T>(
    manager: &SchemaManager<'_>,
    table: T,
    column: ColumnDef,
) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
{
    manager
        .alter_table(Table::alter().table(table).add_column(column).to_owned())
        .await
}

async fn drop_column<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
    C: IntoIden + 'static,
{
    manager
        .alter_table(Table::alter().table(table).drop_column(column).to_owned())
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
