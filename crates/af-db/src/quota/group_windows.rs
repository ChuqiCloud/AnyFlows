use af_domain::{Quota, QuotaWindowRetryAfter, SubscriptionCycle, SubscriptionWindow};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, IntoColumnRef, Query, SimpleExpr},
};

use crate::entity::{
    BillingReservationKey, billing_group_window_reservations, billing_reservations, groups,
};

use super::{
    state::{
        GroupQuotaState, QuotaRepositoryError, QuotaReservationKind, QuotaReservationStatus,
        ReservationState,
    },
    subscription,
};

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const COMMITTED_DAILY_ALIAS: &str = "committed_daily";
const COMMITTED_WEEKLY_ALIAS: &str = "committed_weekly";
const COMMITTED_MONTHLY_ALIAS: &str = "committed_monthly";

/// 单次请求绑定的分组日、周、月共享额度窗口快照。
#[derive(Clone)]
pub(super) struct GroupWindowAllocation {
    key: BillingReservationKey,
    daily_window_start: TimeDateTimeWithTimeZone,
    weekly_window_start: TimeDateTimeWithTimeZone,
    monthly_window_start: TimeDateTimeWithTimeZone,
    reserved_quota: i64,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

/// 单个 UTC 日历窗口的持久化策略与当前状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowState {
    cycle: SubscriptionCycle,
    limit: Option<i64>,
    usage: i64,
    started_at: TimeDateTimeWithTimeZone,
}

/// 同一分组必须在行锁内原子检查的三段共享额度窗口。
#[derive(Clone, Copy)]
struct GroupWindows {
    daily: WindowState,
    weekly: WindowState,
    monthly: WindowState,
}

/// 当前窗口中仍未转为已结算用量的额度总和。
#[derive(Clone, Copy)]
struct WindowCommitments {
    daily: i64,
    weekly: i64,
    monthly: i64,
}

/// 在分组行锁内检查三段窗口容量，并固化本次请求的不可变起点。
pub(super) async fn reserve(
    transaction: &DatabaseTransaction,
    reservation_key: &str,
    parent: &ReservationState,
    group: GroupQuotaState,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    if parent.status != QuotaReservationStatus::Reserved
        || !parent.reservation_kind.uses_group_windows()
        || parent.group_id != group.group_id
        || parent.reserved_quota != amount.units()
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    let (windows, reset) = GroupWindows::from_group(group).normalized(now)?;
    if reset {
        persist_reset(transaction, group, windows, now).await?;
    }
    let commitments =
        active_commitments(transaction, parent.group_id, windows, Some(reservation_key)).await?;
    windows.ensure_precharge_capacity(commitments, amount, now)?;
    persist_allocation(transaction, reservation_key, windows, amount, now).await
}

/// 读取并校验父预留必须具有的分组窗口快照。
pub(super) async fn load_allocation<C>(
    connection: &C,
    reservation_key: &str,
    parent: &ReservationState,
) -> Result<Option<GroupWindowAllocation>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    let key = parse_key(reservation_key)?;
    let model = billing_group_window_reservations::Entity::find_by_id(key.clone())
        .one(connection)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    match (parent.reservation_kind.uses_group_windows(), model) {
        (true, Some(model)) => {
            let allocation = GroupWindowAllocation {
                key,
                daily_window_start: model.daily_window_start,
                weekly_window_start: model.weekly_window_start,
                monthly_window_start: model.monthly_window_start,
                reserved_quota: model.reserved_quota,
                created_at: model.created_at,
                updated_at: model.updated_at,
            };
            validate_allocation(&allocation, parent)?;
            Ok(Some(allocation))
        }
        (false, None) => Ok(None),
        (true, None) | (false, Some(_)) => Err(QuotaRepositoryError::Invariant),
    }
}

