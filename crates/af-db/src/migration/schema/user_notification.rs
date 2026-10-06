use sea_orm_migration::prelude::*;

use crate::migration::iden::{user_notification_events, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建面向用户的通知事实账本；正文、邮箱和 Provider 响应永不落盘。
pub(in crate::migration) async fn create_user_notification_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, user_notification_events::Entity);
    statement
        .col(auto_id(user_notification_events::Column::Id))
        .col(
            ColumnDef::new(user_notification_events::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(user_notification_events::Column::Kind)
                .small_integer()
                .not_null()
                .check(Expr::col(user_notification_events::Column::Kind).is_in([1_i16, 2])),
        )
        .col(
            ColumnDef::new(user_notification_events::Column::Channel)
                .small_integer()
                .not_null()
                .check(Expr::col(user_notification_events::Column::Channel).eq(1_i16)),
        )
        .col(
            ColumnDef::new(user_notification_events::Column::TemplateVersion)
                .string_len(32)
                .not_null(),
        )
        .col(timestamp(
            manager,
            user_notification_events::Column::OccurredAt,
        ))
        .col(
            ColumnDef::new(user_notification_events::Column::DeliveryState)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(
                    Expr::col(user_notification_events::Column::DeliveryState)
                        .is_in([1_i16, 2, 3, 4]),
                ),
        )
        .col(
            ColumnDef::new(user_notification_events::Column::DeliveryAttempts)
                .small_integer()
                .not_null()
                .default(0_i16)
                .check(Expr::col(user_notification_events::Column::DeliveryAttempts).gte(0_i16))
                .check(Expr::col(user_notification_events::Column::DeliveryAttempts).lte(5_i16)),
        )
        .col(
            ColumnDef::new(user_notification_events::Column::SourceKind)
                .small_integer()
                .not_null()
                .check(Expr::col(user_notification_events::Column::SourceKind).is_in([1_i16, 2])),
        )
        .col(
            ColumnDef::new(user_notification_events::Column::SourceKey)
                .string_len(128)
                .not_null(),
        )
        .col(ColumnDef::new(user_notification_events::Column::ObservedQuota).big_integer())
        .col(ColumnDef::new(user_notification_events::Column::ThresholdQuota).big_integer())
        .col(ColumnDef::new(user_notification_events::Column::SubscriptionId).string_len(32))
        .col(nullable_timestamp(
            manager,
            user_notification_events::Column::WindowEndsAt,
        ))
        .col(ColumnDef::new(user_notification_events::Column::QuotaAmount).big_integer())
        .col(ColumnDef::new(user_notification_events::Column::QuotaUsed).big_integer())
        .col(
            ColumnDef::new(user_notification_events::Column::ThresholdPercent)
                .small_integer()
                .check(
                    Expr::col(user_notification_events::Column::ThresholdPercent)
                        .is_null()
                        .or(
                            Expr::col(user_notification_events::Column::ThresholdPercent)
                                .between(1_i16, 99_i16),
                        ),
                ),
        )
        .col(timestamp(
            manager,
            user_notification_events::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_user_notification_events_user")
                .from(
                    user_notification_events::Entity,
                    user_notification_events::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    for index in [
        Index::create()
            .name("uq_user_notification_events_source")
            .table(user_notification_events::Entity)
            .col(user_notification_events::Column::SourceKind)
            .col(user_notification_events::Column::SourceKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_user_notification_events_user_cursor")
            .table(user_notification_events::Entity)
            .col(user_notification_events::Column::UserId)
            .col(user_notification_events::Column::OccurredAt)
            .col(user_notification_events::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_user_notification_events_delivery")
            .table(user_notification_events::Entity)
            .col(user_notification_events::Column::DeliveryState)
            .col(user_notification_events::Column::OccurredAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}
