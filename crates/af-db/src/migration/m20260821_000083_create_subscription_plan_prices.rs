use sea_orm_migration::prelude::*;

use crate::migration::iden::{subscription_plan_prices, subscription_plans};

use super::schema;

/// 创建订阅计划不可变价格事实表。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = schema::table(manager, subscription_plan_prices::Entity);
        statement
            .col(schema::auto_id(subscription_plan_prices::Column::Id))
            .col(
                ColumnDef::new(subscription_plan_prices::Column::PlanId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_plan_prices::Column::Provider)
                    .string_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_plan_prices::Column::Currency)
                    .string_len(3)
                    .not_null(),
            )
            .col(
                ColumnDef::new(subscription_plan_prices::Column::AmountMinor)
                    .big_integer()
                    .not_null(),
            )
            .col(schema::timestamp(
                manager,
                subscription_plan_prices::Column::CreatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_subscription_plan_prices_plan")
                    .from(
                        subscription_plan_prices::Entity,
                        subscription_plan_prices::Column::PlanId,
                    )
                    .to(subscription_plans::Entity, subscription_plans::Column::Id)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .check(Expr::col(subscription_plan_prices::Column::Provider).ne(""))
            .check(Expr::col(subscription_plan_prices::Column::Currency).ne(""))
            .check(Expr::col(subscription_plan_prices::Column::AmountMinor).gt(0_i64));
        manager.create_table(statement).await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_subscription_plan_prices_plan")
                    .table(subscription_plan_prices::Entity)
                    .col(subscription_plan_prices::Column::PlanId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(subscription_plan_prices::Entity)
                    .to_owned(),
            )
            .await
    }
}
