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
    allow_invite_rebate: bool,
) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Sqlite {
        return schema::rebuild_wallet_ledger_for_invite_rebate(manager, allow_invite_rebate).await;
    }

    // SQLite 文件库可配置多连接，重建钱包表必须绑定在同一事务连接内。
    let transaction = manager.get_connection().begin().await?;
    let result = {
        let transactional_manager = SchemaManager::new(&transaction);
        schema::rebuild_wallet_ledger_for_invite_rebate(&transactional_manager, allow_invite_rebate)
            .await
    };
    match result {
        Ok(()) => transaction.commit().await,
        Err(error) => {
            // 回滚错误不能覆盖原始迁移错误，保留更有诊断价值的失败原因。
            let _ = transaction.rollback().await;
            Err(error)
        }
    }
}
