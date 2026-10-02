use std::{fmt, time::Duration};

use af_domain::Status;
use rust_decimal::prelude::ToPrimitive as _;
use sea_orm::{
    ConnectionTrait, DbBackend, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    admin_dashboard_observability::{
        AdminDashboardHourlyRecord, AdminDashboardObservabilityRecord,
        AdminDashboardPerformanceRecord, append_observability_expressions, decode_observability,
    },
    admin_dashboard_outcomes::{AdminDashboardOutcomeRecord, query_outcomes},
    entity::{channels, usage_logs},
};

/// 管理看板使用的持久化聚合快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardRecord {
    request_count: i64,
    quota_consumed: i64,
    upstream_usage_count: i64,
    estimated_usage_count: i64,
    per_token_request_count: i64,
    per_call_request_count: i64,
    free_request_count: i64,
    enabled_channel_count: i64,
    disabled_channel_count: i64,
    auto_disabled_channel_count: i64,
    observability: AdminDashboardObservabilityRecord,
    outcomes: AdminDashboardOutcomeRecord,
}

impl AdminDashboardRecord {
    /// 返回窗口内已经确认并落库的请求数。
    #[must_use]
    pub const fn request_count(&self) -> i64 {
        self.request_count
    }

    /// 返回窗口内最终消耗的额度单位数。
    #[must_use]
    pub const fn quota_consumed(&self) -> i64 {
        self.quota_consumed
    }

    /// 返回由上游响应确认用量的请求数。
    #[must_use]
    pub const fn upstream_usage_count(&self) -> i64 {
        self.upstream_usage_count
    }

    /// 返回由本地估算用量的请求数。
    #[must_use]
    pub const fn estimated_usage_count(&self) -> i64 {
        self.estimated_usage_count
    }

    /// 返回按 token 计费的请求数。
    #[must_use]
    pub const fn per_token_request_count(&self) -> i64 {
        self.per_token_request_count
    }

    /// 返回按次计费的请求数。
    #[must_use]
    pub const fn per_call_request_count(&self) -> i64 {
        self.per_call_request_count
    }

    /// 返回免费请求数。
    #[must_use]
    pub const fn free_request_count(&self) -> i64 {
        self.free_request_count
    }

    /// 返回当前启用的未删除渠道数。
    #[must_use]
    pub const fn enabled_channel_count(&self) -> i64 {
        self.enabled_channel_count
    }

    /// 返回当前手动禁用的未删除渠道数。
    #[must_use]
    pub const fn disabled_channel_count(&self) -> i64 {
        self.disabled_channel_count
    }

    /// 返回当前自动禁用的未删除渠道数。
    #[must_use]
    pub const fn auto_disabled_channel_count(&self) -> i64 {
        self.auto_disabled_channel_count
    }

    /// 返回覆盖整个窗口且连续的一小时用量分桶。
    #[must_use]
    pub fn hourly(&self) -> &[AdminDashboardHourlyRecord] {
        self.observability.hourly()
    }

    /// 返回只基于已持久化耗时样本计算的性能健康聚合。
    #[must_use]
    pub const fn performance(&self) -> AdminDashboardPerformanceRecord {
        self.observability.performance()
    }

    /// 返回窗口内已经进入终态观测的同步模型请求数。
    #[must_use]
    pub const fn outcome_request_count(&self) -> i64 {
        self.outcomes.request_count()
    }

    /// 返回窗口内成功完成的同步模型请求数。
    #[must_use]
    pub const fn successful_request_count(&self) -> i64 {
        self.outcomes.successful_request_count()
    }

    /// 返回窗口内失败完成的同步模型请求数。
    #[must_use]
    pub const fn failed_request_count(&self) -> i64 {
        self.outcomes.failed_request_count()
    }

    /// 返回未进入前十二条可见流向的成功请求数。
    #[must_use]
    pub const fn other_success_count(&self) -> i64 {
        self.outcomes.other_success_count()
    }

    /// 返回按闭合错误分类聚合的失败请求。
    #[must_use]
    pub fn failures(&self) -> &[crate::AdminDashboardFailureRecord] {
        self.outcomes.failures()
    }