/// 将已固化的实际额度计入仍与预留快照匹配的分组窗口。
pub(super) async fn apply_settlement(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    allocation: Option<&GroupWindowAllocation>,
    group: GroupQuotaState,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    if parent.status != QuotaReservationStatus::SettlementPending
        || parent.group_id != group.group_id
        || !parent.matches_actual(actual)
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    if !parent.reservation_kind.uses_group_windows() {
        return allocation
            .is_none()
            .then_some(())
            .ok_or(QuotaRepositoryError::Invariant);
    }
    let Some(allocation) = allocation else {
        return Err(QuotaRepositoryError::Invariant);
    };
    let (windows, reset) = GroupWindows::from_group(group).normalized(now)?;
    if reset {
        persist_reset(transaction, group, windows, now).await?;
    }
    let commitments = active_commitments(transaction, parent.group_id, windows, None).await?;
    windows.ensure_settlement_capacity(allocation, commitments, actual, now)?;
    persist_actual(transaction, parent, windows, allocation, actual, now).await
}

impl GroupWindows {
    fn from_group(group: GroupQuotaState) -> Self {
        Self {
            daily: WindowState {
                cycle: SubscriptionCycle::Daily,
                limit: group.daily_limit,
                usage: group.daily_usage,
                started_at: group.daily_window_start,
            },
            weekly: WindowState {
                cycle: SubscriptionCycle::Weekly,
                limit: group.weekly_limit,
                usage: group.weekly_usage,
                started_at: group.weekly_window_start,
            },
            monthly: WindowState {
                cycle: SubscriptionCycle::Monthly,
                limit: group.monthly_limit,
                usage: group.monthly_usage,
                started_at: group.monthly_window_start,
            },
        }
    }

