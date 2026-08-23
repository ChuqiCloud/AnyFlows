use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{refund_requests, users};

/// 为退款请求增加管理员审批事实，审批状态与 Provider 状态分离。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 不支持一次 ALTER TABLE 携带多个选项，逐列执行以保持三方言一致。
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .add_column(
                        ColumnDef::new(refund_requests::Column::ApprovalStatus)
                            .small_integer()
                            .not_null()
                            .default(1_i16),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .add_column(
                        ColumnDef::new(refund_requests::Column::ApprovalActorId).big_integer(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .add_column(
                        ColumnDef::new(refund_requests::Column::ApprovalReason).string_len(512),
                    )
                    .to_owned(),
            )
            .await?;
        // SQLite 无法为已有表追加外键；应用层仍会校验审批人，其他方言保留数据库约束。
        if manager.get_database_backend() != DbBackend::Sqlite {
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name("fk_refund_requests_approval_actor")
                        .from(
                            refund_requests::Entity,
                            refund_requests::Column::ApprovalActorId,
                        )
                        .to(users::Entity, users::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Restrict)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("idx_refund_requests_approval_status_id")
                    .table(refund_requests::Entity)
                    .col(refund_requests::Column::ApprovalStatus)
                    .col(refund_requests::Column::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_refund_requests_approval_status_id")
                    .table(refund_requests::Entity)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() != DbBackend::Sqlite {
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name("fk_refund_requests_approval_actor")
                        .table(refund_requests::Entity)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .drop_column(refund_requests::Column::ApprovalReason)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .drop_column(refund_requests::Column::ApprovalActorId)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(refund_requests::Entity)
                    .drop_column(refund_requests::Column::ApprovalStatus)
                    .to_owned(),
            )
            .await
    }
}
