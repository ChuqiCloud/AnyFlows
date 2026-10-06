use std::str::FromStr as _;

use af_domain::{ChannelId, Protocol};
use sea_orm::{
    ConnectionTrait, DbBackend, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, Order, Query, SelectStatement},
};

use crate::{
    AdminDashboardRepositoryError, DatabasePool, RequestFailureKind,
    admin_dashboard::{get_count, get_sum, query_all, query_one, record_internal_error},
    entity::{channels, groups, request_outcome_logs, usage_logs},
};

pub const MAX_DASHBOARD_CHANNEL_FLOWS: usize = 12;
pub const MAX_DASHBOARD_FLOW_PATHS: usize = 24;
const SUCCEEDED_OUTCOME: i16 = 1;
const FAILED_OUTCOME: i16 = 2;

/// 管理看板窗口内单个闭合失败分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardFailureRecord {
    kind: RequestFailureKind,
    request_count: i64,
}

impl AdminDashboardFailureRecord {
    #[must_use]
    pub const fn kind(self) -> RequestFailureKind {
        self.kind
    }

    #[must_use]
    pub const fn request_count(self) -> i64 {
        self.request_count
    }
}

/// 管理看板窗口内从入口协议到最终渠道的成功流向。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardChannelFlowRecord {
    protocol: Protocol,
    channel_id: ChannelId,
    channel_name: String,
    request_count: i64,
}

impl AdminDashboardChannelFlowRecord {
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    #[must_use]
    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }

    #[must_use]
    pub const fn request_count(&self) -> i64 {
        self.request_count
    }
}

/// 管理看板窗口内从用户到分组、最终渠道和模型的已结算流向。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardFlowPathRecord {
    user_id: i64,
    group_id: i64,
    group_name: String,
    channel_id: ChannelId,
    channel_name: String,
    model: String,
    request_count: i64,
    quota_consumed: i64,
}

impl AdminDashboardFlowPathRecord {
    #[must_use]
    pub const fn user_id(&self) -> i64 {
        self.user_id
    }

    #[must_use]
    pub const fn group_id(&self) -> i64 {
        self.group_id
    }

    #[must_use]
    pub fn group_name(&self) -> &str {
        &self.group_name
    }

    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    #[must_use]
    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn request_count(&self) -> i64 {
        self.request_count
    }

    #[must_use]
    pub const fn quota_consumed(&self) -> i64 {
        self.quota_consumed
    }
}

/// 管理看板窗口内请求终态、失败构成和可见渠道流向。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdminDashboardOutcomeRecord {
    request_count: i64,
    successful_request_count: i64,
    failed_request_count: i64,
    other_success_count: i64,
    failures: Vec<AdminDashboardFailureRecord>,
    channel_flows: Vec<AdminDashboardChannelFlowRecord>,
    flow_request_count: i64,
    flow_quota_consumed: i64,
    flow_paths: Vec<AdminDashboardFlowPathRecord>,
}

impl AdminDashboardOutcomeRecord {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            request_count: 0,
            successful_request_count: 0,
            failed_request_count: 0,
            other_success_count: 0,
            failures: Vec::new(),
            channel_flows: Vec::new(),
            flow_request_count: 0,
            flow_quota_consumed: 0,
            flow_paths: Vec::new(),
        }
    }

    pub(crate) const fn request_count(&self) -> i64 {
        self.request_count
    }

    pub(crate) const fn successful_request_count(&self) -> i64 {
        self.successful_request_count
    }

    pub(crate) const fn failed_request_count(&self) -> i64 {
        self.failed_request_count
    }

    pub(crate) const fn other_success_count(&self) -> i64 {
        self.other_success_count
    }

    pub(crate) fn failures(&self) -> &[AdminDashboardFailureRecord] {
        &self.failures
    }

    pub(crate) fn channel_flows(&self) -> &[AdminDashboardChannelFlowRecord] {
        &self.channel_flows
    }

    pub(crate) const fn flow_request_count(&self) -> i64 {
        self.flow_request_count
    }

    pub(crate) const fn flow_quota_consumed(&self) -> i64 {
        self.flow_quota_consumed
    }

    pub(crate) fn flow_paths(&self) -> &[AdminDashboardFlowPathRecord] {
        &self.flow_paths
    }
}

