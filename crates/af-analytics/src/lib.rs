//! 用量记录、聚合与分析投影。

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    AdminDashboardRecord, AdminDashboardRepository, AdminDashboardRepositoryError,
    RequestFailureKind, SLOW_FIRST_TOKEN_THRESHOLD_MS, SLOW_REQUEST_THRESHOLD_MS,
};
use af_domain::{ChannelId, Protocol};
use thiserror::Error;

/// 管理看板固定使用最近 24 小时的持久化用量窗口。
pub const ADMIN_DASHBOARD_PERIOD_SECONDS: i64 = 24 * 60 * 60;

/// 调用看板分析服务时携带的管理权限等级。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminDashboardAccess {
    /// 普通管理会话，不允许读取全局经营指标。
    User,
    /// 管理员会话，可以读取全局经营指标。
    Admin,
}

/// 管理员看板公开的最近 24 小时快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboard {
    period_start: i64,
    period_end: i64,
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
    outcome_request_count: i64,
    successful_request_count: i64,
    failed_request_count: i64,
    other_success_count: i64,
    failures: Vec<AdminDashboardFailure>,
    channel_flows: Vec<AdminDashboardChannelFlow>,
    hourly: Vec<AdminDashboardHourlyPoint>,
    performance: AdminDashboardPerformance,
}

/// 管理看板公开的闭合失败分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminDashboardFailureKind {
    InvalidRequest,
    ModelNotAllowed,
    InsufficientQuota,
    QuotaLimited,
    ConcurrencyLimited,
    OutcomeUnknown,
    UpstreamRateLimited,
    UpstreamOverloaded,
    UpstreamAuthentication,
    UpstreamQuota,
    UpstreamModel,
    UpstreamProtocol,
    UpstreamServer,
    UpstreamNetwork,
    Internal,
}

impl From<RequestFailureKind> for AdminDashboardFailureKind {
    fn from(value: RequestFailureKind) -> Self {
        match value {
            RequestFailureKind::InvalidRequest => Self::InvalidRequest,
            RequestFailureKind::ModelNotAllowed => Self::ModelNotAllowed,
            RequestFailureKind::InsufficientQuota => Self::InsufficientQuota,
            RequestFailureKind::QuotaLimited => Self::QuotaLimited,
            RequestFailureKind::ConcurrencyLimited => Self::ConcurrencyLimited,
            RequestFailureKind::OutcomeUnknown => Self::OutcomeUnknown,
            RequestFailureKind::UpstreamRateLimited => Self::UpstreamRateLimited,
            RequestFailureKind::UpstreamOverloaded => Self::UpstreamOverloaded,
            RequestFailureKind::UpstreamAuthentication => Self::UpstreamAuthentication,
            RequestFailureKind::UpstreamQuota => Self::UpstreamQuota,
            RequestFailureKind::UpstreamModel => Self::UpstreamModel,
            RequestFailureKind::UpstreamProtocol => Self::UpstreamProtocol,
            RequestFailureKind::UpstreamServer => Self::UpstreamServer,
            RequestFailureKind::UpstreamNetwork => Self::UpstreamNetwork,
            RequestFailureKind::Internal => Self::Internal,
            _ => Self::Internal,
        }
    }
}

/// 管理看板窗口内一个失败分类的请求数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardFailure {
    kind: AdminDashboardFailureKind,
    request_count: i64,
}

impl AdminDashboardFailure {
    #[must_use]
    pub const fn kind(self) -> AdminDashboardFailureKind {
        self.kind
    }

    #[must_use]
    pub const fn request_count(self) -> i64 {
        self.request_count
    }
}

/// 管理看板窗口内从入口协议到最终成功渠道的流向。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardChannelFlow {
    protocol: Protocol,
    channel_id: ChannelId,
    channel_name: String,
    request_count: i64,
}

