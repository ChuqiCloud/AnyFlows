use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_credit_invoices, organization_credit_repayment_allocations,
    organization_credit_repayments, organizations,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

const INVOICE_SCOPE_INDEX: &str = "uq_org_credit_invoices_scope_currency";
const REPAYMENT_KEY_INDEX: &str = "uq_org_credit_repayments_key";
const REPAYMENT_SOURCE_INDEX: &str = "uq_org_credit_repayments_source";
const REPAYMENT_SCOPE_INDEX: &str = "uq_org_credit_repayments_scope_currency";
const ALLOCATION_KEY_INDEX: &str = "uq_org_credit_allocations_key";
const ALLOCATION_REPAYMENT_INDEX: &str = "idx_org_credit_allocations_repayment";
const ALLOCATION_INVOICE_INDEX: &str = "idx_org_credit_allocations_invoice";

/// 创建企业已确认还款与追加式核销分配事实。
pub(in crate::migration) async fn create_organization_credit_repayments(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_repayments(manager).await?;
    create_invoice_scope_index(manager).await?;
    create_allocations(manager).await
}

/// 按依赖逆序删除企业还款与核销事实。
pub(in crate::migration) async fn drop_organization_credit_repayments(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_credit_repayment_allocations::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(organization_credit_repayments::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_index(
            Index::drop()
                .name(INVOICE_SCOPE_INDEX)
                .table(organization_credit_invoices::Entity)
                .to_owned(),
        )
        .await?;
    Ok(())
}

async fn create_repayments(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use organization_credit_repayments::Column;

    let statement = table(manager, organization_credit_repayments::Entity)
        .if_not_exists()
        .col(auto_id(Column::Id))
        .col(ColumnDef::new(Column::RepaymentKey).char_len(32).not_null())
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::SourceKind)
                .small_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::SourceKey).char_len(32).not_null())
        .col(ColumnDef::new(Column::Currency).char_len(3).not_null())
        .col(ColumnDef::new(Column::AmountMinor).big_integer().not_null())
        .col(nullable_timestamp(manager, Column::ReceivedAt).not_null())
        .col(timestamp(manager, Column::CreatedAt))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::RepaymentKey,
            true,
        ))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::SourceKey,
            true,
        ))
        .check(Expr::col(Column::SourceKind).between(1_i16, 2_i16))
        .check(Expr::cust("UPPER(currency) = currency"))
        .check(Expr::col(Column::AmountMinor).gt(0_i64))
        .check(timestamp_lte_check(
            manager.get_database_backend(),
            Column::ReceivedAt,
            Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_credit_repayments_organization")
                .from(
                    organization_credit_repayments::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(REPAYMENT_KEY_INDEX)
            .table(organization_credit_repayments::Entity)
            .col(Column::RepaymentKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(REPAYMENT_SOURCE_INDEX)
            .table(organization_credit_repayments::Entity)
            .col(Column::OrganizationId)
            .col(Column::SourceKind)
            .col(Column::SourceKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(REPAYMENT_SCOPE_INDEX)
            .table(organization_credit_repayments::Entity)
            .col(Column::OrganizationId)
            .col(Column::Id)
            .col(Column::Currency)
            .unique()
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_invoice_scope_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name(INVOICE_SCOPE_INDEX)
                .table(organization_credit_invoices::Entity)
                .col(organization_credit_invoices::Column::OrganizationId)
                .col(organization_credit_invoices::Column::Id)
                .col(organization_credit_invoices::Column::Currency)
                .unique()
                .to_owned(),
        )
        .await
}

async fn create_allocations(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use organization_credit_repayment_allocations::Column;

    let statement = table(manager, organization_credit_repayment_allocations::Entity)
        .if_not_exists()
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::AllocationKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RepaymentId).big_integer().not_null())
        .col(ColumnDef::new(Column::InvoiceId).big_integer().not_null())
        .col(ColumnDef::new(Column::Currency).char_len(3).not_null())
        .col(ColumnDef::new(Column::AmountMinor).big_integer().not_null())
        .col(nullable_timestamp(manager, Column::AllocatedAt).not_null())
        .col(timestamp(manager, Column::CreatedAt))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::AllocationKey,
            true,
        ))
        .check(Expr::cust("UPPER(currency) = currency"))
        .check(Expr::col(Column::AmountMinor).gt(0_i64))
        .check(timestamp_lte_check(
            manager.get_database_backend(),
            Column::AllocatedAt,
            Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_credit_allocations_organization")
                .from(
                    organization_credit_repayment_allocations::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_credit_allocations_repayment")
                .from(
                    organization_credit_repayment_allocations::Entity,
                    (
                        Column::OrganizationId,
                        Column::RepaymentId,
                        Column::Currency,
                    ),
                )
                .to(
                    organization_credit_repayments::Entity,
                    (
                        organization_credit_repayments::Column::OrganizationId,
                        organization_credit_repayments::Column::Id,
                        organization_credit_repayments::Column::Currency,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_credit_allocations_invoice")
                .from(
                    organization_credit_repayment_allocations::Entity,
                    (Column::OrganizationId, Column::InvoiceId, Column::Currency),
                )
                .to(
                    organization_credit_invoices::Entity,
                    (
                        organization_credit_invoices::Column::OrganizationId,
                        organization_credit_invoices::Column::Id,
                        organization_credit_invoices::Column::Currency,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(ALLOCATION_KEY_INDEX)
            .table(organization_credit_repayment_allocations::Entity)
            .col(Column::AllocationKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(ALLOCATION_REPAYMENT_INDEX)
            .table(organization_credit_repayment_allocations::Entity)
            .col(Column::OrganizationId)
            .col(Column::RepaymentId)
            .col(Column::Id)
            .to_owned(),
        Index::create()
            .name(ALLOCATION_INVOICE_INDEX)
            .table(organization_credit_repayment_allocations::Entity)
            .col(Column::OrganizationId)
            .col(Column::InvoiceId)
            .col(Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn fixed_lower_hex_check<C>(backend: DbBackend, column: C, non_zero: bool) -> SimpleExpr
where
    C: Iden,
{
    let name = Iden::to_string(&column);
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

/// SQLite 的时间列按 RFC3339 文本保存，跨列约束必须转换为时间值后比较。
fn timestamp_lte_check<L, R>(backend: DbBackend, left: L, right: R) -> SimpleExpr
where
    L: Iden + 'static,
    R: Iden + 'static,
{
    if backend != DbBackend::Sqlite {
        return Expr::col(left).lte(Expr::col(right));
    }
    let left: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(left))
        .into();
    let right: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(right))
        .into();
    left.clone()
        .is_not_null()
        .and(right.clone().is_not_null())
        .and(left.lte(right))
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{Query, SqliteQueryBuilder};

    use super::*;

    #[test]
    fn sqlite_timestamp_checks_use_chronological_comparison() {
        let query = Query::select()
            .expr(timestamp_lte_check(
                DbBackend::Sqlite,
                organization_credit_repayments::Column::ReceivedAt,
                organization_credit_repayments::Column::CreatedAt,
            ))
            .to_owned();
        let sql = query.to_string(SqliteQueryBuilder);

        assert!(sql.contains("julianday(\"received_at\")"));
        assert!(sql.contains("julianday(\"created_at\")"));
        assert_eq!(sql.matches("IS NOT NULL").count(), 2);
    }
}
