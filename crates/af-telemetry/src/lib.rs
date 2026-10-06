//! 日志、指标与分布式追踪初始化。

mod error;
mod fields;
mod logging;
mod metrics;
mod panic_hook;
mod request;

pub use af_config::{LogLevel, TelemetrySettings};
pub use error::{MetricLabelError, RequestIdError, TelemetryError};
pub use logging::{TracingHandle, init_tracing};
pub use metrics::{
    ANALYTICS_EXPORT_BACKLOG, BILLING_FLUSH_LAG, CHANNEL_HEALTH, CLIENT_SIMULATION_ATTEMPTS_TOTAL,
    CLIENT_SIMULATION_BODY_PATCHES_TOTAL, CONCURRENCY_SLOTS, ChannelLabel, FIRST_TOKEN_SECONDS,
    GroupLabel, MetricClientSimulationBodyProfile, MetricClientSimulationBodyResult,
    MetricClientSimulationProfile, MetricClientSimulationResult, MetricConcurrencyLevel,
    MetricOrganizationApprovalTimeoutOutcome, MetricPaymentConfirmationOutcome, MetricProtocol,
    MetricRequestRateLimitFailure, MetricRequestRateLimitSubject, MetricRequestStatus,
    MetricUpstreamErrorType, MetricsHandle, ModelLabel, ORGANIZATION_APPROVAL_TIMEOUT_TOTAL,
    PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL, PROMETHEUS_CONTENT_TYPE, PROMETHEUS_PATH,
    QUOTA_CONSUMED_TOTAL, REQUEST_DURATION_SECONDS, REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL,
    REQUEST_RATE_LIMIT_FAILURES_TOTAL, REQUEST_RATE_LIMIT_REJECTIONS_TOTAL,
    REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS, REQUEST_RATE_LIMIT_RULES_PER_CHECK, REQUESTS_TOTAL,
    UPSTREAM_ERRORS_TOTAL, init_metrics, record_client_simulation_attempt,
    record_client_simulation_body_patch, record_organization_approval_timeout,
    record_payment_confirmation, record_request_rate_limit_admission,
    record_request_rate_limit_check, record_request_rate_limit_failure,
    record_request_rate_limit_rejection, set_analytics_export_backlog,
};
pub use panic_hook::install_redacted_panic_hook;
pub use request::{RequestId, request_span};

#[cfg(test)]
mod metrics_tests;
#[cfg(test)]
mod tests;
