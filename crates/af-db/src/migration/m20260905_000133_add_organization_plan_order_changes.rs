use sea_orm_migration::prelude::*;

use super::iden::organization_plan_orders;

/// 为既有企业订单补充续费、升级、降级动作和生效时间事实。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 的 ALTER TABLE 每次只能增加一列，拆分执行保证三方言迁移一致。
        manager
            .alter_table(
                Table::alter()
                    .table(organization_plan_orders::Entity)
                    .add_column(
                        ColumnDef::new(organization_plan_orders::Column::Kind)
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
                    .table(organization_plan_orders::Entity)
                    .add_column(
                        ColumnDef::new(organization_plan_orders::Column::EffectiveAt)
                            .timestamp_with_time_zone(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_plan_orders::Entity)
                    .add_column(
                        ColumnDef::new(organization_plan_orders::Column::AppliedAt)
                            .timestamp_with_time_zone(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_organization_plan_orders_due")
                    .table(organization_plan_orders::Entity)
                    .col(organization_plan_orders::Column::OrganizationId)
                    .col(organization_plan_orders::Column::Status)
                    .col(organization_plan_orders::Column::EffectiveAt)
                    .col(organization_plan_orders::Column::AppliedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_organization_plan_orders_due")
                    .table(organization_plan_orders::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_plan_orders::Entity)
                    .drop_column(organization_plan_orders::Column::AppliedAt)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_plan_orders::Entity)
                    .drop_column(organization_plan_orders::Column::EffectiveAt)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(organization_plan_orders::Entity)
                    .drop_column(organization_plan_orders::Column::Kind)
                    .to_owned(),
            )
            .await
    }
}
