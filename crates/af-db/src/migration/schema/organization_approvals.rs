use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_decisions, organization_approval_requests,
    organization_approval_templates, organizations, users,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

const TEMPLATE_SCOPE_VERSION_INDEX: &str = "uq_org_approval_templates_scope_version";
const TEMPLATE_SCOPE_ID_VERSION_INDEX: &str = "uq_org_approval_templates_scope_id_version";
const TEMPLATE_RESOURCE_INDEX: &str = "idx_org_approval_templates_resource";
const REQUEST_SCOPE_KEY_INDEX: &str = "uq_org_approval_requests_scope_key";
const REQUEST_SCOPE_ID_INDEX: &str = "uq_org_approval_requests_scope_id";
const REQUEST_EXPIRY_INDEX: &str = "idx_org_approval_requests_expiry";
const DECISION_SCOPE_KEY_INDEX: &str = "uq_org_approval_decisions_scope_key";
const DECISION_SCOPE_REQUEST_INDEX: &str = "uq_org_approval_decisions_scope_request";
const DECISION_CREATED_INDEX: &str = "idx_org_approval_decisions_created";

/// 创建企业审批模板版本、申请与单次决定事实。
pub(in crate::migration) async fn create_organization_approvals(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_templates(manager).await?;
    create_requests(manager).await?;
    create_decisions(manager).await
}

/// 按依赖逆序删除企业审批事实表。
pub(in crate::migration) async fn drop_organization_approvals(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_decisions::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_requests::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_templates::Entity)
                .if_exists()
                .to_owned(),
        )
        .await
}

