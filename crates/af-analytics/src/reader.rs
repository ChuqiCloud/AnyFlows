//! 管理看板的存储读取服务与完整性校验。

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use crate::{
    ADMIN_DASHBOARD_PERIOD_SECONDS, AdminDashboard, AdminDashboardAccess,
    AdminDashboardChannelFlow, AdminDashboardHourlyPoint, AdminDashboardPerformance,
    AdminDashboardStorage, AdminDashboardStorageError, AdminDashboardStorageSnapshot,
    AdminDashboardUsageSnapshot,
};

const ADMIN_DASHBOARD_BUCKET_COUNT: usize = 24;
const ADMIN_DASHBOARD_BUCKET_SECONDS: i64 = 60 * 60;

/// 管理看板读取错误，不向边界层泄露存储和统计细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardReadError {
    /// 当前会话不是管理员。
    #[error("需要管理员权限")]
    Forbidden,
    /// 时钟、存储查询或聚合不变量校验失败。
    #[error("管理看板读取失败")]
    Internal,
}

/// 管理看板异步读取结果。
pub type AdminDashboardReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminDashboard, AdminDashboardReadError>> + Send + 'a>>;

/// 管理看板应用层只读端口。
pub trait AdminDashboardReader: Send + Sync {
    /// 校验管理员权限并读取最近 24 小时的真实快照。
    fn read(&self, access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_>;

    fn service_levels(
        &self,
        access: AdminDashboardAccess,
        _query: crate::ServiceLevelQuery,
    ) -> crate::ServiceLevelReadFuture<'_> {
        Box::pin(async move {
            require_admin(access)?;
            Err(AdminDashboardReadError::Internal)
        })
    }
}

type DashboardClock = Arc<dyn Fn() -> Result<i64, AdminDashboardReadError> + Send + Sync>;

/// 使用可替换存储端口的管理看板读取服务。
#[derive(Clone)]
pub struct StorageAdminDashboardReader {
    storage: Arc<dyn AdminDashboardStorage>,
    clock: DashboardClock,
    service_level_storage: Option<Arc<dyn crate::ServiceLevelStorage>>,
}

impl StorageAdminDashboardReader {
    /// 使用系统 UTC 时钟和分析读取存储端口构造服务。
    #[must_use]
    pub fn new(storage: Arc<dyn AdminDashboardStorage>) -> Self {
        Self {
            storage,
            clock: Arc::new(system_unix_seconds),
            service_level_storage: None,
        }
    }

    #[cfg(test)]
    fn with_clock(storage: Arc<dyn AdminDashboardStorage>, clock: DashboardClock) -> Self {
        Self {
            storage,
            clock,
            service_level_storage: None,
        }
    }

    #[must_use]
    pub fn with_service_level_storage(
        mut self,
        storage: Arc<dyn crate::ServiceLevelStorage>,
    ) -> Self {
        self.service_level_storage = Some(storage);
        self
    }
}

impl AdminDashboardReader for StorageAdminDashboardReader {
    fn service_levels(
        &self,
        access: AdminDashboardAccess,
        query: crate::ServiceLevelQuery,
    ) -> crate::ServiceLevelReadFuture<'_> {
        Box::pin(async move {
            require_admin(access)?;
            if !query.is_valid() {
                return Err(AdminDashboardReadError::Internal);
            }
            let (start, end) = period_ending_at((self.clock)()?)?;
            self.service_level_storage
                .as_ref()
                .ok_or(AdminDashboardReadError::Internal)?
                .report(start, end, query)
                .await
                .map_err(map_storage_error)
        })
    }

    fn read(&self, access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_> {
        Box::pin(async move {
            require_admin(access)?;
            let period_end = (self.clock)()?;
            let (period_start, period_end) = period_ending_at(period_end)?;
            let snapshot = self
                .storage
                .snapshot(period_start, period_end)
                .await
                .map_err(map_storage_error)?;
            dashboard_from_storage(period_start, period_end, snapshot)
        })
    }
}

