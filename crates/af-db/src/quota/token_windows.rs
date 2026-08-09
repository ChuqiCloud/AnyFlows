use af_domain::{Quota, QuotaWindowRetryAfter};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, IntoColumnRef, Query, SimpleExpr},
};

use crate::entity::{
    BillingReservationKey, billing_reservations, billing_token_window_reservations, tokens,
};

use super::{
    state::{
        QuotaRepositoryError, QuotaReservationKind, QuotaReservationStatus, ReservationState,
        TokenQuotaState,
    },
    subscription,
};

const WINDOW_5H_SECONDS: i64 = 5 * 60 * 60;
const WINDOW_1D_SECONDS: i64 = 24 * 60 * 60;
const WINDOW_7D_SECONDS: i64 = 7 * 24 * 60 * 60;
const NANOS_PER_SECOND: i128 = 1_000_000_000;
const COMMITTED_5H_ALIAS: &str = "committed_5h";
const COMMITTED_1D_ALIAS: &str = "committed_1d";
const COMMITTED_7D_ALIAS: &str = "committed_7d";

/// 单次请求绑定的三段令牌额度窗口快照。
#[derive(Clone)]
pub(super) struct TokenWindowAllocation {
    key: BillingReservationKey,
    window_5h_start: TimeDateTimeWithTimeZone,
    window_1d_start: TimeDateTimeWithTimeZone,
    window_7d_start: TimeDateTimeWithTimeZone,
    reserved_quota: i64,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

/// 单个固定窗口的持久化策略与当前状态。
#[derive(Clone, Copy)]
struct WindowState {
    limit: Option<i64>,
    usage: i64,
    started_at: TimeDateTimeWithTimeZone,
    duration_seconds: i64,
}

/// 同一令牌必须原子检查的三段额度窗口。
#[derive(Clone, Copy)]
struct TokenWindows {
    five_hours: WindowState,
    one_day: WindowState,
    seven_days: WindowState,
}

/// 当前窗口中仍未转为已结算用量的额度总和。
#[derive(Clone, Copy)]
struct WindowCommitments {
    five_hours: i64,
    one_day: i64,
    seven_days: i64,
}

/// 在令牌行锁内检查三段窗口容量，并固化本次请求的不可变起点。
pub(super) async fn reserve(
    transaction: &DatabaseTransaction,
    reservation_key: &str,
    parent: &ReservationState,
    token: TokenQuotaState,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    if parent.status != QuotaReservationStatus::Reserved
        || !parent.reservation_kind.uses_token_windows()
        || parent.reserved_quota != amount.units()
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    let (windows, reset) = TokenWindows::from_token(token).normalized(now)?;
    if reset {
        persist_reset(transaction, parent, token, windows, now).await?;
    }
    let commitments =
        active_commitments(transaction, parent.token_id, windows, Some(reservation_key)).await?;
    windows.ensure_precharge_capacity(commitments, amount, now)?;
    persist_allocation(transaction, reservation_key, windows, amount, now).await
}

/// 读取并校验父预留必须具有的令牌窗口快照。
pub(super) async fn load_allocation<C>(
    connection: &C,
    reservation_key: &str,
    parent: &ReservationState,
) -> Result<Option<TokenWindowAllocation>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    let key = parse_key(reservation_key)?;
    let model = billing_token_window_reservations::Entity::find_by_id(key.clone())
        .one(connection)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    match (parent.reservation_kind.uses_token_windows(), model) {
        (true, Some(model)) => {
            let allocation = TokenWindowAllocation {
                key,
                window_5h_start: model.window_5h_start,
                window_1d_start: model.window_1d_start,
                window_7d_start: model.window_7d_start,
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

/// 将已固化的实际额度计入仍与预留快照匹配的窗口。
pub(super) async fn apply_settlement(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    allocation: Option<&TokenWindowAllocation>,
    token: TokenQuotaState,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    if parent.status != QuotaReservationStatus::SettlementPending || !parent.matches_actual(actual)
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    if !parent.reservation_kind.uses_token_windows() {
        return allocation
            .is_none()
            .then_some(())
            .ok_or(QuotaRepositoryError::Invariant);
    }
    let Some(allocation) = allocation else {
        return Err(QuotaRepositoryError::Invariant);
    };
    let (windows, reset) = TokenWindows::from_token(token).normalized(now)?;
    if reset {
        persist_reset(transaction, parent, token, windows, now).await?;
    }
    let commitments = active_commitments(transaction, parent.token_id, windows, None).await?;
    windows.ensure_settlement_capacity(allocation, commitments, actual, now)?;
    persist_actual(transaction, parent, windows, allocation, actual, now).await
}

impl TokenWindows {
    fn from_token(token: TokenQuotaState) -> Self {
        Self {
            five_hours: WindowState {
                limit: token.rate_limit_5h,
                usage: token.usage_5h,
                started_at: token.window_5h_start,
                duration_seconds: WINDOW_5H_SECONDS,
            },
            one_day: WindowState {
                limit: token.rate_limit_1d,
                usage: token.usage_1d,
                started_at: token.window_1d_start,
                duration_seconds: WINDOW_1D_SECONDS,
            },
            seven_days: WindowState {
                limit: token.rate_limit_7d,
                usage: token.usage_7d,
                started_at: token.window_7d_start,
                duration_seconds: WINDOW_7D_SECONDS,
            },
        }
    }

    fn normalized(
        self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(Self, bool), QuotaRepositoryError> {
        let (five_hours, reset_5h) = self.five_hours.normalized(now)?;
        let (one_day, reset_1d) = self.one_day.normalized(now)?;
        let (seven_days, reset_7d) = self.seven_days.normalized(now)?;
        Ok((
            Self {
                five_hours,
                one_day,
                seven_days,
            },
            reset_5h || reset_1d || reset_7d,
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
                (self.five_hours, commitments.five_hours),
                (self.one_day, commitments.one_day),
                (self.seven_days, commitments.seven_days),
            ],
            amount,
            now,
            true,
        )
    }

    fn ensure_settlement_capacity(
        self,
        allocation: &TokenWindowAllocation,
        commitments: WindowCommitments,
        actual: Quota,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(), QuotaRepositoryError> {
        let mut relevant = Vec::with_capacity(3);
        for (window, committed, snapshot) in [
            (
                self.five_hours,
                commitments.five_hours,
                allocation.window_5h_start,
            ),
            (
                self.one_day,
                commitments.one_day,
                allocation.window_1d_start,
            ),
            (
                self.seven_days,
                commitments.seven_days,
                allocation.window_7d_start,
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
        if now.unix_timestamp_nanos() >= self.ends_at_nanos()? {
            return Ok((
                Self {
                    usage: 0,
                    started_at: now,
                    ..self
                },
                true,
            ));
        }
        Ok((self, false))
    }

    fn ends_at_nanos(self) -> Result<i128, QuotaRepositoryError> {
        self.started_at
            .unix_timestamp_nanos()
            .checked_add(i128::from(self.duration_seconds) * NANOS_PER_SECOND)
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
        let seconds = remaining
            .checked_add(NANOS_PER_SECOND - 1)
            .and_then(|value| value.checked_div(NANOS_PER_SECOND))
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(QuotaRepositoryError::Invariant)?;
        Ok(seconds)
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
    Err(QuotaRepositoryError::TokenWindowQuotaInsufficient { retry_after })
}

async fn persist_reset(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    observed: TokenQuotaState,
    windows: TokenWindows,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    let backend = transaction.get_database_backend();
    let result = tokens::Entity::update_many()
        .filter(tokens::Column::Id.eq(parent.token_id))
        .filter(tokens::Column::UserId.eq(parent.user_id))
        .filter(tokens::Column::Usage5h.eq(observed.usage_5h))
        .filter(tokens::Column::Usage1d.eq(observed.usage_1d))
        .filter(tokens::Column::Usage7d.eq(observed.usage_7d))
        .filter(timestamp_eq(
            backend,
            tokens::Column::Window5hStart,
            observed.window_5h_start,
        ))
        .filter(timestamp_eq(
            backend,
            tokens::Column::Window1dStart,
            observed.window_1d_start,
        ))
        .filter(timestamp_eq(
            backend,
            tokens::Column::Window7dStart,
            observed.window_7d_start,
        ))
        .col_expr(
            tokens::Column::Usage5h,
            Expr::value(windows.five_hours.usage),
        )
        .col_expr(tokens::Column::Usage1d, Expr::value(windows.one_day.usage))
        .col_expr(
            tokens::Column::Usage7d,
            Expr::value(windows.seven_days.usage),
        )
        .col_expr(
            tokens::Column::Window5hStart,
            Expr::value(windows.five_hours.started_at),
        )
        .col_expr(
            tokens::Column::Window1dStart,
            Expr::value(windows.one_day.started_at),
        )
        .col_expr(
            tokens::Column::Window7dStart,
            Expr::value(windows.seven_days.started_at),
        )
        .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
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
    windows: TokenWindows,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    billing_token_window_reservations::ActiveModel {
        idempotency_key: Set(parse_key(reservation_key)?),
        window_5h_start: Set(windows.five_hours.started_at),
        window_1d_start: Set(windows.one_day.started_at),
        window_7d_start: Set(windows.seven_days.started_at),
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
    windows: TokenWindows,
    allocation: &TokenWindowAllocation,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    let matches_5h = windows.five_hours.started_at == allocation.window_5h_start;
    let matches_1d = windows.one_day.started_at == allocation.window_1d_start;
    let matches_7d = windows.seven_days.started_at == allocation.window_7d_start;
    if !matches_5h && !matches_1d && !matches_7d {
        return Ok(());
    }
    let backend = transaction.get_database_backend();
    let mut update = tokens::Entity::update_many()
        .filter(tokens::Column::Id.eq(parent.token_id))
        .filter(tokens::Column::UserId.eq(parent.user_id))
        .filter(timestamp_eq(
            backend,
            tokens::Column::Window5hStart,
            windows.five_hours.started_at,
        ))
        .filter(timestamp_eq(
            backend,
            tokens::Column::Window1dStart,
            windows.one_day.started_at,
        ))
        .filter(timestamp_eq(
            backend,
            tokens::Column::Window7dStart,
            windows.seven_days.started_at,
        ))
        .col_expr(tokens::Column::UpdatedAt, Expr::value(now));
    if matches_5h {
        update = update
            .filter(tokens::Column::Usage5h.eq(windows.five_hours.usage))
            .col_expr(
                tokens::Column::Usage5h,
                Expr::col(tokens::Column::Usage5h).add(actual.units()),
            );
    }
    if matches_1d {
        update = update
            .filter(tokens::Column::Usage1d.eq(windows.one_day.usage))
            .col_expr(
                tokens::Column::Usage1d,
                Expr::col(tokens::Column::Usage1d).add(actual.units()),
            );
    }
    if matches_7d {
        update = update
            .filter(tokens::Column::Usage7d.eq(windows.seven_days.usage))
            .col_expr(
                tokens::Column::Usage7d,
                Expr::col(tokens::Column::Usage7d).add(actual.units()),
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
    token_id: i64,
    windows: TokenWindows,
    excluded_key: Option<&str>,
) -> Result<WindowCommitments, QuotaRepositoryError> {
    let backend = transaction.get_database_backend();
    if has_invalid_active_allocation(transaction, token_id, excluded_key).await? {
        return Err(QuotaRepositoryError::Invariant);
    }
    let status = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::Status,
    ));
    let committed = Expr::case(
        status.eq(QuotaReservationStatus::Reserved.code()),
        Expr::col((
            billing_token_window_reservations::Entity,
            billing_token_window_reservations::Column::ReservedQuota,
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
                            billing_token_window_reservations::Entity,
                            billing_token_window_reservations::Column::Window5hStart,
                        ),
                        windows.five_hours.started_at,
                    ),
                    committed.clone(),
                )
                .finally(0_i64),
            ),
            Alias::new(COMMITTED_5H_ALIAS),
        )
        .expr_as(
            Func::sum(
                Expr::case(
                    timestamp_eq(
                        backend,
                        (
                            billing_token_window_reservations::Entity,
                            billing_token_window_reservations::Column::Window1dStart,
                        ),
                        windows.one_day.started_at,
                    ),
                    committed.clone(),
                )
                .finally(0_i64),
            ),
            Alias::new(COMMITTED_1D_ALIAS),
        )
        .expr_as(
            Func::sum(
                Expr::case(
                    timestamp_eq(
                        backend,
                        (
                            billing_token_window_reservations::Entity,
                            billing_token_window_reservations::Column::Window7dStart,
                        ),
                        windows.seven_days.started_at,
                    ),
                    committed,
                )
                .finally(0_i64),
            ),
            Alias::new(COMMITTED_7D_ALIAS),
        )
        .from(billing_token_window_reservations::Entity)
        .inner_join(
            billing_reservations::Entity,
            Expr::col((
                billing_token_window_reservations::Entity,
                billing_token_window_reservations::Column::IdempotencyKey,
            ))
            .equals((
                billing_reservations::Entity,
                billing_reservations::Column::IdempotencyKey,
            )),
        )
        .and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::TokenId,
            ))
            .eq(token_id),
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
        five_hours: subscription::read_sum(&result, COMMITTED_5H_ALIAS, backend)?,
        one_day: subscription::read_sum(&result, COMMITTED_1D_ALIAS, backend)?,
        seven_days: subscription::read_sum(&result, COMMITTED_7D_ALIAS, backend)?,
    })
}

async fn has_invalid_active_allocation(
    transaction: &DatabaseTransaction,
    token_id: i64,
    excluded_key: Option<&str>,
) -> Result<bool, QuotaRepositoryError> {
    let reservation_kind = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::ReservationKind,
    ));
    let allocation_key = Expr::col((
        billing_token_window_reservations::Entity,
        billing_token_window_reservations::Column::IdempotencyKey,
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
                billing_token_window_reservations::Entity,
                billing_token_window_reservations::Column::ReservedQuota,
            ))
            .ne(Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::ReservedQuota,
            ))),
        )
        .add(
            Expr::col((
                billing_token_window_reservations::Entity,
                billing_token_window_reservations::Column::CreatedAt,
            ))
            .ne(Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::CreatedAt,
            ))),
        )
        .add(
            Expr::col((
                billing_token_window_reservations::Entity,
                billing_token_window_reservations::Column::UpdatedAt,
            ))
            .ne(Expr::col((
                billing_token_window_reservations::Entity,
                billing_token_window_reservations::Column::CreatedAt,
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
            billing_token_window_reservations::Entity,
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::IdempotencyKey,
            ))
            .equals((
                billing_token_window_reservations::Entity,
                billing_token_window_reservations::Column::IdempotencyKey,
            )),
        )
        .and_where(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::TokenId,
            ))
            .eq(token_id),
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
    allocation: &TokenWindowAllocation,
    parent: &ReservationState,
) -> Result<(), QuotaRepositoryError> {
    if allocation.reserved_quota != parent.reserved_quota
        || allocation.reserved_quota <= 0
        || allocation.updated_at != allocation.created_at
        || [
            (allocation.window_5h_start, WINDOW_5H_SECONDS),
            (allocation.window_1d_start, WINDOW_1D_SECONDS),
            (allocation.window_7d_start, WINDOW_7D_SECONDS),
        ]
        .into_iter()
        .any(|(started_at, duration)| {
            started_at.unix_timestamp_nanos() < 0
                || started_at > allocation.created_at
                || started_at
                    .unix_timestamp_nanos()
                    .checked_add(i128::from(duration) * NANOS_PER_SECOND)
                    .is_none_or(|ends_at| allocation.created_at.unix_timestamp_nanos() >= ends_at)
        })
        || allocation.key.as_str().is_empty()
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
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
