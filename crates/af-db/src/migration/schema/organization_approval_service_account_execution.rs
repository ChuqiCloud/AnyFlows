use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_decisions, organization_approval_requests,
    organization_approval_service_account_executions, organization_budget_policies,
    organization_departments, organization_memberships, organization_service_accounts,
    organizations,
};

use super::{auto_id, nullable_timestamp, table};

const REQUEST_EXECUTION_INDEX: &str = "uq_org_approval_service_account_exec_request";

/// 创建服务账号审批执行事实及企业范围复合引用。
pub(in crate::migration) async fn create_organization_approval_service_account_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    use organization_approval_service_account_executions::Column;
    let statement =
        table(
            manager,
            organization_approval_service_account_executions::Entity,
        )
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
        .col(ColumnDef::new(Column::DecisionId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::ServiceAccountId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::ServiceAccountDepartmentId).big_integer())
        .col(
            ColumnDef::new(Column::ActionKind)
                .small_integer()
                .not_null()
                .check(Expr::col(Column::ActionKind).is_in([1_i16, 2_i16])),
        )
        .col(ColumnDef::new(Column::TargetStatus).small_integer())
        .col(ColumnDef::new(Column::PolicyId).big_integer())
        .col(
            ColumnDef::new(Column::ExpectedAccountVersion)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(Column::ExpectedAccountVersion)
                        .gt(0_i64)
                        .and(Expr::col(Column::ExpectedAccountVersion).lt(i64::MAX)),
                ),
        )
        .col(
            ColumnDef::new(Column::AppliedAccountVersion)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::AppliedAccountVersion).gt(0_i64)),
        )
        .col(ColumnDef::new(Column::ExpectedPolicyVersion).big_integer())
        .col(ColumnDef::new(Column::AppliedPolicyVersion).big_integer())
        .col(ColumnDef::new(Column::ExpectedBindingVersion).big_integer())
        .col(ColumnDef::new(Column::AppliedBindingVersion).big_integer())
        .col(
            ColumnDef::new(Column::ExecutedByMembershipId)
                .big_integer()
                .not_null(),
        )
        .col(nullable_timestamp(manager, Column::ExecutedAt).not_null())
        .check(action_shape())
        .check(
            Expr::col(Column::ActionKind)
                .eq(1_i16)
                .and(
                    Expr::col(Column::AppliedAccountVersion)
                        .eq(Expr::col(Column::ExpectedAccountVersion))
                        .or(Expr::col(Column::AppliedAccountVersion)
                            .eq(Expr::col(Column::ExpectedAccountVersion).add(1_i64))),
                )
                .or(Expr::col(Column::ActionKind).eq(2_i16).and(
                    Expr::col(Column::AppliedAccountVersion)
                        .eq(Expr::col(Column::ExpectedAccountVersion)),
                )),
        )
        .check(Expr::col(Column::ExpectedPolicyVersion).is_null().or(
            Expr::col(Column::AppliedPolicyVersion).eq(Expr::col(Column::ExpectedPolicyVersion)),
        ))
        .check(binding_version_shape())
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_service_account_exec_org")
                .from(
                    organization_approval_service_account_executions::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_service_account_exec_request")
                .from(
                    organization_approval_service_account_executions::Entity,
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
                .name("fk_org_approval_service_account_exec_decision")
                .from(
                    organization_approval_service_account_executions::Entity,
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
                .name("fk_org_approval_service_account_exec_account")
                .from(
                    organization_approval_service_account_executions::Entity,
                    (Column::OrganizationId, Column::ServiceAccountId),
                )
                .to(
                    organization_service_accounts::Entity,
                    (
                        organization_service_accounts::Column::OrganizationId,
                        organization_service_accounts::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_service_account_exec_policy")
                .from(
                    organization_approval_service_account_executions::Entity,
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
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_service_account_exec_actor")
                .from(
                    organization_approval_service_account_executions::Entity,
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
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_service_account_exec_department")
                .from(
                    organization_approval_service_account_executions::Entity,
                    (Column::OrganizationId, Column::ServiceAccountDepartmentId),
                )
                .to(
                    organization_departments::Entity,
                    (
                        organization_departments::Column::OrganizationId,
                        organization_departments::Column::Id,
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
                .table(organization_approval_service_account_executions::Entity)
                .col(Column::OrganizationId)
                .col(Column::RequestId)
                .unique()
                .to_owned(),
        )
        .await
}

fn action_shape() -> SimpleExpr {
    use organization_approval_service_account_executions::Column;
    Expr::col(Column::ActionKind)
        .eq(1_i16)
        .and(Expr::col(Column::TargetStatus).is_in([1_i16, 2_i16, 3_i16]))
        .and(Expr::col(Column::PolicyId).is_null())
        .and(Expr::col(Column::ExpectedPolicyVersion).is_null())
        .and(Expr::col(Column::AppliedPolicyVersion).is_null())
        .and(Expr::col(Column::ExpectedBindingVersion).is_null())
        .and(Expr::col(Column::AppliedBindingVersion).is_null())
        .or(Expr::col(Column::ActionKind)
            .eq(2_i16)
            .and(Expr::col(Column::TargetStatus).is_null())
            .and(Expr::col(Column::PolicyId).is_not_null())
            .and(Expr::col(Column::ExpectedPolicyVersion).gt(0_i64))
            .and(Expr::col(Column::AppliedPolicyVersion).gt(0_i64))
            .and(
                Expr::col(Column::ExpectedBindingVersion)
                    .is_null()
                    .or(Expr::col(Column::ExpectedBindingVersion).gt(0_i64)),
            )
            .and(Expr::col(Column::AppliedBindingVersion).gt(0_i64)))
}

fn binding_version_shape() -> SimpleExpr {
    use organization_approval_service_account_executions::Column;
    Expr::col(Column::ActionKind)
        .eq(1_i16)
        .and(
            Expr::col(Column::ExpectedBindingVersion)
                .is_null()
                .and(Expr::col(Column::AppliedBindingVersion).is_null()),
        )
        .or(Expr::col(Column::ActionKind).eq(2_i16).and(
            Expr::col(Column::ExpectedBindingVersion)
                .is_null()
                .and(Expr::col(Column::AppliedBindingVersion).eq(1_i64))
                .or(Expr::col(Column::ExpectedBindingVersion)
                    .is_not_null()
                    .and(Expr::col(Column::ExpectedBindingVersion).lt(i64::MAX))
                    .and(
                        Expr::col(Column::AppliedBindingVersion)
                            .eq(Expr::col(Column::ExpectedBindingVersion))
                            .or(Expr::col(Column::AppliedBindingVersion)
                                .eq(Expr::col(Column::ExpectedBindingVersion).add(1_i64))),
                    )),
        ))
}

/// 删除服务账号审批执行事实。
pub(in crate::migration) async fn drop_organization_approval_service_account_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_service_account_executions::Entity)
                .if_exists()
                .to_owned(),
        )
        .await
}
