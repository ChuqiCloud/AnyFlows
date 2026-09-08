use sea_orm::{DbBackend, TransactionTrait};
use sea_orm_migration::prelude::*;

use super::schema;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_model_prices(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_model_prices(manager, false).await
    }
}

async fn rebuild_model_prices(
    manager: &SchemaManager<'_>,
    allow_expression: bool,
) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Sqlite {
        return schema::rebuild_model_prices_for_expression(manager, allow_expression).await;
    }

    // SQLite 文件库可能配置多个连接，表存在检查和重建必须固定在同一事务连接内。
    let transaction = manager.get_connection().begin().await?;
    let result = {
        let transactional_manager = SchemaManager::new(&transaction);
        schema::rebuild_model_prices_for_expression(&transactional_manager, allow_expression).await
    };
    match result {
        Ok(()) => transaction.commit().await,
        Err(error) => {
            // 回滚失败不能覆盖更有诊断价值的原始迁移错误。
            let _ = transaction.rollback().await;
            Err(error)
        }
    }
}
