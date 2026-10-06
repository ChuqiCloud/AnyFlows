use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    balance_alert_settings, subscription_balance_alert_events, user_subscriptions, users,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 扩展全局设置并创建订阅窗口剩余额度预警事件表。
pub(in crate::migration) async fn create_subscription_balance_alert_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(balance_alert_settings::Entity)
                .add_column(
                    ColumnDef::new(balance_alert_settings::Column::SubscriptionAlertEnabled)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await?;
    // SQLite 不支持一次 ALTER TABLE 添加多个字段，第二列必须独立执行。
    manager
        .alter_table(
            Table::alter()
                .table(balance_alert_settings::Entity)
                .add_column(
                    ColumnDef::new(balance_alert_settings::Column::SubscriptionRemainingPercent)
                        .small_integer()
                        .not_null()
                        .default(20_i16)
                        .check(
                            Expr::col(balance_alert_settings::Column::SubscriptionRemainingPercent)
                                .gte(1_i16),
                        )
                        .check(
                            Expr::col(balance_alert_settings::Column::SubscriptionRemainingPercent)
                                .lte(99_i16),
                        ),
                )
                .to_owned(),
        )
        .await?;
    create_events(manager).await?;
    create_indexes(manager).await
}

async fn create_events(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, subscription_balance_alert_events::Entity);
    statement
        .col(auto_id(subscription_balance_alert_events::Column::Id))
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::UserSubscriptionId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            subscription_balance_alert_events::Column::WindowStartedAt,
        ))
        .col(timestamp(
            manager,
            subscription_balance_alert_events::Column::WindowEndsAt,
        ))
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::ThresholdPercent)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(subscription_balance_alert_events::Column::ThresholdPercent)
                        .gte(1_i16),
                )
                .check(
                    Expr::col(subscription_balance_alert_events::Column::ThresholdPercent)
                        .lte(99_i16),
                ),
        )
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::QuotaAmount)
                .big_integer()
                .not_null()
                .check(Expr::col(subscription_balance_alert_events::Column::QuotaAmount).gt(0_i64)),
        )
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::ObservedQuotaUsed)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(subscription_balance_alert_events::Column::ObservedQuotaUsed)
                        .gte(0_i64),
                ),
        )
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(
                    Expr::col(subscription_balance_alert_events::Column::Status)
                        .is_in([1_i16, 2, 3, 4, 5]),
                ),
        )
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::AttemptCount)
                .small_integer()
                .not_null()
                .default(0_i16)
                .check(
                    Expr::col(subscription_balance_alert_events::Column::AttemptCount).gte(0_i16),
                )
                .check(
                    Expr::col(subscription_balance_alert_events::Column::AttemptCount).lte(5_i16),
                ),
        )
        .col(timestamp(
            manager,
            subscription_balance_alert_events::Column::NextAttemptAt,
        ))
        .col(nullable_timestamp(
            manager,
            subscription_balance_alert_events::Column::LeaseExpiresAt,
        ))
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::LastErrorKind)
                .small_integer()
                .check(
                    Expr::col(subscription_balance_alert_events::Column::LastErrorKind)
                        .is_in([1_i16, 2, 3]),
                ),
        )
        .col(
            ColumnDef::new(subscription_balance_alert_events::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(subscription_balance_alert_events::Column::Version).gte(1_i64)),
        )
        .col(nullable_timestamp(
            manager,
            subscription_balance_alert_events::Column::SentAt,
        ))
        .col(timestamp(
            manager,
            subscription_balance_alert_events::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            subscription_balance_alert_events::Column::UpdatedAt,
        ))
        .check(event_state_shape())
        .check(
            Expr::col(subscription_balance_alert_events::Column::ObservedQuotaUsed).lte(Expr::col(
                subscription_balance_alert_events::Column::QuotaAmount,
            )),
        )
        .check(
            Expr::col(subscription_balance_alert_events::Column::WindowEndsAt).gt(Expr::col(
                subscription_balance_alert_events::Column::WindowStartedAt,
            )),
        )
        .check(
            Expr::col(subscription_balance_alert_events::Column::SentAt)
                .is_null()
                .or(
                    Expr::col(subscription_balance_alert_events::Column::SentAt).gte(Expr::col(
                        subscription_balance_alert_events::Column::CreatedAt,
                    )),
                ),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_subscription_balance_alert_subscription")
                .from(
                    subscription_balance_alert_events::Entity,
                    subscription_balance_alert_events::Column::UserSubscriptionId,
                )
                .to(user_subscriptions::Entity, user_subscriptions::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_subscription_balance_alert_user")
                .from(
                    subscription_balance_alert_events::Entity,
                    subscription_balance_alert_events::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

fn event_state_shape() -> SimpleExpr {
    let status = Expr::col(subscription_balance_alert_events::Column::Status);
    let attempts = Expr::col(subscription_balance_alert_events::Column::AttemptCount);
    let lease = Expr::col(subscription_balance_alert_events::Column::LeaseExpiresAt);
    let sent = Expr::col(subscription_balance_alert_events::Column::SentAt);
    let error = Expr::col(subscription_balance_alert_events::Column::LastErrorKind);
    status
        .clone()
        .eq(1_i16)
        .and(attempts.clone().gte(0_i16))
        .and(attempts.clone().lte(4_i16))
        .and(lease.clone().is_null())
        .and(sent.clone().is_null())
        .and(
            attempts
                .clone()
                .eq(0_i16)
                .and(error.clone().is_null())
                .or(attempts.clone().gte(1_i16).and(error.is_not_null())),
        )
        .or(status
            .clone()
            .eq(2_i16)
            .and(attempts.clone().gte(1_i16))
            .and(lease.clone().is_not_null())
            .and(sent.clone().is_null()))
        .or(status
            .clone()
            .eq(3_i16)
            .and(attempts.clone().gte(1_i16))
            .and(lease.clone().is_null())
            .and(sent.clone().is_not_null()))
        .or(status
            .clone()
            .eq(4_i16)
            .and(attempts.gte(1_i16))
            .and(lease.clone().is_null())
            .and(sent.clone().is_null()))
        .or(status.eq(5_i16).and(lease.is_null()).and(sent.is_null()))
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_subscription_balance_alert_window")
            .table(subscription_balance_alert_events::Entity)
            .col(subscription_balance_alert_events::Column::UserSubscriptionId)
            .col(subscription_balance_alert_events::Column::WindowStartedAt)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_subscription_balance_alert_due")
            .table(subscription_balance_alert_events::Entity)
            .col(subscription_balance_alert_events::Column::Status)
            .col(subscription_balance_alert_events::Column::NextAttemptAt)
            .col(subscription_balance_alert_events::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_subscription_balance_alert_user_created")
            .table(subscription_balance_alert_events::Entity)
            .col(subscription_balance_alert_events::Column::UserId)
            .col(subscription_balance_alert_events::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}
