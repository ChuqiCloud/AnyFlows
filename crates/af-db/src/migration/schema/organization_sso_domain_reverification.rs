use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_sso_domain_reverification_leases, organization_sso_domains,
};

use super::{nullable_timestamp, table, timestamp};

/// 创建企业 SSO 域名复验租约表，避免多实例重复扫描。
pub(in crate::migration) async fn create_organization_sso_domain_reverification(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(
                manager,
                organization_sso_domain_reverification_leases::Entity,
            )
            .if_not_exists()
            .col(
                ColumnDef::new(organization_sso_domain_reverification_leases::Column::LeaseKey)
                    .string_len(64)
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(organization_sso_domain_reverification_leases::Column::OwnerToken)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                nullable_timestamp(
                    manager,
                    organization_sso_domain_reverification_leases::Column::LeaseExpiresAt,
                )
                .not_null(),
            )
            .col(
                ColumnDef::new(organization_sso_domain_reverification_leases::Column::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(timestamp(
                manager,
                organization_sso_domain_reverification_leases::Column::UpdatedAt,
            ))
            .check(Expr::cust("length(trim(lease_key)) > 0"))
            .check(Expr::cust("length(owner_token) = 32"))
            .check(
                Expr::col(organization_sso_domain_reverification_leases::Column::Version).gt(0_i64),
            )
            .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_organization_sso_domains_reverification")
                .table(organization_sso_domains::Entity)
                .col(organization_sso_domains::Column::Status)
                .col(organization_sso_domains::Column::RecheckAt)
                .col(organization_sso_domains::Column::Id)
                .to_owned(),
        )
        .await
}

/// 删除企业 SSO 域名复验租约表。
pub(in crate::migration) async fn drop_organization_sso_domain_reverification(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_index(
            Index::drop()
                .name("idx_organization_sso_domains_reverification")
                .table(organization_sso_domains::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(organization_sso_domain_reverification_leases::Entity)
                .if_exists()
                .to_owned(),
        )
        .await
}
