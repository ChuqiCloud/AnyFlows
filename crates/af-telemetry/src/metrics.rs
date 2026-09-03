use std::{fmt, sync::Arc, time::Duration};

use ::metrics::Unit;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle as ExporterHandle};

use crate::{MetricLabelError, TelemetryError};

mod guard;

use guard::{GuardedRecorder, LabelRegistries};
#[cfg(test)]
pub(crate) use guard::{
    MAX_CHANNEL_LABELS, MAX_GROUP_LABELS, MAX_MODEL_LABELS, MAX_SERIES_PER_FAMILY,
};

/// Prometheus 抓取端点；真实 HTTP 路由由 `af-http` 挂载。
pub const PROMETHEUS_PATH: &str = "/metrics";
/// Prometheus 文本格式响应类型。
pub const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// 请求计数与成功率指标。
pub const REQUESTS_TOTAL: &str = "requests_total";
/// 请求总耗时直方图，单位为秒。
pub const REQUEST_DURATION_SECONDS: &str = "request_duration_seconds";
/// 流式首字耗时直方图，单位为秒。
pub const FIRST_TOKEN_SECONDS: &str = "first_token_seconds";
/// 渠道健康状态；健康为 1，不健康为 0。
pub const CHANNEL_HEALTH: &str = "channel_health";
/// 分类后的上游错误计数。
pub const UPSTREAM_ERRORS_TOTAL: &str = "upstream_errors_total";
/// 已结算的整数 quota 消耗。
pub const QUOTA_CONSUMED_TOTAL: &str = "quota_consumed_total";
/// 各级并发槽位当前占用量。
pub const CONCURRENCY_SLOTS: &str = "concurrency_slots";
/// 批量计费落盘滞后，单位为秒。
pub const BILLING_FLUSH_LAG: &str = "billing_flush_lag";
/// 按闭合档案和结果累计的客户端仿真 Attempt。
pub const CLIENT_SIMULATION_ATTEMPTS_TOTAL: &str = "client_simulation_attempts_total";
/// 按闭合正文档案和补丁结果累计的请求数。
pub const CLIENT_SIMULATION_BODY_PATCHES_TOTAL: &str = "client_simulation_body_patches_total";
/// 尚未完成 ClickHouse 投递的事实数量。
pub const ANALYTICS_EXPORT_BACKLOG: &str = "analytics_export_backlog";
/// 有限请求速率策略成功准入的请求数。
pub const REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL: &str = "request_rate_limit_admissions_total";
/// 被请求速率策略拒绝的请求数。
pub const REQUEST_RATE_LIMIT_REJECTIONS_TOTAL: &str = "request_rate_limit_rejections_total";
/// 请求速率限制失败关闭的请求数。
pub const REQUEST_RATE_LIMIT_FAILURES_TOTAL: &str = "request_rate_limit_failures_total";
/// 单次请求速率限制检查的规则数量。
pub const REQUEST_RATE_LIMIT_RULES_PER_CHECK: &str = "request_rate_limit_rules_per_check";
/// 请求被限流后需要等待的时间，单位为秒。
pub const REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS: &str = "request_rate_limit_retry_after_seconds";
/// 支付 webhook 确认结果；不代表 Provider 投递或用户已读状态。
pub const PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL: &str = "payment_webhook_confirmations_total";
/// 企业审批超时扫描的固定结果分类。
pub const ORGANIZATION_APPROVAL_TIMEOUT_TOTAL: &str = "organization_approval_timeout_total";

