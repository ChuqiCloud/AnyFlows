//! AnyFlows 进程启动编排、后台任务监督与优雅关闭。

mod adaptor_credential;
mod admin_dashboard_storage;
mod analytics_export;
mod audio_duration;
mod auto_ban_feedback;
mod balance_alert_runtime;
mod billing_audio;
mod billing_chat;
mod billing_compact;
mod billing_embedding;
mod billing_image;
mod billing_rerank;
mod billing_runtime;
mod billing_speech;
mod bootstrap;
mod channel_probe;
#[cfg(test)]
mod channel_probe_tests;
mod concurrency_runtime;
mod credential_feedback;
mod credential_order;
mod credential_usage;
mod debug_trace_runtime;
mod frontend;
mod group_pricing_refresh;
mod health_runtime;
mod model_catalog;
mod model_discovery;
mod model_price_source;
mod oauth_connection;
mod payment_runtime;
mod readiness;
mod request_outcome_runtime;
mod request_rate_limit;
mod responses_websocket_runtime;
mod runtime_extensions;
mod runtime_frontend;
mod scheduled_chat;
mod scheduler_invalidation;
mod shutdown;
mod smtp_delivery;
mod sticky_session;
mod stripe_payment_intent;
mod stripe_refund;
mod supervisor;
#[cfg(test)]
mod test_database;
mod token_request_admission;
mod turnstile;
mod utc_time;
mod video_task_service;

pub use admin_dashboard_storage::{
    ClickHouseAdminDashboardConfig, ClickHouseAdminDashboardConfigError,
    ClickHouseAdminDashboardStorage, DEFAULT_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES,
    DEFAULT_CLICKHOUSE_DASHBOARD_TIMEOUT, DatabaseAdminDashboardChannelStorage,
    MAX_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES, MAX_CLICKHOUSE_DASHBOARD_TIMEOUT,
};
pub use billing_audio::{
    AudioRoutePlanFuture, AudioRoutePlanner, BillingAudioService, PlannedAudioExecution,
    PlannedAudioExecutionFuture,
};
pub use billing_chat::{
    BillingChatService, ChatRoutePlanFuture, ChatRoutePlanner, PlannedChatExecution,
    PlannedChatExecutionFuture,
};
pub use billing_compact::{
    BillingResponsesCompactService, PlannedResponsesCompactExecution,
    PlannedResponsesCompactExecutionFuture, ResponsesCompactRoutePlanFuture,
    ResponsesCompactRoutePlanner,
};
pub use billing_embedding::{
    BillingEmbeddingService, EmbeddingRoutePlanFuture, EmbeddingRoutePlanner,
    PlannedEmbeddingExecution, PlannedEmbeddingExecutionFuture,
};
pub use billing_image::{
    BillingImageService, ImageRoutePlanFuture, ImageRoutePlanner, PlannedImageExecution,
    PlannedImageExecutionFuture,
};
pub use billing_rerank::{
    BillingRerankService, PlannedRerankExecution, PlannedRerankExecutionFuture,
    RerankRoutePlanFuture, RerankRoutePlanner,
};
pub use billing_runtime::{BillingFlushReport, BillingRuntime, BillingRuntimeError};
pub use billing_speech::{
    BillingSpeechService, PlannedSpeechExecution, PlannedSpeechExecutionFuture,
    SpeechRoutePlanFuture, SpeechRoutePlanner,
};
pub use bootstrap::{
    AuthenticationReady, BillingReady, Bootstrap, BootstrapError, Configured, DatabaseReady,
    ListenerReady, RelayReady, RouterReady, ShutdownReport, Supervised, TelemetryReady, run,
};
pub use oauth_connection::OAuthRuntimeConfigError;
pub use request_outcome_runtime::{RequestOutcomeContext, RequestOutcomeRuntime, RoutedExecution};
pub use runtime_extensions::{
    BackgroundTaskRegistrar, RelayExtensionInitializer, RuntimeExtensionContext, RuntimeExtensions,
    RuntimeHttpContext, RuntimeHttpExtensionFactory,
};
pub use scheduled_chat::{
    BoundVideoTaskSubmission, VideoTaskBinding, VideoTaskPollFuture, VideoTaskRuntime,
    VideoTaskSubmissionFuture, VideoTaskSubmissionRuntimeError,
};
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
