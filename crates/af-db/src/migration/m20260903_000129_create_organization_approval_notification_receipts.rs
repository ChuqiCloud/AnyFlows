use sea_orm_migration::prelude::*;

use super::iden::{
    organization_approval_notification_receipts, organization_approval_notifications, users,
};
use super::schema::{table, timestamp};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = table(manager, organization_approval_notification_receipts::Entity)
            .col(
                ColumnDef::new(
                    organization_approval_notification_receipts::Column::RecipientUserId,
                )
                .big_integer()
                .not_null(),
            )
            .col(
                ColumnDef::new(organization_approval_notification_receipts::Column::NotificationId)
                    .big_integer()
                    .not_null(),
            )
            .col(timestamp(
                manager,
                organization_approval_notification_receipts::Column::ReadAt,
            ))
            .primary_key(
                Index::create()
                    .name("pk_org_approval_notification_receipts")
                    .col(organization_approval_notification_receipts::Column::RecipientUserId)
                    .col(organization_approval_notification_receipts::Column::NotificationId),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_approval_notification_receipts_recipient")
                    .from(
                        organization_approval_notification_receipts::Entity,
                        organization_approval_notification_receipts::Column::RecipientUserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_approval_notification_receipts_notification")
                    .from(
                        organization_approval_notification_receipts::Entity,
                        organization_approval_notification_receipts::Column::NotificationId,
                    )
                    .to(
                        organization_approval_notifications::Entity,
                        organization_approval_notifications::Column::Id,
                    )
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned();
        manager.create_table(table).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_org_approval_notification_receipts_recipient_read")
                    .table(organization_approval_notification_receipts::Entity)
                    .col(organization_approval_notification_receipts::Column::RecipientUserId)
                    .col(organization_approval_notification_receipts::Column::ReadAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_approval_notification_receipts::Entity)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
