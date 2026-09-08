use std::{fmt, future::Future, pin::Pin, sync::Arc};

use thiserror::Error;

use crate::{
    AdminDashboardChannelFlow, AdminDashboardFailure, AdminDashboardFlowPath,
    AdminDashboardHourlyPoint, AdminDashboardPerformance,
};

/// 分析存储返回的已确认用量汇总。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardUsageSnapshot {
    pub(crate) request_count: i64,
    pub(crate) quota_consumed: i64,
    pub(crate) upstream_usage_count: i64,
    pub(crate) estimated_usage_count: i64,
    pub(crate) per_token_request_count: i64,
    pub(crate) per_call_request_count: i64,
    pub(crate) free_request_count: i64,
}

impl AdminDashboardUsageSnapshot {
    /// 组合分析存储在同一半开窗口内计算的用量汇总。
    #[must_use]
    pub const fn new(
        request_count: i64,
        quota_consumed: i64,
        upstream_usage_count: i64,
        estimated_usage_count: i64,
        per_token_request_count: i64,
        per_call_request_count: i64,
        free_request_count: i64,
    ) -> Self {
        Self {
            request_count,
            quota_consumed,
            upstream_usage_count,
            estimated_usage_count,
            per_token_request_count,
            per_call_request_count,
            free_request_count,
        }
    }
}

/// 主库返回的当前渠道状态汇总。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardChannelSnapshot {
    pub(crate) enabled_channel_count: i64,
    pub(crate) disabled_channel_count: i64,
    pub(crate) auto_disabled_channel_count: i64,
}

impl AdminDashboardChannelSnapshot {
    /// 组合当前未删除渠道的三类闭合状态计数。
    #[must_use]
    pub const fn new(
        enabled_channel_count: i64,
        disabled_channel_count: i64,
        auto_disabled_channel_count: i64,
    ) -> Self {
        Self {
            enabled_channel_count,
            disabled_channel_count,
            auto_disabled_channel_count,
        }
    }
}

/// 与事务主库解耦的管理看板分析事实快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardAnalyticsSnapshot {
    pub(crate) usage: AdminDashboardUsageSnapshot,
    pub(crate) outcomes: AdminDashboardOutcomeSnapshot,
    pub(crate) hourly: Vec<AdminDashboardHourlyPoint>,
    pub(crate) performance: AdminDashboardPerformance,
}

impl AdminDashboardAnalyticsSnapshot {
    /// 组合一个半开窗口内的用量、终态、小时分桶和性能事实。
    #[must_use]
    pub fn new(
        usage: AdminDashboardUsageSnapshot,
        outcomes: AdminDashboardOutcomeSnapshot,
        hourly: Vec<AdminDashboardHourlyPoint>,
        performance: AdminDashboardPerformance,
    ) -> Self {
        Self {
            usage,
            outcomes,
            hourly,
            performance,
        }
    }
}

/// 分析存储返回的请求终态汇总。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardOutcomeSnapshot {
    pub(crate) request_count: i64,
    pub(crate) successful_request_count: i64,
    pub(crate) failed_request_count: i64,
    pub(crate) other_success_count: i64,
    pub(crate) failures: Vec<AdminDashboardFailure>,
    pub(crate) channel_flows: Vec<AdminDashboardChannelFlow>,
    pub(crate) flow_request_count: i64,
    pub(crate) flow_quota_consumed: i64,
    pub(crate) flow_paths: Vec<AdminDashboardFlowPath>,
}

impl AdminDashboardOutcomeSnapshot {
    /// 组合请求终态、失败分类和可见成功流向。
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "参数与看板终态及四层流向事实一一对应"
    )]
    pub fn new(
        request_count: i64,
        successful_request_count: i64,
        failed_request_count: i64,
        other_success_count: i64,
        failures: Vec<AdminDashboardFailure>,
        channel_flows: Vec<AdminDashboardChannelFlow>,
        flow_request_count: i64,
        flow_quota_consumed: i64,
        flow_paths: Vec<AdminDashboardFlowPath>,
    ) -> Self {
        Self {
            request_count,
            successful_request_count,
            failed_request_count,
            other_success_count,
            failures,
            channel_flows,
            flow_request_count,
            flow_quota_consumed,
            flow_paths,
        }
    }
}

