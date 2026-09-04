use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_plan_prices, organization_plans, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

const MAX_AUDIT_RETENTION_DAYS: i64 = 3_650;
const MAX_VALIDITY_DAYS: i64 = 3_650;

/// 创建企业套餐目录和不可变价格快照表。
pub(crate) async fn create_organization_plan_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_plans(manager).await?;
    create_prices(manager).await?;
    for index in [
        Index::create()
            .name("uq_organization_plans_plan_key")
            .table(organization_plans::Entity)
            .col(organization_plans::Column::PlanKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_organization_plans_status_id")
            .table(organization_plans::Entity)
            .col(organization_plans::Column::Status)
            .col(organization_plans::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_organization_plans_creator")
            .table(organization_plans::Entity)
            .col(organization_plans::Column::CreatedByUserId)
            .col(organization_plans::Column::Id)
            .to_owned(),
        Index::create()
            .name("uq_organization_plan_prices_plan")
            .table(organization_plan_prices::Entity)
            .col(organization_plan_prices::Column::PlanId)
            .unique()
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_plans(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_plans::Entity);
    statement
        .col(auto_id(organization_plans::Column::Id))
        .col(
            ColumnDef::new(organization_plans::Column::PlanKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::Name)
                .string_len(80)
                .not_null(),
        )
        .col(ColumnDef::new(organization_plans::Column::Description).string_len(240))
        .col(
            ColumnDef::new(organization_plans::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(
            ColumnDef::new(organization_plans::Column::TeamManagementEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(organization_plans::Column::SeatLimit)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::TeamLimit)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::OrganizationKeyLimit)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::AuditRetentionDays)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::SsoEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(organization_plans::Column::CustomRolesEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(organization_plans::Column::ValidityDays)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plans::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(nullable_timestamp(
            manager,
            organization_plans::Column::DisabledAt,
        ))
        .col(timestamp(manager, organization_plans::Column::CreatedAt))
        .col(timestamp(manager, organization_plans::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_plans_creator")
                .from(
                    organization_plans::Entity,
                    organization_plans::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(plan_key_check(manager.get_database_backend()))
        .check(Expr::col(organization_plans::Column::Name).ne(""))
        .check(Expr::col(organization_plans::Column::Status).is_in([1_i16, 2_i16]))
        .check(Expr::col(organization_plans::Column::SeatLimit).gt(0_i64))
        .check(Expr::col(organization_plans::Column::TeamLimit).gt(0_i64))
        .check(Expr::col(organization_plans::Column::OrganizationKeyLimit).gt(0_i64))
        .check(
            Expr::col(organization_plans::Column::AuditRetentionDays)
                .between(1_i64, MAX_AUDIT_RETENTION_DAYS),
        )
        .check(
            Expr::col(organization_plans::Column::ValidityDays).between(1_i64, MAX_VALIDITY_DAYS),
        )
        .check(Expr::col(organization_plans::Column::Version).gt(0_i64))
        .check(
            Expr::col(organization_plans::Column::Status)
                .eq(1_i16)
                .and(Expr::col(organization_plans::Column::DisabledAt).is_null())
                .or(Expr::col(organization_plans::Column::Status)
                    .eq(2_i16)
                    .and(Expr::col(organization_plans::Column::DisabledAt).is_not_null())),
        )
        .check(
            Expr::col(organization_plans::Column::DisabledAt)
                .is_null()
                .or(Expr::col(organization_plans::Column::DisabledAt)
                    .gte(Expr::col(organization_plans::Column::CreatedAt))),
        )
        .check(
            Expr::col(organization_plans::Column::UpdatedAt)
                .gte(Expr::col(organization_plans::Column::CreatedAt)),
        );
    manager.create_table(statement).await
}

async fn create_prices(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_plan_prices::Entity);
    statement
        .col(auto_id(organization_plan_prices::Column::Id))
        .col(
            ColumnDef::new(organization_plan_prices::Column::PlanId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_prices::Column::Provider)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_prices::Column::Currency)
                .string_len(3)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_plan_prices::Column::AmountMinor)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            organization_plan_prices::Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_plan_prices_plan")
                .from(
                    organization_plan_prices::Entity,
                    organization_plan_prices::Column::PlanId,
                )
                .to(organization_plans::Entity, organization_plans::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(Expr::col(organization_plan_prices::Column::Provider).ne(""))
        .check(Expr::col(organization_plan_prices::Column::AmountMinor).gt(0_i64));
    manager.create_table(statement).await
}

fn plan_key_check(database_backend: DbBackend) -> SimpleExpr {
    let zero = "0".repeat(32);
    let sql = match database_backend {
        DbBackend::Postgres => {
            format!(r#""plan_key" ~ '^[0-9a-f]{{32}}$' AND "plan_key" <> '{zero}'"#)
        }
        DbBackend::MySql => format!(
            "CHAR_LENGTH(`plan_key`) = 32 AND `plan_key` REGEXP '^[0-9a-f]{{32}}$' AND `plan_key` <> '{zero}'"
        ),
        DbBackend::Sqlite => format!(
            r#"length("plan_key") = 32 AND "plan_key" NOT GLOB '*[^0-9a-f]*' AND "plan_key" <> '{zero}'"#
        ),
    };
    Expr::cust(sql)
}