    /// 返回请求量最高的协议到渠道流向。
    #[must_use]
    pub fn channel_flows(&self) -> &[crate::AdminDashboardChannelFlowRecord] {
        self.outcomes.channel_flows()
    }

    /// 返回具备已结算用量与成功终态连接事实的请求总数。
    #[must_use]
    pub const fn flow_request_count(&self) -> i64 {
        self.outcomes.flow_request_count()
    }

    /// 返回具备完整四层路径事实的已结算额度总量。
    #[must_use]
    pub const fn flow_quota_consumed(&self) -> i64 {
        self.outcomes.flow_quota_consumed()
    }

    /// 返回按请求量排序的有界用户、分组、渠道和模型路径。
    #[must_use]
    pub fn flow_paths(&self) -> &[crate::AdminDashboardFlowPathRecord] {
        self.outcomes.flow_paths()
    }
}

/// 管理看板仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardRepositoryConfigError {
    /// 零超时无法形成有效的查询截止时间。
    #[error("管理看板查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理看板仓储错误，不携带统计值或数据库细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardRepositoryError {
    /// 获取连接或执行聚合查询失败。
    #[error("管理看板数据库查询失败")]
    Query,
    /// 全部聚合查询超过共同的硬截止时间。
    #[error("管理看板数据库查询超时")]
    Timeout,
    /// 查询窗口或持久化聚合结果违反不变量。
    #[error("管理看板持久化状态损坏")]
    Invariant,
}

/// 管理看板的只读聚合仓储。
#[derive(Clone)]
pub struct AdminDashboardRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl AdminDashboardRepository {
    pub async fn service_levels(
        &self,
        start: i64,
        end: i64,
        query: crate::DashboardServiceLevelQuery,
    ) -> Result<crate::DashboardServiceLevelReport, AdminDashboardRepositoryError> {
        crate::admin_dashboard_sla::service_level_report(
            &self.pool,
            self.lookup_timeout,
            start,
            end,
            query,
        )
        .await
    }

    /// 使用共享连接池和整次快照查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminDashboardRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminDashboardRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 聚合半开时间窗口内的用量事实和当前未删除渠道状态。
    pub async fn snapshot(
        &self,
        period_start: i64,
        period_end: i64,
    ) -> Result<AdminDashboardRecord, AdminDashboardRepositoryError> {
        if period_start < 0 || period_start >= period_end {
            return Err(record_internal_error(
                AdminDashboardRepositoryError::Invariant,
            ));
        }
        let period_start = TimeDateTimeWithTimeZone::from_unix_timestamp(period_start)
            .map_err(|_| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let period_end = TimeDateTimeWithTimeZone::from_unix_timestamp(period_end)
            .map_err(|_| record_internal_error(AdminDashboardRepositoryError::Invariant))?;
        let operation = async {
            let (usage, observability) = self.query_usage(period_start, period_end).await?;
            let channels = self.query_channels().await?;
            let outcomes = query_outcomes(&self.pool, period_start, period_end).await?;
            AdminDashboardRecord::try_from_rows(usage, channels, observability, outcomes)
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                AdminDashboardRepositoryError::Timeout,
            )),
        }
    }

    /// 只读取事务主库中当前未删除渠道的闭合状态计数。
    pub async fn channel_snapshot(
        &self,
    ) -> Result<AdminDashboardChannelRecord, AdminDashboardRepositoryError> {
        let operation = self
            .query_channels()
            .with_subscriber(NoSubscriber::default());
        match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.and_then(AdminDashboardChannelRecord::validate),
            Err(_) => Err(record_internal_error(
                AdminDashboardRepositoryError::Timeout,
            )),
        }
    }

    async fn query_usage(
        &self,
        period_start: TimeDateTimeWithTimeZone,
        period_end: TimeDateTimeWithTimeZone,
    ) -> Result<(UsageAggregateRow, AdminDashboardObservabilityRecord), AdminDashboardRepositoryError>
    {
        let backend = self.pool.connection().get_database_backend();
        query_one(
            self.pool.connection(),
            usage_query(period_start, period_end)?,
        )
        .await
        .and_then(|result| {
            let usage = UsageAggregateRow::try_from_query_result(&result, backend)?;
            let observability = decode_observability(
                &result,
                backend,
                period_start.unix_timestamp(),
                period_end.unix_timestamp(),
                usage.request_count,
                usage.quota_consumed,
            )?;
            Ok((usage, observability))
        })
    }

    async fn query_channels(
        &self,
    ) -> Result<AdminDashboardChannelRecord, AdminDashboardRepositoryError> {
        let backend = self.pool.connection().get_database_backend();
        query_one(self.pool.connection(), channel_query())
            .await
            .and_then(|result| AdminDashboardChannelRecord::try_from_query_result(result, backend))
    }
}

