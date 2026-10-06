use std::fmt;

use af_analytics::{
    AdminDashboardChannelFlow, AdminDashboardChannelSnapshot, AdminDashboardChannelStorage,
    AdminDashboardChannelStorageFuture, AdminDashboardFailure, AdminDashboardFailureKind,
    AdminDashboardFlowPath, AdminDashboardHourlyPoint, AdminDashboardOutcomeSnapshot,
    AdminDashboardPerformance, AdminDashboardStorage, AdminDashboardStorageError,
    AdminDashboardStorageFuture, AdminDashboardStorageSnapshot, AdminDashboardUsageSnapshot,
};
use af_db::{
    AdminDashboardRecord, AdminDashboardRepository, AdminDashboardRepositoryError,
    RequestFailureKind, SLOW_FIRST_TOKEN_THRESHOLD_MS, SLOW_REQUEST_THRESHOLD_MS,
};

mod clickhouse;

pub use clickhouse::{
    ClickHouseAdminDashboardConfig, ClickHouseAdminDashboardConfigError,
    ClickHouseAdminDashboardStorage, DEFAULT_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES,
    DEFAULT_CLICKHOUSE_DASHBOARD_TIMEOUT, MAX_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES,
    MAX_CLICKHOUSE_DASHBOARD_TIMEOUT,
};

/// 将当前事务数据库适配为分析读取存储端口。
#[derive(Clone)]
pub(crate) struct DatabaseAdminDashboardStorage {
    repository: AdminDashboardRepository,
}

impl DatabaseAdminDashboardStorage {
    /// 包装已经配置共同查询截止时间的主库聚合仓储。
    #[must_use]
    pub(crate) const fn new(repository: AdminDashboardRepository) -> Self {
        Self { repository }
    }
}

impl AdminDashboardStorage for DatabaseAdminDashboardStorage {
    fn snapshot(&self, period_start: i64, period_end: i64) -> AdminDashboardStorageFuture<'_> {
        Box::pin(async move {
            let record = self
                .repository
                .snapshot(period_start, period_end)
                .await
                .map_err(map_repository_error)?;
            Ok(storage_snapshot(record))
        })
    }
}

impl af_analytics::ServiceLevelStorage for DatabaseAdminDashboardStorage {
    fn report(
        &self,
        start: i64,
        end: i64,
        query: af_analytics::ServiceLevelQuery,
    ) -> af_analytics::ServiceLevelStorageFuture<'_> {
        Box::pin(async move {
            let record = self
                .repository
                .service_levels(
                    start,
                    end,
                    af_db::DashboardServiceLevelQuery {
                        channels: query.dimension == af_analytics::ServiceLevelDimension::Channel,
                        search: query.search,
                        page: query.page,
                        page_size: query.page_size,
                        failures_first: query.failures_first,
                    },
                )
                .await
                .map_err(map_repository_error)?;
            Ok(af_analytics::ServiceLevelReport {
                period_start: start,
                period_end: end,
                total: record.total,
                unattributed_request_count: record.unattributed_request_count,
                items: record
                    .items
                    .into_iter()
                    .map(|row| af_analytics::ServiceLevelRow {
                        key: row.key,
                        name: row.name,
                        request_count: row.request_count,
                        successful_request_count: row.successful_request_count,
                        failed_request_count: row.failed_request_count,
                        unknown_request_count: row.unknown_request_count,
                        average_duration_ms: row.average_duration_ms,
                        hourly: row
                            .hourly
                            .into_iter()
                            .map(|point| af_analytics::ServiceLevelPoint {
                                period_start: point.period_start,
                                successful_request_count: point.successful_request_count,
                                failed_request_count: point.failed_request_count,
                                unknown_request_count: point.unknown_request_count,
                            })
                            .collect(),
                    })
                    .collect(),
            })
        })
    }
}

impl fmt::Debug for DatabaseAdminDashboardStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminDashboardStorage(<redacted>)")
    }
}

/// 仅向分离分析存储提供事务主库权威的当前渠道状态。
#[derive(Clone)]
pub struct DatabaseAdminDashboardChannelStorage {
    repository: AdminDashboardRepository,
}

impl DatabaseAdminDashboardChannelStorage {
    /// 包装共享主库看板仓储，但只开放当前渠道状态查询。
    #[must_use]
    pub const fn new(repository: AdminDashboardRepository) -> Self {
        Self { repository }
    }
}

impl AdminDashboardChannelStorage for DatabaseAdminDashboardChannelStorage {
    fn channel_snapshot(&self) -> AdminDashboardChannelStorageFuture<'_> {
        Box::pin(async move {
            let channels = self
                .repository
                .channel_snapshot()
                .await
                .map_err(map_repository_error)?;
            Ok(AdminDashboardChannelSnapshot::new(
                channels.enabled_channel_count(),
                channels.disabled_channel_count(),
                channels.auto_disabled_channel_count(),
            ))
        })
    }
}

impl fmt::Debug for DatabaseAdminDashboardChannelStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminDashboardChannelStorage(<redacted>)")
    }
}

