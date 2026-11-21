use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use sea_orm_migration::prelude::*;

use super::schema;

const MODEL_PRICES_TABLE: &str = "model_prices";
const MODEL_PRICES_NEXT_TABLE: &str = "model_prices_per_call_next";
const MODEL_PRICES_PREVIOUS_TABLE: &str = "model_prices_per_call_previous";
const MODEL_PRICE_COLUMNS: &str = "model, billing_mode, input_price, output_price, cache_read_price, cache_creation_5m_price, cache_creation_1h_price, billing_expression, peak_windows, version, created_at, updated_at";

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild(manager, false).await
    }
}

async fn rebuild(manager: &SchemaManager<'_>, allow_per_call: bool) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        let transaction = manager.get_connection().begin().await?;
        let result = {
            let transactional_manager = SchemaManager::new(&transaction);
            rebuild_tables(&transactional_manager, allow_per_call).await
        };
        return match result {
            Ok(()) => transaction.commit().await,
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        };
    }
    rebuild_tables(manager, allow_per_call).await
}

async fn rebuild_tables(manager: &SchemaManager<'_>, allow_per_call: bool) -> Result<(), DbErr> {
    if manager
        .has_column(MODEL_PRICES_TABLE, "request_price")
        .await?
        == allow_per_call
    {
        // MySQL 的换表已经提交时，重试只清理备份。
        return drop_table_if_exists(manager, MODEL_PRICES_PREVIOUS_TABLE).await;
    }
    if !allow_per_call {
        ensure_no_per_call_prices(manager).await?;
    }
    drop_table_if_exists(manager, MODEL_PRICES_NEXT_TABLE).await?;
    schema::create_model_prices_with_peak_windows(manager, MODEL_PRICES_NEXT_TABLE, allow_per_call)
        .await?;
    copy_table(manager).await?;
    if manager.get_database_backend() == DbBackend::MySql {
        manager
            .get_connection()
            .execute_unprepared(&format!(
                "RENAME TABLE `{MODEL_PRICES_TABLE}` TO `{MODEL_PRICES_PREVIOUS_TABLE}`, `{MODEL_PRICES_NEXT_TABLE}` TO `{MODEL_PRICES_TABLE}`"
            ))
            .await?;
        return drop_table_if_exists(manager, MODEL_PRICES_PREVIOUS_TABLE).await;
    }
    drop_table_if_exists(manager, MODEL_PRICES_TABLE).await?;
    manager
        .get_connection()
        .execute_unprepared(&format!(
            "ALTER TABLE \"{MODEL_PRICES_NEXT_TABLE}\" RENAME TO \"{MODEL_PRICES_TABLE}\""
        ))
        .await?;
    Ok(())
}

async fn drop_table_if_exists(
    manager: &SchemaManager<'_>,
    name: &'static str,
) -> Result<(), DbErr> {
    manager
        .drop_table(Table::drop().table(Alias::new(name)).if_exists().to_owned())
        .await
}

async fn ensure_no_per_call_prices(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager
        .get_connection()
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            "SELECT 1 FROM model_prices WHERE billing_mode = 4 OR request_price <> 0 LIMIT 1",
        ))
        .await?
        .is_some()
    {
        return Err(DbErr::Custom(
            "存在按次模型价格，无法回退按次定价字段".to_owned(),
        ));
    }
    Ok(())
}

async fn copy_table(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let sql = format!(
        "INSERT INTO {MODEL_PRICES_NEXT_TABLE} ({MODEL_PRICE_COLUMNS}) SELECT {MODEL_PRICE_COLUMNS} FROM {MODEL_PRICES_TABLE}"
    );
    manager.get_connection().execute_unprepared(&sql).await?;
    Ok(())
}
