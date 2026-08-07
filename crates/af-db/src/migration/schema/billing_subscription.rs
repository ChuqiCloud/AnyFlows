use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    billing_reservations, billing_subscription_reservations, user_subscriptions,
};

use super::{table, timestamp};

/// 创建请求预留与订阅窗口之间的一对一资金来源快照。
pub(in crate::migration) async fn create_billing_subscription_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, billing_subscription_reservations::Entity);
    statement
        .col(
            ColumnDef::new(billing_subscription_reservations::Column::IdempotencyKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(billing_subscription_reservations::Column::UserSubscriptionId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            billing_subscription_reservations::Column::WindowStartedAt,
        ))
        .col(timestamp(
            manager,
            billing_subscription_reservations::Column::WindowEndsAt,
        ))
        .col(
            ColumnDef::new(billing_subscription_reservations::Column::ReservedQuota)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(billing_subscription_reservations::Column::ReservedQuota).gt(0_i64),
                ),
        )
        .col(
            ColumnDef::new(billing_subscription_reservations::Column::SubscriptionActualQuota)
                .big_integer()
                .check(
                    Expr::col(billing_subscription_reservations::Column::SubscriptionActualQuota)
                        .gte(0_i64),
                ),
        )
        .col(timestamp(
            manager,
            billing_subscription_reservations::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            billing_subscription_reservations::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_subscription_reservations_reservation")
                .from(
                    billing_subscription_reservations::Entity,
                    billing_subscription_reservations::Column::IdempotencyKey,
                )
                .to(
                    billing_reservations::Entity,
                    billing_reservations::Column::IdempotencyKey,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_subscription_reservations_subscription")
                .from(
                    billing_subscription_reservations::Entity,
                    billing_subscription_reservations::Column::UserSubscriptionId,
                )
                .to(user_subscriptions::Entity, user_subscriptions::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(
            Expr::col(billing_subscription_reservations::Column::WindowEndsAt).gt(Expr::col(
                billing_subscription_reservations::Column::WindowStartedAt,
            )),
        )
        .check(
            Expr::col(billing_subscription_reservations::Column::CreatedAt)
                .gte(Expr::col(
                    billing_subscription_reservations::Column::WindowStartedAt,
                ))
                .and(
                    Expr::col(billing_subscription_reservations::Column::CreatedAt).lt(Expr::col(
                        billing_subscription_reservations::Column::WindowEndsAt,
                    )),
                ),
        )
        .check(
            Expr::col(billing_subscription_reservations::Column::UpdatedAt).gte(Expr::col(
                billing_subscription_reservations::Column::CreatedAt,
            )),
        );
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name("idx_billing_subscription_reservations_subscription")
                .table(billing_subscription_reservations::Entity)
                .col(billing_subscription_reservations::Column::UserSubscriptionId)
                .col(billing_subscription_reservations::Column::WindowEndsAt)
                .col(billing_subscription_reservations::Column::IdempotencyKey)
                .to_owned(),
        )
        .await
}
