use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub(super) struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("async_task_submission_claims"))
                    .add_column(
                        ColumnDef::new(Alias::new("merchant_snapshot"))
                            .json_binary()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("async_task_submission_claims"))
                    .drop_column(Alias::new("merchant_snapshot"))
                    .to_owned(),
            )
            .await
    }
}
