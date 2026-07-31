use sea_orm_migration::prelude::*;

use super::iden::users;

#[derive(DeriveIden)]
enum UserNotificationColumn {
    EmailProductUpdates,
    EmailUsageAlerts,
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
                        ColumnDef::new(UserNotificationColumn::EmailProductUpdates)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        // SQLite 不支持一次 ALTER TABLE 添加多个字段，因此必须拆成两个原子步骤。
        manager
            .alter_table(
                Table::alter()
                    .table(users::Entity)
                    .add_column(
                        ColumnDef::new(UserNotificationColumn::EmailUsageAlerts)
                            .boolean()
                            .not_null()
                            .default(true),
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
                    .drop_column(UserNotificationColumn::EmailProductUpdates)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(users::Entity)
                    .drop_column(UserNotificationColumn::EmailUsageAlerts)
                    .to_owned(),
            )
            .await
    }
}
