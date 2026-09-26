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
                        // MySQL 不允许 TEXT 列使用默认值；有界字符串仍能容纳
                        // 16 KiB 的 UTF-8 配置，同时让三方言共享默认 JSON。
                        ColumnDef::new(site_settings::Column::NavigationJson)
                            .string_len(16_000)
                            .not_null()
                            .default(
                                r#"{"header_links":[],"footer_groups":[],"sidebar_links":[]}"#,
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
                    .table(site_settings::Entity)
                    .drop_column(site_settings::Column::NavigationJson)
                    .to_owned(),
            )
            .await
    }
}
