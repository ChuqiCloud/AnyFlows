use sea_orm_migration::prelude::*;

use super::{iden::async_task_submission_claims, schema::create_async_task_submission_claims};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_async_task_submission_claims(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(async_task_submission_claims::Entity)
                    .to_owned(),
            )
            .await
    }
}
