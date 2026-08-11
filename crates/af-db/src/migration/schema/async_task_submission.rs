use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    async_task_submission_claims, channels, credentials, groups, tokens, users,
};

use super::async_task::hex_check;
use super::{auto_id, table, timestamp};

/// 创建异步任务上游提交 claim 与恢复绑定存储。
pub(in crate::migration) async fn create_async_task_submission_claims(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, async_task_submission_claims::Entity);
    statement
        .col(auto_id(async_task_submission_claims::Column::Id))
        .col(
            ColumnDef::new(async_task_submission_claims::Column::TaskKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::IdempotencyKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::Protocol)
                .string_len(64)
                .not_null()
                .check(Expr::col(async_task_submission_claims::Column::Protocol).ne("")),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::RequestedModel)
                .string_len(256)
                .not_null()
                .check(Expr::col(async_task_submission_claims::Column::RequestedModel).ne("")),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::RequestFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::State)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(async_task_submission_claims::Column::State).is_in([1_i16, 2, 3])),
        )
        .col(ColumnDef::new(async_task_submission_claims::Column::AttemptKey).char_len(32))
        .col(ColumnDef::new(async_task_submission_claims::Column::TargetGroupId).big_integer())
        .col(
            ColumnDef::new(async_task_submission_claims::Column::UpstreamModel)
                .string_len(256)
                .check(
                    Expr::col(async_task_submission_claims::Column::UpstreamModel)
                        .is_null()
                        .or(Expr::col(async_task_submission_claims::Column::UpstreamModel).ne("")),
                ),
        )
        .col(ColumnDef::new(async_task_submission_claims::Column::ChannelId).big_integer())
        .col(ColumnDef::new(async_task_submission_claims::Column::CredentialId).big_integer())
        .col(ColumnDef::new(async_task_submission_claims::Column::CredentialRevision).char_len(16))
        .col(
            ColumnDef::new(async_task_submission_claims::Column::UpstreamTaskId)
                .string_len(512)
                .check(
                    Expr::col(async_task_submission_claims::Column::UpstreamTaskId)
                        .is_null()
                        .or(Expr::col(async_task_submission_claims::Column::UpstreamTaskId).ne("")),
                ),
        )
        .col(ColumnDef::new(async_task_submission_claims::Column::BindingFingerprint).char_len(64))
        .col(
            ColumnDef::new(async_task_submission_claims::Column::AttemptTimeoutMillis)
                .big_integer()
                .check(
                    Expr::col(async_task_submission_claims::Column::AttemptTimeoutMillis)
                        .is_null()
                        .or(
                            Expr::col(async_task_submission_claims::Column::AttemptTimeoutMillis)
                                .between(1_i64, 900_000_i64),
                        ),
                ),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::Status)
                .small_integer()
                .check(
                    Expr::col(async_task_submission_claims::Column::Status)
                        .is_null()
                        .or(Expr::col(async_task_submission_claims::Column::Status)
                            .is_in([1_i16, 2, 3, 4, 5])),
                ),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::ProgressBasisPoints)
                .small_integer()
                .check(
                    Expr::col(async_task_submission_claims::Column::ProgressBasisPoints)
                        .is_null()
                        .or(
                            Expr::col(async_task_submission_claims::Column::ProgressBasisPoints)
                                .between(0_i16, 10_000_i16),
                        ),
                ),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::FailureKind)
                .small_integer()
                .check(
                    Expr::col(async_task_submission_claims::Column::FailureKind)
                        .is_null()
                        .or(Expr::col(async_task_submission_claims::Column::FailureKind)
                            .is_in([1_i16, 2, 3, 4])),
                ),
        )
        .col(
            ColumnDef::new(async_task_submission_claims::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(async_task_submission_claims::Column::Version).gte(1_i64)),
        )
        .col(super::nullable_timestamp(
            manager,
            async_task_submission_claims::Column::AcceptedAt,
        ))
        .col(timestamp(
            manager,
            async_task_submission_claims::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            async_task_submission_claims::Column::UpdatedAt,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "task_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "idempotency_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "request_fingerprint",
            64,
            false,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "attempt_key",
            32,
            true,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "credential_revision",
            16,
            false,
        ))
        .check(hex_check(
            manager.get_database_backend(),
            "binding_fingerprint",
            64,
            false,
        ))
        .check(
            Expr::col(async_task_submission_claims::Column::UpdatedAt)
                .gte(Expr::col(async_task_submission_claims::Column::CreatedAt)),
        )
        .check(
            Expr::col(async_task_submission_claims::Column::AcceptedAt)
                .is_null()
                .or(Expr::col(async_task_submission_claims::Column::AcceptedAt)
                    .gte(Expr::col(async_task_submission_claims::Column::CreatedAt))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_submission_claims_user")
                .from(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_submission_claims_token")
                .from(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::TokenId,
                )
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_submission_claims_group")
                .from(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::GroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_submission_claims_target_group")
                .from(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::TargetGroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_submission_claims_channel")
                .from(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::ChannelId,
                )
                .to(channels::Entity, channels::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_async_task_submission_claims_credential")
                .from(
                    async_task_submission_claims::Entity,
                    async_task_submission_claims::Column::CredentialId,
                )
                .to(credentials::Entity, credentials::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    // MySQL 8.4 禁止 CHECK 引用带级联更新的外键列；仓储写入和读取边界会校验同一状态形状。
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(claim_state_shape());
    }
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name("uq_async_task_submission_claims_task_key")
            .table(async_task_submission_claims::Entity)
            .col(async_task_submission_claims::Column::TaskKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_async_task_submission_claims_owner_idempotency")
            .table(async_task_submission_claims::Entity)
            .col(async_task_submission_claims::Column::UserId)
            .col(async_task_submission_claims::Column::IdempotencyKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_async_task_submission_claims_owner_created")
            .table(async_task_submission_claims::Entity)
            .col(async_task_submission_claims::Column::UserId)
            .col(async_task_submission_claims::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_async_task_submission_claims_state_updated")
            .table(async_task_submission_claims::Entity)
            .col(async_task_submission_claims::Column::State)
            .col(async_task_submission_claims::Column::UpdatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn claim_state_shape() -> SimpleExpr {
    let binding_absent = binding_columns_are_null();
    let accepted_binding = binding_columns_are_present().and(accepted_status_shape());
    Expr::col(async_task_submission_claims::Column::State)
        .eq(1_i16)
        .and(Expr::col(async_task_submission_claims::Column::AttemptKey).is_null())
        .and(binding_absent.clone())
        .or(Expr::col(async_task_submission_claims::Column::State)
            .eq(2_i16)
            .and(Expr::col(async_task_submission_claims::Column::AttemptKey).is_not_null())
            .and(binding_absent))
        .or(Expr::col(async_task_submission_claims::Column::State)
            .eq(3_i16)
            .and(Expr::col(async_task_submission_claims::Column::AttemptKey).is_not_null())
            .and(accepted_binding))
}

fn binding_columns_are_null() -> SimpleExpr {
    Expr::col(async_task_submission_claims::Column::TargetGroupId)
        .is_null()
        .and(Expr::col(async_task_submission_claims::Column::UpstreamModel).is_null())
        .and(Expr::col(async_task_submission_claims::Column::ChannelId).is_null())
        .and(Expr::col(async_task_submission_claims::Column::CredentialId).is_null())
        .and(Expr::col(async_task_submission_claims::Column::CredentialRevision).is_null())
        .and(Expr::col(async_task_submission_claims::Column::UpstreamTaskId).is_null())
        .and(Expr::col(async_task_submission_claims::Column::BindingFingerprint).is_null())
        .and(Expr::col(async_task_submission_claims::Column::AttemptTimeoutMillis).is_null())
        .and(Expr::col(async_task_submission_claims::Column::Status).is_null())
        .and(Expr::col(async_task_submission_claims::Column::ProgressBasisPoints).is_null())
        .and(Expr::col(async_task_submission_claims::Column::FailureKind).is_null())
        .and(Expr::col(async_task_submission_claims::Column::AcceptedAt).is_null())
}

fn binding_columns_are_present() -> SimpleExpr {
    Expr::col(async_task_submission_claims::Column::TargetGroupId)
        .is_not_null()
        .and(Expr::col(async_task_submission_claims::Column::UpstreamModel).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::ChannelId).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::CredentialId).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::CredentialRevision).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::UpstreamTaskId).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::BindingFingerprint).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::AttemptTimeoutMillis).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::Status).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::ProgressBasisPoints).is_not_null())
        .and(Expr::col(async_task_submission_claims::Column::AcceptedAt).is_not_null())
}

fn accepted_status_shape() -> SimpleExpr {
    Expr::col(async_task_submission_claims::Column::Status)
        .is_in([1_i16, 2, 3])
        .and(Expr::col(async_task_submission_claims::Column::FailureKind).is_null())
        .or(Expr::col(async_task_submission_claims::Column::Status)
            .eq(4_i16)
            .and(
                Expr::col(async_task_submission_claims::Column::ProgressBasisPoints).eq(10_000_i16),
            )
            .and(Expr::col(async_task_submission_claims::Column::FailureKind).is_null()))
        .or(Expr::col(async_task_submission_claims::Column::Status)
            .eq(5_i16)
            .and(Expr::col(async_task_submission_claims::Column::ProgressBasisPoints).eq(0_i16))
            .and(Expr::col(async_task_submission_claims::Column::FailureKind).is_not_null()))
}