    fn normalized(
        self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(Self, bool), QuotaRepositoryError> {
        let (daily, reset_daily) = self.daily.normalized(now)?;
        let (weekly, reset_weekly) = self.weekly.normalized(now)?;
        let (monthly, reset_monthly) = self.monthly.normalized(now)?;
        Ok((
            Self {
                daily,
                weekly,
                monthly,
            },
            reset_daily || reset_weekly || reset_monthly,
        ))
    }

    fn ensure_precharge_capacity(
        self,
        commitments: WindowCommitments,
        amount: Quota,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(), QuotaRepositoryError> {
        capacity_result(
            [
                (self.daily, commitments.daily),
                (self.weekly, commitments.weekly),
                (self.monthly, commitments.monthly),
            ],
            amount,
            now,
            true,
        )
    }

    fn ensure_settlement_capacity(
        self,
        allocation: &GroupWindowAllocation,
        commitments: WindowCommitments,
        actual: Quota,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(), QuotaRepositoryError> {
        let mut relevant = Vec::with_capacity(3);
        for (window, committed, snapshot) in [
            (self.daily, commitments.daily, allocation.daily_window_start),
            (
                self.weekly,
                commitments.weekly,
                allocation.weekly_window_start,
            ),
            (
                self.monthly,
                commitments.monthly,
                allocation.monthly_window_start,
            ),
        ] {
            if window.started_at == snapshot {
                if committed < actual.units() {
                    return Err(QuotaRepositoryError::Invariant);
                }
                relevant.push((window, committed));
            }
        }
        capacity_result(relevant, actual, now, false)
    }
}

impl WindowState {
    fn normalized(
        self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(Self, bool), QuotaRepositoryError> {
        if self.limit.is_some_and(|limit| limit < 0)
            || self.usage < 0
            || self.started_at.unix_timestamp_nanos() < 0
            || self.started_at > now
        {
            return Err(QuotaRepositoryError::Invariant);
        }
        let current_start = calendar_window_start(self.cycle, now)?;
        let observed_start = match calendar_window_start(self.cycle, self.started_at) {
            Ok(started_at) => started_at,
            Err(_) if self.usage == 0 => {
                // 早期迁移常量可能无法形成完整周期；零用量记录可以无损回到当前窗口。
                return Ok((
                    Self {
                        started_at: current_start,
                        ..self
                    },
                    true,
                ));
            }
            Err(error) => return Err(error),
        };
        if self.started_at != observed_start {
            // 旧写入口可能留下非日历起点；只有零用量状态可以无损归一化。
            if self.usage != 0 {
                return Err(QuotaRepositoryError::Invariant);
            }
            return Ok((
                Self {
                    started_at: current_start,
                    ..self
                },
                true,
            ));
        }
        if self.started_at < current_start {
            return Ok((
                Self {
                    usage: 0,
                    started_at: current_start,
                    ..self
                },
                true,
            ));
        }
        if self.started_at > current_start {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok((self, false))
    }

    fn ends_at_nanos(self) -> Result<i128, QuotaRepositoryError> {
        let seconds = timestamp_seconds(self.started_at)?;
        let window = SubscriptionWindow::initial(self.cycle, seconds)
            .map_err(|_| QuotaRepositoryError::Invariant)?;
        i128::from(window.ends_at())
            .checked_mul(NANOS_PER_SECOND)
            .ok_or(QuotaRepositoryError::Invariant)
    }

    fn retry_after(self, now: TimeDateTimeWithTimeZone) -> Result<u64, QuotaRepositoryError> {
        let remaining = self
            .ends_at_nanos()?
            .checked_sub(now.unix_timestamp_nanos())
            .ok_or(QuotaRepositoryError::Invariant)?;
        if remaining <= 0 {
            return Err(QuotaRepositoryError::Invariant);
        }
        remaining
            .checked_add(NANOS_PER_SECOND - 1)
            .and_then(|value| value.checked_div(NANOS_PER_SECOND))
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(QuotaRepositoryError::Invariant)
    }
}

fn capacity_result<I>(
    windows: I,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
    add_amount: bool,
) -> Result<(), QuotaRepositoryError>
where
    I: IntoIterator<Item = (WindowState, i64)>,
{
    let mut hard_limit = false;
    let mut retry_after = 0_u64;
    for (window, committed) in windows {
        if committed < 0 {
            return Err(QuotaRepositoryError::Invariant);
        }
        let committed_usage = window
            .usage
            .checked_add(committed)
            .ok_or(QuotaRepositoryError::Invariant)?;
        let projected = if add_amount {
            committed_usage
                .checked_add(amount.units())
                .ok_or(QuotaRepositoryError::Invariant)?
        } else {
            committed_usage
        };
        let Some(limit) = window.limit else {
            continue;
        };
        if projected <= limit {
            continue;
        }
        if amount.units() > limit {
            hard_limit = true;
        } else {
            retry_after = retry_after.max(window.retry_after(now)?);
        }
    }
    if !hard_limit && retry_after == 0 {
        return Ok(());
    }
    let retry_after = if hard_limit {
        None
    } else {
        Some(
            QuotaWindowRetryAfter::from_seconds(retry_after)
                .ok_or(QuotaRepositoryError::Invariant)?,
        )
    };
    Err(QuotaRepositoryError::GroupWindowQuotaInsufficient { retry_after })
}

async fn persist_reset(
    transaction: &DatabaseTransaction,
    observed: GroupQuotaState,
    windows: GroupWindows,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    let backend = transaction.get_database_backend();
    let result = groups::Entity::update_many()
        .filter(groups::Column::Id.eq(observed.group_id))
        .filter(groups::Column::DailyUsage.eq(observed.daily_usage))
        .filter(groups::Column::WeeklyUsage.eq(observed.weekly_usage))
        .filter(groups::Column::MonthlyUsage.eq(observed.monthly_usage))
        .filter(timestamp_eq(
            backend,
            groups::Column::DailyWindowStart,
            observed.daily_window_start,
        ))
        .filter(timestamp_eq(
            backend,
            groups::Column::WeeklyWindowStart,
            observed.weekly_window_start,
        ))
        .filter(timestamp_eq(
            backend,
            groups::Column::MonthlyWindowStart,
            observed.monthly_window_start,
        ))
        .col_expr(groups::Column::DailyUsage, Expr::value(windows.daily.usage))
        .col_expr(
            groups::Column::WeeklyUsage,
            Expr::value(windows.weekly.usage),
        )
        .col_expr(
            groups::Column::MonthlyUsage,
            Expr::value(windows.monthly.usage),
        )
        .col_expr(
            groups::Column::DailyWindowStart,
            Expr::value(windows.daily.started_at),
        )
        .col_expr(
            groups::Column::WeeklyWindowStart,
            Expr::value(windows.weekly.started_at),
        )
        .col_expr(
            groups::Column::MonthlyWindowStart,
            Expr::value(windows.monthly.started_at),
        )
        .col_expr(groups::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

async fn persist_allocation(
    transaction: &DatabaseTransaction,
    reservation_key: &str,
    windows: GroupWindows,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    billing_group_window_reservations::ActiveModel {
        idempotency_key: Set(parse_key(reservation_key)?),
        daily_window_start: Set(windows.daily.started_at),
        weekly_window_start: Set(windows.weekly.started_at),
        monthly_window_start: Set(windows.monthly.started_at),
        reserved_quota: Set(amount.units()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(transaction)
    .await
    .map_err(|_| QuotaRepositoryError::Query)?;
    Ok(())
}

async fn persist_actual(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    windows: GroupWindows,
    allocation: &GroupWindowAllocation,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    let matches_daily = windows.daily.started_at == allocation.daily_window_start;
    let matches_weekly = windows.weekly.started_at == allocation.weekly_window_start;
    let matches_monthly = windows.monthly.started_at == allocation.monthly_window_start;
    if !matches_daily && !matches_weekly && !matches_monthly {
        return Ok(());
    }
    let backend = transaction.get_database_backend();
    let mut update = groups::Entity::update_many()
        .filter(groups::Column::Id.eq(parent.group_id))
        .filter(timestamp_eq(
            backend,
            groups::Column::DailyWindowStart,
            windows.daily.started_at,
        ))
        .filter(timestamp_eq(
            backend,
            groups::Column::WeeklyWindowStart,
            windows.weekly.started_at,
        ))
        .filter(timestamp_eq(
            backend,
            groups::Column::MonthlyWindowStart,
            windows.monthly.started_at,
        ))
        .col_expr(groups::Column::UpdatedAt, Expr::value(now));
    if matches_daily {
        update = update
            .filter(groups::Column::DailyUsage.eq(windows.daily.usage))
            .col_expr(
                groups::Column::DailyUsage,
                Expr::col(groups::Column::DailyUsage).add(actual.units()),
            );
    }
    if matches_weekly {
        update = update
            .filter(groups::Column::WeeklyUsage.eq(windows.weekly.usage))
            .col_expr(
                groups::Column::WeeklyUsage,
                Expr::col(groups::Column::WeeklyUsage).add(actual.units()),
            );
    }
    if matches_monthly {
        update = update
            .filter(groups::Column::MonthlyUsage.eq(windows.monthly.usage))
            .col_expr(
                groups::Column::MonthlyUsage,
                Expr::col(groups::Column::MonthlyUsage).add(actual.units()),
            );
    }
    let result = update
        .exec(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

async fn active_commitments(
    transaction: &DatabaseTransaction,
    group_id: i64,
    windows: GroupWindows,
    excluded_key: Option<&str>,
) -> Result<WindowCommitments, QuotaRepositoryError> {
    let backend = transaction.get_database_backend();
    if has_invalid_active_allocation(transaction, group_id, excluded_key).await? {
        return Err(QuotaRepositoryError::Invariant);
    }
    let status = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::Status,
    ));
    let committed = Expr::case(
        status.eq(QuotaReservationStatus::Reserved.code()),
        Expr::col((
            billing_group_window_reservations::Entity,
            billing_group_window_reservations::Column::ReservedQuota,
        )),
    )
    .finally(Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::ActualQuota,
    )));
    let query = Query::select()
        .expr_as(
            Func::sum(
                Expr::case(
                    timestamp_eq(
                        backend,
                        (
                            billing_group_window_reservations::Entity,
                            billing_group_window_reservations::Column::DailyWindowStart,
                        ),
                        windows.daily.started_at,
                    ),
                    committed.clone(),
                )
                .finally(0_i64),
            ),
            Alias::new(COMMITTED_DAILY_ALIAS),
        )
        .expr_as(
            Func::sum(
                Expr::case(
                    timestamp_eq(
                        backend,
                        (
                            billing_group_window_reservations::Entity,
                            billing_group_window_reservations::Column::WeeklyWindowStart,
                        ),
                        windows.weekly.started_at,
                    ),
                    committed.clone(),
                )
                .finally(0_i64),
            ),
            Alias::new(COMMITTED_WEEKLY_ALIAS),
        )
        .expr_as(
            Func::sum(
                Expr::case(
                    timestamp_eq(
                        backend,
                        (
                            billing_group_window_reservations::Entity,
                            billing_group_window_reservations::Column::MonthlyWindowStart,
                        ),
                        windows.monthly.started_at,
                    ),
                    committed,
                )
                .finally(0_i64),
            ),
            Alias::new(COMMITTED_MONTHLY_ALIAS),
        )
        .from(billing_group_window_reservations::Entity)
        .inner_join(
            billing_reservations::Entity,
            Expr::col((
                billing_group_window_reservations::Entity,
                billing_group_window_reservations::Column::IdempotencyKey,
            ))
            .equals((
                billing_reservations::Entity,
                billing_reservations::Column::IdempotencyKey,
            )),
        )
        .and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::GroupId,
            ))
            .eq(group_id),
        )
        .and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::Status,
            ))
            .is_in([
                QuotaReservationStatus::Reserved.code(),
                QuotaReservationStatus::SettlementPending.code(),
            ]),
        )
        .to_owned();
    let result = transaction
        .query_one(backend.build(&query))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
        .ok_or(QuotaRepositoryError::Invariant)?;
    Ok(WindowCommitments {
        daily: subscription::read_sum(&result, COMMITTED_DAILY_ALIAS, backend)?,
        weekly: subscription::read_sum(&result, COMMITTED_WEEKLY_ALIAS, backend)?,
        monthly: subscription::read_sum(&result, COMMITTED_MONTHLY_ALIAS, backend)?,
    })
}

async fn has_invalid_active_allocation(
    transaction: &DatabaseTransaction,
    group_id: i64,
    excluded_key: Option<&str>,
) -> Result<bool, QuotaRepositoryError> {
    let reservation_kind = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::ReservationKind,
    ));
    let allocation_key = Expr::col((
        billing_group_window_reservations::Entity,
        billing_group_window_reservations::Column::IdempotencyKey,
    ));
    let status = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::Status,
    ));
    let actual = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::ActualQuota,
    ));
    let invalid_request = Condition::any()
        .add(allocation_key.clone().is_null())
        .add(
            Expr::col((
                billing_group_window_reservations::Entity,
                billing_group_window_reservations::Column::ReservedQuota,
            ))
            .ne(Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::ReservedQuota,
            ))),
        )
        .add(
            Expr::col((
                billing_group_window_reservations::Entity,
                billing_group_window_reservations::Column::CreatedAt,
            ))
            .ne(Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::CreatedAt,
            ))),
        )
        .add(
            Expr::col((
                billing_group_window_reservations::Entity,
                billing_group_window_reservations::Column::UpdatedAt,
            ))
            .ne(Expr::col((
                billing_group_window_reservations::Entity,
                billing_group_window_reservations::Column::CreatedAt,
            ))),
        )
        .add(
            status
                .clone()
                .eq(QuotaReservationStatus::Reserved.code())
                .and(actual.clone().is_not_null()),
        )
        .add(
            status
                .eq(QuotaReservationStatus::SettlementPending.code())
                .and(actual.clone().is_null().or(actual.lt(0_i64))),
        );
    // 普通请求必须有窗口快照，批量任务必须没有；两类在途记录可以安全共存。
    let invalid = Condition::any()
        .add(
            Condition::all()
                .add(
                    reservation_kind
                        .clone()
                        .eq(QuotaReservationKind::Request.code()),
                )
                .add(invalid_request),
        )
        .add(
            Condition::all()
                .add(
                    reservation_kind
                        .clone()
                        .eq(QuotaReservationKind::BatchTask.code()),
                )
                .add(allocation_key.is_not_null()),
        )
        .add(reservation_kind.is_not_in([
            QuotaReservationKind::Request.code(),
            QuotaReservationKind::BatchTask.code(),
        ]));
    let mut query = Query::select();
    query
        .column((
            billing_reservations::Entity,
            billing_reservations::Column::IdempotencyKey,
        ))
        .from(billing_reservations::Entity)
        .left_join(
            billing_group_window_reservations::Entity,
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::IdempotencyKey,
            ))
            .equals((
                billing_group_window_reservations::Entity,
                billing_group_window_reservations::Column::IdempotencyKey,
            )),
        )
        .and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::GroupId,
            ))
            .eq(group_id),
        )
        .and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::Status,
            ))
            .is_in([
                QuotaReservationStatus::Reserved.code(),
                QuotaReservationStatus::SettlementPending.code(),
            ]),
        );
    if let Some(excluded_key) = excluded_key {
        query.and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::IdempotencyKey,
            ))
            .ne(excluded_key),
        );
    }
    let query = query.cond_where(invalid).limit(1).to_owned();
    Ok(transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
        .is_some())
}

