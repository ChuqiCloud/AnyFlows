//! AnyFlows 进程启动编排、后台任务监督与优雅关闭。

mod adaptor_credential;
mod auto_ban_feedback;
mod balance_alert_runtime;
mod billing_chat;
mod billing_runtime;
mod bootstrap;
mod channel_probe;
#[cfg(test)]
mod channel_probe_tests;
mod concurrency_runtime;
mod credential_feedback;
mod credential_order;
mod debug_trace_runtime;
mod frontend;
mod group_pricing_refresh;
mod health_runtime;
mod model_catalog;
mod model_discovery;
mod model_price_source;
mod oauth_connection;
mod readiness;
mod responses_websocket_runtime;
mod scheduled_chat;
mod scheduler_invalidation;
mod shutdown;
mod smtp_delivery;
mod sticky_session;
mod stripe_payment_intent;
mod supervisor;
mod utc_time;

pub use billing_chat::{
    BillingChatService, ChatRoutePlanFuture, ChatRoutePlanner, PlannedChatExecution,
    PlannedChatExecutionFuture,
};
pub use billing_runtime::{BillingFlushReport, BillingRuntime, BillingRuntimeError};
pub use bootstrap::{
    AuthenticationReady, BillingReady, Bootstrap, BootstrapError, Configured, DatabaseReady,
    ListenerReady, RelayReady, RouterReady, ShutdownReport, Supervised, TelemetryReady, run,
};
pub use oauth_connection::OAuthRuntimeConfigError;
pub use scheduler_invalidation::SchedulerInvalidationRuntimeError;
pub use shutdown::{ShutdownController, system_shutdown_signal};
pub use smtp_delivery::SmtpEmailDelivery;
pub use supervisor::{BackgroundTaskSupervisor, SupervisorError, SupervisorShutdown};

/// 安装服务进程必须独占的脱敏 panic hook。
///
/// 嵌入式宿主可选择自己的等价安全 hook；直接使用后台任务监督器前必须先完成该策略。
pub fn install_process_safety_hooks() {
    af_telemetry::install_redacted_panic_hook();
}
