use sea_orm_migration::prelude::*;

use super::iden::site_settings;

/// 为固定站点设置记录增加公开导航配置，旧站点默认不显示自定义链接。
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
                        // MySQL 行大小受限，导航配置使用 TEXT 存储；不能声明
                        // 默认值，因此在新增列后显式回填固定站点行。
                        ColumnDef::new(site_settings::Column::NavigationJson).text(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .exec_stmt(
                Query::update()
                    .table(site_settings::Entity)
                    .values([(
                        site_settings::Column::NavigationJson,
                        Expr::value(r#"{"header_links":[],"footer_groups":[],"sidebar_links":[]}"#),
                    )])
                    .and_where(Expr::col(site_settings::Column::Id).eq(1_i16))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(site_settings::Entity)
                    .drop_column(site_settings::Column::NavigationJson)
                    .to_owned(),
            )
            .await
    }
}