const REQUESTS_DESCRIPTION: &str = "AnyFlows 请求总数。";
const REQUEST_DURATION_DESCRIPTION: &str = "AnyFlows 请求总耗时。";
const FIRST_TOKEN_DESCRIPTION: &str = "AnyFlows 流式首字耗时。";
const CHANNEL_HEALTH_DESCRIPTION: &str = "AnyFlows 渠道健康状态。";
const UPSTREAM_ERRORS_DESCRIPTION: &str = "AnyFlows 分类后的上游错误总数。";
const QUOTA_CONSUMED_DESCRIPTION: &str = "AnyFlows 已结算的整数 quota 消耗。";
const CONCURRENCY_SLOTS_DESCRIPTION: &str = "AnyFlows 各级并发槽位占用量。";
const BILLING_FLUSH_LAG_DESCRIPTION: &str = "AnyFlows 批量计费落盘滞后。";
const CLIENT_SIMULATION_ATTEMPTS_DESCRIPTION: &str = "AnyFlows 客户端仿真 Attempt 总数。";
const CLIENT_SIMULATION_BODY_PATCHES_DESCRIPTION: &str = "AnyFlows 客户端仿真正文补丁结果总数。";
const ANALYTICS_EXPORT_BACKLOG_DESCRIPTION: &str = "AnyFlows ClickHouse 事实投递积压数量。";
const REQUEST_RATE_LIMIT_ADMISSIONS_DESCRIPTION: &str = "AnyFlows 有限请求速率策略成功准入总数。";
const REQUEST_RATE_LIMIT_REJECTIONS_DESCRIPTION: &str = "AnyFlows 请求速率限制拒绝总数。";
const REQUEST_RATE_LIMIT_FAILURES_DESCRIPTION: &str = "AnyFlows 请求速率限制失败关闭总数。";
const REQUEST_RATE_LIMIT_RULES_DESCRIPTION: &str = "AnyFlows 单次请求速率限制检查的规则数量。";
const REQUEST_RATE_LIMIT_RETRY_AFTER_DESCRIPTION: &str = "AnyFlows 请求速率限制拒绝后的等待时间。";
const PAYMENT_WEBHOOK_CONFIRMATIONS_DESCRIPTION: &str =
    "AnyFlows 支付 webhook 确认结果总数；不代表通知投递成功。";
pub(crate) const ORGANIZATION_APPROVAL_TIMEOUT_DESCRIPTION: &str =
    "AnyFlows 企业审批超时扫描结果总数。";

const METRICS_CALLSITE: &str = module_path!();
const OTHER_LABEL: &str = "other";

const PAYMENT_CONFIRMATION_LABELS: &[&str] = &["outcome"];
const ORGANIZATION_APPROVAL_TIMEOUT_LABELS: &[&str] = &["outcome"];
// 固定 bucket 避免 exporter 回退为 summary，并覆盖网关的毫秒级到长流式请求。
const REQUEST_DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 900.0,
];
const FIRST_TOKEN_BUCKETS: &[f64] = &[
    0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0, 60.0,
];
const REQUEST_RATE_LIMIT_RULE_BUCKETS: &[f64] = &[1.0, 2.0, 4.0, 8.0];
const REQUEST_RATE_LIMIT_RETRY_AFTER_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 900.0, 3_600.0, 86_400.0,
    604_800.0,
];

/// Prometheus recorder 控制句柄；克隆值共享同一份指标快照。
#[derive(Clone)]
pub struct MetricsHandle {
    exporter: ExporterHandle,
    labels: Arc<LabelRegistries>,
}

impl fmt::Debug for MetricsHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MetricsHandle")
            .finish_non_exhaustive()
    }
}

impl MetricsHandle {
    /// 注册来自受信模型注册表的规范值；不得直接传入请求中的原始模型文本。
    pub fn register_model(&self, value: &str) -> Result<ModelLabel, MetricLabelError> {
        self.labels.register_model(value).map(ModelLabel)
    }

    /// 注册来自受信渠道注册表的规范值。
    pub fn register_channel(&self, value: &str) -> Result<ChannelLabel, MetricLabelError> {
        self.labels.register_channel(value).map(ChannelLabel)
    }

    /// 注册来自受信分组注册表的规范值。
    pub fn register_group(&self, value: &str) -> Result<GroupLabel, MetricLabelError> {
        self.labels.register_group(value).map(GroupLabel)
    }

    /// 记录请求计数和总耗时；状态与协议只接受稳定枚举。
    pub fn record_request(
        &self,
        protocol: MetricProtocol,
        model: &ModelLabel,
        status: MetricRequestStatus,
        duration: Duration,
    ) {
        ::metrics::counter!(
            REQUESTS_TOTAL,
            "protocol" => protocol.as_str(),
            "model" => model.0.clone(),
            "status" => status.as_str()
        )
        .increment(1);
        ::metrics::histogram!(
            REQUEST_DURATION_SECONDS,
            "protocol" => protocol.as_str(),
            "model" => model.0.clone(),
            "status" => status.as_str()
        )
        .record(duration.as_secs_f64());
    }

