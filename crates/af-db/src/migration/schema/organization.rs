use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_memberships, organization_teams, organizations, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

const ORGANIZATION_KEY_INDEX: &str = "uq_organizations_key";
const ORGANIZATION_SLUG_INDEX: &str = "uq_organizations_slug";
const ORGANIZATION_STATUS_INDEX: &str = "idx_organizations_status_id";
const TEAM_SCOPE_ID_INDEX: &str = "uq_organization_teams_scope_id";
const TEAM_SCOPE_NAME_INDEX: &str = "uq_organization_teams_scope_name";
const TEAM_STATUS_INDEX: &str = "idx_organization_teams_scope_status";
const MEMBERSHIP_SCOPE_USER_INDEX: &str = "uq_organization_memberships_scope_user";
const MEMBERSHIP_STATUS_INDEX: &str = "idx_organization_memberships_scope_status";
const MEMBERSHIP_TEAM_INDEX: &str = "idx_organization_memberships_scope_team_status";

/// 创建企业主体、扁平团队和成员关系三张基础表。
pub(in crate::migration) async fn create_organization_foundation(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_organizations(manager).await?;
    create_organization_teams(manager).await?;
    create_organization_memberships(manager).await
}

async fn create_organizations(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organizations::Entity);
    statement
        .col(auto_id(organizations::Column::Id))
        .col(
            ColumnDef::new(organizations::Column::OrganizationKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organizations::Column::Name)
                .string_len(128)
                .not_null()
                .check(Expr::col(organizations::Column::Name).ne("")),
        )
        .col(
            ColumnDef::new(organizations::Column::Slug)
                .string_len(64)
                .not_null()
                .check(Expr::col(organizations::Column::Slug).ne("")),
        )
        .col(
            ColumnDef::new(organizations::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organizations::Column::Status).between(1_i16, 3_i16)),
        )
        .col(
            ColumnDef::new(organizations::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organizations::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organizations::Column::Version).gt(0_i64)),
        )
        .col(timestamp(manager, organizations::Column::CreatedAt))
        .col(timestamp(manager, organizations::Column::UpdatedAt))
        .col(nullable_timestamp(manager, organizations::Column::ClosedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organizations_creator")
                .from(
                    organizations::Entity,
                    organizations::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(
            Expr::col(organizations::Column::ClosedAt)
                .is_null()
                .and(Expr::col(organizations::Column::Status).ne(3_i16))
                .or(Expr::col(organizations::Column::ClosedAt)
                    .is_not_null()
                    .and(Expr::col(organizations::Column::Status).eq(3_i16))),
        )
        .check(organization_key_format_check(
            manager.get_database_backend(),
        ));
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(ORGANIZATION_KEY_INDEX)
            .table(organizations::Entity)
            .col(organizations::Column::OrganizationKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(ORGANIZATION_SLUG_INDEX)
            .table(organizations::Entity)
            .col(organizations::Column::Slug)
            .unique()
            .to_owned(),
        Index::create()
            .name(ORGANIZATION_STATUS_INDEX)
            .table(organizations::Entity)
            .col(organizations::Column::Status)
            .col(organizations::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_organization_teams(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_teams::Entity);
    statement
        .col(auto_id(organization_teams::Column::Id))
        .col(
            ColumnDef::new(organization_teams::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_teams::Column::Name)
                .string_len(128)
                .not_null()
                .check(Expr::col(organization_teams::Column::Name).ne("")),
        )
        .col(ColumnDef::new(organization_teams::Column::Description).text())
        .col(
            ColumnDef::new(organization_teams::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_teams::Column::Status).between(1_i16, 2_i16)),
        )
        .col(
            ColumnDef::new(organization_teams::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_teams::Column::Version).gt(0_i64)),
        )
        .col(timestamp(manager, organization_teams::Column::CreatedAt))
        .col(timestamp(manager, organization_teams::Column::UpdatedAt))
        .col(nullable_timestamp(
            manager,
            organization_teams::Column::DeletedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_teams_organization")
                .from(
                    organization_teams::Entity,
                    organization_teams::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(
            Expr::col(organization_teams::Column::DeletedAt)
                .is_null()
                .and(Expr::col(organization_teams::Column::Status).eq(1_i16))
                .or(Expr::col(organization_teams::Column::DeletedAt)
                    .is_not_null()
                    .and(Expr::col(organization_teams::Column::Status).eq(2_i16))),
        );
    manager.create_table(statement).await?;

    // 复合唯一索引既固定企业内名称，也为成员的同企业团队复合外键提供引用目标。
    for index in [
        Index::create()
            .name(TEAM_SCOPE_ID_INDEX)
            .table(organization_teams::Entity)
            .col(organization_teams::Column::OrganizationId)
            .col(organization_teams::Column::Id)
            .unique()
            .to_owned(),
        Index::create()
            .name(TEAM_SCOPE_NAME_INDEX)
            .table(organization_teams::Entity)
            .col(organization_teams::Column::OrganizationId)
            .col(organization_teams::Column::Name)
            .unique()
            .to_owned(),
        Index::create()
            .name(TEAM_STATUS_INDEX)
            .table(organization_teams::Entity)
            .col(organization_teams::Column::OrganizationId)
            .col(organization_teams::Column::Status)
            .col(organization_teams::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_organization_memberships(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut joined_at = nullable_timestamp(manager, organization_memberships::Column::JoinedAt);
    joined_at.not_null();
    let mut statement = table(manager, organization_memberships::Entity);
    statement
        .col(auto_id(organization_memberships::Column::Id))
        .col(
            ColumnDef::new(organization_memberships::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_memberships::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_memberships::Column::Role)
                .small_integer()
                .not_null()
                .check(Expr::col(organization_memberships::Column::Role).between(1_i16, 5_i16)),
        )
        .col(ColumnDef::new(organization_memberships::Column::TeamId).big_integer())
        .col(
            ColumnDef::new(organization_memberships::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_memberships::Column::Status).between(1_i16, 3_i16)),
        )
        .col(joined_at)
        .col(nullable_timestamp(
            manager,
            organization_memberships::Column::SuspendedAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_memberships::Column::RemovedAt,
        ))
        .col(ColumnDef::new(organization_memberships::Column::InvitedByUserId).big_integer())
        .col(
            ColumnDef::new(organization_memberships::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_memberships::Column::Version).gt(0_i64)),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_memberships_organization")
                .from(
                    organization_memberships::Entity,
                    organization_memberships::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_memberships_user")
                .from(
                    organization_memberships::Entity,
                    organization_memberships::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_memberships_inviter")
                .from(
                    organization_memberships::Entity,
                    organization_memberships::Column::InvitedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_memberships_team_scope")
                .from(
                    organization_memberships::Entity,
                    (
                        organization_memberships::Column::OrganizationId,
                        organization_memberships::Column::TeamId,
                    ),
                )
                .to(
                    organization_teams::Entity,
                    (
                        organization_teams::Column::OrganizationId,
                        organization_teams::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(membership_lifecycle_check());
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(MEMBERSHIP_SCOPE_USER_INDEX)
            .table(organization_memberships::Entity)
            .col(organization_memberships::Column::OrganizationId)
            .col(organization_memberships::Column::UserId)
            .unique()
            .to_owned(),
        Index::create()
            .name(MEMBERSHIP_STATUS_INDEX)
            .table(organization_memberships::Entity)
            .col(organization_memberships::Column::OrganizationId)
            .col(organization_memberships::Column::Status)
            .col(organization_memberships::Column::Role)
            .col(organization_memberships::Column::Id)
            .to_owned(),
        Index::create()
            .name(MEMBERSHIP_TEAM_INDEX)
            .table(organization_memberships::Entity)
            .col(organization_memberships::Column::OrganizationId)
            .col(organization_memberships::Column::TeamId)
            .col(organization_memberships::Column::Status)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn organization_key_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""organization_key" ~ '^[0-9a-f]{32}$' AND "organization_key" <> '00000000000000000000000000000000'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`organization_key`) = 32 AND `organization_key` REGEXP '^[0-9a-f]{32}$' AND `organization_key` <> '00000000000000000000000000000000'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("organization_key") = 32 AND "organization_key" NOT GLOB '*[^0-9a-f]*' AND "organization_key" <> '00000000000000000000000000000000'"#,
        ),
    }
}

fn membership_lifecycle_check() -> SimpleExpr {
    let status = organization_memberships::Column::Status;
    let suspended_at = organization_memberships::Column::SuspendedAt;
    let removed_at = organization_memberships::Column::RemovedAt;
    Expr::col(status)
        .eq(1_i16)
        .and(Expr::col(suspended_at).is_null())
        .and(Expr::col(removed_at).is_null())
        .or(Expr::col(status)
            .eq(2_i16)
            .and(Expr::col(suspended_at).is_not_null())
            .and(Expr::col(removed_at).is_null()))
        .or(Expr::col(status)
            .eq(3_i16)
            .and(Expr::col(removed_at).is_not_null()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_organization_key_check_does_not_use_binary_regex_operands() {
        let rendered = Query::select()
            .expr(organization_key_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