pub(crate) async fn query_outcomes(
    pool: &DatabasePool,
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> Result<AdminDashboardOutcomeRecord, AdminDashboardRepositoryError> {
    let backend = pool.connection().get_database_backend();
    let summary = query_one(
        pool.connection(),
        outcome_summary_query(period_start, period_end),
    )
    .await?;
    let request_count = get_count(&summary, "request_count", backend)?;
    let successful_request_count = get_count(&summary, "successful_request_count", backend)?;
    let failed_request_count = get_count(&summary, "failed_request_count", backend)?;

    let failures = query_all(pool.connection(), failure_query(period_start, period_end))
        .await?
        .into_iter()
        .map(|row| decode_failure(&row, backend))
        .collect::<Result<Vec<_>, _>>()?;
    let channel_flows = query_all(
        pool.connection(),
        channel_flow_query(period_start, period_end),
    )
    .await?
    .into_iter()
    .map(|row| decode_channel_flow(&row, backend))
    .collect::<Result<Vec<_>, _>>()?;
    let flow_summary = query_one(
        pool.connection(),
        flow_summary_query(period_start, period_end),
    )
    .await?;
    let flow_request_count = get_count(&flow_summary, "request_count", backend)?;
    let flow_quota_consumed = get_sum(&flow_summary, "quota_consumed", backend)?;
    let flow_paths = query_all(pool.connection(), flow_path_query(period_start, period_end))
        .await?
        .into_iter()
        .map(|row| decode_flow_path(&row, backend))
        .collect::<Result<Vec<_>, _>>()?;
    let failure_total = checked_sum(failures.iter().map(|row| row.request_count))?;
    let visible_success_total = checked_sum(channel_flows.iter().map(|row| row.request_count))?;
    let visible_flow_requests = checked_sum(flow_paths.iter().map(|row| row.request_count))?;
    let visible_flow_quota = checked_sum(flow_paths.iter().map(|row| row.quota_consumed))?;
    let classified_total = successful_request_count
        .checked_add(failed_request_count)
        .ok_or_else(invariant)?;
    if [
        request_count,
        successful_request_count,
        failed_request_count,
    ]
    .into_iter()
    .any(|value| value < 0)
        || classified_total != request_count
        || failure_total != failed_request_count
        || visible_success_total > successful_request_count
        || flow_request_count < 0
        || flow_quota_consumed < 0
        || visible_flow_requests > flow_request_count
        || visible_flow_quota > flow_quota_consumed
    {
        return Err(invariant());
    }
    Ok(AdminDashboardOutcomeRecord {
        request_count,
        successful_request_count,
        failed_request_count,
        other_success_count: successful_request_count - visible_success_total,
        failures,
        channel_flows,
        flow_request_count,
        flow_quota_consumed,
        flow_paths,
    })
}

fn outcome_summary_query(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Id,
            ))
            .count(),
            Alias::new("request_count"),
        )
        .expr_as(
            conditional_count(SUCCEEDED_OUTCOME),
            Alias::new("successful_request_count"),
        )
        .expr_as(
            conditional_count(FAILED_OUTCOME),
            Alias::new("failed_request_count"),
        )
        .from(request_outcome_logs::Entity)
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::CreatedAt,
            ))
            .gte(period_start),
        )
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::CreatedAt,
            ))
            .lt(period_end),
        )
        .to_owned()
}

fn failure_query(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> SelectStatement {
    Query::select()
        .column((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::ErrorKind,
        ))
        .expr_as(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Id,
            ))
            .count(),
            Alias::new("request_count"),
        )
        .from(request_outcome_logs::Entity)
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Outcome,
            ))
            .eq(FAILED_OUTCOME),
        )
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::CreatedAt,
            ))
            .gte(period_start),
        )
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::CreatedAt,
            ))
            .lt(period_end),
        )
        .group_by_col((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::ErrorKind,
        ))
        .order_by(Alias::new("request_count"), Order::Desc)
        .order_by(
            (
                request_outcome_logs::Entity,
                request_outcome_logs::Column::ErrorKind,
            ),
            Order::Asc,
        )
        .to_owned()
}

