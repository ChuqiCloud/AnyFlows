use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{group_model_ratios, groups, model_prices};

use super::{table, timestamp};

const PRICE_PRECISION: u32 = 38;
const PRICE_SCALE: u32 = 28;
const MODEL_PRICES_TABLE: &str = "model_prices";
const MODEL_PRICES_NEXT_TABLE: &str = "model_prices_next";
const MAX_BILLING_EXPRESSION_BYTES: usize = 8 * 1024;

/// 创建当前 Ratio 定价能力需要的模型价格目录。
pub(in crate::migration) async fn create_model_prices(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_model_prices_table(manager, MODEL_PRICES_TABLE, false).await
}

async fn create_model_prices_table(
    manager: &SchemaManager<'_>,
    table_name: &'static str,
    allow_expression: bool,
) -> Result<(), DbErr> {
    let mut statement = table(manager, Alias::new(table_name));
    statement
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
                    Expr::col(model_prices::Column::BillingMode).is_in(if allow_expression {
                        vec![1_i16, 2_i16, 3_i16]
                    } else {
                        vec![1_i16, 2_i16]
                    }),
                ),
        )
        .col(price(manager, model_prices::Column::InputPrice))
        .col(price(manager, model_prices::Column::OutputPrice))
        .col(price(manager, model_prices::Column::CacheReadPrice))
        .col(price(manager, model_prices::Column::CacheCreation5mPrice))
        .col(price(manager, model_prices::Column::CacheCreation1hPrice));
    if allow_expression {
        statement.col(ColumnDef::new(model_prices::Column::BillingExpression).text());
    }
    statement
        .col(
            ColumnDef::new(model_prices::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(model_prices::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, model_prices::Column::CreatedAt))
        .col(timestamp(manager, model_prices::Column::UpdatedAt));
    if allow_expression {
        statement.check(expression_price_shape_check(manager.get_database_backend()));
    } else {
        // 模式 2 表示显式免费，禁止同时留下会被误读的非零单价。
        statement.check(
            Expr::col(model_prices::Column::BillingMode)
                .eq(1_i16)
                .or(prices_are_zero()),
        );
    }
    manager.create_table(statement.to_owned()).await
}

/// 重建模型价格表，以原子切换 Expression 持久化模式及其跨列约束。
///
/// MySQL DDL 失败时可能留下恢复表；重试只会删除仍有权威源表时的中间副本，
/// 不会删除唯一剩余的数据副本。
pub(in crate::migration) async fn rebuild_model_prices_for_expression(
    manager: &SchemaManager<'_>,
    allow_expression: bool,
) -> Result<(), DbErr> {
    if !allow_expression {
        ensure_no_expression_prices(manager).await?;
    }
    let current_exists = manager.has_table(MODEL_PRICES_TABLE).await?;
    let next_exists = manager.has_table(MODEL_PRICES_NEXT_TABLE).await?;
    match (current_exists, next_exists) {
        (true, _) => {
            if next_exists {
                manager
                    .drop_table(
                        Table::drop()
                            .table(Alias::new(MODEL_PRICES_NEXT_TABLE))
                            .to_owned(),
                    )
                    .await?;
            }
            create_model_prices_table(manager, MODEL_PRICES_NEXT_TABLE, allow_expression).await?;
            copy_model_prices(manager).await?;
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(MODEL_PRICES_TABLE))
                        .to_owned(),
                )
                .await?;
            rename_model_prices(manager).await
        }
        (false, true) => rename_model_prices(manager).await,
        (false, false) => Err(DbErr::Custom("模型价格表重建缺少源表和恢复表".to_owned())),
    }
}

fn prices_are_zero() -> SimpleExpr {
    Expr::col(model_prices::Column::InputPrice)
        .eq(0_i64)
        .and(Expr::col(model_prices::Column::OutputPrice).eq(0_i64))
        .and(Expr::col(model_prices::Column::CacheReadPrice).eq(0_i64))
        .and(Expr::col(model_prices::Column::CacheCreation5mPrice).eq(0_i64))
        .and(Expr::col(model_prices::Column::CacheCreation1hPrice).eq(0_i64))
}

fn expression_price_shape_check(backend: DbBackend) -> SimpleExpr {
    let expression_size = match backend {
        DbBackend::Postgres => Expr::cust(format!(
            "octet_length(\"billing_expression\") BETWEEN 1 AND {MAX_BILLING_EXPRESSION_BYTES}"
        )),
        DbBackend::MySql => Expr::cust(format!(
            "OCTET_LENGTH(`billing_expression`) BETWEEN 1 AND {MAX_BILLING_EXPRESSION_BYTES}"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(CAST(\"billing_expression\" AS BLOB)) BETWEEN 1 AND {MAX_BILLING_EXPRESSION_BYTES}"
        )),
    };
    Expr::col(model_prices::Column::BillingMode)
        .eq(1_i16)
        .and(Expr::col(model_prices::Column::BillingExpression).is_null())
        .or(Expr::col(model_prices::Column::BillingMode)
            .eq(2_i16)
            .and(prices_are_zero())
            .and(Expr::col(model_prices::Column::BillingExpression).is_null()))
        .or(Expr::col(model_prices::Column::BillingMode)
            .eq(3_i16)
            .and(prices_are_zero())
            .and(Expr::col(model_prices::Column::BillingExpression).is_not_null())
            .and(expression_size))
}

async fn ensure_no_expression_prices(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let table_name = if manager.has_table(MODEL_PRICES_TABLE).await? {
        MODEL_PRICES_TABLE
    } else {
        MODEL_PRICES_NEXT_TABLE
    };
    if !manager.has_table(table_name).await? {
        return Ok(());
    }
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("SELECT 1 FROM `{table_name}` WHERE `billing_mode` = 3 LIMIT 1")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("SELECT 1 FROM \"{table_name}\" WHERE \"billing_mode\" = 3 LIMIT 1")
        }
    };
    if manager
        .get_connection()
        .query_one(Statement::from_string(manager.get_database_backend(), sql))
        .await?
        .is_some()
    {
        return Err(DbErr::Custom(
            "存在表达式模型价格，无法回退计费表达式字段".to_owned(),
        ));
    }
    Ok(())
}

async fn copy_model_prices(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let columns = "model, billing_mode, input_price, output_price, cache_read_price, \
cache_creation_5m_price, cache_creation_1h_price, version, created_at, updated_at";
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => format!(
            "INSERT INTO `{MODEL_PRICES_NEXT_TABLE}` (`{}`) SELECT `{}` FROM `{MODEL_PRICES_TABLE}`",
            columns.replace(", ", "`, `"),
            columns.replace(", ", "`, `")
        ),
        DbBackend::Postgres | DbBackend::Sqlite => format!(
            "INSERT INTO \"{MODEL_PRICES_NEXT_TABLE}\" (\"{}\") SELECT \"{}\" FROM \"{MODEL_PRICES_TABLE}\"",
            columns.replace(", ", "\", \""),
            columns.replace(", ", "\", \"")
        ),
    };
    manager.get_connection().execute_unprepared(&sql).await?;
    Ok(())
}

async fn rename_model_prices(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("RENAME TABLE `{MODEL_PRICES_NEXT_TABLE}` TO `{MODEL_PRICES_TABLE}`")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("ALTER TABLE \"{MODEL_PRICES_NEXT_TABLE}\" RENAME TO \"{MODEL_PRICES_TABLE}\"")
        }
    };
    manager.get_connection().execute_unprepared(&sql).await?;
    Ok(())
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
