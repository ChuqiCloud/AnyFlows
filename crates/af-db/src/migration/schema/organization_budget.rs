use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    billing_organization_reservations, organization_budget_policies, organization_budget_windows,
    organization_memberships, organization_teams, organization_wallets, organizations, users,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

const POLICY_SCOPE_INDEX: &str = "idx_organization_budget_policies_scope";
const WINDOW_SCOPE_INDEX: &str = "idx_organization_budget_windows_scope";
const WINDOW_UNIQUE_INDEX: &str = "uq_organization_budget_windows_policy_start";
const RESERVATION_STATUS_INDEX: &str = "idx_billing_organization_reservations_status";

/// 创建企业预算策略、窗口与持久化预留快照。
pub(in crate::migration) async fn create_organization_budget_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    add_wallet_billing_protection(manager).await?;
    create_policies(manager).await?;
    create_windows(manager).await?;
    create_reservations(manager).await?;
    create_indexes(manager).await
}

/// 按外键依赖逆序删除企业预算存储。
pub(in crate::migration) async fn drop_organization_budget_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(billing_organization_reservations::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(organization_budget_windows::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .drop_table(
            Table::drop()
                .table(organization_budget_policies::Entity)
                .to_owned(),
        )
        .await?;
    if manager
        .has_column("organization_wallets", "billing_protected")
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(organization_wallets::Entity)
                    .drop_column(organization_wallets::Column::BillingProtected)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

async fn add_wallet_billing_protection(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager
        .has_column("organization_wallets", "billing_protected")
        .await?
    {
        return Ok(());
    }
    manager
        .alter_table(
            Table::alter()
                .table(organization_wallets::Entity)
                .add_column(
                    ColumnDef::new(organization_wallets::Column::BillingProtected)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await
}

async fn create_policies(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_budget_policies::Entity);
    statement
        .col(auto_id(organization_budget_policies::Column::Id))
        .col(
            ColumnDef::new(organization_budget_policies::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_budget_policies::Column::ScopeKind)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_budget_policies::Column::ScopeKind)
                        .is_in([1_i16, 2_i16, 3_i16]),
                ),
        )
        .col(ColumnDef::new(organization_budget_policies::Column::TeamId).big_integer())
        .col(ColumnDef::new(organization_budget_policies::Column::MembershipId).big_integer())
        .col(
            ColumnDef::new(organization_budget_policies::Column::PeriodKind)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_budget_policies::Column::PeriodKind)
                        .is_in([1_i16, 2_i16, 3_i16, 4_i16]),
                ),
        )
        .col(ColumnDef::new(organization_budget_policies::Column::FixedPeriodSeconds).big_integer())
        .col(ColumnDef::new(organization_budget_policies::Column::QuotaLimit).big_integer())
        .col(
            nullable_timestamp(manager, organization_budget_policies::Column::EffectiveFrom)
                .not_null(),
        )
        .col(nullable_timestamp(
            manager,
            organization_budget_policies::Column::EffectiveUntil,
        ))
        .col(
            ColumnDef::new(organization_budget_policies::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_budget_policies::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_budget_policies::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_budget_policies::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_budget_policies::Column::UpdatedAt,
        ))
        .check(policy_subject_shape())
        .check(policy_period_shape())
        .check(
            Expr::col(organization_budget_policies::Column::QuotaLimit)
                .is_null()
                .or(Expr::col(organization_budget_policies::Column::QuotaLimit).gt(0_i64)),
        )
        .check(
            Expr::col(organization_budget_policies::Column::EffectiveUntil)
                .is_null()
                .or(
                    Expr::col(organization_budget_policies::Column::EffectiveUntil).gt(Expr::col(
                        organization_budget_policies::Column::EffectiveFrom,
                    )),
                ),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_policy_org")
                .from(
                    organization_budget_policies::Entity,
                    organization_budget_policies::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_policy_team")
                .from(
                    organization_budget_policies::Entity,
                    organization_budget_policies::Column::TeamId,
                )
                .to(organization_teams::Entity, organization_teams::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_policy_member")
                .from(
                    organization_budget_policies::Entity,
                    organization_budget_policies::Column::MembershipId,
                )
                .to(
                    organization_memberships::Entity,
                    organization_memberships::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_policy_actor")
                .from(
                    organization_budget_policies::Entity,
                    organization_budget_policies::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

async fn create_windows(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_budget_windows::Entity);
    statement
        .col(auto_id(organization_budget_windows::Column::Id))
        .col(
            ColumnDef::new(organization_budget_windows::Column::PolicyId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_budget_windows::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_budget_windows::Column::ScopeKind)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_budget_windows::Column::ScopeKind)
                        .is_in([1_i16, 2_i16, 3_i16]),
                ),
        )
        .col(ColumnDef::new(organization_budget_windows::Column::TeamId).big_integer())
        .col(ColumnDef::new(organization_budget_windows::Column::MembershipId).big_integer())
        .col(
            ColumnDef::new(organization_budget_windows::Column::PolicyVersion)
                .big_integer()
                .not_null()
                .check(Expr::col(organization_budget_windows::Column::PolicyVersion).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_budget_windows::Column::LimitSnapshot)
                .big_integer()
                .not_null()
                .check(Expr::col(organization_budget_windows::Column::LimitSnapshot).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_budget_windows::Column::UsedQuota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(Expr::col(organization_budget_windows::Column::UsedQuota).gte(0_i64)),
        )
        .col(
            ColumnDef::new(organization_budget_windows::Column::FrozenQuota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(Expr::col(organization_budget_windows::Column::FrozenQuota).gte(0_i64)),
        )
        .col(
            nullable_timestamp(manager, organization_budget_windows::Column::WindowStartsAt)
                .not_null(),
        )
        .col(
            nullable_timestamp(manager, organization_budget_windows::Column::WindowEndsAt)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_budget_windows::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_budget_windows::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_budget_windows::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_budget_windows::Column::UpdatedAt,
        ))
        .check(window_subject_shape())
        .check(
            Expr::col(organization_budget_windows::Column::WindowEndsAt).gt(Expr::col(
                organization_budget_windows::Column::WindowStartsAt,
            )),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_window_policy")
                .from(
                    organization_budget_windows::Entity,
                    organization_budget_windows::Column::PolicyId,
                )
                .to(
                    organization_budget_policies::Entity,
                    organization_budget_policies::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_window_org")
                .from(
                    organization_budget_windows::Entity,
                    organization_budget_windows::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_window_team")
                .from(
                    organization_budget_windows::Entity,
                    organization_budget_windows::Column::TeamId,
                )
                .to(organization_teams::Entity, organization_teams::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_budget_window_member")
                .from(
                    organization_budget_windows::Entity,
                    organization_budget_windows::Column::MembershipId,
                )
                .to(
                    organization_memberships::Entity,
                    organization_memberships::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await
}

async fn create_reservations(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use billing_organization_reservations::Column;
    let mut statement = table(manager, billing_organization_reservations::Entity);
    statement
        .col(
            ColumnDef::new(Column::IdempotencyKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(Column::MembershipId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(Column::TeamId).big_integer())
        .col(ColumnDef::new(Column::UserId).big_integer().not_null())
        .col(
            ColumnDef::new(Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(Column::Status).is_in([1_i16, 2_i16, 3_i16, 4_i16])),
        )
        .col(
            ColumnDef::new(Column::ReservedQuota)
                .big_integer()
                .not_null()
                .check(Expr::col(Column::ReservedQuota).gt(0_i64)),
        )
        .col(
            ColumnDef::new(Column::ActualQuota)
                .big_integer()
                .check(Expr::col(Column::ActualQuota).gte(0_i64)),
        );
    add_allocation_columns(&mut statement, AllocationColumns::organization());
    add_allocation_columns(&mut statement, AllocationColumns::team());
    add_allocation_columns(&mut statement, AllocationColumns::member());
    statement
        .col(nullable_timestamp(manager, Column::ExpiresAt).not_null())
        .col(nullable_timestamp(manager, Column::FinalizedAt))
        .col(timestamp(manager, Column::CreatedAt))
        .col(timestamp(manager, Column::UpdatedAt))
        .check(reservation_state_shape())
        .check(allocation_shape(AllocationColumns::organization()))
        .check(allocation_shape(AllocationColumns::team()))
        .check(allocation_shape(AllocationColumns::member()))
        .check(reservation_key_format(manager.get_database_backend()))
        .check(Expr::col(Column::IdempotencyKey).ne("00000000000000000000000000000000"))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_org",
            Column::OrganizationId,
            organizations::Entity,
            organizations::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_member",
            Column::MembershipId,
            organization_memberships::Entity,
            organization_memberships::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_team",
            Column::TeamId,
            organization_teams::Entity,
            organization_teams::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_user",
            Column::UserId,
            users::Entity,
            users::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_org_policy",
            Column::OrganizationPolicyId,
            organization_budget_policies::Entity,
            organization_budget_policies::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_org_window",
            Column::OrganizationWindowId,
            organization_budget_windows::Entity,
            organization_budget_windows::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_team_policy",
            Column::TeamPolicyId,
            organization_budget_policies::Entity,
            organization_budget_policies::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_team_window",
            Column::TeamWindowId,
            organization_budget_windows::Entity,
            organization_budget_windows::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_member_policy",
            Column::MemberPolicyId,
            organization_budget_policies::Entity,
            organization_budget_policies::Column::Id,
        ))
        .foreign_key(&mut reservation_foreign_key(
            "fk_billing_org_reservation_member_window",
            Column::MemberWindowId,
            organization_budget_windows::Entity,
            organization_budget_windows::Column::Id,
        ));
    manager.create_table(statement).await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name(POLICY_SCOPE_INDEX)
                .table(organization_budget_policies::Entity)
                .col(organization_budget_policies::Column::OrganizationId)
                .col(organization_budget_policies::Column::ScopeKind)
                .col(organization_budget_policies::Column::TeamId)
                .col(organization_budget_policies::Column::MembershipId)
                .col(organization_budget_policies::Column::EffectiveFrom)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(WINDOW_UNIQUE_INDEX)
                .table(organization_budget_windows::Entity)
                .col(organization_budget_windows::Column::PolicyId)
                .col(organization_budget_windows::Column::WindowStartsAt)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(WINDOW_SCOPE_INDEX)
                .table(organization_budget_windows::Entity)
                .col(organization_budget_windows::Column::OrganizationId)
                .col(organization_budget_windows::Column::ScopeKind)
                .col(organization_budget_windows::Column::WindowEndsAt)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(RESERVATION_STATUS_INDEX)
                .table(billing_organization_reservations::Entity)
                .col(billing_organization_reservations::Column::OrganizationId)
                .col(billing_organization_reservations::Column::Status)
                .col(billing_organization_reservations::Column::ExpiresAt)
                .to_owned(),
        )
        .await
}

fn policy_subject_shape() -> SimpleExpr {
    use organization_budget_policies::Column;
    Expr::col(Column::ScopeKind)
        .eq(1_i16)
        .and(Expr::col(Column::TeamId).is_null())
        .and(Expr::col(Column::MembershipId).is_null())
        .or(Expr::col(Column::ScopeKind)
            .eq(2_i16)
            .and(Expr::col(Column::TeamId).is_not_null())
            .and(Expr::col(Column::MembershipId).is_null()))
        .or(Expr::col(Column::ScopeKind)
            .eq(3_i16)
            .and(Expr::col(Column::TeamId).is_null())
            .and(Expr::col(Column::MembershipId).is_not_null()))
}

fn policy_period_shape() -> SimpleExpr {
    use organization_budget_policies::Column;
    Expr::col(Column::PeriodKind)
        .is_in([1_i16, 2_i16, 3_i16])
        .and(Expr::col(Column::FixedPeriodSeconds).is_null())
        .or(Expr::col(Column::PeriodKind)
            .eq(4_i16)
            .and(Expr::col(Column::FixedPeriodSeconds).between(60_i64, 31_622_400_i64)))
}

fn window_subject_shape() -> SimpleExpr {
    use organization_budget_windows::Column;
    Expr::col(Column::ScopeKind)
        .eq(1_i16)
        .and(Expr::col(Column::TeamId).is_null())
        .and(Expr::col(Column::MembershipId).is_null())
        .or(Expr::col(Column::ScopeKind)
            .eq(2_i16)
            .and(Expr::col(Column::TeamId).is_not_null())
            .and(Expr::col(Column::MembershipId).is_null()))
        .or(Expr::col(Column::ScopeKind)
            .eq(3_i16)
            .and(Expr::col(Column::TeamId).is_null())
            .and(Expr::col(Column::MembershipId).is_not_null()))
}

#[derive(Clone, Copy)]
struct AllocationColumns {
    policy_id: billing_organization_reservations::Column,
    window_id: billing_organization_reservations::Column,
    policy_version: billing_organization_reservations::Column,
    window_version: billing_organization_reservations::Column,
    reserved_quota: billing_organization_reservations::Column,
}

impl AllocationColumns {
    const fn organization() -> Self {
        use billing_organization_reservations::Column;
        Self {
            policy_id: Column::OrganizationPolicyId,
            window_id: Column::OrganizationWindowId,
            policy_version: Column::OrganizationPolicyVersion,
            window_version: Column::OrganizationWindowVersion,
            reserved_quota: Column::OrganizationReservedQuota,
        }
    }

    const fn team() -> Self {
        use billing_organization_reservations::Column;
        Self {
            policy_id: Column::TeamPolicyId,
            window_id: Column::TeamWindowId,
            policy_version: Column::TeamPolicyVersion,
            window_version: Column::TeamWindowVersion,
            reserved_quota: Column::TeamReservedQuota,
        }
    }

    const fn member() -> Self {
        use billing_organization_reservations::Column;
        Self {
            policy_id: Column::MemberPolicyId,
            window_id: Column::MemberWindowId,
            policy_version: Column::MemberPolicyVersion,
            window_version: Column::MemberWindowVersion,
            reserved_quota: Column::MemberReservedQuota,
        }
    }
}

fn add_allocation_columns(statement: &mut TableCreateStatement, columns: AllocationColumns) {
    statement
        .col(ColumnDef::new(columns.policy_id).big_integer())
        .col(ColumnDef::new(columns.window_id).big_integer())
        .col(ColumnDef::new(columns.policy_version).big_integer())
        .col(ColumnDef::new(columns.window_version).big_integer())
        .col(ColumnDef::new(columns.reserved_quota).big_integer());
}

fn allocation_shape(columns: AllocationColumns) -> SimpleExpr {
    let absent = Expr::col(columns.policy_id)
        .is_null()
        .and(Expr::col(columns.window_id).is_null())
        .and(Expr::col(columns.policy_version).is_null())
        .and(Expr::col(columns.window_version).is_null())
        .and(Expr::col(columns.reserved_quota).is_null());
    let present = Expr::col(columns.policy_id)
        .is_not_null()
        .and(Expr::col(columns.window_id).is_not_null())
        .and(Expr::col(columns.policy_version).gt(0_i64))
        .and(Expr::col(columns.window_version).gt(0_i64))
        .and(Expr::col(columns.reserved_quota).gt(0_i64));
    absent.or(present)
}

fn reservation_state_shape() -> SimpleExpr {
    use billing_organization_reservations::Column;
    Expr::col(Column::Status)
        .eq(1_i16)
        .and(Expr::col(Column::ActualQuota).is_null())
        .and(Expr::col(Column::FinalizedAt).is_null())
        .or(Expr::col(Column::Status)
            .eq(2_i16)
            .and(Expr::col(Column::ActualQuota).is_not_null())
            .and(Expr::col(Column::FinalizedAt).is_null()))
        .or(Expr::col(Column::Status)
            .eq(3_i16)
            .and(Expr::col(Column::ActualQuota).is_not_null())
            .and(Expr::col(Column::FinalizedAt).is_not_null()))
        .or(Expr::col(Column::Status)
            .eq(4_i16)
            .and(Expr::col(Column::ActualQuota).is_null())
            .and(Expr::col(Column::FinalizedAt).is_not_null()))
}

fn reservation_key_format(backend: DbBackend) -> SimpleExpr {
    match backend {
        DbBackend::Postgres => Expr::cust(r#""idempotency_key" ~ '^[0-9a-f]{32}$'"#),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`idempotency_key`) = 32 AND `idempotency_key` REGEXP '^[0-9a-f]{32}$'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("idempotency_key") = 32 AND "idempotency_key" NOT GLOB '*[^0-9a-f]*'"#,
        ),
    }
}

fn reservation_foreign_key<T, C>(
    name: &str,
    from: billing_organization_reservations::Column,
    to_table: T,
    to_column: C,
) -> ForeignKeyCreateStatement
where
    T: IntoTableRef,
    C: IntoIden,
{
    ForeignKey::create()
        .name(name)
        .from(billing_organization_reservations::Entity, from)
        .to(to_table, to_column)
        .on_update(ForeignKeyAction::Restrict)
        .on_delete(ForeignKeyAction::Restrict)
        .to_owned()
}
