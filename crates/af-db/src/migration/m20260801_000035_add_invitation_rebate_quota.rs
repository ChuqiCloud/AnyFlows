use sea_orm_migration::prelude::*;

use super::iden::authentication_settings;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(authentication_settings::Entity)
                    .add_column(
                        ColumnDef::new(authentication_settings::Column::InvitationRebateQuota)
                            .big_integer()
                            .not_null()
                            .default(0_i64)
                            .check(
                                Expr::col(authentication_settings::Column::InvitationRebateQuota)
                                    .gte(0_i64),
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
                    .table(authentication_settings::Entity)
                    .drop_column(authentication_settings::Column::InvitationRebateQuota)
                    .to_owned(),
            )
            .await
    }
}