    /// 记录流式请求的首字耗时。
    pub fn record_first_token(
        &self,
        protocol: MetricProtocol,
        model: &ModelLabel,
        duration: Duration,
    ) {
        ::metrics::histogram!(
            FIRST_TOKEN_SECONDS,
            "protocol" => protocol.as_str(),
            "model" => model.0.clone()
        )
        .record(duration.as_secs_f64());
    }

    /// 设置渠道健康状态；健康为 1，不健康为 0。
    pub fn set_channel_health(&self, channel: &ChannelLabel, healthy: bool) {
        ::metrics::gauge!(CHANNEL_HEALTH, "channel" => channel.0.clone()).set(u8::from(healthy));
    }

    /// 记录稳定分类后的上游错误。
    pub fn record_upstream_error(
        &self,
        channel: &ChannelLabel,
        error_type: MetricUpstreamErrorType,
    ) {
        ::metrics::counter!(
            UPSTREAM_ERRORS_TOTAL,
            "channel" => channel.0.clone(),
            "type" => error_type.as_str()
        )
        .increment(1);
    }

    /// 累加已完成结算的整数 quota。
    pub fn add_quota_consumed(&self, group: &GroupLabel, quota: u64) {
        ::metrics::counter!(QUOTA_CONSUMED_TOTAL, "group" => group.0.clone()).increment(quota);
    }

    /// 设置指定层级的并发槽位占用量。
    pub fn set_concurrency_slots(&self, level: MetricConcurrencyLevel, slots: u32) {
        ::metrics::gauge!(CONCURRENCY_SLOTS, "level" => level.as_str()).set(slots);
    }

    /// 设置批量计费落盘滞后。
    pub fn set_billing_flush_lag(&self, lag: Duration) {
        ::metrics::gauge!(BILLING_FLUSH_LAG).set(lag.as_secs_f64());
    }

    /// 渲染 Prometheus 文本格式快照，供 `af-http` 的 GET `/metrics` 响应使用。
    #[must_use]
    pub fn render(&self) -> String {
        self.exporter.render()
    }

    /// 抽干待处理直方图样本并发布指标描述，避免维护间隔内样本无界增长。
    pub fn run_upkeep(&self) {
        self.exporter.run_upkeep();
    }
}

/// 记录一次配置了闭合档案的客户端仿真 Attempt。
pub fn record_client_simulation_attempt(
    profile: MetricClientSimulationProfile,
    result: MetricClientSimulationResult,
) {
    ::metrics::counter!(
        CLIENT_SIMULATION_ATTEMPTS_TOTAL,
        "profile" => profile.as_str(),
        "result" => result.as_str()
    )
    .increment(1);
}

/// 记录一次请求级正文补丁结果；不携带日期、提示词或正文内容。
pub fn record_client_simulation_body_patch(
    profile: MetricClientSimulationBodyProfile,
    result: MetricClientSimulationBodyResult,
) {
    ::metrics::counter!(
        CLIENT_SIMULATION_BODY_PATCHES_TOTAL,
        "profile" => profile.as_str(),
        "result" => result.as_str()
    )
    .increment(1);
}

/// 设置尚未完成 ClickHouse 投递的事实数量。
pub fn set_analytics_export_backlog(backlog: u64) {
    ::metrics::gauge!(ANALYTICS_EXPORT_BACKLOG).set(backlog as f64);
}

/// 记录一次有限请求速率策略检查及其原子规则数量。
pub fn record_request_rate_limit_check(rule_count: u32) {
    ::metrics::histogram!(REQUEST_RATE_LIMIT_RULES_PER_CHECK).record(rule_count);
}

/// 记录一次有限请求速率策略成功准入。
pub fn record_request_rate_limit_admission() {
    ::metrics::counter!(REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL).increment(1);
}

