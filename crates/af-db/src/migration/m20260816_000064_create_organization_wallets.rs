use sea_orm::{DbBackend, TransactionTrait};
use sea_orm_migration::prelude::*;

use super::schema;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != DbBackend::Sqlite {
            return schema::create_organization_wallet_storage(manager).await;
        }
        let transaction = manager.get_connection().begin().await?;
        let result = {
            let transactional_manager = SchemaManager::new(&transaction);
            schema::create_organization_wallet_storage(&transactional_manager).await
        };
        match result {
            Ok(()) => transaction.commit().await,
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != DbBackend::Sqlite {
            return schema::drop_organization_wallet_storage(manager).await;
        }
        let transaction = manager.get_connection().begin().await?;
        let result = {
            let transactional_manager = SchemaManager::new(&transaction);
            schema::drop_organization_wallet_storage(&transactional_manager).await
        };
        match result {
            Ok(()) => transaction.commit().await,
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }
}
