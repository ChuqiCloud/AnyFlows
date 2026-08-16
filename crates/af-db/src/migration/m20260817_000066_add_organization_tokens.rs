use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use super::iden::{
    billing_organization_reservations, billing_reservations, groups, organization_memberships,
    organization_teams, organizations, tokens,
};

const TOKEN_ORGANIZATION_INDEX: &str = "idx_tokens_organization_status_id";
const TOKEN_ORGANIZATION_OWNER_INDEX: &str = "idx_tokens_organization_owner_id";
const TOKEN_MEMBERSHIP_INDEX: &str = "idx_tokens_organization_membership_status";
const MEMBERSHIP_SCOPE_ID_INDEX: &str = "uq_organization_memberships_scope_id";
const BILLING_ORGANIZATION_INDEX: &str = "idx_billing_reservations_organization_status";
const TOKEN_SUBJECT_CHECK: &str = "ck_tokens_organization_subject";
const ORGANIZATION_BILLING_GATEWAY_CHECK: &str = "ck_billing_org_reservations_gateway_shape";
const TOKEN_ORGANIZATION_FOREIGN_KEY: &str = "fk_tokens_organization";
const TOKEN_MEMBERSHIP_FOREIGN_KEY: &str = "fk_tokens_organization_membership";
const TOKEN_TEAM_FOREIGN_KEY: &str = "fk_tokens_organization_team";
const BILLING_ORGANIZATION_FOREIGN_KEY: &str = "fk_billing_reservations_organization";
const ORGANIZATION_BILLING_TOKEN_FOREIGN_KEY: &str = "fk_billing_org_reservation_token";
const ORGANIZATION_BILLING_GROUP_FOREIGN_KEY: &str = "fk_billing_org_reservation_group";

