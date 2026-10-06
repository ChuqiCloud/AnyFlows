use sea_orm_migration::prelude::*;

use super::iden::{user_notification_events, user_notification_receipts, users};
use super::schema::{table, timestamp};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, user_notification_receipts::Entity)
                    .col(
                        ColumnDef::new(user_notification_receipts::Column::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(user_notification_receipts::Column::NotificationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(timestamp(
                        manager,
                        user_notification_receipts::Column::ReadAt,
                    ))
                    .primary_key(
                        Index::create()
                            .name("pk_user_notification_receipts")
                            .col(user_notification_receipts::Column::UserId)
                            .col(user_notification_receipts::Column::NotificationId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_notification_receipts_user")
                            .from(
                                user_notification_receipts::Entity,
                                user_notification_receipts::Column::UserId,
                            )
                            .to(users::Entity, users::Column::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_notification_receipts_notification")
                            .from(
                                user_notification_receipts::Entity,
                                user_notification_receipts::Column::NotificationId,
                            )
                            .to(
                                user_notification_events::Entity,
                                user_notification_events::Column::Id,
                            )
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_user_notification_receipts_user_read")
                    .table(user_notification_receipts::Entity)
                    .col(user_notification_receipts::Column::UserId)
                    .col(user_notification_receipts::Column::ReadAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(user_notification_receipts::Entity)
                    .to_owned(),
            )
            .await
    }
}
