use std::time::Duration;

use sea_orm::{
    ConnectionTrait, DbBackend, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{
        Alias, Condition, Expr, Func, JoinType, Order, Query, SelectStatement, SimpleExpr,
    },
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminDashboardRepositoryError, DatabasePool,
    admin_dashboard::{get_count, get_sum, query_all, query_one},
    entity::{channels, request_outcome_logs as outcomes},
};

#[derive(Clone, Debug)]
pub struct DashboardServiceLevelPoint {
    pub period_start: i64,
    pub successful_request_count: i64,
    pub failed_request_count: i64,
    pub unknown_request_count: i64,
}

#[derive(Clone, Debug)]
pub struct DashboardServiceLevelRow {
    pub key: String,
    pub name: String,
    pub request_count: i64,
    pub successful_request_count: i64,
    pub failed_request_count: i64,
    pub unknown_request_count: i64,
    pub average_duration_ms: Option<i64>,
    pub hourly: Vec<DashboardServiceLevelPoint>,
}

#[derive(Clone, Debug)]
pub struct DashboardServiceLevelReport {
    pub total: i64,
    pub unattributed_request_count: i64,
    pub items: Vec<DashboardServiceLevelRow>,
}

#[derive(Clone, Debug)]
pub struct DashboardServiceLevelQuery {
    pub channels: bool,
    pub search: String,
    pub page: u32,
    pub page_size: u32,
    pub failures_first: bool,
}

pub(crate) async fn service_level_report(
    pool: &DatabasePool,
    deadline: Duration,
    start: i64,
    end: i64,
    options: DashboardServiceLevelQuery,
) -> Result<DashboardServiceLevelReport, AdminDashboardRepositoryError> {
    if start < 0
        || end.checked_sub(start) != Some(86_400)
        || options.page == 0
        || options.page > 10_000
        || !(1..=20).contains(&options.page_size)
        || options.search.chars().count() > 128
    {
        return Err(AdminDashboardRepositoryError::Invariant);
    }
    let operation = async {
        let backend = pool.connection().get_database_backend();
        let mut grouped = source_query(start, end, &options)?;
        let key = if options.channels {
            outcomes::Column::ChannelId
        } else {
            outcomes::Column::Model
        };
        grouped
            .column((outcomes::Entity, key))
            .group_by_col((outcomes::Entity, key));
        let total_query = Query::select()
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("total"))
            .from_subquery(
                {
                    let mut count_groups = grouped.clone();
                    count_groups.clear_selects();
                    count_groups.expr_as(Expr::val(1_i64), Alias::new("id"));
                    count_groups
                },
                Alias::new("service_level_groups"),
            )
            .to_owned();
        let total = get_count(
            &query_one(pool.connection(), total_query).await?,
            "total",
            backend,
        )?;
        grouped.clear_selects();
        grouped.column((outcomes::Entity, key));
        if options.channels {
            grouped
                .expr_as(
                    Expr::col((channels::Entity, channels::Column::Name)),
                    Alias::new("name"),
                )
                .group_by_col((channels::Entity, channels::Column::Name));
        }
        grouped.expr_as(
            Expr::col((outcomes::Entity, outcomes::Column::Id)).count(),
            Alias::new("request_count"),
        );
        append_counts(&mut grouped, Condition::all(), "");
        grouped.expr_as(
            Func::sum(
                Expr::case(
                    Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(1_i16),
                    Expr::col((outcomes::Entity, outcomes::Column::DurationMs)),
                )
                .finally(0_i64),
            ),
            Alias::new("duration_sum"),
        );
        for hour in 0..24 {
            let left = timestamp(start + hour * 3600)?;
            let right = timestamp(start + (hour + 1) * 3600)?;
            let condition = Condition::all()
                .add(Expr::col((outcomes::Entity, outcomes::Column::CreatedAt)).gte(left))
                .add(Expr::col((outcomes::Entity, outcomes::Column::CreatedAt)).lt(right));
            append_counts(&mut grouped, condition, &format!("h{hour}_"));
        }
        if options.failures_first {
            grouped.order_by(Alias::new("failed_request_count"), Order::Desc);
        }
        grouped
            .order_by(Alias::new("request_count"), Order::Desc)
            .order_by((outcomes::Entity, key), Order::Asc)
            .limit(u64::from(options.page_size))
            .offset(u64::from(options.page - 1) * u64::from(options.page_size));
        let items = query_all(pool.connection(), grouped)
            .await?
            .iter()
            .map(|row| decode_row(row, backend, start, options.channels))
            .collect::<Result<Vec<_>, _>>()?;
        let mut unassigned = source_query(
            start,
            end,
            &DashboardServiceLevelQuery {
                channels: false,
                search: String::new(),
                ..options.clone()
            },
        )?;
        unassigned
            .expr_as(
                Expr::col((outcomes::Entity, outcomes::Column::Id)).count(),
                Alias::new("count"),
            )
            .and_where(Expr::col((outcomes::Entity, outcomes::Column::ChannelId)).is_null());
        let unattributed_request_count = get_count(
            &query_one(pool.connection(), unassigned).await?,
            "count",
            backend,
        )?;
        Ok(DashboardServiceLevelReport {
            total,
            unattributed_request_count,
            items,
        })
    }
    .with_subscriber(NoSubscriber::default());
    timeout(deadline, operation)
        .await
        .map_err(|_| AdminDashboardRepositoryError::Timeout)?
}

