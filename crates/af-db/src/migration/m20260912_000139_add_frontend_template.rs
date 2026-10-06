use sea_orm_migration::prelude::*;

use super::iden::site_settings;

/// 为固定站点设置记录增加当前外部前端模板选择。
#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(site_settings::Entity)
                    .add_column(
                        ColumnDef::new(site_settings::Column::FrontendTemplateId).string_len(128),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(site_settings::Entity)
                    .drop_column(site_settings::Column::FrontendTemplateId)
                    .to_owned(),
            )
            .await
    }
}
