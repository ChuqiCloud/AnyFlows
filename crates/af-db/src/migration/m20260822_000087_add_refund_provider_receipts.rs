use sea_orm_migration::prelude::*;

use crate::migration::iden::{refund_provider_events, refund_requests};

use super::schema;

/// 保存 Provider 退款标识与验签回执摘要，不保存密钥、签名或原始 payload。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .add_column(
                        ColumnDef::new(refund_requests::Column::ProviderRefundId).string_len(128),
                    )
                    .to_owned(),
            )
            .await?;

        let mut statement = schema::table(manager, refund_provider_events::Entity);
        statement
            .col(schema::auto_id(refund_provider_events::Column::Id))
            .col(
                ColumnDef::new(refund_provider_events::Column::EventKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::RequestKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::Provider)
                    .string_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::ProviderEventId)
                    .string_len(128)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::ProviderRefundId)
                    .string_len(128)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::EventType)
                    .small_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::AmountMinor)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::Currency)
                    .char_len(3)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::SignatureKeyFingerprint)
                    .char_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_provider_events::Column::PayloadSha256)
                    .char_len(64)
                    .not_null(),
            )
            .col(schema::timestamp(
                manager,
                refund_provider_events::Column::ReceivedAt,
            ))
            .col(
                ColumnDef::new(refund_provider_events::Column::ProcessedAt)
                    .timestamp_with_time_zone(),
            )
            .col(schema::timestamp(
                manager,
                refund_provider_events::Column::CreatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_provider_events_request")
                    .from(
                        refund_provider_events::Entity,
                        refund_provider_events::Column::RequestKey,
                    )
                    .to(refund_requests::Entity, refund_requests::Column::RequestKey)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .check(Expr::col(refund_provider_events::Column::EventType).between(1_i16, 2_i16))
            .check(Expr::col(refund_provider_events::Column::AmountMinor).gt(0_i64));
        manager.create_table(statement).await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_refund_provider_events_provider_event")
                    .table(refund_provider_events::Entity)
                    .col(refund_provider_events::Column::Provider)
                    .col(refund_provider_events::Column::ProviderEventId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_refund_provider_events_event_key")
                    .table(refund_provider_events::Entity)
                    .col(refund_provider_events::Column::EventKey)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(refund_provider_events::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .drop_column(refund_requests::Column::ProviderRefundId)
                    .to_owned(),
            )
            .await
    }
}