/// 一个时间窗口内的完整分析读取快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardStorageSnapshot {
    pub(crate) usage: AdminDashboardUsageSnapshot,
    pub(crate) channels: AdminDashboardChannelSnapshot,
    pub(crate) outcomes: AdminDashboardOutcomeSnapshot,
    pub(crate) hourly: Vec<AdminDashboardHourlyPoint>,
    pub(crate) performance: AdminDashboardPerformance,
}

impl AdminDashboardStorageSnapshot {
    /// 组合用量、终态、当前渠道状态和性能观测。
    #[must_use]
    pub fn new(
        usage: AdminDashboardUsageSnapshot,
        channels: AdminDashboardChannelSnapshot,
        outcomes: AdminDashboardOutcomeSnapshot,
        hourly: Vec<AdminDashboardHourlyPoint>,
        performance: AdminDashboardPerformance,
    ) -> Self {
        Self {
            usage,
            channels,
            outcomes,
            hourly,
            performance,
        }
    }

    /// 将可迁移的分析事实与主库权威的当前渠道状态合成完整快照。
    #[must_use]
    pub fn from_parts(
        analytics: AdminDashboardAnalyticsSnapshot,
        channels: AdminDashboardChannelSnapshot,
    ) -> Self {
        Self {
            usage: analytics.usage,
            channels,
            outcomes: analytics.outcomes,
            hourly: analytics.hourly,
            performance: analytics.performance,
        }
    }
}

/// 分析读取存储端口的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardStorageError {
    /// 分析存储或主库当前不可读。
    #[error("管理看板存储暂不可用")]
    Unavailable,
    /// 存储返回的聚合事实违反闭合不变量。
    #[error("管理看板存储快照损坏")]
    Invariant,
}

/// 分析读取存储端口的异步结果。
pub type AdminDashboardStorageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminDashboardStorageSnapshot, AdminDashboardStorageError>>
            + Send
            + 'a,
    >,
>;

/// 可迁移分析事实源的异步结果。
pub type AdminDashboardAnalyticsStorageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminDashboardAnalyticsSnapshot, AdminDashboardStorageError>>
            + Send
            + 'a,
    >,
>;

/// 主库当前渠道状态源的异步结果。
pub type AdminDashboardChannelStorageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminDashboardChannelSnapshot, AdminDashboardStorageError>>
            + Send
            + 'a,
    >,
>;

/// 可由事务库或 ClickHouse 提供的历史分析事实端口。
pub trait AdminDashboardAnalyticsStorage: Send + Sync {
    /// 读取指定半开窗口，不得返回当前渠道配置状态。
    fn analytics_snapshot(
        &self,
        period_start: i64,
        period_end: i64,
    ) -> AdminDashboardAnalyticsStorageFuture<'_>;
}

/// 始终由事务主库提供的当前渠道状态端口。
pub trait AdminDashboardChannelStorage: Send + Sync {
    /// 读取当前未删除渠道的闭合状态计数。
    fn channel_snapshot(&self) -> AdminDashboardChannelStorageFuture<'_>;
}

/// 管理看板使用的可替换分析读取存储端口。
pub trait AdminDashboardStorage: Send + Sync {
    /// 读取指定半开时间窗口；实现必须同时提供主库权威的当前渠道状态。
    fn snapshot(&self, period_start: i64, period_end: i64) -> AdminDashboardStorageFuture<'_>;
}

/// 将可替换分析事实源与主库渠道状态源组合为既有看板端口。
#[derive(Clone)]
pub struct SplitAdminDashboardStorage {
    analytics: Arc<dyn AdminDashboardAnalyticsStorage>,
    channels: Arc<dyn AdminDashboardChannelStorage>,
}

impl SplitAdminDashboardStorage {
    /// 显式绑定分析事实与主库渠道状态；调用方不能省略任一来源。
    #[must_use]
    pub fn new(
        analytics: Arc<dyn AdminDashboardAnalyticsStorage>,
        channels: Arc<dyn AdminDashboardChannelStorage>,
    ) -> Self {
        Self {
            analytics,
            channels,
        }
    }
}

