use sea_orm::{DbBackend, TransactionTrait};
use sea_orm_migration::prelude::*;

use super::schema;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_wallet_ledger(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_wallet_ledger(manager, false).await
    }
}

async fn rebuild_wallet_ledger(
    manager: &SchemaManager<'_>,
    allow_redemption: bool,
) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Sqlite {
        return schema::rebuild_wallet_ledger_for_redemption(manager, allow_redemption).await;
    }

    // SQLite 文件库可配置多连接，重建操作必须固定在一个事务连接内，避免 schema
    // 查询与建表、删表、改名落到不同连接后观察到不一致的表状态。
    let transaction = manager.get_connection().begin().await?;
    let result = {
        let transactional_manager = SchemaManager::new(&transaction);
        schema::rebuild_wallet_ledger_for_redemption(&transactional_manager, allow_redemption).await
    };
    match result {
        Ok(()) => transaction.commit().await,
        Err(error) => {
            // 回滚错误不能覆盖更有诊断价值的原始迁移错误。
            let _ = transaction.rollback().await;
            Err(error)
        }
    }
}