fn channel_flow_query(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> SelectStatement {
    Query::select()
        .column((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::Protocol,
        ))
        .column((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::ChannelId,
        ))
        .column((channels::Entity, channels::Column::Name))
        .expr_as(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Id,
            ))
            .count(),
            Alias::new("request_count"),
        )
        .from(request_outcome_logs::Entity)
        .inner_join(
            channels::Entity,
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::ChannelId,
            ))
            .equals((channels::Entity, channels::Column::Id)),
        )
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Outcome,
            ))
            .eq(SUCCEEDED_OUTCOME),
        )
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::CreatedAt,
            ))
            .gte(period_start),
        )
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::CreatedAt,
            ))
            .lt(period_end),
        )
        .group_by_col((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::Protocol,
        ))
        .group_by_col((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::ChannelId,
        ))
        .group_by_col((channels::Entity, channels::Column::Name))
        .order_by(Alias::new("request_count"), Order::Desc)
        .order_by(
            (
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Protocol,
            ),
            Order::Asc,
        )
        .order_by((channels::Entity, channels::Column::Name), Order::Asc)
        .order_by(
            (
                request_outcome_logs::Entity,
                request_outcome_logs::Column::ChannelId,
            ),
            Order::Asc,
        )
        .limit(MAX_DASHBOARD_CHANNEL_FLOWS as u64)
        .to_owned()
}

fn flow_summary_query(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> SelectStatement {
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::Id)).count(),
            Alias::new("request_count"),
        )
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::Quota)).sum(),
            Alias::new("quota_consumed"),
        );
    append_flow_source(&mut query, period_start, period_end);
    query.to_owned()
}

fn flow_path_query(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> SelectStatement {
    let mut query = Query::select();
    query
        .column((usage_logs::Entity, usage_logs::Column::UserId))
        .column((usage_logs::Entity, usage_logs::Column::GroupId))
        .expr_as(
            Expr::col((groups::Entity, groups::Column::DisplayName)),
            Alias::new("group_name"),
        )
        .column((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::ChannelId,
        ))
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Name)),
            Alias::new("channel_name"),
        )
        .column((usage_logs::Entity, usage_logs::Column::Model))
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::Id)).count(),
            Alias::new("request_count"),
        )
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::Quota)).sum(),
            Alias::new("quota_consumed"),
        );
    append_flow_source(&mut query, period_start, period_end);
    query
        .group_by_col((usage_logs::Entity, usage_logs::Column::UserId))
        .group_by_col((usage_logs::Entity, usage_logs::Column::GroupId))
        .group_by_col((groups::Entity, groups::Column::DisplayName))
        .group_by_col((
            request_outcome_logs::Entity,
            request_outcome_logs::Column::ChannelId,
        ))
        .group_by_col((channels::Entity, channels::Column::Name))
        .group_by_col((usage_logs::Entity, usage_logs::Column::Model))
        .order_by(Alias::new("request_count"), Order::Desc)
        .order_by(Alias::new("quota_consumed"), Order::Desc)
        .order_by((usage_logs::Entity, usage_logs::Column::UserId), Order::Asc)
        .order_by(
            (usage_logs::Entity, usage_logs::Column::GroupId),
            Order::Asc,
        )
        .order_by(
            (
                request_outcome_logs::Entity,
                request_outcome_logs::Column::ChannelId,
            ),
            Order::Asc,
        )
        .order_by((usage_logs::Entity, usage_logs::Column::Model), Order::Asc)
        .limit(MAX_DASHBOARD_FLOW_PATHS as u64)
        .to_owned()
}

/// 固定四层流向的数据来源，避免汇总与明细的窗口或连接口径漂移。
fn append_flow_source(
    query: &mut SelectStatement,
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) {
    query
        .from(usage_logs::Entity)
        .inner_join(
            request_outcome_logs::Entity,
            Expr::col((usage_logs::Entity, usage_logs::Column::RequestId)).equals((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::RequestId,
            )),
        )
        .inner_join(
            groups::Entity,
            Expr::col((usage_logs::Entity, usage_logs::Column::GroupId))
                .equals((groups::Entity, groups::Column::Id)),
        )
        .inner_join(
            channels::Entity,
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::ChannelId,
            ))
            .equals((channels::Entity, channels::Column::Id)),
        )
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::EventType)).eq(1_i16))
        .and_where(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Outcome,
            ))
            .eq(SUCCEEDED_OUTCOME),
        )
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::Model)).is_not_null())
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::CreatedAt)).gte(period_start))
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::CreatedAt)).lt(period_end));
}

