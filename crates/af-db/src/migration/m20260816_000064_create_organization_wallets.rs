use sea_orm::{ConnectionTrait, DbBackend, TransactionTrait};
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
        manager
            .get_connection()
            .execute_unprepared("PRAGMA foreign_keys = OFF")
            .await?;
        let transaction = manager.get_connection().begin().await?;
        let result = {
            let transactional_manager = SchemaManager::new(&transaction);
            schema::create_organization_wallet_storage(&transactional_manager).await
        };
        match result {
            Ok(()) => {
                transaction.commit().await?;
                manager
                    .get_connection()
                    .execute_unprepared("PRAGMA foreign_keys = ON")
                    .await
                    .map(|_| ())
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                let _ = manager
                    .get_connection()
                    .execute_unprepared("PRAGMA foreign_keys = ON")
                    .await;
                Err(error)
            }
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != DbBackend::Sqlite {
            return schema::drop_organization_wallet_storage(manager).await;
        }
        manager
            .get_connection()
            .execute_unprepared("PRAGMA foreign_keys = OFF")
            .await?;
        let transaction = manager.get_connection().begin().await?;
        let result = {
            let transactional_manager = SchemaManager::new(&transaction);
            schema::drop_organization_wallet_storage(&transactional_manager).await
        };
        match result {
            Ok(()) => {
                transaction.commit().await?;
                manager
                    .get_connection()
                    .execute_unprepared("PRAGMA foreign_keys = ON")
                    .await
                    .map(|_| ())
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                let _ = manager
                    .get_connection()
                    .execute_unprepared("PRAGMA foreign_keys = ON")
                    .await;
                Err(error)
            }
        }
    }
}
