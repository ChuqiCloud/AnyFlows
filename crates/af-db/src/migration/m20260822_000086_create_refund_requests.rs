use sea_orm_migration::prelude::*;

use crate::migration::iden::{refund_requests, users};

use super::schema;

/// 创建退款请求事实表；本迁移不触碰钱包余额或 Provider。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = schema::table(manager, refund_requests::Entity);
        statement
            .col(schema::auto_id(refund_requests::Column::Id))
            .col(
                ColumnDef::new(refund_requests::Column::RequestKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::IdempotencyKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::UserId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::OrderKind)
                    .small_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::OrderKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::Provider)
                    .string_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::Currency)
                    .char_len(3)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::OriginalAmountMinor)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::RefundAmountMinor)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::Status)
                    .small_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_requests::Column::Version)
                    .big_integer()
                    .not_null(),
            )
            .col(schema::timestamp(
                manager,
                refund_requests::Column::CreatedAt,
            ))
            .col(schema::timestamp(
                manager,
                refund_requests::Column::UpdatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_requests_user")
                    .from(refund_requests::Entity, refund_requests::Column::UserId)
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .check(Expr::col(refund_requests::Column::OrderKind).between(1_i16, 2_i16))
            .check(Expr::col(refund_requests::Column::OriginalAmountMinor).gt(0_i64))
            .check(Expr::col(refund_requests::Column::RefundAmountMinor).gt(0_i64))
            .check(
                Expr::col(refund_requests::Column::RefundAmountMinor)
                    .lte(Expr::col(refund_requests::Column::OriginalAmountMinor)),
            )
            .check(Expr::col(refund_requests::Column::Status).between(1_i16, 5_i16))
            .check(Expr::col(refund_requests::Column::Version).gt(0_i64));
        manager.create_table(statement).await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_refund_requests_request_key")
                    .table(refund_requests::Entity)
                    .col(refund_requests::Column::RequestKey)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_refund_requests_user_idempotency_key")
                    .table(refund_requests::Entity)
                    .col(refund_requests::Column::UserId)
                    .col(refund_requests::Column::IdempotencyKey)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_refund_requests_order")
                    .table(refund_requests::Entity)
                    .col(refund_requests::Column::OrderKind)
                    .col(refund_requests::Column::OrderKey)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(refund_requests::Entity).to_owned())
            .await
    }
}
