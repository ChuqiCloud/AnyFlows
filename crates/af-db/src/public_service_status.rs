//! Coarse installation-wide status aggregated from terminal request facts.

use std::time::Duration;

use sea_orm::{
    ConnectionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Condition, Expr, Func, Query},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminDashboardRepositoryError, DatabasePool,
    admin_dashboard::{get_count, get_sum, query_one},
    entity::request_outcome_logs as outcomes,
};

pub const PUBLIC_STATUS_PERIOD_SECONDS: i64 = 86_400;
pub const PUBLIC_STATUS_MINIMUM_SAMPLES: i64 = 50;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicStatusPoint {
    pub period_start: i64,
    pub successful_request_count: i64,
    pub failed_request_count: i64,
    pub unknown_request_count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicServiceStatusRecord {
    pub sampled_at: i64,
    pub period_start: i64,
    pub period_end: i64,
    pub request_count: i64,
    pub successful_request_count: i64,
    pub failed_request_count: i64,
    pub unknown_request_count: i64,
    pub average_success_duration_ms: Option<i64>,
    pub hourly: Vec<PublicStatusPoint>,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PublicServiceStatusError {
    #[error("public status repository configuration is invalid")]
    InvalidConfiguration,
    #[error("public status query failed")]
    Query,
    #[error("public status query timed out")]
    Timeout,
    #[error("public status aggregate is invalid")]
    Invariant,
}

#[derive(Clone)]
pub struct PublicServiceStatusRepository {
    pool: DatabasePool,
    timeout: Duration,
}

impl PublicServiceStatusRepository {
    pub fn new(pool: DatabasePool, timeout: Duration) -> Result<Self, PublicServiceStatusError> {
        if timeout.is_zero() {
            return Err(PublicServiceStatusError::InvalidConfiguration);
        }
        Ok(Self { pool, timeout })
    }

    pub async fn report(
        &self,
        now: i64,
    ) -> Result<PublicServiceStatusRecord, PublicServiceStatusError> {
        let period_end = now;
        let period_start = now
            .checked_sub(PUBLIC_STATUS_PERIOD_SECONDS)
            .ok_or(PublicServiceStatusError::Invariant)?;
        if period_start < 0 {
            return Err(PublicServiceStatusError::Invariant);
        }
        let operation = async {
            let backend = self.pool.connection().get_database_backend();
            let mut query = Query::select();
            query
                .from(outcomes::Entity)
                .and_where(
                    Expr::col((outcomes::Entity, outcomes::Column::CreatedAt))
                        .gte(timestamp(period_start)?),
                )
                .and_where(
                    Expr::col((outcomes::Entity, outcomes::Column::CreatedAt))
                        .lt(timestamp(period_end)?),
                );
            query.expr_as(
                Expr::col((outcomes::Entity, outcomes::Column::Id)).count(),
                Alias::new("request_count"),
            );
            append_counts(&mut query, Condition::all(), "");
            query.expr_as(
                Func::sum(
                    Expr::case(
                        Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(1_i16),
                        Expr::col((outcomes::Entity, outcomes::Column::DurationMs)),
                    )
                    .finally(0_i64),
                ),
                Alias::new("success_duration_sum"),
            );
            query.expr_as(
                Func::count(
                    Expr::case(
                        Condition::all()
                            .add(Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(1_i16))
                            .add(
                                Expr::col((outcomes::Entity, outcomes::Column::DurationMs))
                                    .is_not_null(),
                            ),
                        1_i64,
                    )
                    .finally(Expr::value(Option::<i64>::None)),
                ),
                Alias::new("success_duration_count"),
            );
            for hour in 0..24 {
                let left = timestamp(period_start + hour * 3600)?;
                let right = timestamp(period_start + (hour + 1) * 3600)?;
                let condition = Condition::all()
                    .add(Expr::col((outcomes::Entity, outcomes::Column::CreatedAt)).gte(left))
                    .add(Expr::col((outcomes::Entity, outcomes::Column::CreatedAt)).lt(right));
                append_counts(&mut query, condition, &format!("h{hour}_"));
            }
            let result = query_one(self.pool.connection(), query.to_owned())
                .await
                .map_err(map_dashboard_error)?;
            let request_count =
                get_count(&result, "request_count", backend).map_err(map_dashboard_error)?;
            let successful_request_count = get_count(&result, "successful_request_count", backend)
                .map_err(map_dashboard_error)?;
            let failed_request_count =
                get_count(&result, "failed_request_count", backend).map_err(map_dashboard_error)?;
            let unknown_request_count = get_count(&result, "unknown_request_count", backend)
                .map_err(map_dashboard_error)?;
            if request_count < 0
                || successful_request_count + failed_request_count + unknown_request_count
                    > request_count
            {
                return Err(PublicServiceStatusError::Invariant);
            }
            let duration_sum =
                get_sum(&result, "success_duration_sum", backend).map_err(map_dashboard_error)?;
            let success_duration_count = get_count(&result, "success_duration_count", backend)
                .map_err(map_dashboard_error)?;
            let average_success_duration_ms = if success_duration_count > 0 {
                Some(duration_sum / success_duration_count)
            } else {
                None
            };
            let mut hourly = Vec::with_capacity(24);
            for hour in 0..24 {
                let mut point = PublicStatusPoint {
                    period_start: period_start + hour * 3600,
                    successful_request_count: get_count(
                        &result,
                        &format!("h{hour}_successful_request_count"),
                        backend,
                    )
                    .map_err(map_dashboard_error)?,
                    failed_request_count: get_count(
                        &result,
                        &format!("h{hour}_failed_request_count"),
                        backend,
                    )
                    .map_err(map_dashboard_error)?,
                    unknown_request_count: get_count(
                        &result,
                        &format!("h{hour}_unknown_request_count"),
                        backend,
                    )
                    .map_err(map_dashboard_error)?,
                };
                let point_total = point.successful_request_count
                    + point.failed_request_count
                    + point.unknown_request_count;
                if point_total < 5 {
                    point.successful_request_count = 0;
                    point.failed_request_count = 0;
                    point.unknown_request_count = 0;
                }
                hourly.push(point);
            }
            Ok(PublicServiceStatusRecord {
                sampled_at: now,
                period_start,
                period_end,
                request_count,
                successful_request_count,
                failed_request_count,
                unknown_request_count,
                average_success_duration_ms,
                hourly,
            })
        }
        .with_subscriber(NoSubscriber::default());
        match timeout(self.timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(PublicServiceStatusError::Timeout),
        }
    }
}

fn append_counts(
    query: &mut sea_orm::sea_query::SelectStatement,
    interval: Condition,
    prefix: &str,
) {
    let success = Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(1_i16);
    let failure = Condition::all()
        .add(Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(2_i16))
        .add(
            Condition::any()
                .add(Expr::col((outcomes::Entity, outcomes::Column::ErrorKind)).is_null())
                .add(
                    Expr::col((outcomes::Entity, outcomes::Column::ErrorKind))
                        .ne("outcome_unknown"),
                ),
        );
    let unknown = Expr::col((outcomes::Entity, outcomes::Column::ErrorKind)).eq("outcome_unknown");
    for (name, outcome) in [
        ("successful_request_count", Condition::all().add(success)),
        ("failed_request_count", failure),
        ("unknown_request_count", Condition::all().add(unknown)),
    ] {
        let condition = Condition::all().add(interval.clone()).add(outcome);
        query.expr_as(
            Func::count(Expr::case(condition, 1_i64).finally(Expr::value(Option::<i64>::None))),
            Alias::new(format!("{prefix}{name}")),
        );
    }
}

fn timestamp(value: i64) -> Result<TimeDateTimeWithTimeZone, PublicServiceStatusError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| PublicServiceStatusError::Invariant)
}

fn map_dashboard_error(error: AdminDashboardRepositoryError) -> PublicServiceStatusError {
    match error {
        AdminDashboardRepositoryError::Query => PublicServiceStatusError::Query,
        AdminDashboardRepositoryError::Timeout => PublicServiceStatusError::Timeout,
        AdminDashboardRepositoryError::Invariant => PublicServiceStatusError::Invariant,
    }
}
