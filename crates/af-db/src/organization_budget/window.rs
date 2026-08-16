use af_domain::{
    OrganizationBudgetPeriod, OrganizationBudgetPolicyId, OrganizationBudgetScopeKind,
    OrganizationBudgetWindowId, OrganizationId, OrganizationMembershipId, OrganizationTeamId,
    Quota, SubscriptionCycle, SubscriptionWindow, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, ExprTrait, LockType},
};

use crate::entity::{organization_budget_policies, organization_budget_windows};

use super::{
    policy,
    types::{
        OrganizationBudgetAllocationRecord, OrganizationBudgetRepositoryError,
        OrganizationBudgetScope, OrganizationBudgetWindowRecord,
    },
};

/// 读取仍在生效的窗口，历史窗口不在管理查询中无限累积。
pub(super) async fn list_current(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    actor_user_id: UserId,
    now: TimeDateTimeWithTimeZone,
) -> Result<Vec<OrganizationBudgetWindowRecord>, OrganizationBudgetRepositoryError> {
    use crate::entity::organizations;

    if organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .is_none()
    {
        return Err(OrganizationBudgetRepositoryError::NotFound);
    }
    super::policy::require_budget_manager(transaction, organization_id, actor_user_id).await?;
    organization_budget_windows::Entity::find()
        .filter(organization_budget_windows::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_budget_windows::Column::WindowEndsAt.gt(now))
        .order_by_asc(organization_budget_windows::Column::WindowStartsAt)
        .order_by_asc(organization_budget_windows::Column::Id)
        .all(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .into_iter()
        .map(window_record)
        .collect()
}

pub(super) async fn reserve(
    transaction: &DatabaseTransaction,
    policy_model: organization_budget_policies::Model,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<Option<OrganizationBudgetAllocationRecord>, OrganizationBudgetRepositoryError> {
    let policy_record = policy::record(policy_model.clone())?;
    let Some(limit) = policy_record.limit().quota() else {
        return Ok(None);
    };
    let (starts_at, ends_at) = window_bounds(
        policy_record.period(),
        policy_record.effective_from(),
        policy_record.effective_until(),
        now,
    )?;
    let mut window =
        load_or_create(transaction, &policy_model, limit, starts_at, ends_at, now).await?;
    validate_window(&window, &policy_model, starts_at, ends_at)?;
    let next_version = window
        .version
        .checked_add(1)
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let result = organization_budget_windows::Entity::update_many()
        .filter(organization_budget_windows::Column::Id.eq(window.id))
        .filter(organization_budget_windows::Column::Version.eq(window.version))
        .filter(
            Expr::col(organization_budget_windows::Column::UsedQuota)
                .add(Expr::col(organization_budget_windows::Column::FrozenQuota))
                .add(amount.units())
                .lte(window.limit_snapshot),
        )
        .col_expr(
            organization_budget_windows::Column::FrozenQuota,
            Expr::col(organization_budget_windows::Column::FrozenQuota).add(amount.units()),
        )
        .col_expr(
            organization_budget_windows::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            organization_budget_windows::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::BudgetQuotaInsufficient);
    }
    window.version = next_version;
    window.frozen_quota = window
        .frozen_quota
        .checked_add(amount.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    Ok(Some(allocation(&window, amount)?))
}

pub(super) async fn settle(
    transaction: &DatabaseTransaction,
    allocation: OrganizationBudgetAllocationRecord,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let window = lock_window(transaction, allocation.window_id.get()).await?;
    validate_allocation(&window, allocation)?;
    let next_version = window
        .version
        .checked_add(1)
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let remaining_frozen = window
        .frozen_quota
        .checked_sub(allocation.reserved_quota.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let next_used = window
        .used_quota
        .checked_add(actual.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let committed = next_used
        .checked_add(remaining_frozen)
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    if committed > window.limit_snapshot {
        return Err(OrganizationBudgetRepositoryError::BudgetQuotaInsufficient);
    }
    let result = organization_budget_windows::Entity::update_many()
        .filter(organization_budget_windows::Column::Id.eq(window.id))
        .filter(organization_budget_windows::Column::Version.eq(window.version))
        .filter(organization_budget_windows::Column::FrozenQuota.eq(window.frozen_quota))
        .filter(organization_budget_windows::Column::UsedQuota.eq(window.used_quota))
        .col_expr(
            organization_budget_windows::Column::FrozenQuota,
            Expr::value(remaining_frozen),
        )
        .col_expr(
            organization_budget_windows::Column::UsedQuota,
            Expr::value(next_used),
        )
        .col_expr(
            organization_budget_windows::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            organization_budget_windows::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(())
}

pub(super) async fn release(
    transaction: &DatabaseTransaction,
    allocation: OrganizationBudgetAllocationRecord,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let window = lock_window(transaction, allocation.window_id.get()).await?;
    validate_allocation(&window, allocation)?;
    let remaining = window
        .frozen_quota
        .checked_sub(allocation.reserved_quota.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let next_version = window
        .version
        .checked_add(1)
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let result = organization_budget_windows::Entity::update_many()
        .filter(organization_budget_windows::Column::Id.eq(window.id))
        .filter(organization_budget_windows::Column::Version.eq(window.version))
        .filter(organization_budget_windows::Column::FrozenQuota.eq(window.frozen_quota))
        .col_expr(
            organization_budget_windows::Column::FrozenQuota,
            Expr::value(remaining),
        )
        .col_expr(
            organization_budget_windows::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            organization_budget_windows::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(())
}

async fn load_or_create(
    transaction: &DatabaseTransaction,
    policy: &organization_budget_policies::Model,
    limit: Quota,
    starts_at: TimeDateTimeWithTimeZone,
    ends_at: TimeDateTimeWithTimeZone,
    now: TimeDateTimeWithTimeZone,
) -> Result<organization_budget_windows::Model, OrganizationBudgetRepositoryError> {
    if let Some(window) = load_window(transaction, policy.id, starts_at, true).await? {
        return Ok(window);
    }
    let inserted = organization_budget_windows::ActiveModel {
        policy_id: Set(policy.id),
        organization_id: Set(policy.organization_id),
        scope_kind: Set(policy.scope_kind),
        team_id: Set(policy.team_id),
        membership_id: Set(policy.membership_id),
        policy_version: Set(policy.version),
        limit_snapshot: Set(limit.units()),
        used_quota: Set(0),
        frozen_quota: Set(0),
        window_starts_at: Set(starts_at),
        window_ends_at: Set(ends_at),
        version: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    match inserted {
        Ok(window) => Ok(window),
        Err(_) => load_window(transaction, policy.id, starts_at, true)
            .await?
            .ok_or(OrganizationBudgetRepositoryError::Query),
    }
}

async fn load_window(
    transaction: &DatabaseTransaction,
    policy_id: i64,
    starts_at: TimeDateTimeWithTimeZone,
    lock: bool,
) -> Result<Option<organization_budget_windows::Model>, OrganizationBudgetRepositoryError> {
    let mut query = organization_budget_windows::Entity::find()
        .filter(organization_budget_windows::Column::PolicyId.eq(policy_id))
        .filter(organization_budget_windows::Column::WindowStartsAt.eq(starts_at));
    if lock && transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)
}

async fn lock_window(
    transaction: &DatabaseTransaction,
    window_id: i64,
) -> Result<organization_budget_windows::Model, OrganizationBudgetRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organization_budget_windows::Entity::update_many()
            .filter(organization_budget_windows::Column::Id.eq(window_id))
            .col_expr(
                organization_budget_windows::Column::Version,
                Expr::col(organization_budget_windows::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationBudgetRepositoryError::Invariant);
        }
    }
    let mut query = organization_budget_windows::Entity::find_by_id(window_id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .ok_or(OrganizationBudgetRepositoryError::Invariant)
}

fn validate_window(
    window: &organization_budget_windows::Model,
    policy: &organization_budget_policies::Model,
    starts_at: TimeDateTimeWithTimeZone,
    ends_at: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    if window.policy_id != policy.id
        || window.organization_id != policy.organization_id
        || window.scope_kind != policy.scope_kind
        || window.team_id != policy.team_id
        || window.membership_id != policy.membership_id
        || window.policy_version != policy.version
        || window.limit_snapshot != policy.quota_limit.unwrap_or_default()
        || window.used_quota < 0
        || window.frozen_quota < 0
        || window.version <= 0
        || window.window_starts_at != starts_at
        || window.window_ends_at != ends_at
    {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(())
}

fn validate_allocation(
    window: &organization_budget_windows::Model,
    allocation: OrganizationBudgetAllocationRecord,
) -> Result<(), OrganizationBudgetRepositoryError> {
    if window.id != allocation.window_id.get()
        || window.policy_id != allocation.policy_id.get()
        || window.policy_version != allocation.policy_version
        || allocation.window_version <= 0
        || window.version < allocation.window_version
        || allocation.reserved_quota.is_zero()
        || window.used_quota < 0
        || window.frozen_quota < allocation.reserved_quota.units()
    {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(())
}

fn allocation(
    window: &organization_budget_windows::Model,
    reserved_quota: Quota,
) -> Result<OrganizationBudgetAllocationRecord, OrganizationBudgetRepositoryError> {
    Ok(OrganizationBudgetAllocationRecord {
        policy_id: OrganizationBudgetPolicyId::new(window.policy_id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        window_id: OrganizationBudgetWindowId::new(window.id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        policy_version: window.policy_version,
        window_version: window.version,
        reserved_quota,
    })
}

fn window_record(
    model: organization_budget_windows::Model,
) -> Result<OrganizationBudgetWindowRecord, OrganizationBudgetRepositoryError> {
    let kind = OrganizationBudgetScopeKind::try_from(model.scope_kind)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let scope = match (kind, model.team_id, model.membership_id) {
        (OrganizationBudgetScopeKind::Organization, None, None) => {
            OrganizationBudgetScope::Organization
        }
        (OrganizationBudgetScopeKind::Team, Some(team_id), None) => OrganizationBudgetScope::Team(
            OrganizationTeamId::new(team_id)
                .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        ),
        (OrganizationBudgetScopeKind::Member, None, Some(membership_id)) => {
            OrganizationBudgetScope::Member(
                OrganizationMembershipId::new(membership_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            )
        }
        _ => return Err(OrganizationBudgetRepositoryError::Invariant),
    };
    if model.policy_id <= 0
        || model.organization_id <= 0
        || model.policy_version <= 0
        || model.limit_snapshot < 0
        || model.used_quota < 0
        || model.frozen_quota < 0
        || model.version <= 0
        || model.window_starts_at >= model.window_ends_at
    {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(OrganizationBudgetWindowRecord {
        id: OrganizationBudgetWindowId::new(model.id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        policy_id: OrganizationBudgetPolicyId::new(model.policy_id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        organization_id: OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        scope,
        policy_version: model.policy_version,
        limit_snapshot: Quota::new(model.limit_snapshot)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        used_quota: Quota::new(model.used_quota)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        frozen_quota: Quota::new(model.frozen_quota)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        window_starts_at: model.window_starts_at,
        window_ends_at: model.window_ends_at,
        version: model.version,
    })
}

fn window_bounds(
    period: OrganizationBudgetPeriod,
    effective_from: TimeDateTimeWithTimeZone,
    effective_until: Option<TimeDateTimeWithTimeZone>,
    now: TimeDateTimeWithTimeZone,
) -> Result<(TimeDateTimeWithTimeZone, TimeDateTimeWithTimeZone), OrganizationBudgetRepositoryError>
{
    if now < effective_from {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    let (started_at, mut ends_at) = match period {
        OrganizationBudgetPeriod::UtcDay => calendar_bounds(SubscriptionCycle::Daily, now)?,
        OrganizationBudgetPeriod::UtcWeek => calendar_bounds(SubscriptionCycle::Weekly, now)?,
        OrganizationBudgetPeriod::UtcMonth => calendar_bounds(SubscriptionCycle::Monthly, now)?,
        OrganizationBudgetPeriod::Fixed { seconds } => {
            let elapsed = now
                .unix_timestamp()
                .checked_sub(effective_from.unix_timestamp())
                .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
            let period_seconds = i64::from(seconds);
            let offset = elapsed
                .checked_div(period_seconds)
                .and_then(|periods| periods.checked_mul(period_seconds))
                .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
            let start = effective_from
                .unix_timestamp()
                .checked_add(offset)
                .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
            let end = start
                .checked_add(period_seconds)
                .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
            (
                TimeDateTimeWithTimeZone::from_unix_timestamp(start)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
                TimeDateTimeWithTimeZone::from_unix_timestamp(end)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            )
        }
    };
    if let Some(until) = effective_until
        && until < ends_at
    {
        ends_at = until;
    }
    if started_at >= ends_at || now >= ends_at {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok((started_at, ends_at))
}

fn calendar_bounds(
    cycle: SubscriptionCycle,
    now: TimeDateTimeWithTimeZone,
) -> Result<(TimeDateTimeWithTimeZone, TimeDateTimeWithTimeZone), OrganizationBudgetRepositoryError>
{
    let seconds = u64::try_from(now.unix_timestamp())
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let window = SubscriptionWindow::initial(cycle, seconds)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    Ok((
        TimeDateTimeWithTimeZone::from_unix_timestamp(
            i64::try_from(window.started_at())
                .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        )
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        TimeDateTimeWithTimeZone::from_unix_timestamp(
            i64::try_from(window.ends_at())
                .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        )
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
    ))
}
