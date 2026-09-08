use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_entitlements, organizations};

use super::{auto_id, nullable_timestamp, timestamp};

const ORGANIZATION_ENTITLEMENT_SCOPE_INDEX: &str = "uq_organization_entitlements_organization";
const ORGANIZATION_ENTITLEMENT_VALIDITY_INDEX: &str = "idx_organization_entitlements_validity";
const MAX_AUDIT_RETENTION_DAYS: i64 = 3_650;

/// 创建每个企业唯一的当前商业授权记录。
pub(in crate::migration) async fn create_organization_entitlements(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = super::table(manager, organization_entitlements::Entity);
    statement
        .col(auto_id(organization_entitlements::Column::Id))
        .col(
            ColumnDef::new(organization_entitlements::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::TeamManagementEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::SeatLimit)
                .big_integer()
                .not_null()
                .check(Expr::col(organization_entitlements::Column::SeatLimit).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::TeamLimit)
                .big_integer()
                .not_null()
                .check(Expr::col(organization_entitlements::Column::TeamLimit).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::OrganizationKeyLimit)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(organization_entitlements::Column::OrganizationKeyLimit).gt(0_i64),
                ),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::AuditRetentionDays)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(organization_entitlements::Column::AuditRetentionDays)
                        .between(1_i64, MAX_AUDIT_RETENTION_DAYS),
                ),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::SsoEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(organization_entitlements::Column::CustomRolesEnabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(timestamp(
            manager,
            organization_entitlements::Column::ValidFrom,
        ))
        .col(timestamp(
            manager,
            organization_entitlements::Column::ValidUntil,
        ))
        .col(nullable_timestamp(
            manager,
            organization_entitlements::Column::GraceUntil,
        ))
        .col(
            ColumnDef::new(organization_entitlements::Column::Source)
                .string_len(64)
                .not_null()
                .check(Expr::col(organization_entitlements::Column::Source).ne("")),
        )
        .col(ColumnDef::new(organization_entitlements::Column::ExternalReference).string_len(128))
        .col(
            ColumnDef::new(organization_entitlements::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_entitlements::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_entitlements::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_entitlements::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_entitlements_organization")
                .from(
                    organization_entitlements::Entity,
                    organization_entitlements::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(
            Expr::col(organization_entitlements::Column::ValidFrom)
                .lt(Expr::col(organization_entitlements::Column::ValidUntil)),
        )
        .check(
            Expr::col(organization_entitlements::Column::GraceUntil)
                .is_null()
                .or(Expr::col(organization_entitlements::Column::GraceUntil)
                    .gte(Expr::col(organization_entitlements::Column::ValidUntil))),
        );
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(ORGANIZATION_ENTITLEMENT_SCOPE_INDEX)
            .table(organization_entitlements::Entity)
            .col(organization_entitlements::Column::OrganizationId)
            .unique()
            .to_owned(),
        Index::create()
            .name(ORGANIZATION_ENTITLEMENT_VALIDITY_INDEX)
            .table(organization_entitlements::Entity)
            .col(organization_entitlements::Column::OrganizationId)
            .col(organization_entitlements::Column::ValidUntil)
            .col(organization_entitlements::Column::GraceUntil)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    backfill_existing_organizations(manager).await
}

async fn backfill_existing_organizations(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            r#"
INSERT INTO organization_entitlements
(organization_id, team_management_enabled, seat_limit, team_limit,
 organization_key_limit, audit_retention_days, sso_enabled, custom_roles_enabled,
 valid_from, valid_until, grace_until, source, external_reference, version,
 created_at, updated_at)
SELECT o.id, TRUE,
       GREATEST(5, (SELECT COUNT(*) FROM organization_memberships m
                    WHERE m.organization_id = o.id AND m.status = 1)),
       GREATEST(2, (SELECT COUNT(*) FROM organization_teams t
                    WHERE t.organization_id = o.id AND t.status = 1)),
       2, 90, FALSE, FALSE, '2000-01-01 00:00:00.000000',
       '2000-01-02 00:00:00.000000',
       '2000-01-02 00:00:00.000000',
       'migration', NULL, 1, CURRENT_TIMESTAMP(6), CURRENT_TIMESTAMP(6)
FROM organizations o
"#
        }
        DbBackend::Postgres => {
            r#"
INSERT INTO "organization_entitlements"
("organization_id", "team_management_enabled", "seat_limit", "team_limit",
 "organization_key_limit", "audit_retention_days", "sso_enabled", "custom_roles_enabled",
 "valid_from", "valid_until", "grace_until", "source", "external_reference", "version",
 "created_at", "updated_at")
SELECT o."id", TRUE,
       GREATEST(5, (SELECT COUNT(*) FROM "organization_memberships" m
                    WHERE m."organization_id" = o."id" AND m."status" = 1)),
       GREATEST(2, (SELECT COUNT(*) FROM "organization_teams" t
                    WHERE t."organization_id" = o."id" AND t."status" = 1)),
       2, 90, FALSE, FALSE, TIMESTAMPTZ '2000-01-01 00:00:00+00',
       TIMESTAMPTZ '2000-01-02 00:00:00+00',
       TIMESTAMPTZ '2000-01-02 00:00:00+00',
       'migration', NULL, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
FROM "organizations" o
"#
        }
        DbBackend::Sqlite => {
            r#"
INSERT INTO "organization_entitlements"
("organization_id", "team_management_enabled", "seat_limit", "team_limit",
 "organization_key_limit", "audit_retention_days", "sso_enabled", "custom_roles_enabled",
 "valid_from", "valid_until", "grace_until", "source", "external_reference", "version",
 "created_at", "updated_at")
SELECT o."id", 1,
       max(5, (SELECT COUNT(*) FROM "organization_memberships" m
               WHERE m."organization_id" = o."id" AND m."status" = 1)),
       max(2, (SELECT COUNT(*) FROM "organization_teams" t
               WHERE t."organization_id" = o."id" AND t."status" = 1)),
       2, 90, 0, 0, '2000-01-01 00:00:00+00:00',
       '2000-01-02 00:00:00+00:00', '2000-01-02 00:00:00+00:00',
       'migration', NULL, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
FROM "organizations" o
"#
        }
    };
    manager.get_connection().execute_unprepared(sql).await?;
    Ok(())
}
