use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 历史 claim 的来源不可重建，保留 NULL，由读取层沿用原 routing group。
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("async_task_submission_claims"))
                    .add_column(
                        ColumnDef::new(Alias::new("billing_source_group_id"))
                            .big_integer()
                            .check(
                                Expr::col(Alias::new("billing_source_group_id"))
                                    .is_null()
                                    .or(Expr::col(Alias::new("billing_source_group_id")).gt(0_i64)),
                            ),
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
                    .drop_column(Alias::new("billing_source_group_id"))
                    .to_owned(),
            )
            .await
    }
}
