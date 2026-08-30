use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_decisions, organization_approval_funds_executions,
    organization_approval_requests, organization_memberships, organization_wallet_ledger_entries,
    organizations,
};

use super::{auto_id, nullable_timestamp, table};

const REQUEST_EXECUTION_INDEX: &str = "uq_org_approval_funds_exec_request";

/// 创建企业钱包调账审批执行事实及账本事件引用。
pub(in crate::migration) async fn create_organization_approval_funds_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    use organization_approval_funds_executions::Column;
    let statement = table(manager, organization_approval_funds_executions::Entity)
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
        .col(ColumnDef::new(Column::DecisionId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::EventKey)
                .char_len(32)
                .not_null()
                .check(event_key_check(manager.get_database_backend()))
                .check(Expr::col(Column::EventKey).ne("00000000000000000000000000000000")),
        )
        .col(
            ColumnDef::new(Column::QuotaDelta)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::QuotaDelta).ne(0_i64))
                .check(Expr::col(Column::QuotaDelta).ne(i64::MIN)),
        )
        .col(
            ColumnDef::new(Column::Reason)
                .string_len(500)
                .not_null()
                .check(Expr::col(Column::Reason).ne("")),
        )
        .col(
            ColumnDef::new(Column::ExpectedWalletVersion)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(Column::ExpectedWalletVersion)
                        .gt(0_i64)
                        .and(Expr::col(Column::ExpectedWalletVersion).lt(i64::MAX)),
                ),
        )
        .col(
            ColumnDef::new(Column::AppliedWalletVersion)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::AppliedWalletVersion).gt(0_i64)),
        )
        .col(
            ColumnDef::new(Column::BalanceBefore)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::BalanceBefore).gte(0_i64)),
        )
        .col(
            ColumnDef::new(Column::BalanceAfter)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::BalanceAfter).gte(0_i64)),
        )
        .col(
            ColumnDef::new(Column::LedgerEntryId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::ExecutedByMembershipId)
                .big_integer()
                .not_null(),
        )
        .col(nullable_timestamp(manager, Column::ExecutedAt).not_null())
        .check(
            Expr::col(Column::ExpectedWalletVersion)
                .add(1_i64)
                .eq(Expr::col(Column::AppliedWalletVersion)),
        )
        .check(
            Expr::col(Column::BalanceAfter)
                .eq(Expr::col(Column::BalanceBefore).add(Expr::col(Column::QuotaDelta))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_funds_exec_org")
                .from(
                    organization_approval_funds_executions::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_funds_exec_request")
                .from(
                    organization_approval_funds_executions::Entity,
                    (Column::OrganizationId, Column::RequestId),
                )
                .to(
                    organization_approval_requests::Entity,
                    (
                        organization_approval_requests::Column::OrganizationId,
                        organization_approval_requests::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_funds_exec_decision")
                .from(
                    organization_approval_funds_executions::Entity,
                    (Column::OrganizationId, Column::DecisionId),
                )
                .to(
                    organization_approval_decisions::Entity,
                    (
                        organization_approval_decisions::Column::OrganizationId,
                        organization_approval_decisions::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_funds_exec_ledger")
                .from(
                    organization_approval_funds_executions::Entity,
                    (Column::OrganizationId, Column::EventKey),
                )
                .to(
                    organization_wallet_ledger_entries::Entity,
                    (
                        organization_wallet_ledger_entries::Column::OrganizationId,
                        organization_wallet_ledger_entries::Column::EventKey,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_funds_exec_ledger_id")
                .from(
                    organization_approval_funds_executions::Entity,
                    Column::LedgerEntryId,
                )
                .to(
                    organization_wallet_ledger_entries::Entity,
                    organization_wallet_ledger_entries::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_funds_exec_actor")
                .from(
                    organization_approval_funds_executions::Entity,
                    (Column::OrganizationId, Column::ExecutedByMembershipId),
                )
                .to(
                    organization_memberships::Entity,
                    (
                        organization_memberships::Column::OrganizationId,
                        organization_memberships::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(REQUEST_EXECUTION_INDEX)
                .table(organization_approval_funds_executions::Entity)
                .col(Column::OrganizationId)
                .col(Column::RequestId)
                .unique()
                .to_owned(),
        )
        .await
}

/// 删除企业钱包调账审批执行事实。
pub(in crate::migration) async fn drop_organization_approval_funds_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_funds_executions::Entity)
                .if_exists()
                .to_owned(),
        )
        .await
}

fn event_key_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""event_key" ~ '^[0-9a-f]{32}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`event_key`) = 32 AND `event_key` REGEXP '^[0-9a-f]{32}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("event_key") = 32 AND "event_key" NOT GLOB '*[^0-9a-f]*'"#)
        }
    }
}
