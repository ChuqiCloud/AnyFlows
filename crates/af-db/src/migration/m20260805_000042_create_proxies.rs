use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::proxies,
    schema::{self, CREDENTIAL_PROXY_FOREIGN_KEY},
};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_proxies(manager).await?;
        schema::create_credential_proxy_reference(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            for trigger in [
                "trg_credentials_proxy_insert",
                "trg_credentials_proxy_update",
                "trg_proxies_restrict_delete",
            ] {
                manager
                    .get_connection()
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                    .await?;
            }
        } else {
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name(CREDENTIAL_PROXY_FOREIGN_KEY)
                        .table(super::iden::credentials::Entity)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_table(Table::drop().table(proxies::Entity).to_owned())
            .await
    }
}
