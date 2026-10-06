use sea_orm_migration::prelude::*;

use crate::migration::iden::{subscription_orders, subscription_plans, users};

use super::schema;

/// 创建订阅购买订单表及用户范围内的幂等约束。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = schema::table(manager, subscription_orders::Entity);
        statement
            .col(schema::auto_id(subscription_orders::Column::Id))
            .col(
                ColumnDef::new(subscription_orders::Column::OrderKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::UserId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::PlanId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::PlanKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::PlanVersion)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::Provider)
                    .string_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::Currency)
                    .char_len(3)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::AmountMinor)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::QuotaAmount)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::Status)
                    .small_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::IdempotencyKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_orders::Column::Version)
                    .big_integer()
                    .not_null(),
            )
            .col(ColumnDef::new(subscription_orders::Column::ProviderOrderId).string_len(128))
            .col(ColumnDef::new(subscription_orders::Column::ExpiresAt).timestamp_with_time_zone())
            .col(ColumnDef::new(subscription_orders::Column::PaidAt).timestamp_with_time_zone())
            .col(ColumnDef::new(subscription_orders::Column::ClosedAt).timestamp_with_time_zone())
            .col(schema::timestamp(
                manager,
                subscription_orders::Column::CreatedAt,
            ))
            .col(schema::timestamp(
                manager,
                subscription_orders::Column::UpdatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_subscription_orders_user")
                    .from(
                        subscription_orders::Entity,
                        subscription_orders::Column::UserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_subscription_orders_plan")
                    .from(
                        subscription_orders::Entity,
                        subscription_orders::Column::PlanId,
                    )
                    .to(subscription_plans::Entity, subscription_plans::Column::Id)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .check(Expr::col(subscription_orders::Column::PlanVersion).gt(0_i64))
            .check(Expr::col(subscription_orders::Column::AmountMinor).gt(0_i64))
            .check(Expr::col(subscription_orders::Column::QuotaAmount).gt(0_i64))
            .check(Expr::col(subscription_orders::Column::Version).gt(0_i64))
            .check(Expr::col(subscription_orders::Column::Status).between(1_i16, 6_i16));
        manager.create_table(statement).await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_subscription_orders_order_key")
                    .table(subscription_orders::Entity)
                    .col(subscription_orders::Column::OrderKey)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_subscription_orders_user_idempotency_key")
                    .table(subscription_orders::Entity)
                    .col(subscription_orders::Column::UserId)
                    .col(subscription_orders::Column::IdempotencyKey)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(subscription_orders::Entity).to_owned())
            .await
    }
}