fn validate_allocation(
    allocation: &GroupWindowAllocation,
    parent: &ReservationState,
) -> Result<(), QuotaRepositoryError> {
    if allocation.reserved_quota != parent.reserved_quota
        || allocation.reserved_quota <= 0
        || allocation.updated_at != allocation.created_at
        || [
            (allocation.daily_window_start, SubscriptionCycle::Daily),
            (allocation.weekly_window_start, SubscriptionCycle::Weekly),
            (allocation.monthly_window_start, SubscriptionCycle::Monthly),
        ]
        .into_iter()
        .any(|(started_at, cycle)| {
            !snapshot_contains_created_at(started_at, cycle, allocation.created_at)
        })
        || allocation.key.as_str().is_empty()
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

fn snapshot_contains_created_at(
    started_at: TimeDateTimeWithTimeZone,
    cycle: SubscriptionCycle,
    created_at: TimeDateTimeWithTimeZone,
) -> bool {
    if started_at.unix_timestamp_nanos() < 0 || started_at > created_at {
        return false;
    }
    let Ok(seconds) = timestamp_seconds(started_at) else {
        return false;
    };
    SubscriptionWindow::initial(cycle, seconds)
        .ok()
        .and_then(|window| i128::from(window.ends_at()).checked_mul(NANOS_PER_SECOND))
        .is_some_and(|ends_at| created_at.unix_timestamp_nanos() < ends_at)
}

fn calendar_window_start(
    cycle: SubscriptionCycle,
    at: TimeDateTimeWithTimeZone,
) -> Result<TimeDateTimeWithTimeZone, QuotaRepositoryError> {
    let window = SubscriptionWindow::initial(cycle, timestamp_seconds(at)?)
        .map_err(|_| QuotaRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(
        i64::try_from(window.started_at()).map_err(|_| QuotaRepositoryError::Invariant)?,
    )
    .map_err(|_| QuotaRepositoryError::Invariant)
}

fn timestamp_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, QuotaRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| QuotaRepositoryError::Invariant)
}