/// 记录一次限流拒绝及 Redis 服务端窗口剩余时间。
pub fn record_request_rate_limit_rejection(
    subject: MetricRequestRateLimitSubject,
    retry_after: Duration,
) {
    ::metrics::counter!(
        REQUEST_RATE_LIMIT_REJECTIONS_TOTAL,
        "subject" => subject.as_str()
    )
    .increment(1);
    ::metrics::histogram!(
        REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS,
        "subject" => subject.as_str()
    )
    .record(retry_after);
}

/// 记录一次限流基础设施或协议异常导致的失败关闭。
pub fn record_request_rate_limit_failure(failure: MetricRequestRateLimitFailure) {
    ::metrics::counter!(
        REQUEST_RATE_LIMIT_FAILURES_TOTAL,
        "reason" => failure.as_str()
    )
    .increment(1);
}

/// 记录一次支付 webhook 确认结果；指标不改变订单或通知事实状态。
pub fn record_payment_confirmation(outcome: MetricPaymentConfirmationOutcome) {
    ::metrics::counter!(
        PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL,
        "outcome" => outcome.as_str()
    )
    .increment(1);
}

/// 记录企业审批超时扫描的有限结果；不携带企业、申请或资源标识。
pub fn record_organization_approval_timeout(
    outcome: MetricOrganizationApprovalTimeoutOutcome,
    count: u64,
) {
    if count == 0 {
        return;
    }
    ::metrics::counter!(
        ORGANIZATION_APPROVAL_TIMEOUT_TOTAL,
        "outcome" => outcome.as_str()
    )
    .increment(count);
}

macro_rules! registered_label {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Eq, Hash, PartialEq)]
        pub struct $name(String);

        impl $name {
            /// 返回所有未注册或超出容量值共享的有限兜底桶。
            #[must_use]
            pub fn other() -> Self {
                Self(OTHER_LABEL.to_owned())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::other()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    };
}

registered_label!(
    /// 经过受控注册且有容量上限的模型指标标签。
    ModelLabel
);
registered_label!(
    /// 经过受控注册且有容量上限的渠道指标标签。
    ChannelLabel
);
registered_label!(
    /// 经过受控注册且有容量上限的分组指标标签。
    GroupLabel
);

/// 指标使用的有限客户端仿真档案，避免 telemetry 反向依赖业务领域层。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricClientSimulationProfile {
    AnthropicCliHeadersV1,
}

/// 指标使用的有限客户端仿真正文档案。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricClientSimulationBodyProfile {
    AnthropicCliSystemDateV1,
}

impl MetricClientSimulationBodyProfile {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AnthropicCliSystemDateV1 => "anthropic_cli_system_date_v1",
        }
    }
}

/// 指标使用的正文补丁闭合结果。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricClientSimulationBodyResult {
    Applied,
    Rejected,
}

impl MetricClientSimulationBodyResult {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Rejected => "rejected",
        }
    }
}

impl MetricClientSimulationProfile {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AnthropicCliHeadersV1 => "anthropic_cli_headers_v1",
        }
    }
}

/// 指标使用的有限客户端仿真结果。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricClientSimulationResult {
    NotApplied,
    Applied,
    Failed,
}

/// 限流指标使用的固定业务主体类型，不携带主体标识。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricRequestRateLimitSubject {
    User,
    Group,
    Token,
}

impl MetricRequestRateLimitSubject {
    const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
            Self::Token => "token",
        }
    }
}

/// 限流失败关闭的有限分类，不携带底层错误文本。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricRequestRateLimitFailure {
    InvalidRule,
    StoreMissing,
    StoreUnavailable,
    StoreProtocol,
    StoreOther,
    InvalidRetryAfter,
}

/// 支付 webhook 确认的稳定结果分类；不包含订单、用户或 Provider 标识。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricPaymentConfirmationOutcome {
    Applied,
    Acknowledged,
    Existing,
    Rejected,
    IgnoredNonTerminal,
    NotFound,
    VerificationRejected,
    Conflict,
    BindingConflict,
    OutcomeUnknown,
    Unavailable,
    Invariant,
    InvalidRequest,
}

/// 企业审批超时扫描使用的固定结果分类，避免把申请标识写入指标标签。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricOrganizationApprovalTimeoutOutcome {
    LeaseHeld,
    ScanFailed,
    Applied,
    Existing,
    Skipped,
    Failed,
    OutcomeUnknownReplay,
    Truncated,
}

impl MetricOrganizationApprovalTimeoutOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LeaseHeld => "lease_held",
            Self::ScanFailed => "scan_failed",
            Self::Applied => "applied",
            Self::Existing => "existing",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::OutcomeUnknownReplay => "outcome_unknown_replay",
            Self::Truncated => "truncated",
        }
    }
}

impl MetricPaymentConfirmationOutcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Acknowledged => "acknowledged",
            Self::Existing => "existing",
            Self::Rejected => "rejected",
            Self::IgnoredNonTerminal => "ignored_non_terminal",
            Self::NotFound => "not_found",
            Self::VerificationRejected => "verification_rejected",
            Self::Conflict => "conflict",
            Self::BindingConflict => "binding_conflict",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unavailable => "unavailable",
            Self::Invariant => "invariant",
            Self::InvalidRequest => "invalid_request",
        }
    }
}

impl MetricRequestRateLimitFailure {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRule => "invalid_rule",
            Self::StoreMissing => "store_missing",
            Self::StoreUnavailable => "store_unavailable",
            Self::StoreProtocol => "store_protocol",
            Self::StoreOther => "store_other",
            Self::InvalidRetryAfter => "invalid_retry_after",
        }
    }
}

impl MetricClientSimulationResult {
    const fn as_str(self) -> &'static str {
        match self {
            Self::NotApplied => "not_applied",
            Self::Applied => "applied",
            Self::Failed => "failed",
        }
    }
}

/// 指标使用的有限协议分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricProtocol {
    OpenAi,
    Anthropic,
    Gemini,
    Bedrock,
    Other,
}

impl MetricProtocol {
    const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Bedrock => "bedrock",
            Self::Other => OTHER_LABEL,
        }
    }
}

/// 指标使用的有限请求终态分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricRequestStatus {
    Success,
    ClientError,
    UpstreamError,
    InternalError,
    Cancelled,
}

impl MetricRequestStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::ClientError => "client_error",
            Self::UpstreamError => "upstream_error",
            Self::InternalError => "internal_error",
            Self::Cancelled => "cancelled",
        }
    }
}

/// 指标使用的有限上游错误分类；原始错误文本不得进入标签。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricUpstreamErrorType {
    Timeout,
    Authentication,
    RateLimited,
    InvalidRequest,
    Unavailable,
    Protocol,
    Other,
}

impl MetricUpstreamErrorType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Authentication => "authentication",
            Self::RateLimited => "rate_limited",
            Self::InvalidRequest => "invalid_request",
            Self::Unavailable => "unavailable",
            Self::Protocol => "protocol",
            Self::Other => OTHER_LABEL,
        }
    }
}

/// 指标使用的有限并发控制层级。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricConcurrencyLevel {
    Account,
    User,
    Token,
}

impl MetricConcurrencyLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::User => "user",
            Self::Token => "token",
        }
    }
}

/// 安装进程级 Prometheus recorder，不启动独立 HTTP listener 或 push gateway。
///
/// 指标不受 tracing 日志级别控制。调用方必须在业务任务启动前完成初始化，
/// 并由 `af-http` 在受保护的运维路由挂载 [`PROMETHEUS_PATH`]。
pub fn init_metrics() -> Result<MetricsHandle, TelemetryError> {
    let (recorder, handle) = build_metrics_recorder();
    ::metrics::set_global_recorder(recorder)
        .map_err(|_| TelemetryError::MetricsInitializationConflict)?;
    describe_metric_families();
    Ok(handle)
}

fn prometheus_builder() -> PrometheusBuilder {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full(REQUEST_DURATION_SECONDS.to_owned()),
            REQUEST_DURATION_BUCKETS,
        )
        .expect("请求耗时 bucket 常量不得为空")
        .set_buckets_for_metric(
            Matcher::Full(FIRST_TOKEN_SECONDS.to_owned()),
            FIRST_TOKEN_BUCKETS,
        )
        .expect("首字耗时 bucket 常量不得为空")
        .set_buckets_for_metric(
            Matcher::Full(REQUEST_RATE_LIMIT_RULES_PER_CHECK.to_owned()),
            REQUEST_RATE_LIMIT_RULE_BUCKETS,
        )
        .expect("限流规则数量 bucket 常量不得为空")
        .set_buckets_for_metric(
            Matcher::Full(REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS.to_owned()),
            REQUEST_RATE_LIMIT_RETRY_AFTER_BUCKETS,
        )
        .expect("限流等待时间 bucket 常量不得为空")
}

