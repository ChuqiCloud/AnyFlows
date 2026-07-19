//! 日志、指标与分布式追踪初始化。

mod error;
mod fields;
mod logging;
mod metrics;
mod request;

pub use af_config::{LogLevel, TelemetrySettings};
pub use error::{MetricLabelError, RequestIdError, TelemetryError};
pub use logging::{TracingHandle, init_tracing};
pub use metrics::{
    BILLING_FLUSH_LAG, CHANNEL_HEALTH, CONCURRENCY_SLOTS, ChannelLabel, FIRST_TOKEN_SECONDS,
    GroupLabel, MetricConcurrencyLevel, MetricProtocol, MetricRequestStatus,
    MetricUpstreamErrorType, MetricsHandle, ModelLabel, PROMETHEUS_CONTENT_TYPE, PROMETHEUS_PATH,
    QUOTA_CONSUMED_TOTAL, REQUEST_DURATION_SECONDS, REQUESTS_TOTAL, UPSTREAM_ERRORS_TOTAL,
    init_metrics,
};
pub use request::{RequestId, request_span};

#[cfg(test)]
mod metrics_tests;
#[cfg(test)]
mod tests;