impl fmt::Debug for StorageAdminDashboardReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StorageAdminDashboardReader(<redacted>)")
    }
}

fn dashboard_from_storage(
    period_start: i64,
    period_end: i64,
    snapshot: AdminDashboardStorageSnapshot,
) -> Result<AdminDashboard, AdminDashboardReadError> {
    validate_storage_snapshot(period_start, period_end, &snapshot)?;
    let AdminDashboardStorageSnapshot {
        usage,
        channels,
        outcomes,
        hourly,
        performance,
    } = snapshot;
    Ok(AdminDashboard {
        period_start,
        period_end,
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
        outcome_request_count: outcomes.request_count,
        successful_request_count: outcomes.successful_request_count,
        failed_request_count: outcomes.failed_request_count,
        other_success_count: outcomes.other_success_count,
        failures: outcomes.failures,
        channel_flows: outcomes.channel_flows,
        flow_request_count: outcomes.flow_request_count,
        flow_quota_consumed: outcomes.flow_quota_consumed,
        flow_paths: outcomes.flow_paths,
        hourly,
        performance,
    })
}

fn require_admin(access: AdminDashboardAccess) -> Result<(), AdminDashboardReadError> {
    if access == AdminDashboardAccess::Admin {
        Ok(())
    } else {
        Err(AdminDashboardReadError::Forbidden)
    }
}

fn period_ending_at(period_end: i64) -> Result<(i64, i64), AdminDashboardReadError> {
    let period_start = period_end
        .checked_sub(ADMIN_DASHBOARD_PERIOD_SECONDS)
        .filter(|start| *start >= 0)
        .ok_or(AdminDashboardReadError::Internal)?;
    Ok((period_start, period_end))
}

fn system_unix_seconds() -> Result<i64, AdminDashboardReadError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AdminDashboardReadError::Internal)?;
    i64::try_from(elapsed.as_secs()).map_err(|_| AdminDashboardReadError::Internal)
}

fn validate_storage_snapshot(
    period_start: i64,
    period_end: i64,
    snapshot: &AdminDashboardStorageSnapshot,
) -> Result<(), AdminDashboardReadError> {
    let usage = snapshot.usage;
    let channels = snapshot.channels;
    let outcomes = &snapshot.outcomes;
    let usage_values = [
        usage.request_count,
        usage.quota_consumed,
        usage.upstream_usage_count,
        usage.estimated_usage_count,
        usage.per_token_request_count,
        usage.per_call_request_count,
        usage.free_request_count,
    ];
    let channel_values = [
        channels.enabled_channel_count,
        channels.disabled_channel_count,
        channels.auto_disabled_channel_count,
    ];
    let outcome_values = [
        outcomes.request_count,
        outcomes.successful_request_count,
        outcomes.failed_request_count,
        outcomes.other_success_count,
    ];
    let usage_source_total = usage
        .upstream_usage_count
        .checked_add(usage.estimated_usage_count);
    let billing_mode_total = usage
        .per_token_request_count
        .checked_add(usage.per_call_request_count)
        .and_then(|value| value.checked_add(usage.free_request_count));
    let outcome_total = outcomes
        .successful_request_count
        .checked_add(outcomes.failed_request_count);
    let failure_total = checked_sum(
        outcomes
            .failures
            .iter()
            .map(|failure| failure.request_count()),
    )?;
    let visible_success_total = checked_sum(
        outcomes
            .channel_flows
            .iter()
            .map(AdminDashboardChannelFlow::request_count),
    )?;
    let classified_success_total = visible_success_total.checked_add(outcomes.other_success_count);
    let visible_flow_requests =
        checked_sum(outcomes.flow_paths.iter().map(|path| path.request_count()))?;
    let visible_flow_quota =
        checked_sum(outcomes.flow_paths.iter().map(|path| path.quota_consumed()))?;

    if period_end.checked_sub(period_start) != Some(ADMIN_DASHBOARD_PERIOD_SECONDS)
        || usage_values.into_iter().any(|value| value < 0)
        || channel_values.into_iter().any(|value| value < 0)
        || outcome_values.into_iter().any(|value| value < 0)
        || usage_source_total != Some(usage.request_count)
        || billing_mode_total != Some(usage.request_count)
        || outcome_total != Some(outcomes.request_count)
        || failure_total != outcomes.failed_request_count
        || classified_success_total != Some(outcomes.successful_request_count)
        || outcomes.flow_request_count < 0
        || outcomes.flow_quota_consumed < 0
        || visible_flow_requests > outcomes.flow_request_count
        || visible_flow_quota > outcomes.flow_quota_consumed
        || outcomes
            .failures
            .iter()
            .any(|failure| failure.request_count() <= 0)
        || outcomes
            .channel_flows
            .iter()
            .any(|flow| flow.request_count() <= 0 || flow.channel_name().trim().is_empty())
        || outcomes.flow_paths.iter().any(|path| {
            path.user_id() <= 0
                || path.group_id() <= 0
                || path.request_count() <= 0
                || path.quota_consumed() < 0
                || path.group_name().trim().is_empty()
                || path.channel_name().trim().is_empty()
                || path.model().trim().is_empty()
        })
        || !valid_hourly_points(period_start, period_end, &snapshot.hourly, usage)?
        || !valid_performance(snapshot.performance)
    {
        return Err(AdminDashboardReadError::Internal);
    }
    Ok(())
}

