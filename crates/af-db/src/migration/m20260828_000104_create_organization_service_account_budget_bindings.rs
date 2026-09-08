use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_budget_policies, organization_service_account_budget_bindings as binding,
        organization_service_accounts, organization_teams, organizations,
    },
    schema::{auto_id, table, timestamp},
};

const POLICY_SCOPE_ID_INDEX: &str = "uq_organization_budget_policies_scope_id";

/// 创建服务账号与企业/团队预算策略的显式绑定事实。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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

        manager
            .create_table(
                table(manager, binding::Entity)
                    .if_not_exists()
                    .col(auto_id(binding::Column::Id))
                    .col(
                        ColumnDef::new(binding::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(binding::Column::ServiceAccountId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(binding::Column::PolicyId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(binding::Column::PolicyVersion)
                            .big_integer()
                            .not_null()
                            .check(Expr::col(binding::Column::PolicyVersion).gt(0_i64)),
                    )
                    .col(
                        ColumnDef::new(binding::Column::ScopeKind)
                            .small_integer()
                            .not_null()
                            .check(Expr::col(binding::Column::ScopeKind).is_in([1_i16, 2_i16])),
                    )
                    .col(ColumnDef::new(binding::Column::TeamId).big_integer())
                    .col(
                        ColumnDef::new(binding::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(binding::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, binding::Column::CreatedAt))
                    .col(timestamp(manager, binding::Column::UpdatedAt))
                    .check(binding_subject_shape())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_account_budget_binding_org")
                            .from(binding::Entity, binding::Column::OrganizationId)
                            .to(organizations::Entity, organizations::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_account_budget_binding_account")
                            .from(
                                binding::Entity,
                                (
                                    binding::Column::OrganizationId,
                                    binding::Column::ServiceAccountId,
                                ),
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
                            .name("fk_service_account_budget_binding_policy")
                            .from(
                                binding::Entity,
                                (binding::Column::OrganizationId, binding::Column::PolicyId),
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
                            .name("fk_service_account_budget_binding_team")
                            .from(
                                binding::Entity,
                                (binding::Column::OrganizationId, binding::Column::TeamId),
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
                    .to_owned(),
            )
            .await?;

        for index in [
            Index::create()
                .name("uq_service_account_budget_binding_account")
                .table(binding::Entity)
                .col(binding::Column::OrganizationId)
                .col(binding::Column::ServiceAccountId)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_service_account_budget_binding_policy")
                .table(binding::Entity)
                .col(binding::Column::OrganizationId)
                .col(binding::Column::PolicyId)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(binding::Entity).to_owned())
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
}

fn binding_subject_shape() -> SimpleExpr {
    use binding::Column;
    Expr::col(Column::ScopeKind)
        .eq(1_i16)
        .and(Expr::col(Column::TeamId).is_null())
        .or(Expr::col(Column::ScopeKind)
            .eq(2_i16)
            .and(Expr::col(Column::TeamId).is_not_null()))
}
