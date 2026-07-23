use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{group_model_ratios, groups, model_prices};

use super::{table, timestamp};

const PRICE_PRECISION: u32 = 38;
const PRICE_SCALE: u32 = 28;

/// 创建当前 Ratio 定价能力需要的模型价格目录。
pub(in crate::migration) async fn create_model_prices(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let free_prices_are_zero =
        Expr::col(model_prices::Column::BillingMode)
            .eq(1_i16)
            .or(Expr::col(model_prices::Column::InputPrice)
                .eq(0_i64)
                .and(Expr::col(model_prices::Column::OutputPrice).eq(0_i64))
                .and(Expr::col(model_prices::Column::CacheReadPrice).eq(0_i64))
                .and(Expr::col(model_prices::Column::CacheCreation5mPrice).eq(0_i64))
                .and(Expr::col(model_prices::Column::CacheCreation1hPrice).eq(0_i64)));

    manager
        .create_table(
            table(manager, model_prices::Entity)
                .col(
                    ColumnDef::new(model_prices::Column::Model)
                        .string_len(256)
                        .not_null()
                        .primary_key()
                        .check(Expr::col(model_prices::Column::Model).ne("")),
                )
                .col(
                    ColumnDef::new(model_prices::Column::BillingMode)
                        .small_integer()
                        .not_null()
                        .check(
                            Expr::col(model_prices::Column::BillingMode).is_in([1_i16, 2_i16]),
                        ),
                )
                .col(price(manager, model_prices::Column::InputPrice))
                .col(price(manager, model_prices::Column::OutputPrice))
                .col(price(manager, model_prices::Column::CacheReadPrice))
                .col(price(
                    manager,
                    model_prices::Column::CacheCreation5mPrice,
                ))
                .col(price(
                    manager,
                    model_prices::Column::CacheCreation1hPrice,
                ))
                .col(
                    ColumnDef::new(model_prices::Column::Version)
                        .big_integer()
                        .not_null()
                        .default(1_i64)
                        .check(Expr::col(model_prices::Column::Version).gte(1_i64)),
                )
                .col(timestamp(manager, model_prices::Column::CreatedAt))
                .col(timestamp(manager, model_prices::Column::UpdatedAt))
                // 模式 2 表示显式免费，禁止同时留下会被误读的非零单价。
                .check(free_prices_are_zero)
                .to_owned(),
        )
        .await
}

/// 创建来源分组到实际计费分组的附加倍率表。
pub(in crate::migration) async fn create_group_model_ratios(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let statement = table(manager, group_model_ratios::Entity)
        .col(
            ColumnDef::new(group_model_ratios::Column::SourceGroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(group_model_ratios::Column::TargetGroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(group_model_ratios::Column::RatioMicros)
                .big_integer()
                .not_null()
                .check(Expr::col(group_model_ratios::Column::RatioMicros).gte(0_i64)),
        )
        .col(timestamp(manager, group_model_ratios::Column::CreatedAt))
        .col(timestamp(manager, group_model_ratios::Column::UpdatedAt))
        .primary_key(
            Index::create()
                .name("pk_group_model_ratios")
                .col(group_model_ratios::Column::SourceGroupId)
                .col(group_model_ratios::Column::TargetGroupId),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_group_model_ratios_source")
                .from(
                    group_model_ratios::Entity,
                    group_model_ratios::Column::SourceGroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_group_model_ratios_target")
                .from(
                    group_model_ratios::Entity,
                    group_model_ratios::Column::TargetGroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned();
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name("idx_group_model_ratios_target")
                .table(group_model_ratios::Entity)
                .col(group_model_ratios::Column::TargetGroupId)
                .to_owned(),
        )
        .await
}

fn price<T>(manager: &SchemaManager<'_>, column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    // SQLite DDL 生成器不接受超过 16 位的声明精度，且底层只能按 REAL 读取 Decimal；
    // PostgreSQL/MySQL 则必须固定 38/28，避免生产价格在持久化时被截断。
    if manager.get_database_backend() == DbBackend::Sqlite {
        definition.decimal();
    } else {
        definition.decimal_len(PRICE_PRECISION, PRICE_SCALE);
    }
    definition
        .not_null()
        .default(0_i64)
        .check(Expr::col(column).gte(0_i64));
    definition
}
