use sea_orm_migration::prelude::*;

use super::iden::site_settings;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_small_integer(manager, site_settings::Column::BalanceDisplayMode, 1, 1, 2).await?;
        add_text(
            manager,
            site_settings::Column::BalanceUnitName,
            64,
            "算力积分",
        )
        .await?;
        add_text(
            manager,
            site_settings::Column::BalanceUnitSymbol,
            24,
            "积分",
        )
        .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(site_settings::Entity)
                    .add_column(
                        ColumnDef::new(site_settings::Column::QuotaUnitsPerDisplayUnit)
                            .big_integer()
                            .not_null()
                            .default(10_000_i64)
                            .check(
                                Expr::col(site_settings::Column::QuotaUnitsPerDisplayUnit)
                                    .gt(0_i64),
                            ),
                    )
                    .to_owned(),
            )
            .await?;
        add_small_integer(
            manager,
            site_settings::Column::BalanceSymbolPosition,
            2,
            1,
            2,
        )
        .await?;
        add_small_integer(
            manager,
            site_settings::Column::BalanceFractionDigits,
            0,
            0,
            4,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 每条 ALTER TABLE 只删除一列，按新增顺序逆序执行以保持三方言一致。
        for column in [
            site_settings::Column::BalanceFractionDigits,
            site_settings::Column::BalanceSymbolPosition,
            site_settings::Column::QuotaUnitsPerDisplayUnit,
            site_settings::Column::BalanceUnitSymbol,
            site_settings::Column::BalanceUnitName,
            site_settings::Column::BalanceDisplayMode,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(site_settings::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

async fn add_small_integer(
    manager: &SchemaManager<'_>,
    column: site_settings::Column,
    default: i16,
    minimum: i16,
    maximum: i16,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(site_settings::Entity)
                .add_column(
                    ColumnDef::new(column)
                        .small_integer()
                        .not_null()
                        .default(default)
                        .check(Expr::col(column).between(minimum, maximum)),
                )
                .to_owned(),
        )
        .await
}

async fn add_text(
    manager: &SchemaManager<'_>,
    column: site_settings::Column,
    maximum_length: u32,
    default: &'static str,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(site_settings::Entity)
                .add_column(
                    ColumnDef::new(column)
                        .string_len(maximum_length)
                        .not_null()
                        .default(default)
                        .check(Expr::col(column).ne("")),
                )
                .to_owned(),
        )
        .await
}
