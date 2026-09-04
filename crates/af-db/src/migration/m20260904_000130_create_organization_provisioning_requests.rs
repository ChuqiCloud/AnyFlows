use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::{organization_provisioning_requests, organizations, users},
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

const REQUEST_KEY_INDEX: &str = "uq_org_provision_requests_key";
const USER_IDEMPOTENCY_INDEX: &str = "uq_org_provision_requests_user_idempotency";
const ACTIVE_USER_INDEX: &str = "uq_org_provision_requests_active_user";
const ACTIVE_SLUG_INDEX: &str = "uq_org_provision_requests_active_slug";
const USER_CREATED_INDEX: &str = "idx_org_provision_requests_user_created";
const STATUS_CREATED_INDEX: &str = "idx_org_provision_requests_status_created";

/// 创建企业开通申请事实；审批与企业创建由后续受信事务接入。
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use organization_provisioning_requests::Column;

        let statement = table(manager, organization_provisioning_requests::Entity)
            .col(auto_id(Column::Id))
            .col(ColumnDef::new(Column::RequestKey).char_len(32).not_null())
            .col(
                ColumnDef::new(Column::IdempotencyKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Column::ApplicantUserId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Column::OrganizationName)
                    .string_len(128)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Column::OrganizationSlug)
                    .string_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Column::BusinessReason)
                    .string_len(2_000)
                    .not_null(),
            )
            .col(ColumnDef::new(Column::Status).small_integer().not_null())
            .col(ColumnDef::new(Column::ActiveSlot).small_integer())
            .col(
                ColumnDef::new(Column::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(ColumnDef::new(Column::ReviewerUserId).big_integer())
            .col(ColumnDef::new(Column::DecisionReason).string_len(512))
            .col(ColumnDef::new(Column::OrganizationId).big_integer())
            .col(timestamp(manager, Column::CreatedAt))
            .col(timestamp(manager, Column::UpdatedAt))
            .col(nullable_timestamp(manager, Column::DecidedAt))
            .check(fixed_lower_hex_check(
                manager.get_database_backend(),
                Column::RequestKey,
            ))
            .check(fixed_lower_hex_check(
                manager.get_database_backend(),
                Column::IdempotencyKey,
            ))
            .check(text_shape_check(
                manager.get_database_backend(),
                Column::OrganizationName,
                128,
            ))
            .check(slug_check(manager.get_database_backend()))
            .check(text_shape_check(
                manager.get_database_backend(),
                Column::BusinessReason,
                2_000,
            ))
            .check(
                Expr::col(Column::DecisionReason)
                    .is_null()
                    .or(text_shape_check(
                        manager.get_database_backend(),
                        Column::DecisionReason,
                        512,
                    )),
            )
            .check(Expr::col(Column::Status).between(1_i16, 3_i16))
            .check(Expr::col(Column::Version).gt(0_i64))
            .check(status_shape_check())
            .check(timestamp_lte_check(
                manager.get_database_backend(),
                Column::CreatedAt,
                Column::UpdatedAt,
            ))
            .check(optional_timestamp_range_check(
                manager.get_database_backend(),
                Column::CreatedAt,
                Column::DecidedAt,
                Column::UpdatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_provision_requests_applicant")
                    .from(
                        organization_provisioning_requests::Entity,
                        Column::ApplicantUserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_provision_requests_reviewer")
                    .from(
                        organization_provisioning_requests::Entity,
                        Column::ReviewerUserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_provision_requests_organization")
                    .from(
                        organization_provisioning_requests::Entity,
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
                .name(REQUEST_KEY_INDEX)
                .table(organization_provisioning_requests::Entity)
                .col(Column::RequestKey)
                .unique()
                .to_owned(),
            Index::create()
                .name(USER_IDEMPOTENCY_INDEX)
                .table(organization_provisioning_requests::Entity)
                .col(Column::ApplicantUserId)
                .col(Column::IdempotencyKey)
                .unique()
                .to_owned(),
            Index::create()
                .name(ACTIVE_USER_INDEX)
                .table(organization_provisioning_requests::Entity)
                .col(Column::ApplicantUserId)
                .col(Column::ActiveSlot)
                .unique()
                .to_owned(),
            Index::create()
                .name(ACTIVE_SLUG_INDEX)
                .table(organization_provisioning_requests::Entity)
                .col(Column::OrganizationSlug)
                .col(Column::ActiveSlot)
                .unique()
                .to_owned(),
            Index::create()
                .name(USER_CREATED_INDEX)
                .table(organization_provisioning_requests::Entity)
                .col(Column::ApplicantUserId)
                .col(Column::CreatedAt)
                .col(Column::Id)
                .to_owned(),
            Index::create()
                .name(STATUS_CREATED_INDEX)
                .table(organization_provisioning_requests::Entity)
                .col(Column::Status)
                .col(Column::CreatedAt)
                .col(Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_provisioning_requests::Entity)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

fn fixed_lower_hex_check<C>(backend: DbBackend, column: C) -> SimpleExpr
where
    C: Iden,
{
    let name = Iden::to_string(&column);
    let zero = "0".repeat(32);
    match backend {
        DbBackend::Postgres => Expr::cust(format!(
            "\"{name}\" ~ '^[0-9a-f]{{32}}$' AND \"{name}\" <> '{zero}'"
        )),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{name}`) = 32 AND `{name}` REGEXP '^[0-9a-f]{{32}}$' AND `{name}` <> '{zero}'"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(\"{name}\") = 32 AND \"{name}\" NOT GLOB '*[^0-9a-f]*' AND \"{name}\" <> '{zero}'"
        )),
    }
}

fn text_shape_check<C>(backend: DbBackend, column: C, maximum_bytes: usize) -> SimpleExpr
where
    C: Iden,
{
    let name = Iden::to_string(&column);
    match backend {
        DbBackend::Postgres => Expr::cust(format!(
            "octet_length(\"{name}\") BETWEEN 1 AND {maximum_bytes} AND \"{name}\" = btrim(\"{name}\")"
        )),
        DbBackend::MySql => Expr::cust(format!(
            "OCTET_LENGTH(`{name}`) BETWEEN 1 AND {maximum_bytes} AND `{name}` = TRIM(`{name}`)"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(CAST(\"{name}\" AS BLOB)) BETWEEN 1 AND {maximum_bytes} AND \"{name}\" = trim(\"{name}\")"
        )),
    }
}

fn slug_check(backend: DbBackend) -> SimpleExpr {
    match backend {
        DbBackend::Postgres => Expr::cust(
            "octet_length(\"organization_slug\") BETWEEN 1 AND 64 AND \"organization_slug\" ~ '^[a-z0-9]+(-[a-z0-9]+)*$'",
        ),
        DbBackend::MySql => Expr::cust(
            "OCTET_LENGTH(`organization_slug`) BETWEEN 1 AND 64 AND `organization_slug` REGEXP '^[a-z0-9]+(-[a-z0-9]+)*$'",
        ),
        DbBackend::Sqlite => Expr::cust(
            "length(CAST(\"organization_slug\" AS BLOB)) BETWEEN 1 AND 64 AND \"organization_slug\" NOT GLOB '*[^a-z0-9-]*' AND \"organization_slug\" NOT LIKE '-%' AND \"organization_slug\" NOT LIKE '%-' AND \"organization_slug\" NOT LIKE '%--%'",
        ),
    }
}

fn status_shape_check() -> SimpleExpr {
    Expr::cust(
        "(status = 1 AND active_slot = 1 AND reviewer_user_id IS NULL AND decision_reason IS NULL AND organization_id IS NULL AND decided_at IS NULL) OR (status = 2 AND active_slot IS NULL AND reviewer_user_id IS NOT NULL AND organization_id IS NOT NULL AND decided_at IS NOT NULL) OR (status = 3 AND active_slot IS NULL AND reviewer_user_id IS NOT NULL AND decision_reason IS NOT NULL AND organization_id IS NULL AND decided_at IS NOT NULL)",
    )
}

/// SQLite 将时间保存为文本，跨列约束需要转换为真实时间后比较。
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

fn optional_timestamp_range_check<L, V, R>(
    backend: DbBackend,
    left: L,
    value: V,
    right: R,
) -> SimpleExpr
where
    L: Iden + Clone + 'static,
    V: Iden + Clone + 'static,
    R: Iden + Clone + 'static,
{
    Expr::col(value.clone())
        .is_null()
        .or(timestamp_lte_check(backend, left, value.clone())
            .and(timestamp_lte_check(backend, value, right)))
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, Query, SqliteQueryBuilder};

    use super::*;

    #[test]
    fn constraint_sql_is_dialect_specific_and_preserves_byte_limits() {
        let postgres = Query::select()
            .expr(text_shape_check(
                DbBackend::Postgres,
                organization_provisioning_requests::Column::BusinessReason,
                2_000,
            ))
            .to_owned()
            .to_string(PostgresQueryBuilder);
        let mysql = Query::select()
            .expr(slug_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        let sqlite = Query::select()
            .expr(fixed_lower_hex_check(
                DbBackend::Sqlite,
                organization_provisioning_requests::Column::RequestKey,
            ))
            .to_owned()
            .to_string(SqliteQueryBuilder);

        assert!(postgres.contains("octet_length"));
        assert!(postgres.contains("2000"));
        assert!(mysql.contains("REGEXP"));
        assert!(mysql.contains("64"));
        assert!(sqlite.contains("NOT GLOB"));
        assert!(sqlite.contains(&"0".repeat(32)));
    }
}