async fn create_templates(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use organization_approval_templates::Column;

    let statement = table(manager, organization_approval_templates::Entity)
        .if_not_exists()
        .col(auto_id(Column::Id))
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::TemplateKey)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::ResourceType)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(
            ColumnDef::new(Column::DefinitionDigest)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(manager, Column::CreatedAt))
        .check(Expr::cust("length(trim(template_key)) > 0"))
        .check(Expr::col(Column::ResourceType).between(1_i16, 5_i16))
        .check(Expr::col(Column::Version).gt(0_i64))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::DefinitionDigest,
            64,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_templates_organization")
                .from(
                    organization_approval_templates::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_templates_creator")
                .from(
                    organization_approval_templates::Entity,
                    Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(TEMPLATE_SCOPE_VERSION_INDEX)
            .table(organization_approval_templates::Entity)
            .col(Column::OrganizationId)
            .col(Column::TemplateKey)
            .col(Column::Version)
            .unique()
            .to_owned(),
        Index::create()
            .name(TEMPLATE_SCOPE_ID_VERSION_INDEX)
            .table(organization_approval_templates::Entity)
            .col(Column::OrganizationId)
            .col(Column::Id)
            .col(Column::Version)
            .unique()
            .to_owned(),
        Index::create()
            .name(TEMPLATE_RESOURCE_INDEX)
            .table(organization_approval_templates::Entity)
            .col(Column::OrganizationId)
            .col(Column::ResourceType)
            .col(Column::TemplateKey)
            .col(Column::Version)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_requests(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use organization_approval_requests::Column;

    let statement = table(manager, organization_approval_requests::Entity)
        .if_not_exists()
        .col(auto_id(Column::Id))
        .col(ColumnDef::new(Column::RequestKey).char_len(32).not_null())
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::ResourceType)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::ResourceSummary)
                .string_len(1024)
                .not_null(),
        )
        .col(ColumnDef::new(Column::TemplateId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::TemplateVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::RequestedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(nullable_timestamp(manager, Column::ExpiresAt).not_null())
        .col(timestamp(manager, Column::CreatedAt))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::RequestKey,
            32,
        ))
        .check(Expr::col(Column::ResourceType).between(1_i16, 5_i16))
        .check(Expr::cust("length(trim(resource_summary)) > 0"))
        .check(Expr::col(Column::TemplateVersion).gt(0_i64))
        .check(timestamp_lte_check(
            manager.get_database_backend(),
            Column::CreatedAt,
            Column::ExpiresAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_requests_organization")
                .from(
                    organization_approval_requests::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_requests_template")
                .from(
                    organization_approval_requests::Entity,
                    (
                        Column::OrganizationId,
                        Column::TemplateId,
                        Column::TemplateVersion,
                    ),
                )
                .to(
                    organization_approval_templates::Entity,
                    (
                        organization_approval_templates::Column::OrganizationId,
                        organization_approval_templates::Column::Id,
                        organization_approval_templates::Column::Version,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_requests_requester")
                .from(
                    organization_approval_requests::Entity,
                    Column::RequestedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(REQUEST_SCOPE_KEY_INDEX)
            .table(organization_approval_requests::Entity)
            .col(Column::OrganizationId)
            .col(Column::RequestKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(REQUEST_SCOPE_ID_INDEX)
            .table(organization_approval_requests::Entity)
            .col(Column::OrganizationId)
            .col(Column::Id)
            .unique()
            .to_owned(),
        Index::create()
            .name(REQUEST_EXPIRY_INDEX)
            .table(organization_approval_requests::Entity)
            .col(Column::OrganizationId)
            .col(Column::ExpiresAt)
            .col(Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_decisions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use organization_approval_decisions::Column;

    let statement = table(manager, organization_approval_decisions::Entity)
        .if_not_exists()
        .col(auto_id(Column::Id))
        .col(ColumnDef::new(Column::DecisionKey).char_len(32).not_null())
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::DecisionKind)
                .small_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::DecidedByUserId).big_integer())
        .col(ColumnDef::new(Column::Reason).string_len(512))
        .col(nullable_timestamp(manager, Column::DecidedAt).not_null())
        .col(timestamp(manager, Column::CreatedAt))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            Column::DecisionKey,
            32,
        ))
        .check(Expr::col(Column::DecisionKind).between(1_i16, 3_i16))
        .check(
            Expr::col(Column::DecisionKind)
                .eq(3_i16)
                .and(Expr::col(Column::DecidedByUserId).is_null())
                .or(Expr::col(Column::DecisionKind)
                    .is_in([1_i16, 2_i16])
                    .and(Expr::col(Column::DecidedByUserId).is_not_null())),
        )
        .check(
            Expr::col(Column::Reason)
                .is_null()
                .or(Expr::cust("length(trim(reason)) > 0")),
        )
        .check(timestamp_lte_check(
            manager.get_database_backend(),
            Column::DecidedAt,
            Column::CreatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_decisions_organization")
                .from(
                    organization_approval_decisions::Entity,
                    Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_org_approval_decisions_request")
                .from(
                    organization_approval_decisions::Entity,
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
                .name("fk_org_approval_decisions_decider")
                .from(
                    organization_approval_decisions::Entity,
                    Column::DecidedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(DECISION_SCOPE_KEY_INDEX)
            .table(organization_approval_decisions::Entity)
            .col(Column::OrganizationId)
            .col(Column::DecisionKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(DECISION_SCOPE_REQUEST_INDEX)
            .table(organization_approval_decisions::Entity)
            .col(Column::OrganizationId)
            .col(Column::RequestId)
            .unique()
            .to_owned(),
        Index::create()
            .name(DECISION_CREATED_INDEX)
            .table(organization_approval_decisions::Entity)
            .col(Column::OrganizationId)
            .col(Column::CreatedAt)
            .col(Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn fixed_lower_hex_check<C>(backend: DbBackend, column: C, length: usize) -> SimpleExpr
where
    C: Iden,
{
    let name = Iden::to_string(&column);
    match backend {
        DbBackend::Postgres => Expr::cust(format!("\"{name}\" ~ '^[0-9a-f]{{{length}}}$'")),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{name}`) = {length} AND `{name}` REGEXP '^[0-9a-f]{{{length}}}$'"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(\"{name}\") = {length} AND \"{name}\" NOT GLOB '*[^0-9a-f]*'"
        )),
    }
}

/// SQLite 将时间列按 RFC3339 文本保存，跨列约束必须转换为时间值后比较。
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
                organization_approval_decisions::Column::DecidedAt,
                organization_approval_decisions::Column::CreatedAt,
            ))
            .to_owned();
        let sql = query.to_string(SqliteQueryBuilder);

        assert!(sql.contains("julianday(\"decided_at\")"));
        assert!(sql.contains("julianday(\"created_at\")"));
        assert_eq!(sql.matches("IS NOT NULL").count(), 2);
    }

    #[test]
    fn fact_keys_use_their_persisted_hex_width() {
        let query = Query::select()
            .expr(fixed_lower_hex_check(
                DbBackend::Sqlite,
                organization_approval_requests::Column::RequestKey,
                32,
            ))
            .to_owned();
        let sql = query.to_string(SqliteQueryBuilder);

        assert!(sql.contains("length(\"request_key\") = 32"));
        assert!(!sql.contains("= 64"));
    }
}