const SQLITE_TRIGGERS: &[&str] = &[
    "trg_tokens_organization_insert",
    "trg_tokens_organization_update",
    "trg_organizations_tokens_restrict_delete",
    "trg_organization_memberships_tokens_restrict_delete",
    "trg_organization_teams_tokens_restrict_delete",
    "trg_billing_reservations_organization_insert",
    "trg_billing_reservations_organization_update",
    "trg_organizations_billing_reservations_restrict_delete",
    "trg_billing_org_gateway_insert",
    "trg_billing_org_gateway_update",
    "trg_tokens_billing_org_restrict_delete",
    "trg_groups_billing_org_restrict_delete",
];

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column(
            manager,
            tokens::Entity,
            ColumnDef::new(tokens::Column::OrganizationId).big_integer(),
        )
        .await?;
        add_column(
            manager,
            tokens::Entity,
            ColumnDef::new(tokens::Column::OrganizationMembershipId).big_integer(),
        )
        .await?;
        let mut organization_team_id = ColumnDef::new(tokens::Column::OrganizationTeamId);
        organization_team_id.big_integer();
        if manager.get_database_backend() != DbBackend::MySql {
            organization_team_id.check(token_subject_shape());
        }
        add_column(manager, tokens::Entity, &mut organization_team_id).await?;
        add_column(
            manager,
            billing_reservations::Entity,
            ColumnDef::new(billing_reservations::Column::OrganizationId).big_integer(),
        )
        .await?;
        add_column(
            manager,
            billing_organization_reservations::Entity,
            ColumnDef::new(billing_organization_reservations::Column::TokenId).big_integer(),
        )
        .await?;
        add_column(
            manager,
            billing_organization_reservations::Entity,
            ColumnDef::new(billing_organization_reservations::Column::GroupId).big_integer(),
        )
        .await?;
        add_column(
            manager,
            billing_organization_reservations::Entity,
            ColumnDef::new(billing_organization_reservations::Column::ReservationKind)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(
                    Expr::col(billing_organization_reservations::Column::ReservationKind)
                        .is_in([1_i16, 2_i16]),
                ),
        )
        .await?;
        let mut token_reserved_quota =
            ColumnDef::new(billing_organization_reservations::Column::TokenReservedQuota);
        token_reserved_quota.big_integer().not_null().default(0_i64);
        if manager.get_database_backend() != DbBackend::MySql {
            token_reserved_quota.check(organization_billing_gateway_shape());
        }
        add_column(
            manager,
            billing_organization_reservations::Entity,
            &mut token_reserved_quota,
        )
        .await?;

        create_mysql_table_checks(manager).await?;

        for index in [
            Index::create()
                .name(MEMBERSHIP_SCOPE_ID_INDEX)
                .table(organization_memberships::Entity)
                .col(organization_memberships::Column::OrganizationId)
                .col(organization_memberships::Column::Id)
                .unique()
                .to_owned(),
            Index::create()
                .name(TOKEN_ORGANIZATION_INDEX)
                .table(tokens::Entity)
                .col(tokens::Column::OrganizationId)
                .col(tokens::Column::Status)
                .col(tokens::Column::Id)
                .to_owned(),
            Index::create()
                .name(TOKEN_ORGANIZATION_OWNER_INDEX)
                .table(tokens::Entity)
                .col(tokens::Column::OrganizationId)
                .col(tokens::Column::UserId)
                .col(tokens::Column::Id)
                .unique()
                .to_owned(),
            Index::create()
                .name(TOKEN_MEMBERSHIP_INDEX)
                .table(tokens::Entity)
                .col(tokens::Column::OrganizationMembershipId)
                .col(tokens::Column::Status)
                .to_owned(),
            Index::create()
                .name(BILLING_ORGANIZATION_INDEX)
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::OrganizationId)
                .col(billing_reservations::Column::Status)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        create_references(manager).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_references(manager).await?;
        drop_mysql_table_checks(manager).await?;

        for (table, index) in [
            (
                billing_reservations::Entity.into_iden(),
                BILLING_ORGANIZATION_INDEX,
            ),
            (tokens::Entity.into_iden(), TOKEN_MEMBERSHIP_INDEX),
            (tokens::Entity.into_iden(), TOKEN_ORGANIZATION_OWNER_INDEX),
            (tokens::Entity.into_iden(), TOKEN_ORGANIZATION_INDEX),
            (
                organization_memberships::Entity.into_iden(),
                MEMBERSHIP_SCOPE_ID_INDEX,
            ),
        ] {
            manager
                .drop_index(Index::drop().name(index).table(table).to_owned())
                .await?;
        }

        drop_column(
            manager,
            billing_organization_reservations::Entity,
            billing_organization_reservations::Column::TokenReservedQuota,
        )
        .await?;
        drop_column(
            manager,
            billing_organization_reservations::Entity,
            billing_organization_reservations::Column::ReservationKind,
        )
        .await?;
        drop_column(
            manager,
            billing_organization_reservations::Entity,
            billing_organization_reservations::Column::GroupId,
        )
        .await?;
        drop_column(
            manager,
            billing_organization_reservations::Entity,
            billing_organization_reservations::Column::TokenId,
        )
        .await?;
        drop_column(
            manager,
            billing_reservations::Entity,
            billing_reservations::Column::OrganizationId,
        )
        .await?;
        drop_column(manager, tokens::Entity, tokens::Column::OrganizationTeamId).await?;
        drop_column(
            manager,
            tokens::Entity,
            tokens::Column::OrganizationMembershipId,
        )
        .await?;
        drop_column(manager, tokens::Entity, tokens::Column::OrganizationId).await
    }
}

