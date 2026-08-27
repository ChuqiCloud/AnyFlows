use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_scim_audit_logs as audit_iden, organizations as organization_iden},
    schema::{auto_id, table, timestamp},
};

const APPEND_ONLY_TRIGGER: &str = "trg_organization_scim_audit_logs_append_only_update";
const POSTGRES_APPEND_ONLY_FUNCTION: &str = "reject_organization_scim_audit_logs_update";

/// 创建只追加的 SCIM 脱敏审计事实表。
#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, audit_iden::Entity)
                    .if_not_exists()
                    .col(auto_id(audit_iden::Column::Id))
                    .col(
                        ColumnDef::new(audit_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::TokenKey)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::EventType)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::Result)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::Reason)
                            .string_len(32)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::ResourceKind)
                            .small_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::ResourceDigest)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::ResourceVersion)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::RequestFingerprint)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit_iden::Column::RetryAfterSeconds)
                            .integer()
                            .null(),
                    )
                    .col(timestamp(manager, audit_iden::Column::CreatedAt))
                    .check(
                        Expr::col(audit_iden::Column::EventType).is_in([
                            "token_issued",
                            "token_revoked",
                            "request_accepted",
                            "request_rejected",
                            "resource_synchronized",
                            "lifecycle_transition",
                            "rate_limit_rejected",
                        ]),
                    )
                    .check(
                        Expr::col(audit_iden::Column::Result)
                            .is_in(["applied", "noop", "rejected", "failed"]),
                    )
                    .check(
                        Expr::col(audit_iden::Column::Reason).is_null()
                            .or(Expr::col(audit_iden::Column::Reason).is_in([
                                "invalid_token",
                                "token_revoked",
                                "token_expired",
                                "cross_organization",
                                "stale_resource",
                                "entitlement_unavailable",
                                "seat_capacity_exceeded",
                                "rate_limit_exceeded",
                                "storage_unavailable",
                                "invalid_request",
                                "internal_failure",
                            ])),
                    )
                    .check(
                        Expr::col(audit_iden::Column::ResourceKind)
                            .is_null()
                            .or(Expr::col(audit_iden::Column::ResourceKind).is_in([1_i16, 2_i16])),
                    )
                    .check(
                        Expr::col(audit_iden::Column::ResourceVersion)
                            .is_null()
                            .or(Expr::col(audit_iden::Column::ResourceVersion).gt(0_i64)),
                    )
                    .check(
                        Expr::col(audit_iden::Column::RetryAfterSeconds)
                            .is_null()
                            .or(Expr::col(audit_iden::Column::RetryAfterSeconds).gte(0_i32)),
                    )
                    .check(Expr::cust(
                        "((resource_kind IS NULL AND resource_digest IS NULL AND resource_version IS NULL) OR (resource_kind IS NOT NULL AND resource_digest IS NOT NULL AND resource_version IS NOT NULL))",
                    ))
                    .check(Expr::cust(
                        "((result IN ('rejected', 'failed') AND reason IS NOT NULL) OR (result IN ('applied', 'noop') AND reason IS NULL))",
                    ))
                    .check(Expr::cust(
                        "((event_type = 'rate_limit_rejected' AND result = 'rejected' AND retry_after_seconds IS NOT NULL) OR (event_type <> 'rate_limit_rejected' AND retry_after_seconds IS NULL))",
                    ))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_scim_audit_logs_organization")
                            .from(
                                audit_iden::Entity,
                                audit_iden::Column::OrganizationId,
                            )
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("uq_organization_scim_audit_logs_request_event")
                .table(audit_iden::Entity)
                .col(audit_iden::Column::OrganizationId)
                .col(audit_iden::Column::RequestFingerprint)
                .col(audit_iden::Column::EventType)
                .unique()
                .to_owned(),
            Index::create()
                .name("ix_organization_scim_audit_logs_scope_id")
                .table(audit_iden::Entity)
                .col(audit_iden::Column::OrganizationId)
                .col(audit_iden::Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        create_append_only_guard(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_append_only_guard(manager).await?;
        manager
            .drop_table(Table::drop().table(audit_iden::Entity).to_owned())
            .await
    }
}

async fn create_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_organization_scim_audit_logs_append_only_update BEFORE UPDATE ON organization_scim_audit_logs \
                 BEGIN SELECT RAISE(ABORT, 'organization SCIM audit logs are append only'); END",
            )
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(
                    "CREATE FUNCTION reject_organization_scim_audit_logs_update() RETURNS trigger LANGUAGE plpgsql AS $$ \
                     BEGIN RAISE EXCEPTION 'organization SCIM audit logs are append only'; RETURN OLD; END $$",
                )
                .await?;
            connection
                .execute_unprepared(
                    "CREATE TRIGGER trg_organization_scim_audit_logs_append_only_update BEFORE UPDATE ON organization_scim_audit_logs \
                     FOR EACH ROW EXECUTE FUNCTION reject_organization_scim_audit_logs_update()",
                )
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(
                "CREATE TRIGGER trg_organization_scim_audit_logs_append_only_update BEFORE UPDATE ON organization_scim_audit_logs \
                 FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'organization SCIM audit logs are append only'",
            )
            .await
            .map(|_| ()),
    }
}

async fn drop_append_only_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => connection
            .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {APPEND_ONLY_TRIGGER}"))
            .await
            .map(|_| ()),
        DbBackend::Postgres => {
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {APPEND_ONLY_TRIGGER} ON organization_scim_audit_logs"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_APPEND_ONLY_FUNCTION}()"
                ))
                .await
                .map(|_| ())
        }
        DbBackend::MySql => connection
            .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {APPEND_ONLY_TRIGGER}"))
            .await
            .map(|_| ()),
    }
}
