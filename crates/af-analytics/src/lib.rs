//! 用量记录、聚合与分析投影。

use af_domain::{ChannelId, Protocol};

mod export;
mod reader;
mod service_levels;
mod storage;

pub use service_levels::{
    ServiceLevelDimension, ServiceLevelPoint, ServiceLevelQuery, ServiceLevelReadFuture,
    ServiceLevelReport, ServiceLevelRow, ServiceLevelStorage, ServiceLevelStorageFuture,
};

pub use export::{
    AnalyticsExportControl, AnalyticsExportControlError, AnalyticsExportQueueSnapshot,
    AnalyticsExportReplayFuture, AnalyticsExportStatusFuture,
};

pub use reader::{
    AdminDashboardReadError, AdminDashboardReadFuture, AdminDashboardReader,
    StorageAdminDashboardReader,
};
pub use storage::{
    AdminDashboardAnalyticsSnapshot, AdminDashboardAnalyticsStorage,
    AdminDashboardAnalyticsStorageFuture, AdminDashboardChannelSnapshot,
    AdminDashboardChannelStorage, AdminDashboardChannelStorageFuture,
    AdminDashboardOutcomeSnapshot, AdminDashboardStorage, AdminDashboardStorageError,
    AdminDashboardStorageFuture, AdminDashboardStorageSnapshot, AdminDashboardUsageSnapshot,
    SplitAdminDashboardStorage,
};

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
    flow_request_count: i64,
    flow_quota_consumed: i64,
    flow_paths: Vec<AdminDashboardFlowPath>,
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

/// 管理看板窗口内一个失败分类的请求数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardFailure {
    kind: AdminDashboardFailureKind,
    request_count: i64,
}

impl AdminDashboardFailure {
    /// 构造分析存储返回的闭合失败分类。
    #[must_use]
    pub const fn new(kind: AdminDashboardFailureKind, request_count: i64) -> Self {
        Self {
            kind,
            request_count,
        }
    }

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
    /// 构造一个协议到渠道的成功流向聚合。
    #[must_use]
    pub fn new(
        protocol: Protocol,
        channel_id: ChannelId,
        channel_name: String,
        request_count: i64,
    ) -> Self {
        Self {
            protocol,
            channel_id,
            channel_name,
            request_count,
        }
    }

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

/// 管理看板窗口内一条用户、分组、最终渠道和模型的已结算路径。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardFlowPath {
    user_id: i64,
    group_id: i64,
    group_name: String,
    channel_id: ChannelId,
    channel_name: String,
    model: String,
    request_count: i64,
    quota_consumed: i64,
}

impl AdminDashboardFlowPath {
    /// 构造不携带用户名、令牌或请求正文的低敏聚合路径。
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "参数与闭合四层流向字段一一对应")]
    pub fn new(
        user_id: i64,
        group_id: i64,
        group_name: String,
        channel_id: ChannelId,
        channel_name: String,
        model: String,
        request_count: i64,
        quota_consumed: i64,
    ) -> Self {
        Self {
            user_id,
            group_id,
            group_name,
            channel_id,
            channel_name,
            model,
            request_count,
            quota_consumed,
        }
    }

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

/// 管理看板单个连续一小时分桶。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardHourlyPoint {
    period_start: i64,
    period_end: i64,
    request_count: i64,
    quota_consumed: i64,
}

impl AdminDashboardHourlyPoint {
    /// 构造一个连续小时分桶。
    #[must_use]
    pub const fn new(
        period_start: i64,
        period_end: i64,
        request_count: i64,
        quota_consumed: i64,
    ) -> Self {
        Self {
            period_start,
            period_end,
            request_count,
            quota_consumed,
        }
    }

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
    /// 构造真实耗时样本摘要。
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "参数与性能快照字段一一对应")]
    pub const fn new(
        first_token_sample_count: i64,
        average_first_token_ms: Option<i64>,
        slow_first_token_count: i64,
        slow_first_token_threshold_ms: i64,
        duration_sample_count: i64,
        average_duration_ms: Option<i64>,
        slow_request_count: i64,
        slow_request_threshold_ms: i64,
    ) -> Self {
        Self {
            first_token_sample_count,
            average_first_token_ms,
            slow_first_token_count,
            slow_first_token_threshold_ms,
            duration_sample_count,
            average_duration_ms,
            slow_request_count,
            slow_request_threshold_ms,
        }
    }

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

    /// 返回可连接为四层路径的已结算请求总数。
    #[must_use]
    pub const fn flow_request_count(&self) -> i64 {
        self.flow_request_count
    }

    /// 返回可连接为四层路径的已结算额度总量。
    #[must_use]
    pub const fn flow_quota_consumed(&self) -> i64 {
        self.flow_quota_consumed
    }

    /// 返回按请求量排序且数量受限的四层聚合路径。
    #[must_use]
    pub fn flow_paths(&self) -> &[AdminDashboardFlowPath] {
        &self.flow_paths
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
}