impl AdminDashboardChannelFlow {
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

/// 管理看板单个连续一小时分桶。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardHourlyPoint {
    period_start: i64,
    period_end: i64,
    request_count: i64,
    quota_consumed: i64,
}

impl AdminDashboardHourlyPoint {
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

/// 管理看板窗口内真实耗时样本的健康摘要。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardPerformance {
    first_token_sample_count: i64,
    average_first_token_ms: Option<i64>,
    slow_first_token_count: i64,
    slow_first_token_threshold_ms: i64,
    duration_sample_count: i64,
    average_duration_ms: Option<i64>,
    slow_request_count: i64,
    slow_request_threshold_ms: i64,
}

impl AdminDashboardPerformance {
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
    pub const fn slow_first_token_threshold_ms(self) -> i64 {
        self.slow_first_token_threshold_ms
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

    #[must_use]
    pub const fn slow_request_threshold_ms(self) -> i64 {
        self.slow_request_threshold_ms
    }
}

impl AdminDashboard {
    /// 返回统计窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn period_start(&self) -> i64 {
        self.period_start
    }

    /// 返回统计窗口终点的 Unix 秒数。
    #[must_use]
    pub const fn period_end(&self) -> i64 {
        self.period_end
    }

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

    /// 返回窗口内已经写入终态事实的同步模型请求数。
    #[must_use]
    pub const fn outcome_request_count(&self) -> i64 {
        self.outcome_request_count
    }

    /// 返回窗口内成功完成的同步模型请求数。
    #[must_use]
    pub const fn successful_request_count(&self) -> i64 {
        self.successful_request_count
    }

    /// 返回窗口内失败完成的同步模型请求数。
    #[must_use]
    pub const fn failed_request_count(&self) -> i64 {
        self.failed_request_count
    }

    /// 返回未进入前十二条可见流向的成功请求数。
    #[must_use]
    pub const fn other_success_count(&self) -> i64 {
        self.other_success_count
    }

    /// 返回窗口内按闭合分类聚合的失败请求。
    #[must_use]
    pub fn failures(&self) -> &[AdminDashboardFailure] {
        &self.failures
    }

    /// 返回请求量最高的协议到渠道成功流向。
    #[must_use]
    pub fn channel_flows(&self) -> &[AdminDashboardChannelFlow] {
        &self.channel_flows
    }

    /// 返回覆盖完整窗口的连续一小时分桶。
    #[must_use]
    pub fn hourly(&self) -> &[AdminDashboardHourlyPoint] {
        &self.hourly
    }

    /// 返回窗口内真实耗时样本的健康摘要。
    #[must_use]
    pub const fn performance(&self) -> AdminDashboardPerformance {
        self.performance
    }

    fn from_record(period_start: i64, period_end: i64, record: AdminDashboardRecord) -> Self {
        let performance = record.performance();
        Self {
            period_start,
            period_end,
            request_count: record.request_count(),
            quota_consumed: record.quota_consumed(),
            upstream_usage_count: record.upstream_usage_count(),
            estimated_usage_count: record.estimated_usage_count(),
            per_token_request_count: record.per_token_request_count(),
            per_call_request_count: record.per_call_request_count(),
            free_request_count: record.free_request_count(),
            enabled_channel_count: record.enabled_channel_count(),
            disabled_channel_count: record.disabled_channel_count(),
            auto_disabled_channel_count: record.auto_disabled_channel_count(),
            outcome_request_count: record.outcome_request_count(),
            successful_request_count: record.successful_request_count(),
            failed_request_count: record.failed_request_count(),
            other_success_count: record.other_success_count(),
            failures: record
                .failures()
                .iter()
                .copied()
                .map(|failure| AdminDashboardFailure {
                    kind: failure.kind().into(),
                    request_count: failure.request_count(),
                })
                .collect(),
            channel_flows: record
                .channel_flows()
                .iter()
                .map(|flow| AdminDashboardChannelFlow {
                    protocol: flow.protocol(),
                    channel_id: flow.channel_id(),
                    channel_name: flow.channel_name().to_owned(),
                    request_count: flow.request_count(),
                })
                .collect(),
            hourly: record
                .hourly()
                .iter()
                .copied()
                .map(|bucket| AdminDashboardHourlyPoint {
                    period_start: bucket.period_start(),
                    period_end: bucket.period_end(),
                    request_count: bucket.request_count(),
                    quota_consumed: bucket.quota_consumed(),
                })
                .collect(),
            performance: AdminDashboardPerformance {
                first_token_sample_count: performance.first_token_sample_count(),
                average_first_token_ms: performance.average_first_token_ms(),
                slow_first_token_count: performance.slow_first_token_count(),
                slow_first_token_threshold_ms: SLOW_FIRST_TOKEN_THRESHOLD_MS,
                duration_sample_count: performance.duration_sample_count(),
                average_duration_ms: performance.average_duration_ms(),
                slow_request_count: performance.slow_request_count(),
                slow_request_threshold_ms: SLOW_REQUEST_THRESHOLD_MS,
            },
        }
    }
}