fn valid_hourly_points(
    period_start: i64,
    period_end: i64,
    hourly: &[AdminDashboardHourlyPoint],
    usage: AdminDashboardUsageSnapshot,
) -> Result<bool, AdminDashboardReadError> {
    if hourly.len() != ADMIN_DASHBOARD_BUCKET_COUNT {
        return Ok(false);
    }
    let mut expected_start = period_start;
    for point in hourly {
        let expected_end = expected_start
            .checked_add(ADMIN_DASHBOARD_BUCKET_SECONDS)
            .ok_or(AdminDashboardReadError::Internal)?;
        if point.period_start() != expected_start
            || point.period_end() != expected_end
            || point.request_count() < 0
            || point.quota_consumed() < 0
        {
            return Ok(false);
        }
        expected_start = expected_end;
    }
    let request_total = checked_sum(hourly.iter().map(|point| point.request_count()))?;
    let quota_total = checked_sum(hourly.iter().map(|point| point.quota_consumed()))?;
    Ok(expected_start == period_end
        && request_total == usage.request_count
        && quota_total == usage.quota_consumed)
}

fn valid_performance(performance: AdminDashboardPerformance) -> bool {
    valid_performance_sample(
        performance.first_token_sample_count(),
        performance.average_first_token_ms(),
        performance.slow_first_token_count(),
        performance.slow_first_token_threshold_ms(),
    ) && valid_performance_sample(
        performance.duration_sample_count(),
        performance.average_duration_ms(),
        performance.slow_request_count(),
        performance.slow_request_threshold_ms(),
    )
}

fn valid_performance_sample(
    sample_count: i64,
    average_ms: Option<i64>,
    slow_count: i64,
    slow_threshold_ms: i64,
) -> bool {
    sample_count >= 0
        && slow_count >= 0
        && slow_count <= sample_count
        && slow_threshold_ms > 0
        && match (sample_count, average_ms) {
            (0, None) => true,
            (1.., Some(average_ms)) => average_ms >= 0,
            _ => false,
        }
}

fn checked_sum(mut values: impl Iterator<Item = i64>) -> Result<i64, AdminDashboardReadError> {
    values.try_fold(0_i64, |total, value| {
        total
            .checked_add(value)
            .ok_or(AdminDashboardReadError::Internal)
    })
}

