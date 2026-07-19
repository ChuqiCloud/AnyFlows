//! 日志、指标与分布式追踪初始化。

mod error;
mod fields;
mod logging;
mod request;

pub use af_config::{LogLevel, TelemetrySettings};
pub use error::{RequestIdError, TelemetryError};
pub use logging::{TracingHandle, init_tracing};
pub use request::{RequestId, request_span};

#[cfg(test)]
mod tests;