/// 管理看板读取错误，不向边界层泄露数据库和统计细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardReadError {
    /// 当前会话不是管理员。
    #[error("需要管理员权限")]
    Forbidden,
    /// 时钟、数据库查询或持久化不变量校验失败。
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
}

type DashboardClock = Arc<dyn Fn() -> Result<i64, AdminDashboardReadError> + Send + Sync>;

/// 使用数据库聚合仓储的管理看板读取服务。
#[derive(Clone)]
pub struct DatabaseAdminDashboardReader {
    repository: AdminDashboardRepository,
    clock: DashboardClock,
}

impl DatabaseAdminDashboardReader {
    /// 使用系统 UTC 时钟和真实聚合仓储构造读取服务。
    #[must_use]
    pub fn new(repository: AdminDashboardRepository) -> Self {
        Self {
            repository,
            clock: Arc::new(system_unix_seconds),
        }
    }

    #[cfg(test)]
    fn with_clock(repository: AdminDashboardRepository, clock: DashboardClock) -> Self {
        Self { repository, clock }
    }
}

impl AdminDashboardReader for DatabaseAdminDashboardReader {
    fn read(&self, access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_> {
        Box::pin(async move {
            require_admin(access)?;
            let period_end = (self.clock)()?;
            let (period_start, period_end) = period_ending_at(period_end)?;
            let record = self
                .repository
                .snapshot(period_start, period_end)
                .await
                .map_err(map_repository_error)?;
            Ok(AdminDashboard::from_record(
                period_start,
                period_end,
                record,
            ))
        })
    }
}

impl fmt::Debug for DatabaseAdminDashboardReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminDashboardReader(<redacted>)")
    }
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

fn map_repository_error(_: AdminDashboardRepositoryError) -> AdminDashboardReadError {
    AdminDashboardReadError::Internal
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use af_db::{AdminDashboardRepository, DatabaseOptions};

    use super::*;

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
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            af_db::MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repository =
            AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2)).unwrap();
        let reader = DatabaseAdminDashboardReader::with_clock(
            repository,
            Arc::new(|| Ok(ADMIN_DASHBOARD_PERIOD_SECONDS + 7)),
        );
        let dashboard = reader.read(AdminDashboardAccess::Admin).await.unwrap();
        assert_eq!(dashboard.period_start(), 7);
        assert_eq!(dashboard.period_end(), ADMIN_DASHBOARD_PERIOD_SECONDS + 7);
        assert_eq!(dashboard.request_count(), 0);
        assert_eq!(dashboard.outcome_request_count(), 0);
        assert_eq!(dashboard.successful_request_count(), 0);
        assert_eq!(dashboard.failed_request_count(), 0);
        assert_eq!(dashboard.other_success_count(), 0);
        assert!(dashboard.failures().is_empty());
        assert!(dashboard.channel_flows().is_empty());
        pool.close().await.unwrap();
    }
}
