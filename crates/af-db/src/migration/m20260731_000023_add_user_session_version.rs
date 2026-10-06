use sea_orm_migration::prelude::*;

use super::iden::users;

#[derive(DeriveIden)]
enum UserSessionColumn {
    SessionVersion,
}

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(users::Entity)
                    .add_column(
                        ColumnDef::new(UserSessionColumn::SessionVersion)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(UserSessionColumn::SessionVersion).gte(1_i64)),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(users::Entity)
                    .drop_column(UserSessionColumn::SessionVersion)
                    .to_owned(),
            )
            .await
    }
}
