use sea_orm_migration::prelude::*;

use crate::migration::iden::{balance_alert_events, balance_alert_settings, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建余额预警设置、个人阈值和持久化投递事件。
pub(in crate::migration) async fn create_balance_alert_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(users::Entity)
                .add_column(
                    ColumnDef::new(users::Column::BalanceAlertThreshold)
                        .big_integer()
                        .check(Expr::col(users::Column::BalanceAlertThreshold).gt(0_i64)),
                )
                .to_owned(),
        )
        .await?;
    create_settings(manager).await?;
    create_events(manager).await?;
    create_indexes(manager).await
}

async fn create_settings(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, balance_alert_settings::Entity);
    statement
        .col(
            ColumnDef::new(balance_alert_settings::Column::Id)
                .small_integer()
                .not_null()
                .primary_key()
                .check(Expr::col(balance_alert_settings::Column::Id).eq(1_i16)),
        )
        .col(
            ColumnDef::new(balance_alert_settings::Column::Enabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(balance_alert_settings::Column::DefaultThresholdQuota)
                .big_integer()
                .not_null()
                .default(1_000_i64)
                .check(Expr::col(balance_alert_settings::Column::DefaultThresholdQuota).gt(0_i64)),
        )
        .col(
            ColumnDef::new(balance_alert_settings::Column::ReminderIntervalSeconds)
                .big_integer()
                .not_null()
                .default(86_400_i64)
                .check(
                    Expr::col(balance_alert_settings::Column::ReminderIntervalSeconds)
                        .gte(3_600_i64),
                )
                .check(
                    Expr::col(balance_alert_settings::Column::ReminderIntervalSeconds)
                        .lte(604_800_i64),
                ),
        )
        .col(
            ColumnDef::new(balance_alert_settings::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(balance_alert_settings::Column::Version).gte(1_i64)),
        )
        .col(timestamp(
            manager,
            balance_alert_settings::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            balance_alert_settings::Column::UpdatedAt,
        ));
    manager.create_table(statement).await?;

    // 固定行确保配置写入始终锁定同一记录，并保留关闭状态下的可用草稿。
    manager
        .exec_stmt(
            Query::insert()
                .into_table(balance_alert_settings::Entity)
                .columns([
                    balance_alert_settings::Column::Id,
                    balance_alert_settings::Column::Enabled,
                    balance_alert_settings::Column::DefaultThresholdQuota,
                    balance_alert_settings::Column::ReminderIntervalSeconds,
                    balance_alert_settings::Column::Version,
                ])
                .values_panic([
                    1_i16.into(),
                    false.into(),
                    1_000_i64.into(),
                    86_400_i64.into(),
                    1_i64.into(),
                ])
                .to_owned(),
        )
        .await
}

async fn create_events(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, balance_alert_events::Entity);
    statement
        .col(auto_id(balance_alert_events::Column::Id))
        .col(
            ColumnDef::new(balance_alert_events::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(balance_alert_events::Column::WindowStartedAtEpoch)
                .big_integer()
                .not_null()
                .check(Expr::col(balance_alert_events::Column::WindowStartedAtEpoch).gte(0_i64)),
        )
        .col(
            ColumnDef::new(balance_alert_events::Column::ThresholdQuota)
                .big_integer()
                .not_null()
                .check(Expr::col(balance_alert_events::Column::ThresholdQuota).gt(0_i64)),
        )
        .col(
            ColumnDef::new(balance_alert_events::Column::ObservedQuota)
                .big_integer()
                .not_null()
                .check(Expr::col(balance_alert_events::Column::ObservedQuota).gte(0_i64)),
        )
        .col(
            ColumnDef::new(balance_alert_events::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(balance_alert_events::Column::Status).is_in([1_i16, 2, 3, 4, 5])),
        )
        .col(
            ColumnDef::new(balance_alert_events::Column::AttemptCount)
                .small_integer()
                .not_null()
                .default(0_i16)
                .check(Expr::col(balance_alert_events::Column::AttemptCount).gte(0_i16))
                .check(Expr::col(balance_alert_events::Column::AttemptCount).lte(5_i16)),
        )
        .col(timestamp(
            manager,
            balance_alert_events::Column::NextAttemptAt,
        ))
        .col(nullable_timestamp(
            manager,
            balance_alert_events::Column::LeaseExpiresAt,
        ))
        .col(
            ColumnDef::new(balance_alert_events::Column::LastErrorKind)
                .small_integer()
                .check(Expr::col(balance_alert_events::Column::LastErrorKind).is_in([1_i16, 2, 3])),
        )
        .col(
            ColumnDef::new(balance_alert_events::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(balance_alert_events::Column::Version).gte(1_i64)),
        )
        .col(nullable_timestamp(
            manager,
            balance_alert_events::Column::SentAt,
        ))
        .col(timestamp(manager, balance_alert_events::Column::CreatedAt))
        .col(timestamp(manager, balance_alert_events::Column::UpdatedAt))
        .check(event_state_shape())
        .check(
            Expr::col(balance_alert_events::Column::SentAt)
                .is_null()
                .or(Expr::col(balance_alert_events::Column::SentAt)
                    .gte(Expr::col(balance_alert_events::Column::CreatedAt))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_balance_alert_events_user")
                .from(
                    balance_alert_events::Entity,
                    balance_alert_events::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

fn event_state_shape() -> SimpleExpr {
    let status = Expr::col(balance_alert_events::Column::Status);
    let attempts = Expr::col(balance_alert_events::Column::AttemptCount);
    let lease = Expr::col(balance_alert_events::Column::LeaseExpiresAt);
    let sent = Expr::col(balance_alert_events::Column::SentAt);
    let error = Expr::col(balance_alert_events::Column::LastErrorKind);
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
            .name("uq_balance_alert_events_user_window")
            .table(balance_alert_events::Entity)
            .col(balance_alert_events::Column::UserId)
            .col(balance_alert_events::Column::WindowStartedAtEpoch)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_balance_alert_events_due")
            .table(balance_alert_events::Entity)
            .col(balance_alert_events::Column::Status)
            .col(balance_alert_events::Column::NextAttemptAt)
            .col(balance_alert_events::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_balance_alert_events_user_created")
            .table(balance_alert_events::Entity)
            .col(balance_alert_events::Column::UserId)
            .col(balance_alert_events::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}