fn map_storage_error(_: AdminDashboardStorageError) -> AdminDashboardReadError {
    AdminDashboardReadError::Internal
}

#[cfg(test)]
mod tests {
    use af_domain::{ChannelId, Protocol};

    use super::*;
    use crate::{
        AdminDashboardChannelSnapshot, AdminDashboardFailure, AdminDashboardFailureKind,
        AdminDashboardFlowPath, AdminDashboardOutcomeSnapshot, AdminDashboardStorageFuture,
    };

    #[derive(Clone)]
    struct StaticStorage {
        expected_start: i64,
        expected_end: i64,
        result: Result<AdminDashboardStorageSnapshot, AdminDashboardStorageError>,
    }

    impl AdminDashboardStorage for StaticStorage {
        fn snapshot(&self, period_start: i64, period_end: i64) -> AdminDashboardStorageFuture<'_> {
            let result = if period_start == self.expected_start && period_end == self.expected_end {
                self.result.clone()
            } else {
                Err(AdminDashboardStorageError::Invariant)
            };
            Box::pin(async move { result })
        }
    }

    #[test]
    fn access_and_period_boundaries_fail_closed() {
        assert_eq!(
            require_admin(AdminDashboardAccess::User),
            Err(AdminDashboardReadError::Forbidden)
        );
        assert_eq!(
            period_ending_at(ADMIN_DASHBOARD_PERIOD_SECONDS - 1),
            Err(AdminDashboardReadError::Internal)
        );
        assert_eq!(
            period_ending_at(ADMIN_DASHBOARD_PERIOD_SECONDS),
            Ok((0, ADMIN_DASHBOARD_PERIOD_SECONDS))
        );
    }

    #[tokio::test]
    async fn injected_clock_keeps_the_window_deterministic() {
        let period_start = 7;
        let period_end = ADMIN_DASHBOARD_PERIOD_SECONDS + period_start;
        let storage: Arc<dyn AdminDashboardStorage> = Arc::new(StaticStorage {
            expected_start: period_start,
            expected_end: period_end,
            result: Ok(empty_snapshot(period_start)),
        });
        let reader = StorageAdminDashboardReader::with_clock(
            storage,
            Arc::new(|| Ok(ADMIN_DASHBOARD_PERIOD_SECONDS + 7)),
        );
        let dashboard = reader.read(AdminDashboardAccess::Admin).await.unwrap();
        assert_eq!(dashboard.period_start(), period_start);
        assert_eq!(dashboard.period_end(), period_end);
        assert_eq!(dashboard.request_count(), 0);
        assert_eq!(dashboard.outcome_request_count(), 0);
        assert_eq!(dashboard.successful_request_count(), 0);
        assert_eq!(dashboard.failed_request_count(), 0);
        assert_eq!(dashboard.other_success_count(), 0);
        assert!(dashboard.failures().is_empty());
        assert!(dashboard.channel_flows().is_empty());
    }

    #[tokio::test]
    async fn complete_snapshot_maps_all_public_metrics() {
        let period_start = 10;
        let period_end = ADMIN_DASHBOARD_PERIOD_SECONDS + period_start;
        let mut snapshot = empty_snapshot(period_start);
        snapshot.usage = AdminDashboardUsageSnapshot::new(5, 50, 3, 2, 2, 2, 1);
        snapshot.channels = AdminDashboardChannelSnapshot::new(4, 1, 2);
        snapshot.outcomes = AdminDashboardOutcomeSnapshot::new(
            5,
            4,
            1,
            1,
            vec![AdminDashboardFailure::new(
                AdminDashboardFailureKind::UpstreamRateLimited,
                1,
            )],
            vec![AdminDashboardChannelFlow::new(
                Protocol::OpenAiChat,
                ChannelId::new(7).unwrap(),
                "primary".to_owned(),
                3,
            )],
            2,
            40,
            vec![AdminDashboardFlowPath::new(
                11,
                3,
                "默认分组".to_owned(),
                ChannelId::new(7).unwrap(),
                "primary".to_owned(),
                "gpt-5".to_owned(),
                2,
                40,
            )],
        );
        snapshot.hourly[0] =
            AdminDashboardHourlyPoint::new(period_start, period_start + 3_600, 5, 50);
        snapshot.performance =
            AdminDashboardPerformance::new(2, Some(120), 1, 2_000, 3, Some(800), 1, 10_000);
        let storage: Arc<dyn AdminDashboardStorage> = Arc::new(StaticStorage {
            expected_start: period_start,
            expected_end: period_end,
            result: Ok(snapshot),
        });
        let reader =
            StorageAdminDashboardReader::with_clock(storage, Arc::new(move || Ok(period_end)));

        let dashboard = reader.read(AdminDashboardAccess::Admin).await.unwrap();
        assert_eq!(dashboard.request_count(), 5);
        assert_eq!(dashboard.quota_consumed(), 50);
        assert_eq!(dashboard.upstream_usage_count(), 3);
        assert_eq!(dashboard.estimated_usage_count(), 2);
        assert_eq!(dashboard.enabled_channel_count(), 4);
        assert_eq!(dashboard.disabled_channel_count(), 1);
        assert_eq!(dashboard.auto_disabled_channel_count(), 2);
        assert_eq!(dashboard.outcome_request_count(), 5);
        assert_eq!(dashboard.successful_request_count(), 4);
        assert_eq!(dashboard.failed_request_count(), 1);
        assert_eq!(dashboard.other_success_count(), 1);
        assert_eq!(
            dashboard.failures()[0].kind(),
            AdminDashboardFailureKind::UpstreamRateLimited
        );
        assert_eq!(dashboard.channel_flows()[0].channel_name(), "primary");
        assert_eq!(dashboard.flow_request_count(), 2);
        assert_eq!(dashboard.flow_quota_consumed(), 40);
        assert_eq!(dashboard.flow_paths()[0].model(), "gpt-5");
        assert_eq!(dashboard.hourly()[0].request_count(), 5);
        assert_eq!(dashboard.performance().average_duration_ms(), Some(800));
    }

    #[tokio::test]
    async fn malformed_or_unavailable_storage_fails_closed() {
        let period_start = 10;
        let period_end = ADMIN_DASHBOARD_PERIOD_SECONDS + period_start;
        let mut malformed = empty_snapshot(period_start);
        malformed.usage.request_count = 1;
        let malformed_storage: Arc<dyn AdminDashboardStorage> = Arc::new(StaticStorage {
            expected_start: period_start,
            expected_end: period_end,
            result: Ok(malformed),
        });
        let reader = StorageAdminDashboardReader::with_clock(
            malformed_storage,
            Arc::new(move || Ok(period_end)),
        );
        assert_eq!(
            reader.read(AdminDashboardAccess::Admin).await.unwrap_err(),
            AdminDashboardReadError::Internal
        );

        let unavailable_storage: Arc<dyn AdminDashboardStorage> = Arc::new(StaticStorage {
            expected_start: period_start,
            expected_end: period_end,
            result: Err(AdminDashboardStorageError::Unavailable),
        });
        let reader = StorageAdminDashboardReader::with_clock(
            unavailable_storage,
            Arc::new(move || Ok(period_end)),
        );
        assert_eq!(
            reader.read(AdminDashboardAccess::Admin).await.unwrap_err(),
            AdminDashboardReadError::Internal
        );
    }

    #[test]
    fn malformed_snapshot_invariants_fail_closed() {
        let period_start = 10;
        let period_end = ADMIN_DASHBOARD_PERIOD_SECONDS + period_start;
        let cases = [
            {
                let mut snapshot = empty_snapshot(period_start);
                snapshot.usage.quota_consumed = -1;
                snapshot
            },
            {
                let mut snapshot = empty_snapshot(period_start);
                snapshot.outcomes.failures = vec![AdminDashboardFailure::new(
                    AdminDashboardFailureKind::Internal,
                    0,
                )];
                snapshot
            },
            {
                let mut snapshot = empty_snapshot(period_start);
                snapshot.hourly[1] =
                    AdminDashboardHourlyPoint::new(period_start, period_start + 3_600, 0, 0);
                snapshot
            },
            {
                let mut snapshot = empty_snapshot(period_start);
                snapshot.performance =
                    AdminDashboardPerformance::new(0, Some(0), 0, 2_000, 0, None, 0, 10_000);
                snapshot
            },
            {
                let mut snapshot = empty_snapshot(period_start);
                snapshot.outcomes.failures = vec![
                    AdminDashboardFailure::new(AdminDashboardFailureKind::Internal, i64::MAX),
                    AdminDashboardFailure::new(AdminDashboardFailureKind::Internal, 1),
                ];
                snapshot
            },
        ];

        for snapshot in cases {
            assert_eq!(
                dashboard_from_storage(period_start, period_end, snapshot),
                Err(AdminDashboardReadError::Internal)
            );
        }
    }

    #[test]
    fn performance_sample_shape_boundaries_fail_closed() {
        let valid_cases = [
            (0, None, 0, 1),
            (1, Some(0), 0, 1),
            (2, Some(i64::MAX), 2, i64::MAX),
        ];
        for (sample_count, average_ms, slow_count, slow_threshold_ms) in valid_cases {
            assert!(valid_performance_sample(
                sample_count,
                average_ms,
                slow_count,
                slow_threshold_ms
            ));
        }

        let invalid_cases = [
            (-1, None, 0, 1),
            (0, Some(0), 0, 1),
            (1, None, 0, 1),
            (1, Some(-1), 0, 1),
            (1, Some(0), -1, 1),
            (1, Some(0), 2, 1),
            (1, Some(0), 0, 0),
        ];
        for (sample_count, average_ms, slow_count, slow_threshold_ms) in invalid_cases {
            assert!(!valid_performance_sample(
                sample_count,
                average_ms,
                slow_count,
                slow_threshold_ms
            ));
        }
    }

    #[test]
    fn hourly_bucket_and_aggregate_overflow_fail_closed() {
        let hourly = vec![
            AdminDashboardHourlyPoint::new(i64::MAX, i64::MAX, 0, 0);
            ADMIN_DASHBOARD_BUCKET_COUNT
        ];
        assert_eq!(
            valid_hourly_points(
                i64::MAX,
                i64::MAX,
                &hourly,
                AdminDashboardUsageSnapshot::new(0, 0, 0, 0, 0, 0, 0),
            ),
            Err(AdminDashboardReadError::Internal)
        );

        assert_eq!(
            checked_sum([i64::MAX, 1].into_iter()),
            Err(AdminDashboardReadError::Internal)
        );
        assert_eq!(checked_sum([i64::MAX].into_iter()), Ok(i64::MAX));
    }

    fn empty_snapshot(period_start: i64) -> AdminDashboardStorageSnapshot {
        let hourly = (0..ADMIN_DASHBOARD_BUCKET_COUNT)
            .map(|index| {
                let start = period_start + i64::try_from(index).unwrap() * 60 * 60;
                AdminDashboardHourlyPoint::new(start, start + 60 * 60, 0, 0)
            })
            .collect();
        AdminDashboardStorageSnapshot::new(
            AdminDashboardUsageSnapshot::new(0, 0, 0, 0, 0, 0, 0),
            AdminDashboardChannelSnapshot::new(0, 0, 0),
            AdminDashboardOutcomeSnapshot::new(
                0,
                0,
                0,
                0,
                Vec::new(),
                Vec::new(),
                0,
                0,
                Vec::new(),
            ),
            hourly,
            AdminDashboardPerformance::new(0, None, 0, 2_000, 0, None, 0, 10_000),
        )
    }
}
