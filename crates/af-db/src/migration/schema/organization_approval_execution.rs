use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_decisions, organization_approval_member_executions,
    organization_approval_requests, organization_memberships, organization_teams, organizations,
};

use super::{auto_id, nullable_timestamp, table};

const REQUEST_EXECUTION_INDEX: &str = "uq_org_approval_member_exec_request";
const DECISION_SCOPE_ID_INDEX: &str = "uq_org_approval_decisions_scope_id";

/// 创建成员审批执行事实及复合外键所需的企业范围引用索引。
pub(in crate::migration) async fn create_organization_approval_member_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name(DECISION_SCOPE_ID_INDEX)
                .table(organization_approval_decisions::Entity)
                .col(organization_approval_decisions::Column::OrganizationId)
                .col(organization_approval_decisions::Column::Id)
                .unique()
                .to_owned(),
        )
        .await?;
    use organization_approval_member_executions::Column;
    let statement = table(manager, organization_approval_member_executions::Entity)
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
        .col(ColumnDef::new(Column::DecisionId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::TargetMembershipId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::TargetRole)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::TargetStatus)
                .small_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::TargetTeamId).big_integer())
        .col(
            ColumnDef::new(Column::ExpectedMembershipVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::AppliedMembershipVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::ExecutedByMembershipId)
                .big_integer()
                .not_null(),
        )
        .col(nullable_timestamp(manager, Column::ExecutedAt).not_null())
        .check(Expr::col(Column::TargetRole).between(1_i16, 5_i16))
        .check(Expr::col(Column::TargetStatus).between(1_i16, 3_i16))
        .check(Expr::col(Column::ExpectedMembershipVersion).gt(0_i64))
        .check(Expr::col(Column::AppliedMembershipVersion).gt(0_i64))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_member_exec_org")
                .from(
                    organization_approval_member_executions::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_member_exec_request")
                .from(
                    organization_approval_member_executions::Entity,
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
                .name("fk_org_approval_member_exec_decision")
                .from(
                    organization_approval_member_executions::Entity,
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
        .foreign_key(&mut membership_foreign_key(
            "fk_org_approval_member_exec_target",
            Column::TargetMembershipId,
        ))
        .foreign_key(&mut membership_foreign_key(
            "fk_org_approval_member_exec_actor",
            Column::ExecutedByMembershipId,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_member_exec_team")
                .from(
                    organization_approval_member_executions::Entity,
                    (Column::OrganizationId, Column::TargetTeamId),
                )
                .to(
                    organization_teams::Entity,
                    (
                        organization_teams::Column::OrganizationId,
                        organization_teams::Column::Id,
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
                .table(organization_approval_member_executions::Entity)
                .col(Column::OrganizationId)
                .col(Column::RequestId)
                .unique()
                .to_owned(),
        )
        .await
}

fn membership_foreign_key(
    name: &'static str,
    member_column: organization_approval_member_executions::Column,
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(
            organization_approval_member_executions::Entity,
            (
                organization_approval_member_executions::Column::OrganizationId,
                member_column,
            ),
        )
        .to(
            organization_memberships::Entity,
            (
                organization_memberships::Column::OrganizationId,
                organization_memberships::Column::Id,
            ),
        )
        .on_update(ForeignKeyAction::Restrict)
        .on_delete(ForeignKeyAction::Restrict)
        .to_owned()
}

/// 删除成员审批执行事实及本迁移新增的复合引用索引。
pub(in crate::migration) async fn drop_organization_approval_member_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_member_executions::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_index(
            Index::drop()
                .name(DECISION_SCOPE_ID_INDEX)
                .table(organization_approval_decisions::Entity)
                .to_owned(),
        )
        .await?;
    Ok(())
}