fn timestamp(value: i64) -> Result<TimeDateTimeWithTimeZone, AdminDashboardRepositoryError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| AdminDashboardRepositoryError::Invariant)
}

fn source_query(
    start: i64,
    end: i64,
    options: &DashboardServiceLevelQuery,
) -> Result<SelectStatement, AdminDashboardRepositoryError> {
    let mut query = Query::select();
    query
        .from(outcomes::Entity)
        .and_where(
            Expr::col((outcomes::Entity, outcomes::Column::CreatedAt)).gte(timestamp(start)?),
        )
        .and_where(Expr::col((outcomes::Entity, outcomes::Column::CreatedAt)).lt(timestamp(end)?));
    let label = if options.channels {
        query
            .join(
                JoinType::LeftJoin,
                channels::Entity,
                Expr::col((outcomes::Entity, outcomes::Column::ChannelId))
                    .equals((channels::Entity, channels::Column::Id)),
            )
            .and_where(Expr::col((outcomes::Entity, outcomes::Column::ChannelId)).is_not_null());
        Expr::col((channels::Entity, channels::Column::Name))
    } else {
        Expr::col((outcomes::Entity, outcomes::Column::Model))
    };
    if !options.search.is_empty() {
        query.and_where(
            Expr::expr(Func::lower(label)).like(format!("%{}%", options.search.to_lowercase())),
        );
    }
    Ok(query.to_owned())
}

fn append_counts(query: &mut SelectStatement, condition: Condition, prefix: &str) {
    let success = Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(1_i16);
    let unknown = Expr::col((outcomes::Entity, outcomes::Column::ErrorKind)).eq("outcome_unknown");
    let known_failure = Condition::all()
        .add(Expr::col((outcomes::Entity, outcomes::Column::Outcome)).eq(2_i16))
        .add(
            Condition::any()
                .add(Expr::col((outcomes::Entity, outcomes::Column::ErrorKind)).is_null())
                .add(
                    Expr::col((outcomes::Entity, outcomes::Column::ErrorKind))
                        .ne("outcome_unknown"),
                ),
        );
    for (name, state) in [
        ("successful_request_count", Condition::all().add(success)),
        ("failed_request_count", known_failure),
        ("unknown_request_count", Condition::all().add(unknown)),
    ] {
        let expression: SimpleExpr = Func::count(
            Expr::case(Condition::all().add(condition.clone()).add(state), 1_i64)
                .finally(Expr::val(Option::<i64>::None)),
        )
        .into();
        query.expr_as(expression, Alias::new(format!("{prefix}{name}")));
    }
}

fn decode_row(
    row: &QueryResult,
    backend: DbBackend,
    start: i64,
    channels: bool,
) -> Result<DashboardServiceLevelRow, AdminDashboardRepositoryError> {
    let error = |_| AdminDashboardRepositoryError::Invariant;
    let key = if channels {
        row.try_get::<i64>("", "channel_id")
            .map_err(error)?
            .to_string()
    } else {
        row.try_get::<String>("", "model").map_err(error)?
    };
    let name = if channels {
        row.try_get::<Option<String>>("", "name")
            .map_err(error)?
            .unwrap_or_else(|| key.clone())
    } else {
        key.clone()
    };
    let successful_request_count = get_count(row, "successful_request_count", backend)?;
    let hourly = (0..24)
        .map(|hour| {
            Ok(DashboardServiceLevelPoint {
                period_start: start + hour * 3600,
                successful_request_count: get_count(
                    row,
                    &format!("h{hour}_successful_request_count"),
                    backend,
                )?,
                failed_request_count: get_count(
                    row,
                    &format!("h{hour}_failed_request_count"),
                    backend,
                )?,
                unknown_request_count: get_count(
                    row,
                    &format!("h{hour}_unknown_request_count"),
                    backend,
                )?,
            })
        })
        .collect::<Result<Vec<_>, AdminDashboardRepositoryError>>()?;
    Ok(DashboardServiceLevelRow {
        key,
        name,
        request_count: get_count(row, "request_count", backend)?,
        successful_request_count,
        failed_request_count: get_count(row, "failed_request_count", backend)?,
        unknown_request_count: get_count(row, "unknown_request_count", backend)?,
        average_duration_ms: if successful_request_count == 0 {
            None
        } else {
            Some(get_sum(row, "duration_sum", backend)? / successful_request_count)
        },
        hourly,
    })
}