/// MySQL 不允许列级 CHECK 引用同表其他列，因此在所有列就绪后补命名表级约束。
async fn create_mysql_table_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }
    for statement in [
        "ALTER TABLE `tokens` ADD CONSTRAINT `ck_tokens_organization_subject` CHECK ((`organization_id` IS NULL AND `organization_membership_id` IS NULL AND `organization_team_id` IS NULL) OR (`organization_id` IS NOT NULL AND `organization_membership_id` IS NOT NULL))",
        "ALTER TABLE `billing_organization_reservations` ADD CONSTRAINT `ck_billing_org_reservations_gateway_shape` CHECK ((`reservation_kind` = 1 AND `token_id` IS NULL AND `group_id` IS NULL AND `token_reserved_quota` = 0) OR (`reservation_kind` = 2 AND `token_id` IS NOT NULL AND `group_id` IS NOT NULL AND (`token_reserved_quota` = 0 OR `token_reserved_quota` = `reserved_quota`)))",
    ] {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

async fn drop_mysql_table_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }
    for (table, constraint) in [
        (
            "billing_organization_reservations",
            ORGANIZATION_BILLING_GATEWAY_CHECK,
        ),
        ("tokens", TOKEN_SUBJECT_CHECK),
    ] {
        manager
            .get_connection()
            .execute_unprepared(&format!("ALTER TABLE `{table}` DROP CHECK `{constraint}`"))
            .await?;
    }
    Ok(())
}

async fn create_references(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        return create_sqlite_reference_triggers(manager).await;
    }
    for foreign_key in [
        ForeignKey::create()
            .name(TOKEN_ORGANIZATION_FOREIGN_KEY)
            .from(tokens::Entity, tokens::Column::OrganizationId)
            .to(organizations::Entity, organizations::Column::Id)
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
        ForeignKey::create()
            .name(TOKEN_MEMBERSHIP_FOREIGN_KEY)
            .from(
                tokens::Entity,
                (
                    tokens::Column::OrganizationId,
                    tokens::Column::OrganizationMembershipId,
                ),
            )
            .to(
                organization_memberships::Entity,
                (
                    organization_memberships::Column::OrganizationId,
                    organization_memberships::Column::Id,
                ),
            )
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
        ForeignKey::create()
            .name(TOKEN_TEAM_FOREIGN_KEY)
            .from(
                tokens::Entity,
                (
                    tokens::Column::OrganizationId,
                    tokens::Column::OrganizationTeamId,
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
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
        ForeignKey::create()
            .name(BILLING_ORGANIZATION_FOREIGN_KEY)
            .from(
                billing_reservations::Entity,
                billing_reservations::Column::OrganizationId,
            )
            .to(organizations::Entity, organizations::Column::Id)
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
        ForeignKey::create()
            .name(ORGANIZATION_BILLING_TOKEN_FOREIGN_KEY)
            .from(
                billing_organization_reservations::Entity,
                (
                    billing_organization_reservations::Column::OrganizationId,
                    billing_organization_reservations::Column::UserId,
                    billing_organization_reservations::Column::TokenId,
                ),
            )
            .to(
                tokens::Entity,
                (
                    tokens::Column::OrganizationId,
                    tokens::Column::UserId,
                    tokens::Column::Id,
                ),
            )
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
        ForeignKey::create()
            .name(ORGANIZATION_BILLING_GROUP_FOREIGN_KEY)
            .from(
                billing_organization_reservations::Entity,
                billing_organization_reservations::Column::GroupId,
            )
            .to(groups::Entity, groups::Column::Id)
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict)
            .to_owned(),
    ] {
        manager.create_foreign_key(foreign_key).await?;
    }
    Ok(())
}

async fn drop_references(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        for trigger in SQLITE_TRIGGERS {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                .await?;
        }
        return Ok(());
    }
    for (table, name) in [
        (
            billing_organization_reservations::Entity.into_iden(),
            ORGANIZATION_BILLING_GROUP_FOREIGN_KEY,
        ),
        (
            billing_organization_reservations::Entity.into_iden(),
            ORGANIZATION_BILLING_TOKEN_FOREIGN_KEY,
        ),
        (
            billing_reservations::Entity.into_iden(),
            BILLING_ORGANIZATION_FOREIGN_KEY,
        ),
        (tokens::Entity.into_iden(), TOKEN_TEAM_FOREIGN_KEY),
        (tokens::Entity.into_iden(), TOKEN_MEMBERSHIP_FOREIGN_KEY),
        (tokens::Entity.into_iden(), TOKEN_ORGANIZATION_FOREIGN_KEY),
    ] {
        manager
            .drop_foreign_key(ForeignKey::drop().name(name).table(table).to_owned())
            .await?;
    }
    Ok(())
}

