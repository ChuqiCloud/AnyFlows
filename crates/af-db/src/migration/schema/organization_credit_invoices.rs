use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_credit_invoices, organizations};

use super::{auto_id, nullable_timestamp, table, timestamp};

const INVOICE_KEY_INDEX: &str = "uq_organization_credit_invoices_key";
const INVOICE_SOURCE_INDEX: &str = "uq_organization_credit_invoices_source";
const INVOICE_DUE_INDEX: &str = "idx_organization_credit_invoices_due";

/// 创建企业账期账单事实表；来源正文和资金动作均留在表外。
pub(in crate::migration) async fn create_organization_credit_invoices(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let statement = table(manager, organization_credit_invoices::Entity)
        .if_not_exists()
        .col(auto_id(organization_credit_invoices::Column::Id))
        .col(
            ColumnDef::new(organization_credit_invoices::Column::InvoiceKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::SourceKind)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::SourceKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::Currency)
                .char_len(3)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::AmountMinor)
                .big_integer()
                .not_null()
                .default(0_i64),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::TermVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::PeriodKind)
                .small_integer()
                .not_null(),
        )
        .col(ColumnDef::new(organization_credit_invoices::Column::PeriodDays).big_integer())
        .col(nullable_timestamp(manager, organization_credit_invoices::Column::IssuedAt).not_null())
        .col(nullable_timestamp(manager, organization_credit_invoices::Column::DueAt).not_null())
        .col(
            ColumnDef::new(organization_credit_invoices::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(
            nullable_timestamp(
                manager,
                organization_credit_invoices::Column::StatusChangedAt,
            )
            .not_null(),
        )
        .col(
            ColumnDef::new(organization_credit_invoices::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(nullable_timestamp(
            manager,
            organization_credit_invoices::Column::SettledAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_credit_invoices::Column::VoidedAt,
        ))
        .col(timestamp(
            manager,
            organization_credit_invoices::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_credit_invoices::Column::UpdatedAt,
        ))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            organization_credit_invoices::Column::InvoiceKey,
            true,
        ))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            organization_credit_invoices::Column::SourceKey,
            true,
        ))
        .check(Expr::col(organization_credit_invoices::Column::SourceKind).between(1_i16, 2_i16))
        .check(Expr::cust("UPPER(currency) = currency"))
        .check(Expr::col(organization_credit_invoices::Column::AmountMinor).gte(0_i64))
        .check(Expr::col(organization_credit_invoices::Column::TermVersion).gt(0_i64))
        .check(
            Expr::col(organization_credit_invoices::Column::PeriodKind)
                .eq(1_i16)
                .and(Expr::col(organization_credit_invoices::Column::PeriodDays).is_not_null())
                .or(Expr::col(organization_credit_invoices::Column::PeriodKind)
                    .eq(2_i16)
                    .and(Expr::col(organization_credit_invoices::Column::PeriodDays).is_null())),
        )
        .check(
            Expr::col(organization_credit_invoices::Column::PeriodDays)
                .is_null()
                .or(Expr::col(organization_credit_invoices::Column::PeriodDays)
                    .between(1_i64, 3_660_i64)),
        )
        .check(
            Expr::col(organization_credit_invoices::Column::DueAt)
                .gt(Expr::col(organization_credit_invoices::Column::IssuedAt)),
        )
        .check(
            Expr::col(organization_credit_invoices::Column::StatusChangedAt)
                .gte(Expr::col(organization_credit_invoices::Column::IssuedAt)),
        )
        .check(Expr::col(organization_credit_invoices::Column::Status).between(1_i16, 4_i16))
        .check(
            Expr::col(organization_credit_invoices::Column::Status)
                .is_in([1_i16, 2_i16])
                .and(Expr::col(organization_credit_invoices::Column::SettledAt).is_null())
                .and(Expr::col(organization_credit_invoices::Column::VoidedAt).is_null())
                .or(Expr::col(organization_credit_invoices::Column::Status)
                    .eq(3_i16)
                    .and(Expr::col(organization_credit_invoices::Column::SettledAt).is_not_null())
                    .and(Expr::col(organization_credit_invoices::Column::VoidedAt).is_null()))
                .or(Expr::col(organization_credit_invoices::Column::Status)
                    .eq(4_i16)
                    .and(Expr::col(organization_credit_invoices::Column::SettledAt).is_null())
                    .and(Expr::col(organization_credit_invoices::Column::VoidedAt).is_not_null())),
        )
        .check(
            Expr::col(organization_credit_invoices::Column::SettledAt)
                .is_null()
                .or(Expr::col(organization_credit_invoices::Column::SettledAt)
                    .gte(Expr::col(organization_credit_invoices::Column::IssuedAt))),
        )
        .check(
            Expr::col(organization_credit_invoices::Column::VoidedAt)
                .is_null()
                .or(Expr::col(organization_credit_invoices::Column::VoidedAt)
                    .gte(Expr::col(organization_credit_invoices::Column::IssuedAt))),
        )
        .check(Expr::col(organization_credit_invoices::Column::Version).gt(0_i64))
        .check(
            Expr::col(organization_credit_invoices::Column::UpdatedAt)
                .gte(Expr::col(organization_credit_invoices::Column::CreatedAt)),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_credit_invoices_organization")
                .from(
                    organization_credit_invoices::Entity,
                    organization_credit_invoices::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(INVOICE_KEY_INDEX)
            .table(organization_credit_invoices::Entity)
            .col(organization_credit_invoices::Column::InvoiceKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(INVOICE_SOURCE_INDEX)
            .table(organization_credit_invoices::Entity)
            .col(organization_credit_invoices::Column::OrganizationId)
            .col(organization_credit_invoices::Column::SourceKind)
            .col(organization_credit_invoices::Column::SourceKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(INVOICE_DUE_INDEX)
            .table(organization_credit_invoices::Entity)
            .col(organization_credit_invoices::Column::OrganizationId)
            .col(organization_credit_invoices::Column::Status)
            .col(organization_credit_invoices::Column::DueAt)
            .col(organization_credit_invoices::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn fixed_lower_hex_check(
    backend: DbBackend,
    column: organization_credit_invoices::Column,
    non_zero: bool,
) -> SimpleExpr {
    let name = column.to_string();
    let zeros = "0".repeat(32);
    match backend {
        DbBackend::Postgres => Expr::cust(format!(
            "\"{name}\" ~ '^[0-9a-f]{{32}}$'{}",
            if non_zero {
                format!(" AND \"{name}\" <> '{zeros}'")
            } else {
                String::new()
            }
        )),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{name}`) = 32 AND `{name}` REGEXP '^[0-9a-f]{{32}}$'{}",
            if non_zero {
                format!(" AND `{name}` <> '{zeros}'")
            } else {
                String::new()
            }
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(\"{name}\") = 32 AND \"{name}\" NOT GLOB '*[^0-9a-f]*'{}",
            if non_zero {
                format!(" AND \"{name}\" <> '{zeros}'")
            } else {
                String::new()
            }
        )),
    }
}

/// 按外键和索引依赖逆序删除账单事实表。
pub(in crate::migration) async fn drop_organization_credit_invoices(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_credit_invoices::Entity)
                .to_owned(),
        )
        .await
}
