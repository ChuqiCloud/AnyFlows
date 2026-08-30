use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_budget_executions, organization_approval_decisions,
    organization_approval_requests, organization_budget_policies, organization_departments,
    organization_memberships, organization_teams, organizations,
};

use super::{auto_id, nullable_timestamp, table};

const POLICY_SCOPE_ID_INDEX: &str = "uq_org_budget_policies_scope_id";
const REQUEST_EXECUTION_INDEX: &str = "uq_org_approval_budget_exec_request";

/// 创建预算审批执行事实及企业范围复合外键。
pub(in crate::migration) async fn create_organization_approval_budget_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name(POLICY_SCOPE_ID_INDEX)
                .table(organization_budget_policies::Entity)
                .col(organization_budget_policies::Column::OrganizationId)
                .col(organization_budget_policies::Column::Id)
                .unique()
                .to_owned(),
        )
        .await?;

    use organization_approval_budget_executions::Column;
    let statement = table(manager, organization_approval_budget_executions::Entity)
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
        .col(ColumnDef::new(Column::DecisionId).big_integer().not_null())
        .col(ColumnDef::new(Column::PolicyId).big_integer().not_null())
        .col(ColumnDef::new(Column::ScopeKind).small_integer().not_null())
        .col(ColumnDef::new(Column::TeamId).big_integer())
        .col(ColumnDef::new(Column::MembershipId).big_integer())
        .col(ColumnDef::new(Column::DepartmentId).big_integer())
        .col(
            ColumnDef::new(Column::PeriodKind)
                .small_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::FixedPeriodSeconds).big_integer())
        .col(ColumnDef::new(Column::QuotaLimit).big_integer())
        .col(nullable_timestamp(manager, Column::EffectiveFrom).not_null())
        .col(
            ColumnDef::new(Column::ExpectedPolicyVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::AppliedPolicyVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::ExecutedByMembershipId)
                .big_integer()
                .not_null(),
        )
        .col(nullable_timestamp(manager, Column::ExecutedAt).not_null())
        .check(Expr::col(Column::ScopeKind).between(1_i16, 4_i16))
        .check(Expr::col(Column::PeriodKind).between(1_i16, 4_i16))
        .check(
            Expr::col(Column::QuotaLimit)
                .is_null()
                .or(Expr::col(Column::QuotaLimit).gt(0_i64)),
        )
        .check(Expr::col(Column::ExpectedPolicyVersion).gte(0_i64))
        .check(Expr::col(Column::AppliedPolicyVersion).gt(0_i64))
        .check(scope_shape())
        .check(period_shape())
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_budget_exec_org")
                .from(
                    organization_approval_budget_executions::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_budget_exec_request")
                .from(
                    organization_approval_budget_executions::Entity,
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
                .name("fk_org_approval_budget_exec_decision")
                .from(
                    organization_approval_budget_executions::Entity,
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
                .name("fk_org_approval_budget_exec_policy")
                .from(
                    organization_approval_budget_executions::Entity,
                    (Column::OrganizationId, Column::PolicyId),
                )
                .to(
                    organization_budget_policies::Entity,
                    (
                        organization_budget_policies::Column::OrganizationId,
                        organization_budget_policies::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(&mut membership_foreign_key(
            "fk_org_approval_budget_exec_actor",
            Column::ExecutedByMembershipId,
        ))
        .foreign_key(&mut optional_scope_foreign_key(
            "fk_org_approval_budget_exec_team",
            Column::TeamId,
            organization_teams::Entity,
            organization_teams::Column::OrganizationId,
            organization_teams::Column::Id,
        ))
        .foreign_key(&mut optional_scope_foreign_key(
            "fk_org_approval_budget_exec_member",
            Column::MembershipId,
            organization_memberships::Entity,
            organization_memberships::Column::OrganizationId,
            organization_memberships::Column::Id,
        ))
        .foreign_key(&mut optional_scope_foreign_key(
            "fk_org_approval_budget_exec_department",
            Column::DepartmentId,
            organization_departments::Entity,
            organization_departments::Column::OrganizationId,
            organization_departments::Column::Id,
        ))
        .to_owned();
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(REQUEST_EXECUTION_INDEX)
                .table(organization_approval_budget_executions::Entity)
                .col(Column::OrganizationId)
                .col(Column::RequestId)
                .unique()
                .to_owned(),
        )
        .await
}

fn membership_foreign_key(
    name: &'static str,
    member_column: organization_approval_budget_executions::Column,
) -> ForeignKeyCreateStatement {
    optional_scope_foreign_key(
        name,
        member_column,
        organization_memberships::Entity,
        organization_memberships::Column::OrganizationId,
        organization_memberships::Column::Id,
    )
}

fn optional_scope_foreign_key<T, O, C>(
    name: &'static str,
    scope_column: organization_approval_budget_executions::Column,
    target_table: T,
    target_organization_column: O,
    target_column: C,
) -> ForeignKeyCreateStatement
where
    T: IntoTableRef,
    O: IntoIden,
    C: IntoIden,
{
    ForeignKey::create()
        .name(name)
        .from(
            organization_approval_budget_executions::Entity,
            (
                organization_approval_budget_executions::Column::OrganizationId,
                scope_column,
            ),
        )
        .to(target_table, (target_organization_column, target_column))
        .on_update(ForeignKeyAction::Restrict)
        .on_delete(ForeignKeyAction::Restrict)
        .to_owned()
}

fn scope_shape() -> SimpleExpr {
    use organization_approval_budget_executions::Column;
    Expr::col(Column::ScopeKind)
        .eq(1_i16)
        .and(Expr::col(Column::TeamId).is_null())
        .and(Expr::col(Column::MembershipId).is_null())
        .and(Expr::col(Column::DepartmentId).is_null())
        .or(Expr::col(Column::ScopeKind)
            .eq(2_i16)
            .and(Expr::col(Column::TeamId).is_not_null())
            .and(Expr::col(Column::MembershipId).is_null())
            .and(Expr::col(Column::DepartmentId).is_null()))
        .or(Expr::col(Column::ScopeKind)
            .eq(3_i16)
            .and(Expr::col(Column::TeamId).is_null())
            .and(Expr::col(Column::MembershipId).is_not_null())
            .and(Expr::col(Column::DepartmentId).is_null()))
        .or(Expr::col(Column::ScopeKind)
            .eq(4_i16)
            .and(Expr::col(Column::TeamId).is_null())
            .and(Expr::col(Column::MembershipId).is_null())
            .and(Expr::col(Column::DepartmentId).is_not_null()))
}

fn period_shape() -> SimpleExpr {
    use organization_approval_budget_executions::Column;
    Expr::col(Column::PeriodKind)
        .is_in([1_i16, 2_i16, 3_i16])
        .and(Expr::col(Column::FixedPeriodSeconds).is_null())
        .or(Expr::col(Column::PeriodKind)
            .eq(4_i16)
            .and(Expr::col(Column::FixedPeriodSeconds).between(60_i64, 31_622_400_i64)))
}

/// 删除预算审批执行事实及策略复合引用索引。
pub(in crate::migration) async fn drop_organization_approval_budget_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_budget_executions::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_index(
            Index::drop()
                .name(POLICY_SCOPE_ID_INDEX)
                .table(organization_budget_policies::Entity)
                .to_owned(),
        )
        .await
}
