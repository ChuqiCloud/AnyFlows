use super::schema::nullable_timestamp;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in ["merchant_pools", "merchant_profiles", "merchant_services"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(name))
                        .add_column(nullable_timestamp(manager, Alias::new("deleted_at")))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Keep retired supply disabled even when an older application reads it again.
        for name in ["merchant_services", "merchant_profiles", "merchant_pools"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(name))
                        .drop_column(Alias::new("deleted_at"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
