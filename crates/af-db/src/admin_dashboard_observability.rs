use sea_orm::{
    DbBackend, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, SelectStatement, SimpleExpr},
};

use crate::{
    admin_dashboard::{AdminDashboardRepositoryError, get_count, get_sum, record_internal_error},
    entity::usage_logs,
};

/// 管理看板固定返回的一小时分桶数量。
pub const ADMIN_DASHBOARD_BUCKET_COUNT: usize = 24;
/// 单个管理看板分桶覆盖的秒数。
pub const ADMIN_DASHBOARD_BUCKET_SECONDS: i64 = 60 * 60;
/// 首字耗时达到该阈值后计入慢首字样本。
pub const SLOW_FIRST_TOKEN_THRESHOLD_MS: i64 = 2_000;
/// 总耗时达到该阈值后计入慢请求样本。
pub const SLOW_REQUEST_THRESHOLD_MS: i64 = 10_000;

/// 管理看板单个一小时窗口的持久化用量事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardHourlyRecord {
    period_start: i64,
    period_end: i64,
    request_count: i64,
    quota_consumed: i64,
}

impl AdminDashboardHourlyRecord {
    #[must_use]
    pub const fn period_start(self) -> i64 {
        self.period_start
    }

    #[must_use]
    pub const fn period_end(self) -> i64 {
        self.period_end
    }

    #[must_use]
    pub const fn request_count(self) -> i64 {
        self.request_count
    }

    #[must_use]
    pub const fn quota_consumed(self) -> i64 {
        self.quota_consumed
    }
}

/// 管理看板窗口内有真实观测值的性能健康聚合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardPerformanceRecord {
    first_token_sample_count: i64,
    average_first_token_ms: Option<i64>,
    slow_first_token_count: i64,
    duration_sample_count: i64,
    average_duration_ms: Option<i64>,
    slow_request_count: i64,
}

impl AdminDashboardPerformanceRecord {
    #[must_use]
    pub const fn first_token_sample_count(self) -> i64 {
        self.first_token_sample_count
    }

    #[must_use]
    pub const fn average_first_token_ms(self) -> Option<i64> {
        self.average_first_token_ms
    }

    #[must_use]
    pub const fn slow_first_token_count(self) -> i64 {
        self.slow_first_token_count
    }

    #[must_use]
    pub const fn duration_sample_count(self) -> i64 {
        self.duration_sample_count
    }

    #[must_use]
    pub const fn average_duration_ms(self) -> Option<i64> {
        self.average_duration_ms
    }

    #[must_use]
    pub const fn slow_request_count(self) -> i64 {
        self.slow_request_count
    }
}

/// 管理看板时序和性能健康聚合结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdminDashboardObservabilityRecord {
    hourly: Vec<AdminDashboardHourlyRecord>,
    performance: AdminDashboardPerformanceRecord,
}

impl AdminDashboardObservabilityRecord {
    pub(crate) fn hourly(&self) -> &[AdminDashboardHourlyRecord] {
        &self.hourly
    }

    pub(crate) const fn performance(&self) -> AdminDashboardPerformanceRecord {
        self.performance
    }

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            hourly: Vec::new(),
            performance: AdminDashboardPerformanceRecord {
                first_token_sample_count: 0,
                average_first_token_ms: None,
                slow_first_token_count: 0,
                duration_sample_count: 0,
                average_duration_ms: None,
                slow_request_count: 0,
            },
        }
    }
}