impl AdminDashboardStorage for SplitAdminDashboardStorage {
    fn snapshot(&self, period_start: i64, period_end: i64) -> AdminDashboardStorageFuture<'_> {
        Box::pin(async move {
            let (analytics, channels) = futures_util::try_join!(
                self.analytics.analytics_snapshot(period_start, period_end),
                self.channels.channel_snapshot()
            )?;
            Ok(AdminDashboardStorageSnapshot::from_parts(
                analytics, channels,
            ))
        })
    }
}

impl fmt::Debug for SplitAdminDashboardStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SplitAdminDashboardStorage(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticAnalytics(AdminDashboardAnalyticsSnapshot);

    impl AdminDashboardAnalyticsStorage for StaticAnalytics {
        fn analytics_snapshot(&self, _: i64, _: i64) -> AdminDashboardAnalyticsStorageFuture<'_> {
            let snapshot = self.0.clone();
            Box::pin(async move { Ok(snapshot) })
        }
    }

    struct StaticChannels(AdminDashboardChannelSnapshot);

    impl AdminDashboardChannelStorage for StaticChannels {
        fn channel_snapshot(&self) -> AdminDashboardChannelStorageFuture<'_> {
            let snapshot = self.0;
            Box::pin(async move { Ok(snapshot) })
        }
    }

    struct FailingAnalytics(AdminDashboardStorageError);

    impl AdminDashboardAnalyticsStorage for FailingAnalytics {
        fn analytics_snapshot(&self, _: i64, _: i64) -> AdminDashboardAnalyticsStorageFuture<'_> {
            let error = self.0;
            Box::pin(async move { Err(error) })
        }
    }

    struct FailingChannels(AdminDashboardStorageError);

    impl AdminDashboardChannelStorage for FailingChannels {
        fn channel_snapshot(&self) -> AdminDashboardChannelStorageFuture<'_> {
            let error = self.0;
            Box::pin(async move { Err(error) })
        }
    }

    #[tokio::test]
    async fn split_storage_combines_analytics_with_explicit_channel_authority() {
        let analytics = AdminDashboardAnalyticsSnapshot::new(
            AdminDashboardUsageSnapshot::new(0, 0, 0, 0, 0, 0, 0),
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
            Vec::new(),
            AdminDashboardPerformance::new(0, None, 0, 2_000, 0, None, 0, 10_000),
        );
        let channels = AdminDashboardChannelSnapshot::new(2, 3, 5);
        let storage = SplitAdminDashboardStorage::new(
            Arc::new(StaticAnalytics(analytics.clone())),
            Arc::new(StaticChannels(channels)),
        );

        assert_eq!(
            storage.snapshot(7, 86_407).await.unwrap(),
            AdminDashboardStorageSnapshot::from_parts(analytics, channels)
        );
    }

    #[tokio::test]
    async fn split_storage_preserves_each_source_failure() {
        let analytics = AdminDashboardAnalyticsSnapshot::new(
            AdminDashboardUsageSnapshot::new(0, 0, 0, 0, 0, 0, 0),
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
            Vec::new(),
            AdminDashboardPerformance::new(0, None, 0, 2_000, 0, None, 0, 10_000),
        );
        let channels = AdminDashboardChannelSnapshot::new(0, 0, 0);

        let analytics_failure = SplitAdminDashboardStorage::new(
            Arc::new(FailingAnalytics(AdminDashboardStorageError::Unavailable)),
            Arc::new(StaticChannels(channels)),
        );
        assert_eq!(
            analytics_failure.snapshot(7, 86_407).await,
            Err(AdminDashboardStorageError::Unavailable)
        );

        let channel_failure = SplitAdminDashboardStorage::new(
            Arc::new(StaticAnalytics(analytics)),
            Arc::new(FailingChannels(AdminDashboardStorageError::Invariant)),
        );
        assert_eq!(
            channel_failure.snapshot(7, 86_407).await,
            Err(AdminDashboardStorageError::Invariant)
        );
    }
}
