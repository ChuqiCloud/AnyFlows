use sea_orm::{ActiveModelTrait, EntityTrait, Set, entity::prelude::TimeDateTimeWithTimeZone};
use sea_orm_migration::prelude::*;

use crate::entity::{organization_sso_policies, organizations};

use super::{
    iden::{organization_sso_policies as policy_iden, organizations as organization_iden},
    schema::{table, timestamp},
};

/// 创建企业 SSO 强制策略表，并为历史企业回填安全的 Optional 默认值。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, policy_iden::Entity)
                    .col(
                        ColumnDef::new(policy_iden::Column::OrganizationId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(policy_iden::Column::EnforcementPolicy)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(
                                Expr::col(policy_iden::Column::EnforcementPolicy)
                                    .between(1_i16, 2_i16),
                            ),
                    )
                    .col(
                        ColumnDef::new(policy_iden::Column::RecoveryReady)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(policy_iden::Column::Version)
                            .big_integer()
                            .not_null()
                            .default(1_i64)
                            .check(Expr::col(policy_iden::Column::Version).gt(0_i64)),
                    )
                    .col(timestamp(manager, policy_iden::Column::CreatedAt))
                    .col(timestamp(manager, policy_iden::Column::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organization_sso_policies_organization")
                            .from(policy_iden::Entity, policy_iden::Column::OrganizationId)
                            .to(organization_iden::Entity, organization_iden::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        let now = TimeDateTimeWithTimeZone::now_utc();
        for organization in organizations::Entity::find()
            .all(manager.get_connection())
            .await?
        {
            organization_sso_policies::ActiveModel {
                organization_id: Set(organization.id),
                enforcement_policy: Set(1),
                recovery_ready: Set(false),
                version: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(manager.get_connection())
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(policy_iden::Entity).to_owned())
            .await
    }
}