/// SQLite 会混用原生时间文本与 RFC3339 参数，等值条件必须按时间值比较。
fn timestamp_eq<C>(backend: DbBackend, column: C, value: TimeDateTimeWithTimeZone) -> SimpleExpr
where
    C: IntoColumnRef,
{
    if backend != DbBackend::Sqlite {
        return Expr::col(column).eq(value);
    }
    let column: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::col(column))
        .into();
    let value: SimpleExpr = Func::cust(Alias::new("julianday"))
        .arg(Expr::value(value))
        .into();
    column.eq(value)
}

fn parse_key(value: &str) -> Result<BillingReservationKey, QuotaRepositoryError> {
    BillingReservationKey::parse(value).map_err(|_| QuotaRepositoryError::Invariant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_calendar_windows_normalize_to_day_week_and_month_boundaries() {
        let now =
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_769_860_800).expect("测试时间必须有效");
        let windows = GroupWindows {
            daily: WindowState {
                cycle: SubscriptionCycle::Daily,
                limit: None,
                usage: 0,
                started_at: now,
            },
            weekly: WindowState {
                cycle: SubscriptionCycle::Weekly,
                limit: None,
                usage: 0,
                started_at: now,
            },
            monthly: WindowState {
                cycle: SubscriptionCycle::Monthly,
                limit: None,
                usage: 0,
                started_at: now,
            },
        };

        let (normalized, reset) = windows.normalized(now).expect("日历窗口必须可归一化");
        assert!(reset);
        assert_eq!(normalized.daily.started_at.unix_timestamp(), 1_769_817_600);
        assert_eq!(normalized.weekly.started_at.unix_timestamp(), 1_769_385_600);
        assert_eq!(
            normalized.monthly.started_at.unix_timestamp(),
            1_767_225_600
        );
    }

    #[test]
    fn noncanonical_window_with_usage_fails_closed() {
        let now =
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_769_860_800).expect("测试时间必须有效");
        let state = WindowState {
            cycle: SubscriptionCycle::Daily,
            limit: None,
            usage: 1,
            started_at: now,
        };
        assert_eq!(state.normalized(now), Err(QuotaRepositoryError::Invariant));
    }

    #[test]
    fn zero_usage_epoch_weekly_anchor_recovers_but_used_state_fails_closed() {
        let now =
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_769_860_800).expect("测试时间必须有效");
        let state = WindowState {
            cycle: SubscriptionCycle::Weekly,
            limit: None,
            usage: 0,
            started_at: TimeDateTimeWithTimeZone::UNIX_EPOCH,
        };

        let (normalized, reset) = state.normalized(now).expect("零用量旧锚点应可安全恢复");
        assert!(reset);
        assert_eq!(normalized.usage, 0);
        assert_eq!(
            normalized.started_at,
            calendar_window_start(SubscriptionCycle::Weekly, now).unwrap()
        );
        assert_eq!(
            WindowState { usage: 1, ..state }.normalized(now),
            Err(QuotaRepositoryError::Invariant)
        );
    }

    #[test]
    fn monthly_window_preserves_retry_after_beyond_upstream_seven_day_bound() {
        let started_at =
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_767_225_600).expect("月初时间必须有效");
        let now =
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_767_225_601).expect("测试时间必须有效");
        let window = WindowState {
            cycle: SubscriptionCycle::Monthly,
            limit: Some(100),
            usage: 100,
            started_at,
        };
        let now_seconds = u64::try_from(now.unix_timestamp()).unwrap();
        let expected = SubscriptionWindow::initial(SubscriptionCycle::Monthly, now_seconds)
            .unwrap()
            .ends_at()
            - now_seconds;

        let error = capacity_result([(window, 0)], Quota::new(1).unwrap(), now, true)
            .expect_err("月窗口已满时必须返回可恢复等待时间");
        let QuotaRepositoryError::GroupWindowQuotaInsufficient {
            retry_after: Some(retry_after),
        } = error
        else {
            panic!("月窗口不足返回了意外错误：{error:?}");
        };
        assert!(expected > af_domain::UpstreamRetryAfter::MAX_SECONDS);
        assert_eq!(u64::from(retry_after.seconds()), expected);
    }
}