fn storage_snapshot(record: AdminDashboardRecord) -> AdminDashboardStorageSnapshot {
    let usage = AdminDashboardUsageSnapshot::new(
        record.request_count(),
        record.quota_consumed(),
        record.upstream_usage_count(),
        record.estimated_usage_count(),
        record.per_token_request_count(),
        record.per_call_request_count(),
        record.free_request_count(),
    );
    let channels = AdminDashboardChannelSnapshot::new(
        record.enabled_channel_count(),
        record.disabled_channel_count(),
        record.auto_disabled_channel_count(),
    );
    let outcomes = AdminDashboardOutcomeSnapshot::new(
        record.outcome_request_count(),
        record.successful_request_count(),
        record.failed_request_count(),
        record.other_success_count(),
        record
            .failures()
            .iter()
            .map(|failure| {
                AdminDashboardFailure::new(
                    map_failure_kind(failure.kind()),
                    failure.request_count(),
                )
            })
            .collect(),
        record
            .channel_flows()
            .iter()
            .map(|flow| {
                AdminDashboardChannelFlow::new(
                    flow.protocol(),
                    flow.channel_id(),
                    flow.channel_name().to_owned(),
                    flow.request_count(),
                )
            })
            .collect(),
        record.flow_request_count(),
        record.flow_quota_consumed(),
        record
            .flow_paths()
            .iter()
            .map(|path| {
                AdminDashboardFlowPath::new(
                    path.user_id(),
                    path.group_id(),
                    path.group_name().to_owned(),
                    path.channel_id(),
                    path.channel_name().to_owned(),
                    path.model().to_owned(),
                    path.request_count(),
                    path.quota_consumed(),
                )
            })
            .collect(),
    );
    let hourly = record
        .hourly()
        .iter()
        .map(|point| {
            AdminDashboardHourlyPoint::new(
                point.period_start(),
                point.period_end(),
                point.request_count(),
                point.quota_consumed(),
            )
        })
        .collect();
    let performance = record.performance();
    AdminDashboardStorageSnapshot::new(
        usage,
        channels,
        outcomes,
        hourly,
        AdminDashboardPerformance::new(
            performance.first_token_sample_count(),
            performance.average_first_token_ms(),
            performance.slow_first_token_count(),
            SLOW_FIRST_TOKEN_THRESHOLD_MS,
            performance.duration_sample_count(),
            performance.average_duration_ms(),
            performance.slow_request_count(),
            SLOW_REQUEST_THRESHOLD_MS,
        ),
    )
}

fn map_failure_kind(kind: RequestFailureKind) -> AdminDashboardFailureKind {
    match kind {
        RequestFailureKind::InvalidRequest => AdminDashboardFailureKind::InvalidRequest,
        RequestFailureKind::ModelNotAllowed => AdminDashboardFailureKind::ModelNotAllowed,
        RequestFailureKind::InsufficientQuota => AdminDashboardFailureKind::InsufficientQuota,
        RequestFailureKind::QuotaLimited => AdminDashboardFailureKind::QuotaLimited,
        RequestFailureKind::ConcurrencyLimited => AdminDashboardFailureKind::ConcurrencyLimited,
        RequestFailureKind::OutcomeUnknown => AdminDashboardFailureKind::OutcomeUnknown,
        RequestFailureKind::UpstreamRateLimited => AdminDashboardFailureKind::UpstreamRateLimited,
        RequestFailureKind::UpstreamOverloaded => AdminDashboardFailureKind::UpstreamOverloaded,
        RequestFailureKind::UpstreamAuthentication => {
            AdminDashboardFailureKind::UpstreamAuthentication
        }
        RequestFailureKind::UpstreamQuota => AdminDashboardFailureKind::UpstreamQuota,
        RequestFailureKind::UpstreamModel => AdminDashboardFailureKind::UpstreamModel,
        RequestFailureKind::UpstreamProtocol => AdminDashboardFailureKind::UpstreamProtocol,
        RequestFailureKind::UpstreamServer => AdminDashboardFailureKind::UpstreamServer,
        RequestFailureKind::UpstreamNetwork => AdminDashboardFailureKind::UpstreamNetwork,
        RequestFailureKind::Internal => AdminDashboardFailureKind::Internal,
        _ => AdminDashboardFailureKind::Internal,
    }
}

fn map_repository_error(error: AdminDashboardRepositoryError) -> AdminDashboardStorageError {
    match error {
        AdminDashboardRepositoryError::Query | AdminDashboardRepositoryError::Timeout => {
            AdminDashboardStorageError::Unavailable
        }
        AdminDashboardRepositoryError::Invariant => AdminDashboardStorageError::Invariant,
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use af_analytics::{AdminDashboardAccess, AdminDashboardReader, StorageAdminDashboardReader};
    use af_db::{DatabaseOptions, MigrationOptions};

    use super::*;

    #[tokio::test]
    async fn empty_database_adapts_to_a_closed_dashboard_snapshot() {
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repository =
            AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2)).unwrap();
        let channels = DatabaseAdminDashboardChannelStorage::new(repository.clone())
            .channel_snapshot()
            .await
            .unwrap();
        let storage: Arc<dyn AdminDashboardStorage> =
            Arc::new(DatabaseAdminDashboardStorage::new(repository));
        let dashboard = StorageAdminDashboardReader::new(storage)
            .read(AdminDashboardAccess::Admin)
            .await
            .unwrap();

        assert_eq!(dashboard.request_count(), 0);
        assert_eq!(dashboard.outcome_request_count(), 0);
        assert_eq!(dashboard.hourly().len(), 24);
        assert_eq!(channels, AdminDashboardChannelSnapshot::new(0, 0, 0));
        pool.close().await.unwrap();
    }
}