impl fmt::Debug for AdminDashboardRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminDashboardRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UsageAggregateRow {
    request_count: i64,
    quota_consumed: i64,
    upstream_usage_count: i64,
    estimated_usage_count: i64,
    per_token_request_count: i64,
    per_call_request_count: i64,
    free_request_count: i64,
}

impl UsageAggregateRow {
    fn try_from_query_result(
        result: &QueryResult,
        backend: DbBackend,
    ) -> Result<Self, AdminDashboardRepositoryError> {
        Ok(Self {
            request_count: get_count(result, "request_count", backend)?,
            quota_consumed: get_sum(result, "quota_consumed", backend)?,
            upstream_usage_count: get_count(result, "upstream_usage_count", backend)?,
            estimated_usage_count: get_count(result, "estimated_usage_count", backend)?,
            per_token_request_count: get_count(result, "per_token_request_count", backend)?,
            per_call_request_count: get_count(result, "per_call_request_count", backend)?,
            free_request_count: get_count(result, "free_request_count", backend)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardChannelRecord {
    channel_count: i64,
    enabled_channel_count: i64,
    disabled_channel_count: i64,
    auto_disabled_channel_count: i64,
}

impl AdminDashboardChannelRecord {
    /// 返回当前启用的未删除渠道数。
    #[must_use]
    pub const fn enabled_channel_count(self) -> i64 {
        self.enabled_channel_count
    }

    /// 返回当前人工停用的未删除渠道数。
    #[must_use]
    pub const fn disabled_channel_count(self) -> i64 {
        self.disabled_channel_count
    }

    /// 返回当前自动停用的未删除渠道数。
    #[must_use]
    pub const fn auto_disabled_channel_count(self) -> i64 {
        self.auto_disabled_channel_count
    }

    fn try_from_query_result(
        result: QueryResult,
        backend: DbBackend,
    ) -> Result<Self, AdminDashboardRepositoryError> {
        Ok(Self {
            channel_count: get_count(&result, "channel_count", backend)?,
            enabled_channel_count: get_count(&result, "enabled_channel_count", backend)?,
            disabled_channel_count: get_count(&result, "disabled_channel_count", backend)?,
            auto_disabled_channel_count: get_count(
                &result,
                "auto_disabled_channel_count",
                backend,
            )?,
        })
    }

    fn validate(self) -> Result<Self, AdminDashboardRepositoryError> {
        let status_count = self
            .enabled_channel_count
            .checked_add(self.disabled_channel_count)
            .and_then(|count| count.checked_add(self.auto_disabled_channel_count));
        if [
            self.channel_count,
            self.enabled_channel_count,
            self.disabled_channel_count,
            self.auto_disabled_channel_count,
        ]
        .into_iter()
        .any(|value| value < 0)
            || status_count != Some(self.channel_count)
        {
            return Err(record_internal_error(
                AdminDashboardRepositoryError::Invariant,
            ));
        }
        Ok(self)
    }
}

impl AdminDashboardRecord {
    fn try_from_rows(
        usage: UsageAggregateRow,
        channels: AdminDashboardChannelRecord,
        observability: AdminDashboardObservabilityRecord,
        outcomes: AdminDashboardOutcomeRecord,
    ) -> Result<Self, AdminDashboardRepositoryError> {
        let usage_values = [
            usage.request_count,
            usage.quota_consumed,
            usage.upstream_usage_count,
            usage.estimated_usage_count,
            usage.per_token_request_count,
            usage.per_call_request_count,
            usage.free_request_count,
        ];
        let channels = channels.validate()?;
        let source_count = usage
            .upstream_usage_count
            .checked_add(usage.estimated_usage_count);
        let billing_count = usage
            .per_token_request_count
            .checked_add(usage.per_call_request_count)
            .and_then(|count| count.checked_add(usage.free_request_count));
        if usage_values.into_iter().any(|value| value < 0)
            || source_count != Some(usage.request_count)
            || billing_count != Some(usage.request_count)
        {
            return Err(record_internal_error(
                AdminDashboardRepositoryError::Invariant,
            ));
        }
        Ok(Self {
            request_count: usage.request_count,
            quota_consumed: usage.quota_consumed,
            upstream_usage_count: usage.upstream_usage_count,
            estimated_usage_count: usage.estimated_usage_count,
            per_token_request_count: usage.per_token_request_count,
            per_call_request_count: usage.per_call_request_count,
            free_request_count: usage.free_request_count,
            enabled_channel_count: channels.enabled_channel_count,
            disabled_channel_count: channels.disabled_channel_count,
            auto_disabled_channel_count: channels.auto_disabled_channel_count,
            observability,
            outcomes,
        })
    }
}

fn usage_query(
    period_start: TimeDateTimeWithTimeZone,
    period_end: TimeDateTimeWithTimeZone,
) -> Result<SelectStatement, AdminDashboardRepositoryError> {
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::Id)).count(),
            Alias::new("request_count"),
        )
        .expr_as(
            Expr::col((usage_logs::Entity, usage_logs::Column::Quota)).sum(),
            Alias::new("quota_consumed"),
        )
        .expr_as(
            conditional_count(usage_logs::Column::UsageSource, 1_i16),
            Alias::new("upstream_usage_count"),
        )
        .expr_as(
            conditional_count(usage_logs::Column::UsageSource, 2_i16),
            Alias::new("estimated_usage_count"),
        )
        .expr_as(
            conditional_count(usage_logs::Column::BillingMode, 1_i16),
            Alias::new("per_token_request_count"),
        )
        .expr_as(
            conditional_count(usage_logs::Column::BillingMode, 3_i16),
            Alias::new("per_call_request_count"),
        )
        .expr_as(
            conditional_count(usage_logs::Column::BillingMode, 2_i16),
            Alias::new("free_request_count"),
        );
    append_observability_expressions(
        &mut query,
        period_start.unix_timestamp(),
        period_end.unix_timestamp(),
    )?;
    Ok(query
        .from(usage_logs::Entity)
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::EventType)).eq(1_i16))
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::CreatedAt)).gte(period_start))
        .and_where(Expr::col((usage_logs::Entity, usage_logs::Column::CreatedAt)).lt(period_end))
        .to_owned())
}

