use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        match manager.get_database_backend() {
            DbBackend::MySql => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE plugin_storage MODIFY storage_key VARCHAR(128) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
                    )
                    .await?;
            }
            DbBackend::Postgres => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE plugin_storage ALTER COLUMN storage_key TYPE VARCHAR(128) COLLATE \"C\" USING storage_key::VARCHAR(128)",
                    )
                    .await?;
            }
            DbBackend::Sqlite => {}
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        match manager.get_database_backend() {
            DbBackend::MySql => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE plugin_storage MODIFY storage_key VARCHAR(128) NOT NULL",
                    )
                    .await?;
            }
            DbBackend::Postgres => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE plugin_storage ALTER COLUMN storage_key TYPE VARCHAR(128) COLLATE \"default\" USING storage_key::VARCHAR(128)",
                    )
                    .await?;
            }
            DbBackend::Sqlite => {}
        }
        Ok(())
    }
}