/// 向总览查询追加固定 24 个分桶和性能健康表达式，避免重复扫描和快照竞争。
pub(crate) fn append_observability_expressions(
    query: &mut SelectStatement,
    period_start: i64,
    period_end: i64,
) -> Result<(), AdminDashboardRepositoryError> {
    validate_window(period_start, period_end)?;
    query
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::FirstTokenMs)).count(),
            Alias::new("first_token_sample_count"),
        )
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::FirstTokenMs)).sum(),
            Alias::new("first_token_total_ms"),
        )
        .expr_as(
            conditional_count(
                Expr::col((usage_logs::Entity, usage_logs::Column::FirstTokenMs))
                    .gte(SLOW_FIRST_TOKEN_THRESHOLD_MS),
            ),
            Alias::new("slow_first_token_count"),
        )
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::DurationMs)).count(),
            Alias::new("duration_sample_count"),
        )
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::DurationMs)).sum(),
            Alias::new("duration_total_ms"),
        )
        .expr_as(
            conditional_count(
                Expr::col((usage_logs::Entity, usage_logs::Column::DurationMs))
                    .gte(SLOW_REQUEST_THRESHOLD_MS),
            ),
            Alias::new("slow_request_count"),
        );

    for index in 0..ADMIN_DASHBOARD_BUCKET_COUNT {
        let offset = i64::try_from(index)
            .ok()
            .and_then(|value| value.checked_mul(ADMIN_DASHBOARD_BUCKET_SECONDS))
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let bucket_start = period_start
            .checked_add(offset)
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let bucket_end = bucket_start
            .checked_add(ADMIN_DASHBOARD_BUCKET_SECONDS)
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let condition = period_condition(timestamp(bucket_start)?, timestamp(bucket_end)?);
        query
            .expr_as(
                conditional_count(condition.clone()),
                Alias::new(format!("request_count_{index:02}")),
            )
            .expr_as(
                conditional_sum(condition, usage_logs::Column::Quota),
                Alias::new(format!("quota_consumed_{index:02}")),
            );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_observability(
    result: &QueryResult,
    backend: DbBackend,
    period_start: i64,
    period_end: i64,
    expected_request_count: i64,
    expected_quota_consumed: i64,
) -> Result<AdminDashboardObservabilityRecord, AdminDashboardRepositoryError> {
    validate_window(period_start, period_end)?;
    if expected_request_count < 0 || expected_quota_consumed < 0 {
        return Err(record_internal_error(
            AdminDashboardRepositoryError::Invariant,
        ));
    }
    let first_token_sample_count = get_count(result, "first_token_sample_count", backend)?;
    let first_token_total_ms = get_sum(result, "first_token_total_ms", backend)?;
    let slow_first_token_count = get_count(result, "slow_first_token_count", backend)?;
    let duration_sample_count = get_count(result, "duration_sample_count", backend)?;
    let duration_total_ms = get_sum(result, "duration_total_ms", backend)?;
    let slow_request_count = get_count(result, "slow_request_count", backend)?;
    if [
        first_token_sample_count,
        first_token_total_ms,
        slow_first_token_count,
        duration_sample_count,
        duration_total_ms,
        slow_request_count,
    ]
    .into_iter()
    .any(|value| value < 0)
        || first_token_sample_count > expected_request_count
        || duration_sample_count > expected_request_count
        || slow_first_token_count > first_token_sample_count
        || slow_request_count > duration_sample_count
    {
        return Err(record_internal_error(
            AdminDashboardRepositoryError::Invariant,
        ));
    }

    let mut hourly = Vec::with_capacity(ADMIN_DASHBOARD_BUCKET_COUNT);
    let mut request_total = 0_i64;
    let mut quota_total = 0_i64;
    for index in 0..ADMIN_DASHBOARD_BUCKET_COUNT {
        let offset = i64::try_from(index)
            .ok()
            .and_then(|value| value.checked_mul(ADMIN_DASHBOARD_BUCKET_SECONDS))
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let bucket_start = period_start
            .checked_add(offset)
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let bucket_end = bucket_start
            .checked_add(ADMIN_DASHBOARD_BUCKET_SECONDS)
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let request_count = get_count(result, &format!("request_count_{index:02}"), backend)?;
        let quota_consumed = get_sum(result, &format!("quota_consumed_{index:02}"), backend)?;
        if request_count < 0 || quota_consumed < 0 {
            return Err(record_internal_error(
                AdminDashboardRepositoryError::Invariant,
            ));
        }
        request_total = request_total
            .checked_add(request_count)
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        quota_total = quota_total
            .checked_add(quota_consumed)
            .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        hourly.push(AdminDashboardHourlyRecord {
            period_start: bucket_start,
            period_end: bucket_end,
            request_count,
            quota_consumed,
        });
    }
    if request_total != expected_request_count
        || quota_total != expected_quota_consumed
        || hourly.last().map(|bucket| bucket.period_end) != Some(period_end)
    {
        return Err(record_internal_error(
            AdminDashboardRepositoryError::Invariant,
        ));
    }

    Ok(AdminDashboardObservabilityRecord {
        hourly,
        performance: AdminDashboardPerformanceRecord {
            first_token_sample_count,
            average_first_token_ms: average(first_token_total_ms, first_token_sample_count)?,
            slow_first_token_count,
            duration_sample_count,
            average_duration_ms: average(duration_total_ms, duration_sample_count)?,
            slow_request_count,
        },
    })
}

fn validate_window(
    period_start: i64,
    period_end: i64,
) -> Result<(), AdminDashboardRepositoryError> {
    let expected = ADMIN_DASHBOARD_BUCKET_SECONDS.checked_mul(24);
    if period_start < 0 || period_end.checked_sub(period_start) != expected {
        return Err(record_internal_error(
            AdminDashboardRepositoryError::Invariant,
        ));
    }
    Ok(())
}

fn average(total: i64, samples: i64) -> Result<Option<i64>, AdminDashboardRepositoryError> {
    match samples {
        0 if total == 0 => Ok(None),
        0 => Err(record_internal_error(
            AdminDashboardRepositoryError::Invariant,
        )),
        _ => Ok(Some(total / samples)),
    }
}

fn timestamp(value: i64) -> Result<TimeDateTimeWithTimeZone, AdminDashboardRepositoryError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| record_internal_error(AdminDashboardRepositoryError::Invariant))
}

fn period_condition(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> SimpleExpr {
    Expr::col((usage_logs::Entity, usage_logs::Column::CreatedAt))
        .gte(period_start)
        .and(Expr::col((usage_logs::Entity, usage_logs::Column::CreatedAt)).lt(period_end))
}

fn conditional_count(condition: SimpleExpr) -> SimpleExpr {
    Func::count(Expr::case(condition, 1_i64).finally(Expr::value(Option::<i64>::None))).into()
}

fn conditional_sum(condition: SimpleExpr, column: usage_logs::Column) -> SimpleExpr {
    Func::sum(Expr::case(condition, Expr::col((usage_logs::Entity, column))).finally(0_i64)).into()
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::Query;

    use super::*;

    #[test]
    fn observability_query_remains_portable_across_supported_dialects() {
        for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
            let mut query = Query::select();
            append_observability_expressions(&mut query, 0, 86_400).unwrap();
            let statement = backend.build(&query.from(usage_logs::Entity).to_owned());
            assert!(!statement.sql.is_empty());
        }
    }

    #[test]
    fn rolling_window_must_cover_exactly_twenty_four_hours() {
        assert_eq!(validate_window(7, 86_407), Ok(()));
        assert_eq!(
            validate_window(7, 86_406),
            Err(AdminDashboardRepositoryError::Invariant)
        );
    }
}
