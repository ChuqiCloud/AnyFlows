use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_credit_terms, organizations, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

const CREDIT_TERM_SCOPE_VERSION_INDEX: &str = "uq_organization_credit_terms_scope_version";
const CREDIT_TERM_LOOKUP_INDEX: &str = "idx_organization_credit_terms_lookup";
const MAX_NET_DAYS: i64 = 3_660;

/// 创建独立的企业授信/账期策略事实表及查询索引。
pub(in crate::migration) async fn create_organization_credit_terms(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let statement = table(manager, organization_credit_terms::Entity)
        .if_not_exists()
        .col(auto_id(organization_credit_terms::Column::Id))
        .col(
            ColumnDef::new(organization_credit_terms::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_terms::Column::Currency)
                .char_len(3)
                .not_null()
                .check(Expr::col(organization_credit_terms::Column::Currency).ne("")),
        )
        .col(
            ColumnDef::new(organization_credit_terms::Column::CreditLimit)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(Expr::col(organization_credit_terms::Column::CreditLimit).gte(0_i64)),
        )
        .col(
            ColumnDef::new(organization_credit_terms::Column::PeriodKind)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_credit_terms::Column::PeriodKind).between(1_i16, 2_i16),
                ),
        )
        .col(ColumnDef::new(organization_credit_terms::Column::PeriodDays).big_integer())
        .col(
            ColumnDef::new(organization_credit_terms::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_credit_terms::Column::Status).between(1_i16, 3_i16)),
        )
        .col(
            nullable_timestamp(manager, organization_credit_terms::Column::EffectiveFrom)
                .not_null(),
        )
        .col(nullable_timestamp(
            manager,
            organization_credit_terms::Column::EffectiveUntil,
        ))
        .col(
            ColumnDef::new(organization_credit_terms::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_credit_terms::Column::Version).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_credit_terms::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            organization_credit_terms::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_credit_terms::Column::UpdatedAt,
        ))
        // 币种不允许依赖连接排序规则，统一要求三位大写 ASCII 代码。
        .check(Expr::cust("UPPER(currency) = currency"))
        .check(
            Expr::col(organization_credit_terms::Column::PeriodDays)
                .is_null()
                .or(Expr::col(organization_credit_terms::Column::PeriodDays)
                    .between(1_i64, MAX_NET_DAYS)),
        )
        .check(
            Expr::col(organization_credit_terms::Column::PeriodKind)
                .eq(1_i16)
                .and(Expr::col(organization_credit_terms::Column::PeriodDays).is_not_null())
                .or(Expr::col(organization_credit_terms::Column::PeriodKind)
                    .eq(2_i16)
                    .and(Expr::col(organization_credit_terms::Column::PeriodDays).is_null())),
        )
        .check(
            Expr::col(organization_credit_terms::Column::EffectiveUntil)
                .is_null()
                .or(Expr::col(organization_credit_terms::Column::EffectiveUntil)
                    .gt(Expr::col(organization_credit_terms::Column::EffectiveFrom))),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_credit_terms_organization")
                .from(
                    organization_credit_terms::Entity,
                    organization_credit_terms::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_credit_terms_creator")
                .from(
                    organization_credit_terms::Entity,
                    organization_credit_terms::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(CREDIT_TERM_SCOPE_VERSION_INDEX)
            .table(organization_credit_terms::Entity)
            .col(organization_credit_terms::Column::OrganizationId)
            .col(organization_credit_terms::Column::Version)
            .unique()
            .to_owned(),
        Index::create()
            .name(CREDIT_TERM_LOOKUP_INDEX)
            .table(organization_credit_terms::Entity)
            .col(organization_credit_terms::Column::OrganizationId)
            .col(organization_credit_terms::Column::Status)
            .col(organization_credit_terms::Column::EffectiveFrom)
            .col(organization_credit_terms::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}