async fn create_sqlite_reference_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let statements = [
        "CREATE TRIGGER trg_tokens_organization_insert BEFORE INSERT ON tokens \
         WHEN NOT ((NEW.organization_id IS NULL AND NEW.organization_membership_id IS NULL AND NEW.organization_team_id IS NULL) OR \
         (NEW.organization_id IS NOT NULL AND NEW.organization_membership_id IS NOT NULL)) OR \
         (NEW.organization_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organizations WHERE id = NEW.organization_id)) OR \
         (NEW.organization_membership_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_memberships WHERE id = NEW.organization_membership_id AND organization_id = NEW.organization_id)) OR \
         (NEW.organization_team_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_teams WHERE id = NEW.organization_team_id AND organization_id = NEW.organization_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid token organization subject'); END",
        "CREATE TRIGGER trg_tokens_organization_update BEFORE UPDATE OF organization_id, organization_membership_id, organization_team_id ON tokens \
         WHEN NOT ((NEW.organization_id IS NULL AND NEW.organization_membership_id IS NULL AND NEW.organization_team_id IS NULL) OR \
         (NEW.organization_id IS NOT NULL AND NEW.organization_membership_id IS NOT NULL)) OR \
         (NEW.organization_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organizations WHERE id = NEW.organization_id)) OR \
         (NEW.organization_membership_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_memberships WHERE id = NEW.organization_membership_id AND organization_id = NEW.organization_id)) OR \
         (NEW.organization_team_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organization_teams WHERE id = NEW.organization_team_id AND organization_id = NEW.organization_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid token organization subject'); END",
        "CREATE TRIGGER trg_organizations_tokens_restrict_delete BEFORE DELETE ON organizations \
         WHEN EXISTS (SELECT 1 FROM tokens WHERE organization_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'organization is referenced by tokens'); END",
        "CREATE TRIGGER trg_organization_memberships_tokens_restrict_delete BEFORE DELETE ON organization_memberships \
         WHEN EXISTS (SELECT 1 FROM tokens WHERE organization_membership_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'membership is referenced by tokens'); END",
        "CREATE TRIGGER trg_organization_teams_tokens_restrict_delete BEFORE DELETE ON organization_teams \
         WHEN EXISTS (SELECT 1 FROM tokens WHERE organization_team_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'team is referenced by tokens'); END",
        "CREATE TRIGGER trg_billing_reservations_organization_insert BEFORE INSERT ON billing_reservations \
         WHEN NEW.organization_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organizations WHERE id = NEW.organization_id) \
         BEGIN SELECT RAISE(ABORT, 'billing reservation organization missing'); END",
        "CREATE TRIGGER trg_billing_reservations_organization_update BEFORE UPDATE OF organization_id ON billing_reservations \
         WHEN NEW.organization_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM organizations WHERE id = NEW.organization_id) \
         BEGIN SELECT RAISE(ABORT, 'billing reservation organization missing'); END",
        "CREATE TRIGGER trg_organizations_billing_reservations_restrict_delete BEFORE DELETE ON organizations \
         WHEN EXISTS (SELECT 1 FROM billing_reservations WHERE organization_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'organization is referenced by billing reservations'); END",
        "CREATE TRIGGER trg_billing_org_gateway_insert BEFORE INSERT ON billing_organization_reservations \
         WHEN NOT ((NEW.reservation_kind = 1 AND NEW.token_id IS NULL AND NEW.group_id IS NULL AND NEW.token_reserved_quota = 0) OR \
         (NEW.reservation_kind = 2 AND NEW.token_id IS NOT NULL AND NEW.group_id IS NOT NULL AND (NEW.token_reserved_quota = 0 OR NEW.token_reserved_quota = NEW.reserved_quota))) OR \
         (NEW.token_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM tokens WHERE id = NEW.token_id AND user_id = NEW.user_id AND organization_id = NEW.organization_id)) OR \
         (NEW.group_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM groups WHERE id = NEW.group_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid organization gateway reservation'); END",
        "CREATE TRIGGER trg_billing_org_gateway_update BEFORE UPDATE OF token_id, group_id, reservation_kind, token_reserved_quota ON billing_organization_reservations \
         WHEN NOT ((NEW.reservation_kind = 1 AND NEW.token_id IS NULL AND NEW.group_id IS NULL AND NEW.token_reserved_quota = 0) OR \
         (NEW.reservation_kind = 2 AND NEW.token_id IS NOT NULL AND NEW.group_id IS NOT NULL AND (NEW.token_reserved_quota = 0 OR NEW.token_reserved_quota = NEW.reserved_quota))) OR \
         (NEW.token_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM tokens WHERE id = NEW.token_id AND user_id = NEW.user_id AND organization_id = NEW.organization_id)) OR \
         (NEW.group_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM groups WHERE id = NEW.group_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid organization gateway reservation'); END",
        "CREATE TRIGGER trg_tokens_billing_org_restrict_delete BEFORE DELETE ON tokens \
         WHEN EXISTS (SELECT 1 FROM billing_organization_reservations WHERE token_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'token is referenced by organization billing'); END",
        "CREATE TRIGGER trg_groups_billing_org_restrict_delete BEFORE DELETE ON groups \
         WHEN EXISTS (SELECT 1 FROM billing_organization_reservations WHERE group_id = OLD.id) \
         BEGIN SELECT RAISE(ABORT, 'group is referenced by organization billing'); END",
    ];
    for statement in statements {
        manager
            .get_connection()
            .execute_unprepared(statement)
            .await?;
    }
    Ok(())
}