pub(crate) fn build_metrics_recorder() -> (GuardedRecorder, MetricsHandle) {
    let exporter = prometheus_builder().build_recorder();
    let labels = Arc::new(LabelRegistries::default());
    let handle = MetricsHandle {
        exporter: exporter.handle(),
        labels: Arc::clone(&labels),
    };
    (GuardedRecorder::new(exporter, labels), handle)
}

pub(crate) fn describe_metric_families() {
    ::metrics::describe_counter!(REQUESTS_TOTAL, Unit::Count, REQUESTS_DESCRIPTION);
    ::metrics::describe_histogram!(
        REQUEST_DURATION_SECONDS,
        Unit::Seconds,
        REQUEST_DURATION_DESCRIPTION
    );
    ::metrics::describe_histogram!(FIRST_TOKEN_SECONDS, Unit::Seconds, FIRST_TOKEN_DESCRIPTION);
    ::metrics::describe_gauge!(CHANNEL_HEALTH, CHANNEL_HEALTH_DESCRIPTION);
    ::metrics::describe_counter!(
        UPSTREAM_ERRORS_TOTAL,
        Unit::Count,
        UPSTREAM_ERRORS_DESCRIPTION
    );
    ::metrics::describe_counter!(
        QUOTA_CONSUMED_TOTAL,
        Unit::Count,
        QUOTA_CONSUMED_DESCRIPTION
    );
    ::metrics::describe_gauge!(
        CONCURRENCY_SLOTS,
        Unit::Count,
        CONCURRENCY_SLOTS_DESCRIPTION
    );
    ::metrics::describe_gauge!(
        BILLING_FLUSH_LAG,
        Unit::Seconds,
        BILLING_FLUSH_LAG_DESCRIPTION
    );
    ::metrics::describe_counter!(
        CLIENT_SIMULATION_ATTEMPTS_TOTAL,
        Unit::Count,
        CLIENT_SIMULATION_ATTEMPTS_DESCRIPTION
    );
    ::metrics::describe_counter!(
        CLIENT_SIMULATION_BODY_PATCHES_TOTAL,
        Unit::Count,
        CLIENT_SIMULATION_BODY_PATCHES_DESCRIPTION
    );
    ::metrics::describe_gauge!(
        ANALYTICS_EXPORT_BACKLOG,
        Unit::Count,
        ANALYTICS_EXPORT_BACKLOG_DESCRIPTION
    );
    ::metrics::describe_counter!(
        REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL,
        Unit::Count,
        REQUEST_RATE_LIMIT_ADMISSIONS_DESCRIPTION
    );
    ::metrics::describe_counter!(
        REQUEST_RATE_LIMIT_REJECTIONS_TOTAL,
        Unit::Count,
        REQUEST_RATE_LIMIT_REJECTIONS_DESCRIPTION
    );
    ::metrics::describe_counter!(
        REQUEST_RATE_LIMIT_FAILURES_TOTAL,
        Unit::Count,
        REQUEST_RATE_LIMIT_FAILURES_DESCRIPTION
    );
    ::metrics::describe_histogram!(
        REQUEST_RATE_LIMIT_RULES_PER_CHECK,
        Unit::Count,
        REQUEST_RATE_LIMIT_RULES_DESCRIPTION
    );
    ::metrics::describe_histogram!(
        REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS,
        Unit::Seconds,
        REQUEST_RATE_LIMIT_RETRY_AFTER_DESCRIPTION
    );
    ::metrics::describe_counter!(
        PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL,
        Unit::Count,
        PAYMENT_WEBHOOK_CONFIRMATIONS_DESCRIPTION
    );
    ::metrics::describe_counter!(
        ORGANIZATION_APPROVAL_TIMEOUT_TOTAL,
        Unit::Count,
        ORGANIZATION_APPROVAL_TIMEOUT_DESCRIPTION
    );
}
