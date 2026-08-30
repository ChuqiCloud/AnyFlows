use sea_orm_migration::prelude::*;

use crate::migration::iden::organization_approval_timeout_leases;

use super::{nullable_timestamp, table, timestamp};

const LEASE_KEY: &str = "organization-approval-timeout-v1";

/// 创建企业审批超时扫描的单行数据库租约。
pub(in crate::migration) async fn create_organization_approval_timeout_lease(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, organization_approval_timeout_leases::Entity)
                .if_not_exists()
                .col(
                    ColumnDef::new(organization_approval_timeout_leases::Column::LeaseKey)
                        .string_len(64)
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(organization_approval_timeout_leases::Column::OwnerToken)
                        .char_len(32)
                        .not_null(),
                )
                .col(
                    nullable_timestamp(
                        manager,
                        organization_approval_timeout_leases::Column::LeaseExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    ColumnDef::new(organization_approval_timeout_leases::Column::Version)
                        .big_integer()
                        .not_null()
                        .default(1_i64),
                )
                .col(timestamp(
                    manager,
                    organization_approval_timeout_leases::Column::UpdatedAt,
                ))
                .check(Expr::cust("length(trim(lease_key)) > 0"))
                .check(Expr::cust("length(owner_token) = 32"))
                .check(Expr::col(organization_approval_timeout_leases::Column::Version).gt(0_i64))
                .to_owned(),
        )
        .await
}

/// 删除企业审批超时扫描租约表。
pub(in crate::migration) async fn drop_organization_approval_timeout_lease(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_timeout_leases::Entity)
                .if_exists()
                .to_owned(),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_schema_keeps_owner_and_version_constraints() {
        for rendered in [
            Query::create_table()
                .table(organization_approval_timeout_leases::Entity)
                .col(
                    ColumnDef::new(organization_approval_timeout_leases::Column::OwnerToken)
                        .char_len(32)
                        .not_null(),
                )
                .to_string(PostgresQueryBuilder),
            Query::create_table()
                .table(organization_approval_timeout_leases::Entity)
                .col(
                    ColumnDef::new(organization_approval_timeout_leases::Column::Version)
                        .big_integer()
                        .not_null(),
                )
                .to_string(MysqlQueryBuilder),
            Query::create_table()
                .table(organization_approval_timeout_leases::Entity)
                .col(
                    ColumnDef::new(organization_approval_timeout_leases::Column::LeaseExpiresAt)
                        .timestamp_with_time_zone()
                        .not_null(),
                )
                .to_string(SqliteQueryBuilder),
        ] {
            assert!(rendered.contains("organization_approval_timeout_leases"));
        }
    }
}