fn channel_query() -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((channels::Entity, channels::Column::Id)).count(),
            Alias::new("channel_count"),
        )
        .expr_as(
            conditional_channel_count(Status::Enabled),
            Alias::new("enabled_channel_count"),
        )
        .expr_as(
            conditional_channel_count(Status::Disabled),
            Alias::new("disabled_channel_count"),
        )
        .expr_as(
            conditional_channel_count(Status::AutoDisabled),
            Alias::new("auto_disabled_channel_count"),
        )
        .from(channels::Entity)
        .and_where(Expr::col((channels::Entity, channels::Column::DeletedAt)).is_null())
        .to_owned()
}

fn conditional_count(column: usage_logs::Column, expected: i16) -> sea_orm::sea_query::SimpleExpr {
    Func::count(
        Expr::case(Expr::col((usage_logs::Entity, column)).eq(expected), 1_i64)
            .finally(Expr::value(Option::<i64>::None)),
    )
    .into()
}

fn conditional_channel_count(status: Status) -> sea_orm::sea_query::SimpleExpr {
    Func::count(
        Expr::case(
            Expr::col((channels::Entity, channels::Column::Status)).eq(status.code()),
            1_i64,
        )
        .finally(Expr::value(Option::<i64>::None)),
    )
    .into()
}