fn conditional_count(outcome: i16) -> sea_orm::sea_query::SimpleExpr {
    Func::count(
        Expr::case(
            Expr::col((
                request_outcome_logs::Entity,
                request_outcome_logs::Column::Outcome,
            ))
            .eq(outcome),
            1_i64,
        )
        .finally(Expr::value(Option::<i64>::None)),
    )
    .into()
}

fn decode_failure(
    row: &QueryResult,
    backend: DbBackend,
) -> Result<AdminDashboardFailureRecord, AdminDashboardRepositoryError> {
    let kind = row
        .try_get::<Option<String>>("", "error_kind")
        .ok()
        .flatten()
        .and_then(|value| RequestFailureKind::from_str(&value).ok())
        .ok_or_else(invariant)?;
    let request_count = get_count(row, "request_count", backend)?;
    if request_count <= 0 {
        return Err(invariant());
    }
    Ok(AdminDashboardFailureRecord {
        kind,
        request_count,
    })
}

fn decode_channel_flow(
    row: &QueryResult,
    backend: DbBackend,
) -> Result<AdminDashboardChannelFlowRecord, AdminDashboardRepositoryError> {
    let protocol = row
        .try_get::<String>("", "protocol")
        .ok()
        .and_then(|value| Protocol::from_str(&value).ok())
        .ok_or_else(invariant)?;
    let channel_id = row
        .try_get::<i64>("", "channel_id")
        .ok()
        .and_then(|value| ChannelId::new(value).ok())
        .ok_or_else(invariant)?;
    let channel_name = row
        .try_get::<String>("", "name")
        .ok()
        .filter(|value| valid_channel_name(value))
        .ok_or_else(invariant)?;
    let request_count = get_count(row, "request_count", backend)?;
    if request_count <= 0 {
        return Err(invariant());
    }
    Ok(AdminDashboardChannelFlowRecord {
        protocol,
        channel_id,
        channel_name,
        request_count,
    })
}

fn decode_flow_path(
    row: &QueryResult,
    backend: DbBackend,
) -> Result<AdminDashboardFlowPathRecord, AdminDashboardRepositoryError> {
    let user_id = row
        .try_get::<i64>("", "user_id")
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(invariant)?;
    let group_id = row
        .try_get::<i64>("", "group_id")
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(invariant)?;
    let group_name = row
        .try_get::<String>("", "group_name")
        .ok()
        .filter(|value| valid_label(value, 128))
        .ok_or_else(invariant)?;
    let channel_id = row
        .try_get::<i64>("", "channel_id")
        .ok()
        .and_then(|value| ChannelId::new(value).ok())
        .ok_or_else(invariant)?;
    let channel_name = row
        .try_get::<String>("", "channel_name")
        .ok()
        .filter(|value| valid_label(value, 128))
        .ok_or_else(invariant)?;
    let model = row
        .try_get::<String>("", "model")
        .ok()
        .filter(|value| valid_label(value, 255))
        .ok_or_else(invariant)?;
    let request_count = get_count(row, "request_count", backend)?;
    let quota_consumed = get_sum(row, "quota_consumed", backend)?;
    if request_count <= 0 || quota_consumed < 0 {
        return Err(invariant());
    }
    Ok(AdminDashboardFlowPathRecord {
        user_id,
        group_id,
        group_name,
        channel_id,
        channel_name,
        model,
        request_count,
        quota_consumed,
    })
}

fn checked_sum(
    mut values: impl Iterator<Item = i64>,
) -> Result<i64, AdminDashboardRepositoryError> {
    values.try_fold(0_i64, |total, value| {
        total.checked_add(value).ok_or_else(invariant)
    })
}

fn valid_channel_name(value: &str) -> bool {
    valid_label(value, 128)
}

fn valid_label(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn invariant() -> AdminDashboardRepositoryError {
    record_internal_error(AdminDashboardRepositoryError::Invariant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_queries_remain_portable_across_supported_dialects() {
        let start = TimeDateTimeWithTimeZone::from_unix_timestamp(1_000).unwrap();
        let end = TimeDateTimeWithTimeZone::from_unix_timestamp(87_400).unwrap();
        for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
            for query in [
                outcome_summary_query(start, end),
                failure_query(start, end),
                channel_flow_query(start, end),
                flow_summary_query(start, end),
                flow_path_query(start, end),
            ] {
                assert!(!backend.build(&query).sql.is_empty());
            }
        }
    }
}