fn token_subject_shape() -> SimpleExpr {
    let personal = Expr::col(tokens::Column::OrganizationId)
        .is_null()
        .and(Expr::col(tokens::Column::OrganizationMembershipId).is_null())
        .and(Expr::col(tokens::Column::OrganizationTeamId).is_null());
    let enterprise = Expr::col(tokens::Column::OrganizationId)
        .is_not_null()
        .and(Expr::col(tokens::Column::OrganizationMembershipId).is_not_null());
    personal.or(enterprise)
}

fn organization_billing_gateway_shape() -> SimpleExpr {
    let independent = Expr::col(billing_organization_reservations::Column::ReservationKind)
        .eq(1_i16)
        .and(Expr::col(billing_organization_reservations::Column::TokenId).is_null())
        .and(Expr::col(billing_organization_reservations::Column::GroupId).is_null())
        .and(Expr::col(billing_organization_reservations::Column::TokenReservedQuota).eq(0_i64));
    let gateway = Expr::col(billing_organization_reservations::Column::ReservationKind)
        .eq(2_i16)
        .and(Expr::col(billing_organization_reservations::Column::TokenId).is_not_null())
        .and(Expr::col(billing_organization_reservations::Column::GroupId).is_not_null())
        .and(
            Expr::col(billing_organization_reservations::Column::TokenReservedQuota)
                .eq(0_i64)
                .or(
                    Expr::col(billing_organization_reservations::Column::TokenReservedQuota)
                        .equals(billing_organization_reservations::Column::ReservedQuota),
                ),
        );
    independent.or(gateway)
}

async fn add_column<T>(
    manager: &SchemaManager<'_>,
    table: T,
    column: &mut ColumnDef,
) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
{
    manager
        .alter_table(Table::alter().table(table).add_column(column).to_owned())
        .await
}

async fn drop_column<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
    C: IntoIden + 'static,
{
    manager
        .alter_table(Table::alter().table(table).drop_column(column).to_owned())
        .await
}