pub(crate) async fn query_one(
    connection: &sea_orm::DatabaseConnection,
    statement: SelectStatement,
) -> Result<QueryResult, AdminDashboardRepositoryError> {
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&statement))
        .await
        .map_err(|_| record_internal_error(AdminDashboardRepositoryError::Query))?
        .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))
}

pub(crate) async fn query_all(
    connection: &sea_orm::DatabaseConnection,
    statement: SelectStatement,
) -> Result<Vec<QueryResult>, AdminDashboardRepositoryError> {
    let backend = connection.get_database_backend();
    connection
        .query_all(backend.build(&statement))
        .await
        .map_err(|_| record_internal_error(AdminDashboardRepositoryError::Query))
}

pub(crate) fn get_count(
    result: &QueryResult,
    column: &str,
    backend: DbBackend,
) -> Result<i64, AdminDashboardRepositoryError> {
    match backend {
        DbBackend::MySql => result.try_get::<i64>("", column).ok().or_else(|| {
            result
                .try_get::<u64>("", column)
                .ok()
                .and_then(|value| i64::try_from(value).ok())
        }),
        DbBackend::Postgres | DbBackend::Sqlite => result.try_get::<i64>("", column).ok(),
    }
    .ok_or_else(|| record_internal_error(AdminDashboardRepositoryError::Invariant))
}

pub(crate) fn get_sum(
    result: &QueryResult,
    column: &str,
    backend: DbBackend,
) -> Result<i64, AdminDashboardRepositoryError> {
    match backend {
        DbBackend::Sqlite => result
            .try_get::<Option<i64>>("", column)
            .map(|value| value.unwrap_or(0)),
        DbBackend::Postgres | DbBackend::MySql => result
            .try_get::<Option<rust_decimal::Decimal>>("", column)
            .and_then(|value| match value {
                None => Ok(0),
                Some(value) if value.fract().is_zero() => value
                    .to_i64()
                    .ok_or_else(|| sea_orm::DbErr::Type("额度聚合超出 i64 范围".to_owned())),
                Some(_) => Err(sea_orm::DbErr::Type("额度聚合不是整数".to_owned())),
            }),
    }
    .map_err(|_| record_internal_error(AdminDashboardRepositoryError::Invariant))
}

/// 只记录闭合内部分类，避免任何聚合值或数据库细节进入日志。
pub(crate) fn record_internal_error(
    error: AdminDashboardRepositoryError,
) -> AdminDashboardRepositoryError {
    let error_kind = match error {
        AdminDashboardRepositoryError::Query => "admin_dashboard_query",
        AdminDashboardRepositoryError::Timeout => "admin_dashboard_timeout",
        AdminDashboardRepositoryError::Invariant => "admin_dashboard_invariant",
    };
    tracing::error!(
        target: "af_db::admin_dashboard",
        error_kind,
        "管理看板仓储发生内部错误"
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_validation_rejects_unclosed_dimensions() {
        let usage = UsageAggregateRow {
            request_count: 2,
            quota_consumed: 10,
            upstream_usage_count: 2,
            estimated_usage_count: 1,
            per_token_request_count: 1,
            per_call_request_count: 0,
            free_request_count: 1,
        };
        let channels = AdminDashboardChannelRecord {
            channel_count: 1,
            enabled_channel_count: 1,
            disabled_channel_count: 0,
            auto_disabled_channel_count: 0,
        };
        assert_eq!(
            AdminDashboardRecord::try_from_rows(
                usage,
                channels,
                AdminDashboardObservabilityRecord::empty(),
                AdminDashboardOutcomeRecord::empty(),
            ),
            Err(AdminDashboardRepositoryError::Invariant)
        );
    }

    #[test]
    fn generated_queries_remain_portable_across_supported_dialects() {
        let start = TimeDateTimeWithTimeZone::from_unix_timestamp(1_000).unwrap();
        let end = TimeDateTimeWithTimeZone::from_unix_timestamp(87_400).unwrap();
        for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
            let usage = backend.build(&usage_query(start, end).unwrap());
            let channels = backend.build(&channel_query());
            assert!(!usage.sql.is_empty());
            assert!(!channels.sql.is_empty());
        }
    }
}
