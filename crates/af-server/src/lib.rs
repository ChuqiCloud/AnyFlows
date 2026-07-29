//! AnyFlows 进程启动编排、后台任务监督与优雅关闭。

mod billing_chat;
mod billing_runtime;
mod bootstrap;
mod channel_probe;
#[cfg(test)]
mod channel_probe_tests;
mod credential_feedback;
mod credential_order;
mod frontend;
mod model_catalog;
mod readiness;
mod scheduled_chat;
mod shutdown;
mod supervisor;
mod utc_time;

pub use billing_chat::{
    BillingChatService, ChatRoutePlanner, PlannedChatExecution, PlannedChatExecutionFuture,
};
pub use billing_runtime::{BillingFlushReport, BillingRuntime, BillingRuntimeError};
pub use bootstrap::{
    AuthenticationReady, BillingReady, Bootstrap, BootstrapError, Configured, DatabaseReady,
    ListenerReady, RelayReady, RouterReady, ShutdownReport, Supervised, TelemetryReady, run,
};
pub use shutdown::{ShutdownController, system_shutdown_signal};
pub use supervisor::{BackgroundTaskSupervisor, SupervisorError, SupervisorShutdown};

/// 安装服务进程必须独占的脱敏 panic hook。
///
/// 嵌入式宿主可选择自己的等价安全 hook；直接使用后台任务监督器前必须先完成该策略。
pub fn install_process_safety_hooks() {
    af_telemetry::install_redacted_panic_hook();
}
