use std::{future::Future, sync::Arc, time::Duration};

use af_account::{
    CredentialDecryptionError, CredentialDecryptor, CredentialEncryptionError, CredentialEncryptor,
    DiagnosticSnapshotCipher, OAuthConnectionCoordinator, OAuthLoginClient, OAuthProviderProfile,
    OAuthRefreshSupervisor, PlainSystemSecret, SystemSecretCipher, SystemSecretError,
    SystemSecretKind,
};
use af_admin::{
    AdminBalanceAlertSettingsService, AdminChannelReader, AdminChannelWriter,
    AdminCredentialProxyService, AdminCustomOAuth2ProviderService, AdminEmailSettingsService,
    AdminGroupReader, AdminGroupWriter, AdminModelPriceService, AdminModelProviderCatalogService,
    AdminModelReader, AdminModelSyncService, AdminModelWriter, AdminNetworkSettingsError,
    AdminRefundService, AdminRouteReader, AdminRouteWriter, AdminTokenReader, AdminTokenWriter,
    AdminUsageLogReader, AdminUserReader, AdminUserWriter, AdminWalletService, BalanceAlertTask,
    DatabaseAdminBalanceAlertSettingsService, DatabaseAdminChannelReader,
    DatabaseAdminChannelWriter, DatabaseAdminCredentialProxyService,
    DatabaseAdminCustomOAuth2ProviderService, DatabaseAdminEmailSettingsService,
    DatabaseAdminGroupReader, DatabaseAdminGroupWriter, DatabaseAdminModelPriceService,
    DatabaseAdminModelProviderCatalogService, DatabaseAdminModelReader,
    DatabaseAdminModelSyncService, DatabaseAdminModelWriter, DatabaseAdminNetworkSettingsService,
    DatabaseAdminPaymentSettingsService, DatabaseAdminRefundService, DatabaseAdminRouteReader,
    DatabaseAdminRouteWriter, DatabaseAdminTokenReader, DatabaseAdminTokenWriter,
    DatabaseAdminUsageLogReader, DatabaseAdminUserReader, DatabaseAdminUserWriter,
    DatabaseAdminWalletService, DatabaseAnnouncementService, DatabaseInitialSetup,
    DatabaseOAuthLoginService, DatabasePasskeyAuthenticationService, DatabasePasswordResetService,
    DatabasePlaygroundConversationService, DatabasePlaygroundShareService,
    DatabaseRedemptionService, DatabaseRegistrationService, DatabaseSessionAuthenticator,
    DatabaseSiteSettingsService, DatabaseSubscriptionService, DatabaseTokenAuthenticator,
    DatabaseUserInvitationService, DatabaseUserNotificationService, DatabaseUserProfileService,
    DatabaseUserTokenService, DatabaseUserWalletService, EmailBindingService, EmailDelivery,
    InitialSetup, ModelCatalogReader, NetworkSettingsRuntimeApplier, NetworkSettingsRuntimeError,
    OAuthLoginService, PasskeyAuthenticationService, PasskeyAuthenticationServiceConfigError,
    PasswordResetService, PasswordResetServiceConfigError, PlaygroundConversationService,
    PlaygroundShareService, RedemptionService, RegistrationService, RegistrationServiceConfigError,
    RuntimeRefreshingAdminGroupWriter, RuntimeRefreshingInitialSetup, SessionAuthenticator,
    SessionAuthenticatorConfigError, SiteSettingsService, SubscriptionService, TokenAuthenticator,
    UserInvitationService, UserProfileService, UserTokenService, UserTopupService,
    UserWalletService,
};
use af_analytics::{
    AdminDashboardChannelStorage, AdminDashboardReader, SplitAdminDashboardStorage,
    StorageAdminDashboardReader,
};
use af_billing::{
    BillingPrechargePort, BillingSettlementPort, CachedRequestPricingSnapshotSource,
    DatabaseUsageRecordSink, GroupPricingCache, GroupPricingCacheError, ModelPriceCache,
    ModelPriceCacheError, RefundSignalConsumer, RefundSignalConsumerError, RefundSignalPort,
    RefundSignalQueue, RefundSignalQueueError, RefundSignalSink, TaskBillingReleaseSink,
    TaskBillingReservePort, TaskBillingSettlementPort, UsageRecordConsumer,
    UsageRecordConsumerError, UsageRecordPort, UsageRecordQueue, UsageRecordQueueError,
    UsageRecordSink,
};
use af_cache::{
    CacheError, RedisConcurrencyConfig, RedisConcurrencyStore, RedisConfig, RedisHealthConfig,
    RedisHealthStore, RedisRequestRateLimitConfig, RedisRequestRateLimitStore,
    RedisStickySessionConfig, RedisStickySessionStore,
};
use af_config::{AppConfig, ConfigError};
use af_db::{
    AdminChannelRepository, AdminChannelRepositoryConfigError, AdminDashboardRepository,
    AdminDashboardRepositoryConfigError, AdminGroupRepository, AdminGroupRepositoryConfigError,
    AdminModelRepository, AdminModelRepositoryConfigError, AdminRouteRepository,
    AdminRouteRepositoryConfigError, AdminTokenRepository, AdminTokenRepositoryConfigError,
    AdminUsageLogRepository, AdminUsageLogRepositoryConfigError, AdminUserRepository,
    AdminUserRepositoryConfigError, AnalyticsExportRepository, AnalyticsExportRepositoryError,
    AnnouncementRepository, AnnouncementRepositoryConfigError, AsyncTaskBillingRepository,
    AsyncTaskRepository, AsyncTaskSubmissionRepository, AuthChallengeRateLimitRepository,
    AuthChallengeRateLimitRepositoryConfigError, AuthChallengeRepository,
    AuthChallengeRepositoryConfigError, BalanceAlertRepository, BalanceAlertRepositoryConfigError,
    BalanceAlertSettingsRepository, BalanceAlertSettingsRepositoryConfigError,
    ChannelStateRepository, CredentialProxyRepository, CredentialProxyRepositoryConfigError,
    CredentialStateRepository, CustomOAuth2LoginRepository, CustomOAuth2LoginRepositoryError,
    CustomOAuth2ProviderRepository, CustomOAuth2ProviderRepositoryError, DatabaseError,
    DatabaseMigrationExtension, DatabaseOptions, DatabaseOptionsError, DatabasePool,
    DebugTraceRepository, DebugTraceRepositoryConfigError, DebugTraceRepositoryError,
    DebugTraceSnapshotCipherError, EmailSettingsRepository, EmailSettingsRepositoryConfigError,
    InitialSetupRepository, InitialSetupRepositoryConfigError, MigrationOptions,
    ModelPriceRepository, ModelProviderCatalogRepository, ModelProviderCatalogRepositoryError,
    ModelSyncRepository, NetworkSettingsMode, NetworkSettingsRecord, NetworkSettingsRepository,
    NetworkSettingsRepositoryConfigError, OAuthLoginRepository, OAuthLoginRepositoryError,
    PasskeyRepository, PasswordResetRepository, PasswordResetRepositoryConfigError,
    PaymentSecretUpdate, PaymentSettingsRepository, PaymentSettingsRepositoryConfigError,
    PaymentSettingsRepositoryError, PaymentSettingsWriteRecord, PlaygroundConversationRepository,
    PlaygroundConversationRepositoryConfigError, PlaygroundShareRepository,
    PlaygroundShareRepositoryConfigError, QuotaRepository, RedemptionRepository,
    RedemptionRepositoryConfigError, RefundRepository, RefundRepositoryConfigError,
    RegistrationRepository, RegistrationRepositoryConfigError, RequestOutcomeRepository,
    SiteSettingsRepository, SiteSettingsRepositoryConfigError, SiteSettingsRepositoryError,
    SmartRouteRuntimeRepository, SmartRouteRuntimeRepositoryConfigError,
    SubscriptionBalanceAlertRepository, SubscriptionBalanceAlertRepositoryConfigError,
    SubscriptionRepository, SubscriptionRepositoryConfigError, TokenAuthRepository,
    TokenAuthRepositoryConfigError, TokenRequestAdmissionRepository,
    TokenRequestAdmissionRepositoryConfigError, TopupRepository, TopupRepositoryConfigError,
    UsageLogRepository, UserInvitationRepository, UserInvitationRepositoryConfigError,
    UserNotificationRepository, UserNotificationRepositoryConfigError, UserProfileRepository,
    UserProfileRepositoryConfigError, UserSessionRepository, UserSessionRepositoryConfigError,
    UserTokenRepository, UserTokenRepositoryConfigError, WalletLedgerRepository,
    WalletLedgerRepositoryConfigError, connect_and_migrate_with_extension,
};
use af_http::{
    AdminChannelProbe, AdminOAuthConnectionService, AudioService, ChatService, EmbeddingService,
    FrontendAssetSource, HttpExtensions, HttpListener, HttpRouter, ImageService,
    PaymentWebhookProcessorRegistry, QueryApiKeyPolicy, ReadinessHandle,
    RefundReceiptProcessorRegistry, RerankService, ResponsesCompactService, ServeError,
    ServeOutcome, SpeechService, TurnstileVerifier, VideoTaskService,
    build_frontend_template_router, build_payment_webhook_router,
    build_refund_receipt_webhook_router, build_router, serve_with_graceful_shutdown,
};
use af_httpclient::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_CACHED_CLIENTS, DEFAULT_READ_TIMEOUT,
    DEFAULT_REQUEST_TIMEOUT, HttpClientConfig, HttpClientError, HttpClientProvider, HttpTimeouts,
    ProxyConfig, RemoteDnsPolicy,
};
use af_scheduler::{
    ChannelIndexCacheError, ChannelIndexRefreshConfig, ChannelIndexRefreshSupervisor, ChannelProbe,
    ChannelProbeSupervisor, ChannelProbeSupervisorConfig, InMemoryChannelIndex,
    IndexedWeightedScheduler, StickyWaitPolicy, SubscriptionCycleSupervisor,
    SubscriptionCycleSupervisorConfig,
};
use af_telemetry::{MetricsHandle, TelemetryError, TracingHandle, init_metrics, init_tracing};
use thiserror::Error;
use url::Url;
use webauthn_rs::prelude::{Webauthn, WebauthnBuilder};

use crate::readiness::DatabaseReadinessProbe;
use crate::{
    AudioRoutePlanner, BackgroundTaskSupervisor, BillingAudioService, BillingChatService,
    BillingEmbeddingService, BillingFlushReport, BillingImageService, BillingRerankService,
    BillingResponsesCompactService, BillingRuntime, BillingRuntimeError, BillingSpeechService,
    ChatRoutePlanner, EmbeddingRoutePlanner, ImageRoutePlanner, RequestOutcomeRuntime,
    RerankRoutePlanner, ResponsesCompactRoutePlanner, RuntimeExtensionContext, RuntimeExtensions,
    RuntimeHttpContext, ShutdownController, SpeechRoutePlanner, SupervisorError,
    SupervisorShutdown,
    balance_alert_runtime::BalanceAlertRuntime,
    channel_probe::{BoundedAdminChannelProbe, DatabaseChannelProbe},
    concurrency_runtime::{ConcurrencyCleanupWorker, ConcurrencyReleaseWorker, ConcurrencyRuntime},
    debug_trace_runtime::{AdminDebugTraceRuntimeApplier, DebugTraceRuntime, DebugTraceWorker},
    frontend::{EmbeddedFrontendAssets, EmbeddedNextFrontendAssets},
    group_pricing_refresh::RuntimeGroupPricingRefresher,
    model_catalog::RuntimeModelCatalogReader,
    model_discovery::DatabaseUpstreamModelDiscoverer,
    model_price_source::{ModelPriceDiscoverer, RuntimeModelPriceRefresher},
    oauth_connection::{
        OAuthRuntimeConfigError, PreparedOAuthLoopbackServer, initialize_oauth_refresh_supervisor,
        initialize_oauth_runtime, register_oauth_loopback_servers,
    },
    payment_runtime::PaymentRuntime,
    request_rate_limit::{RequestRateLimitStore, RuntimeTokenAuthenticator},
    runtime_frontend::FrontendTemplateManager,
    scheduled_chat::ScheduledChatService,
    scheduler_invalidation::{
        PreparedSchedulerInvalidationRuntime, SchedulerInvalidationRuntimeError,
        prepare_scheduler_invalidation_runtime, register_scheduler_invalidation_tasks,
    },
    smtp_delivery::SmtpEmailDelivery,
    system_shutdown_signal,
    token_request_admission::TokenRequestAdmissionAuthenticator,
    turnstile::CloudflareTurnstileVerifier,
    video_task_service::DatabaseVideoTaskService,
};

const BILLING_QUEUE_RETRY_DELAY: Duration = Duration::from_millis(100);
/// 为数据库读取和任务调度保留的探活硬截止余量。
const CHANNEL_PROBE_HARD_TIMEOUT_PADDING: Duration = Duration::from_secs(10);
/// OAuth 一次性授权会话的固定清理周期；会话自身 TTL 仍由配置约束。
const OAUTH_SESSION_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// 已加载并校验静态配置。
pub struct Configured;

/// 已安装进程级日志与指标记录器。
pub struct TelemetryReady {
    _tracing: TracingHandle,
    _metrics: MetricsHandle,
}

/// 已连接数据库并执行全部待处理迁移。
///
/// 动态 Option、缓存连接与预热的真实实现应在本阶段之后、监督器启动之前插入；
/// 当前仍无相应 Source/Repository，因此不会用空操作冒充这些阶段。
pub struct DatabaseReady {
    _telemetry: TelemetryReady,
    database: DatabasePool,
    passkey_public_base_url: Option<String>,
    frontend_template_id: Option<String>,
}

/// 已使用数据库连接池装配令牌认证服务。
pub struct AuthenticationReady {
    infrastructure: DatabaseReady,
    authenticator: Arc<dyn TokenAuthenticator>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    initial_setup: Arc<dyn InitialSetup>,
    registration_service: Arc<dyn RegistrationService>,
    passkey_authentication_service: Option<Arc<dyn PasskeyAuthenticationService>>,
    password_reset_service: Arc<dyn PasswordResetService>,
    user_profile_service: Arc<dyn UserProfileService>,
    user_wallet_service: Arc<dyn UserWalletService>,
    account_verification_provider: Arc<dyn af_admin::AccountVerificationProvider>,
    verification_settings_service: Option<Arc<af_admin::DatabaseVerificationSettingsService>>,
    user_notification_service: Arc<dyn af_admin::UserNotificationService>,
    extensions: RuntimeExtensions,
    platform_audit_service: Arc<dyn af_admin::PlatformAuditService>,
    redemption_service: Arc<dyn RedemptionService>,
    subscription_service: Arc<dyn SubscriptionService>,
    subscription_repository: SubscriptionRepository,
    user_invitation_service: Arc<dyn UserInvitationService>,
    site_settings_service: Arc<dyn SiteSettingsService>,
    site_settings_repository: SiteSettingsRepository,
    announcement_service: Arc<dyn af_admin::AnnouncementService>,
    admin_email_settings_service: Arc<dyn AdminEmailSettingsService>,
    admin_credential_proxy_service: Arc<dyn AdminCredentialProxyService>,
    custom_oauth2_provider_repository: CustomOAuth2ProviderRepository,
    model_provider_catalog_repository: ModelProviderCatalogRepository,
    network_settings_repository: NetworkSettingsRepository,
    payment_settings_repository: PaymentSettingsRepository,
    system_secret_cipher: SystemSecretCipher,
    admin_balance_alert_settings_service: Arc<dyn AdminBalanceAlertSettingsService>,
    balance_alert_task: BalanceAlertTask,
    playground_share_service: Arc<dyn PlaygroundShareService>,
    playground_conversation_service: Arc<dyn PlaygroundConversationService>,
    user_token_service: Arc<dyn UserTokenService>,
    admin_channel_reader: Arc<dyn AdminChannelReader>,
    admin_channel_writer: Arc<dyn AdminChannelWriter>,
    admin_dashboard_reader: Arc<dyn AdminDashboardReader>,
    admin_dashboard_channel_storage: Arc<dyn AdminDashboardChannelStorage>,
    admin_group_reader: Arc<dyn AdminGroupReader>,
    admin_group_writer: Arc<dyn AdminGroupWriter>,
    admin_route_reader: Arc<dyn AdminRouteReader>,
    admin_route_writer: Arc<dyn AdminRouteWriter>,
    admin_model_reader: Arc<dyn AdminModelReader>,
    admin_model_writer: Arc<dyn AdminModelWriter>,
    admin_token_reader: Arc<dyn AdminTokenReader>,
    admin_token_writer: Arc<dyn AdminTokenWriter>,
    admin_usage_log_reader: Arc<dyn AdminUsageLogReader>,
    admin_user_reader: Arc<dyn AdminUserReader>,
    admin_user_writer: Arc<dyn AdminUserWriter>,
    admin_wallet_service: Arc<dyn AdminWalletService>,
    query_api_key_policy: QueryApiKeyPolicy,
    turnstile_verifier: Option<Arc<dyn TurnstileVerifier>>,
}

/// 生产转发共用的受控 HTTP Client 已完成安全配置与装配。
pub struct RelayReady {
    infrastructure: AuthenticationReady,
    upstream_clients: HttpClientProvider,
    oauth_login_service: Arc<dyn OAuthLoginService>,
    admin_network_settings_service: Arc<DatabaseAdminNetworkSettingsService>,
    admin_oauth_connection_service: Arc<dyn AdminOAuthConnectionService>,
    oauth_connection_coordinator: Arc<OAuthConnectionCoordinator>,
    oauth_loopback_servers: Vec<PreparedOAuthLoopbackServer>,
    oauth_profiles: Vec<Arc<OAuthProviderProfile>>,
}

/// 计费 WAL、定价缓存、额度仓储及内部可靠队列均已完成装配。
pub struct BillingReady {
    infrastructure: AuthenticationReady,
    chat: Arc<dyn ChatService>,
    responses_compact: Arc<dyn ResponsesCompactService>,
    audio: Arc<dyn AudioService>,
    speech: Arc<dyn SpeechService>,
    embedding: Arc<dyn EmbeddingService>,
    image: Arc<dyn ImageService>,
    rerank: Arc<dyn RerankService>,
    video_task: Arc<dyn VideoTaskService>,
    billing: BillingRuntime,
    usage_record_queue: UsageRecordQueue,
    usage_record_consumer: UsageRecordConsumer,
    usage_record_worker_count: usize,
    refund_signal_queue: RefundSignalQueue,
    refund_signal_consumer: RefundSignalConsumer,
    upstream_clients: HttpClientProvider,
    oauth_login_service: Arc<dyn OAuthLoginService>,
    admin_network_settings_service: Arc<DatabaseAdminNetworkSettingsService>,
    admin_oauth_connection_service: Arc<dyn AdminOAuthConnectionService>,
    oauth_connection_coordinator: Arc<OAuthConnectionCoordinator>,
    oauth_loopback_servers: Vec<PreparedOAuthLoopbackServer>,
    oauth_refresh_supervisor: Option<OAuthRefreshSupervisor>,
    channel_index: InMemoryChannelIndex,
    scheduler_invalidation: PreparedSchedulerInvalidationRuntime,
    model_catalog_reader: Arc<dyn ModelCatalogReader>,
    admin_model_sync_service: Arc<dyn AdminModelSyncService>,
    admin_model_price_service: Arc<dyn AdminModelPriceService>,
    payment_runtime: Arc<PaymentRuntime>,
    admin_payment_settings_service: Arc<dyn af_admin::AdminPaymentSettingsService>,
    admin_refund_service: Arc<dyn AdminRefundService>,
    concurrency_release_worker: Option<ConcurrencyReleaseWorker>,
    concurrency_cleanup_worker: Option<ConcurrencyCleanupWorker>,
    debug_trace_repository: DebugTraceRepository,
    debug_trace_runtime: DebugTraceRuntime,
    debug_trace_worker: DebugTraceWorker,
    analytics_export_repository: Option<AnalyticsExportRepository>,
}

/// 后台任务监督域已经建立，周期 flush、订阅周期、渠道索引、渠道探活与可靠队列 worker 已注册。
pub struct Supervised {
    infrastructure: AuthenticationReady,
    chat: Arc<dyn ChatService>,
    responses_compact: Arc<dyn ResponsesCompactService>,
    audio: Arc<dyn AudioService>,
    speech: Arc<dyn SpeechService>,
    embedding: Arc<dyn EmbeddingService>,
    image: Arc<dyn ImageService>,
    rerank: Arc<dyn RerankService>,
    video_task: Arc<dyn VideoTaskService>,
    billing: BillingRuntime,
    usage_record_queue: UsageRecordQueue,
    refund_signal_queue: RefundSignalQueue,
    _channel_index: InMemoryChannelIndex,
    model_catalog_reader: Arc<dyn ModelCatalogReader>,
    admin_model_sync_service: Arc<dyn AdminModelSyncService>,
    admin_model_price_service: Arc<dyn AdminModelPriceService>,
    payment_runtime: Arc<PaymentRuntime>,
    admin_payment_settings_service: Arc<dyn af_admin::AdminPaymentSettingsService>,
    admin_refund_service: Arc<dyn AdminRefundService>,
    admin_network_settings_service: Arc<DatabaseAdminNetworkSettingsService>,
    admin_oauth_connection_service: Arc<dyn AdminOAuthConnectionService>,
    oauth_login_service: Arc<dyn OAuthLoginService>,
    admin_channel_probe: Arc<dyn AdminChannelProbe>,
    upstream_clients: HttpClientProvider,
    debug_trace_repository: DebugTraceRepository,
    debug_trace_runtime: DebugTraceRuntime,
    analytics_export_control: Option<Arc<dyn af_analytics::AnalyticsExportControl>>,
    supervisor: BackgroundTaskSupervisor,
}

/// HTTP Router 已构建，可以绑定端口并开始服务。
pub struct RouterReady {
    runtime: Supervised,
    router: HttpRouter,
    readiness: ReadinessHandle,
}

/// HTTP 监听器已绑定，可以读取实际地址并开始服务。
pub struct ListenerReady {
    runtime: Supervised,
    router: HttpRouter,
    listener: HttpListener,
    readiness: ReadinessHandle,
}

/// 分阶段服务构建器；每个转换消费前一阶段，禁止跳过初始化顺序。
///
/// ```compile_fail
/// use af_server::{Bootstrap, Configured};
///
/// fn cannot_skip_database(bootstrap: Bootstrap<Configured>) {
///     let _ = bootstrap.build_router();
/// }
/// ```
///
/// ```compile_fail
/// use af_server::{Bootstrap, DatabaseReady};
///
/// fn cannot_skip_authentication(bootstrap: Bootstrap<DatabaseReady>) {
///     let _ = bootstrap.init_relay();
/// }
/// ```
pub struct Bootstrap<S> {
    config: AppConfig,
    shutdown_timeout: Duration,
    shutdown: ShutdownController,
    http_extensions: HttpExtensions,
    database_migration_extension: Option<Arc<dyn DatabaseMigrationExtension>>,
    stage: S,
}

/// 完整服务关闭结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    /// HTTP 在途连接的收尾方式。
    pub http: ServeOutcome,
    /// 后台任务的收尾方式。
    pub background: SupervisorShutdown,
    /// 数据库关闭前最终确认的计费 WAL 批次。
    pub billing: BillingFlushReport,
}

/// 顶层启动、监听与资源关闭错误。
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum BootstrapError {
    /// 静态配置加载或校验失败。
    #[error("加载服务配置失败")]
    Config(#[source] ConfigError),
    /// 发行版提供的运行时扩展装配失败。
    #[error("运行时扩展装配失败")]
    RuntimeExtension(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// tracing 或 metrics 初始化失败。
    #[error("初始化可观测性失败")]
    Telemetry(#[source] TelemetryError),
    /// 数据库启动参数无效。
    #[error("数据库启动参数无效")]
    DatabaseOptions(#[source] DatabaseOptionsError),
    /// 数据库连接、迁移或关闭失败。
    #[error("数据库生命周期操作失败")]
    Database(#[source] DatabaseError),
    /// 令牌仓储的启动参数无效。
    #[error("令牌认证启动参数无效")]
    Authentication(#[source] TokenAuthRepositoryConfigError),
    /// 令牌累计请求数准入仓储的启动参数无效。
    #[error("令牌请求数准入启动参数无效")]
    TokenRequestAdmissionRepository(#[source] TokenRequestAdmissionRepositoryConfigError),
    /// 用户会话仓储的启动参数无效。
    #[error("用户会话启动参数无效")]
    SessionRepository(#[source] UserSessionRepositoryConfigError),
    /// 用户登录 OAuth 仓储的启动参数无效。
    #[error("用户登录 OAuth 仓储启动参数无效")]
    OAuthLoginRepository(#[source] OAuthLoginRepositoryError),
    /// 自定义 OAuth2 登录事务仓储的启动参数无效。
    #[error("自定义 OAuth2 登录仓储启动参数无效")]
    CustomOAuth2LoginRepository(#[source] CustomOAuth2LoginRepositoryError),
    /// 管理会话 JWT 的启动配置无效或缺失。
    #[error("管理会话启动配置无效")]
    SessionAuthentication(#[source] SessionAuthenticatorConfigError),
    /// 首次安装仓储的启动参数无效。
    #[error("首次安装仓储启动参数无效")]
    InitialSetupRepository(#[source] InitialSetupRepositoryConfigError),
    /// 注册策略与限流仓储的启动参数无效。
    #[error("注册策略仓储启动参数无效")]
    RegistrationRepository(#[source] RegistrationRepositoryConfigError),
    /// 认证挑战仓储的启动参数无效。
    #[error("认证挑战仓储启动参数无效")]
    AuthChallengeRepository(#[source] AuthChallengeRepositoryConfigError),
    /// 认证挑战发送限流仓储的启动参数无效。
    #[error("认证挑战发送限流仓储启动参数无效")]
    AuthChallengeRateLimitRepository(#[source] AuthChallengeRateLimitRepositoryConfigError),
    /// 注册 IP 指纹密钥缺失或无效。
    #[error("注册服务启动配置无效")]
    RegistrationService(#[source] RegistrationServiceConfigError),
    /// 密码重置仓储的启动参数无效。
    #[error("密码重置仓储启动参数无效")]
    PasswordResetRepository(#[source] PasswordResetRepositoryConfigError),
    /// 密码重置安全派生密钥缺失或无效。
    #[error("密码重置服务启动配置无效")]
    PasswordResetService(#[source] PasswordResetServiceConfigError),
    /// 站点设置仓储的启动参数无效。
    #[error("站点设置仓储启动参数无效")]
    SiteSettingsRepository(#[source] SiteSettingsRepositoryConfigError),
    /// 公告仓储的启动参数无效。
    #[error("公告仓储启动参数无效")]
    AnnouncementRepository(#[source] AnnouncementRepositoryConfigError),
    /// 启动期读取站点公开地址失败。
    #[error("读取站点公开地址失败")]
    SiteSettingsRead(#[source] SiteSettingsRepositoryError),
    /// Passkey 登录的限流派生密钥缺失或无效。
    #[error("Passkey 登录服务启动配置无效")]
    PasskeyAuthenticationService(#[source] PasskeyAuthenticationServiceConfigError),
    /// SMTP 设置仓储的启动参数无效。
    #[error("SMTP 设置仓储启动参数无效")]
    EmailSettingsRepository(#[source] EmailSettingsRepositoryConfigError),
    /// 全局网络设置仓储的启动参数无效。
    #[error("网络设置仓储启动参数无效")]
    NetworkSettingsRepository(#[source] NetworkSettingsRepositoryConfigError),
    /// 在线支付设置仓储的启动参数无效。
    #[error("支付设置仓储启动参数无效")]
    PaymentSettingsRepository(#[source] PaymentSettingsRepositoryConfigError),
    /// 充值订单与支付事件仓储的启动参数无效。
    #[error("充值仓储启动参数无效")]
    TopupRepository(#[source] TopupRepositoryConfigError),
    /// 退款请求与回执仓储的启动参数无效。
    #[error("退款仓储启动参数无效")]
    RefundRepository(#[source] RefundRepositoryConfigError),
    /// 凭据专属代理目录仓储的启动参数无效。
    #[error("专属代理目录仓储启动参数无效")]
    CredentialProxyRepository(#[source] CredentialProxyRepositoryConfigError),
    /// 自定义 OAuth2 Provider 仓储无法从数据库连接池装配。
    #[error("自定义 OAuth2 Provider 仓储启动参数无效")]
    CustomOAuth2ProviderRepository(#[source] CustomOAuth2ProviderRepositoryError),
    /// 模型厂商目录仓储无法从数据库连接池装配。
    #[error("模型厂商目录仓储启动参数无效")]
    ModelProviderCatalogRepository(#[source] ModelProviderCatalogRepositoryError),
    /// 余额预警全局设置仓储的启动参数无效。
    #[error("余额预警设置仓储启动参数无效")]
    BalanceAlertSettingsRepository(#[source] BalanceAlertSettingsRepositoryConfigError),
    /// 余额预警事件仓储的启动参数无效。
    #[error("余额预警事件仓储启动参数无效")]
    BalanceAlertRepository(#[source] BalanceAlertRepositoryConfigError),
    /// 订阅窗口预警事件仓储的启动参数无效。
    #[error("订阅窗口预警事件仓储启动参数无效")]
    SubscriptionBalanceAlertRepository(#[source] SubscriptionBalanceAlertRepositoryConfigError),
    /// SMTP 密码加解密器无法从启动配置安全装配。
    #[error("初始化 SMTP 密码加解密器失败")]
    SystemSecret(#[source] SystemSecretError),
    /// Playground 分享仓储的启动参数无效。
    #[error("Playground 分享仓储启动参数无效")]
    PlaygroundShareRepository(#[source] PlaygroundShareRepositoryConfigError),
    /// Playground 私有会话仓储的启动参数无效。
    #[error("Playground 会话历史仓储启动参数无效")]
    PlaygroundConversationRepository(#[source] PlaygroundConversationRepositoryConfigError),
    /// 普通用户 API Key 仓储的启动参数无效。
    #[error("用户 API Key 仓储启动参数无效")]
    UserTokenRepository(#[source] UserTokenRepositoryConfigError),
    /// 管理员用户仓储的启动参数无效。
    #[error("管理员用户仓储启动参数无效")]
    AdminUserRepository(#[source] AdminUserRepositoryConfigError),
    /// 钱包账本仓储的启动参数无效。
    #[error("钱包账本仓储启动参数无效")]
    WalletLedgerRepository(#[source] WalletLedgerRepositoryConfigError),
    /// 兑换码仓储的启动参数无效。
    #[error("兑换码仓储启动参数无效")]
    RedemptionRepository(#[source] RedemptionRepositoryConfigError),
    /// 订阅仓储的启动参数无效。
    #[error("订阅仓储启动参数无效")]
    SubscriptionRepository(#[source] SubscriptionRepositoryConfigError),
    /// 普通用户资料仓储的启动参数无效。
    #[error("用户资料仓储启动参数无效")]
    UserProfileRepository(#[source] UserProfileRepositoryConfigError),
    /// 当前用户通知事实账本的启动参数无效。
    #[error("用户通知事实账本仓储启动参数无效")]
    UserNotificationRepository(#[source] UserNotificationRepositoryConfigError),
    /// 当前用户邀请读仓储的启动参数无效。
    #[error("用户邀请仓储启动参数无效")]
    UserInvitationRepository(#[source] UserInvitationRepositoryConfigError),
    /// 管理员分组仓储的启动参数无效。
    #[error("管理员分组仓储启动参数无效")]
    AdminGroupRepository(#[source] AdminGroupRepositoryConfigError),
    /// 管理员智能路由仓储的启动参数无效。
    #[error("管理员智能路由仓储启动参数无效")]
    AdminRouteRepository(#[source] AdminRouteRepositoryConfigError),
    /// 智能路由运行时仓储的启动参数无效。
    #[error("智能路由运行时仓储启动参数无效")]
    SmartRouteRuntimeRepository(#[source] SmartRouteRuntimeRepositoryConfigError),
    /// 管理员模型元数据仓储的启动参数无效。
    #[error("管理员模型元数据仓储启动参数无效")]
    AdminModelRepository(#[source] AdminModelRepositoryConfigError),
    /// 管理员令牌仓储的启动参数无效。
    #[error("管理员令牌仓储启动参数无效")]
    AdminTokenRepository(#[source] AdminTokenRepositoryConfigError),
    /// 管理员用量日志仓储的启动参数无效。
    #[error("管理员用量日志仓储启动参数无效")]
    AdminUsageLogRepository(#[source] AdminUsageLogRepositoryConfigError),
    /// 管理看板聚合仓储的启动参数无效。
    #[error("管理看板仓储启动参数无效")]
    AdminDashboardRepository(#[source] AdminDashboardRepositoryConfigError),
    /// ClickHouse 管理看板分析读取配置无法安全装配。
    #[error("ClickHouse 管理看板分析读取配置无效")]
    ClickHouseAdminDashboardConfig(
        #[source] crate::admin_dashboard_storage::ClickHouseAdminDashboardConfigError,
    ),
    /// ClickHouse 事实投递 outbox 仓储的启动参数无效。
    #[error("ClickHouse 事实投递 outbox 配置无效")]
    AnalyticsExportRepository(#[source] AnalyticsExportRepositoryError),
    /// 管理员渠道仓储的启动参数无效。
    #[error("管理员渠道仓储启动参数无效")]
    AdminChannelRepository(#[source] AdminChannelRepositoryConfigError),
    /// 调试追踪仓储的启动参数无效。
    #[error("调试追踪仓储启动参数无效")]
    DebugTraceRepository(#[source] DebugTraceRepositoryConfigError),
    /// 诊断快照加密器无法复用启动主密钥。
    #[error("初始化诊断快照加密器失败")]
    DebugTraceSnapshotCipher(#[source] DebugTraceSnapshotCipherError),
    /// 调试追踪初始设置无法读取或状态损坏。
    #[error("加载调试追踪初始设置失败")]
    DebugTraceSettings(#[source] DebugTraceRepositoryError),
    /// HTTP 监听、服务或强制关闭失败。
    #[error("HTTP 服务生命周期操作失败")]
    Http(#[source] ServeError),
    /// 受控上游 HTTP Client 配置或构造失败。
    #[error("初始化上游 HTTP Client 失败")]
    HttpClient(#[source] HttpClientError),
    /// 支付宝实名认证 provider 的启动配置无效。
    #[error("初始化支付宝实名认证 provider 失败")]
    AlipayVerificationProvider(#[source] af_admin::AlipayProviderConfigError),
    /// 数据库网络设置无法应用到当前 HTTP Client。
    #[error("应用运行时网络设置失败")]
    NetworkSettingsRuntime(#[source] AdminNetworkSettingsError),
    /// 启动期支付设置导入、读取或运行时应用失败。
    #[error("初始化支付运行时失败")]
    PaymentRuntime,
    /// 上游账号 OAuth profile、会话或 loopback 合约无法安全装配。
    #[error("初始化上游账号 OAuth 运行时失败")]
    OAuthRuntime(#[source] OAuthRuntimeConfigError),
    /// 生产渠道凭据解密器无法从启动配置安全装配。
    #[error("初始化凭据解密器失败")]
    CredentialDecryption(#[source] CredentialDecryptionError),
    /// 管理凭据加密器无法从启动配置安全装配。
    #[error("初始化凭据加密器失败")]
    CredentialEncryption(#[source] CredentialEncryptionError),
    /// 计费 WAL 或数据库 sink 初始化、恢复或落库失败。
    #[error("计费运行时生命周期操作失败")]
    Billing(#[source] BillingRuntimeError),
    /// 启动期 WAL 恢复超过配置的硬截止时间。
    #[error("启动期计费 WAL 恢复超时")]
    BillingStartupFlushTimeout,
    /// 关闭前最终 WAL flush 超过配置的硬截止时间。
    #[error("关闭前计费 WAL 刷新超时")]
    BillingShutdownFlushTimeout,
    /// 用量记录队列容量配置无效。
    #[error("初始化用量记录队列失败")]
    UsageRecordQueue(#[source] UsageRecordQueueError),
    /// 用量记录消费者配置无效。
    #[error("初始化用量记录消费者失败")]
    UsageRecordConsumer(#[source] UsageRecordConsumerError),
    /// 模型定价缓存首轮加载失败。
    #[error("初始化模型定价缓存失败")]
    ModelPriceCache(#[source] ModelPriceCacheError),
    /// 分组计费缓存首轮加载失败。
    #[error("初始化分组计费缓存失败")]
    GroupPricingCache(#[source] GroupPricingCacheError),
    /// 渠道索引首轮全量加载失败。
    #[error("初始化渠道索引失败")]
    ChannelIndex(#[source] ChannelIndexCacheError),
    /// 调度 outbox 或 Redis 失效广播无法安全装配。
    #[error("初始化调度快照失效广播失败")]
    SchedulerInvalidation(#[source] SchedulerInvalidationRuntimeError),
    /// Redis 粘性会话存储无法安全装配。
    #[error("初始化调度粘性会话失败")]
    StickySession(#[source] CacheError),
    /// Redis 调度健康状态无法安全装配。
    #[error("初始化调度熔断健康状态失败")]
    SchedulerHealth(#[source] CacheError),
    /// Redis 并发槽位存储无法安全装配。
    #[error("初始化调度并发槽位失败")]
    Concurrency(#[source] CacheError),
    /// Redis 生产请求限流存储无法安全装配。
    #[error("初始化生产请求限流失败")]
    RequestRateLimit(#[source] CacheError),
    /// 内部退款信号队列容量配置无效。
    #[error("初始化退款信号队列失败")]
    RefundSignalQueue(#[source] RefundSignalQueueError),
    /// 内部退款信号消费者配置无效。
    #[error("初始化退款信号消费者失败")]
    RefundSignalConsumer(#[source] RefundSignalConsumerError),
    /// 关闭截止到达时仍有未获持久化确认的用量事实。
    #[error("用量记录未能在关闭截止前排空")]
    UsageRecordDrainIncomplete,
    /// 关闭截止到达时仍有未完成的内部退款补偿信号。
    #[error("退款信号未能在关闭截止前排空")]
    RefundSignalDrainIncomplete,
    /// 后台任务强制关闭失败。
    #[error("后台任务生命周期操作失败")]
    Supervisor(#[source] SupervisorError),
}

impl BootstrapError {
    /// 返回当前错误是否表示仍有任务可能持有运行期资源。
    ///
    /// 该状态无法在同一进程内安全恢复，顶层入口必须立即结束进程，禁止继续析构 DB
    /// 等共享资源后维持服务运行。
    #[must_use]
    pub const fn requires_immediate_exit(&self) -> bool {
        matches!(
            self,
            Self::Http(ServeError::ForceStopTimeout)
                | Self::Supervisor(SupervisorError::ForceStopTimeout)
        )
    }
}

fn build_user_profile_service(
    repository: UserProfileRepository,
    balance_alert_settings: BalanceAlertSettingsRepository,
    passkey_repository: PasskeyRepository,
    system_secret_cipher: SystemSecretCipher,
    public_base_url: Option<&str>,
    email_binding: Option<EmailBindingService>,
) -> DatabaseUserProfileService {
    let service = DatabaseUserProfileService::new(repository, balance_alert_settings)
        .with_system_secret_cipher(system_secret_cipher)
        .with_passkey_repository(passkey_repository.clone())
        .with_email_binding_option(email_binding);
    if let Some(webauthn) = build_passkey_runtime(public_base_url) {
        service.with_passkey_runtime(passkey_repository, webauthn)
    } else {
        service
    }
}

/// 仅从数据库中的站点公开地址派生唯一 WebAuthn RP/Origin。
fn build_passkey_runtime(public_base_url: Option<&str>) -> Option<Webauthn> {
    let (rp_id, origin) = passkey_origin(public_base_url)?;
    WebauthnBuilder::new(&rp_id, &origin)
        .ok()?
        .rp_name("AnyFlows")
        .build()
        .ok()
}

fn passkey_origin(public_base_url: Option<&str>) -> Option<(String, Url)> {
    let mut origin = Url::parse(public_base_url?).ok()?;
    if !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return None;
    }
    let rp_id = origin.domain()?.to_owned();
    let allowed_transport = origin.scheme() == "https"
        || (origin.scheme() == "http" && rp_id.eq_ignore_ascii_case("localhost"));
    if !allowed_transport {
        return None;
    }
    // 品牌页面可以配置路径，但 WebAuthn 只信任规范化后的 Origin。
    origin.set_path("");
    Some((rp_id, origin))
}

impl Bootstrap<Configured> {
    /// 从已加载配置创建第一阶段；仍会重新校验，避免嵌入式调用绕过约束。
    pub fn new(config: AppConfig) -> Result<Self, BootstrapError> {
        config.validate().map_err(BootstrapError::Config)?;
        let shutdown_timeout = Duration::from_secs(config.server().shutdown_timeout_secs());
        Ok(Self {
            config,
            shutdown_timeout,
            shutdown: ShutdownController::new(),
            http_extensions: HttpExtensions::default(),
            database_migration_extension: None,
            stage: Configured,
        })
    }

    /// 注册由发行版提供的可选 HTTP 扩展。
    ///
    /// 扩展在数据库、认证和后台任务完成装配后统一加入公共路由。公共服务默认不注册
    /// 扩展，因此同一套启动链既可用于开源核心，也可用于包含企业模块的发行版。
    #[must_use]
    pub fn with_http_extensions(mut self, extensions: HttpExtensions) -> Self {
        self.http_extensions = extensions;
        self
    }

    /// 注册由发行版提供的数据库迁移扩展。
    ///
    /// 扩展迁移会在公共迁移完成后、同一迁移连接上执行。公共版本默认不注册扩展，
    /// 因此不会改变现有数据库启动路径。
    #[must_use]
    pub fn with_database_migration_extension(
        mut self,
        extension: Arc<dyn DatabaseMigrationExtension>,
    ) -> Self {
        self.database_migration_extension = Some(extension);
        self
    }

    /// 安装 stdout JSON tracing 与进程级 Prometheus recorder。
    pub fn init_telemetry(self) -> Result<Bootstrap<TelemetryReady>, BootstrapError> {
        let tracing = init_tracing(self.config.telemetry()).map_err(BootstrapError::Telemetry)?;
        let metrics = init_metrics().map_err(BootstrapError::Telemetry)?;
        Ok(self.transition(TelemetryReady {
            _tracing: tracing,
            _metrics: metrics,
        }))
    }
}

impl Bootstrap<TelemetryReady> {
    /// 创建数据库连接池并在开始监听前执行 pending migrations。
    pub async fn connect_database(self) -> Result<Bootstrap<DatabaseReady>, BootstrapError> {
        let database_options = database_options(&self.config)?;
        let migration_options = migration_options(&self.config)?;
        let database = connect_and_migrate_with_extension(
            &database_options,
            migration_options,
            self.database_migration_extension.as_deref(),
        )
        .await
        .map_err(BootstrapError::Database)?;
        let site_settings_repository = SiteSettingsRepository::new(
            database.clone(),
            Duration::from_secs(self.config.auth().lookup_timeout_secs()),
        )
        .map_err(BootstrapError::SiteSettingsRepository)?;
        let site_settings = site_settings_repository
            .settings()
            .await
            .map_err(BootstrapError::SiteSettingsRead)?;
        let passkey_public_base_url = site_settings.public_base_url().map(str::to_owned);
        let frontend_template_id = site_settings.frontend_template_id().map(str::to_owned);
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: DatabaseReady {
                _telemetry: stage,
                database,
                passkey_public_base_url,
                frontend_template_id,
            },
        })
    }
}

impl Bootstrap<DatabaseReady> {
    /// 使用同一数据库连接池装配令牌认证，并固定查询截止时间与 carrier 策略。
    pub fn init_authentication(self) -> Result<Bootstrap<AuthenticationReady>, BootstrapError> {
        self.init_authentication_with_extensions(|_| Ok(RuntimeExtensions::default()))
    }

    /// 使用发行版提供的装配函数初始化运行时扩展。
    ///
    /// 装配函数在数据库迁移和公共认证服务初始化后调用一次；返回的服务将用于后续
    /// 转发、计费和 HTTP 路由装配。装配失败会直接终止启动。
    pub fn init_authentication_with_extensions<F>(
        self,
        build_extensions: F,
    ) -> Result<Bootstrap<AuthenticationReady>, BootstrapError>
    where
        F: FnOnce(RuntimeExtensionContext<'_>) -> Result<RuntimeExtensions, BootstrapError>,
    {
        let auth = self.config.auth();
        let repository = TokenAuthRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::Authentication)?;
        let token_request_admission_repository = TokenRequestAdmissionRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::TokenRequestAdmissionRepository)?;
        let session_repository = UserSessionRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::SessionRepository)?;
        let session_authenticator = DatabaseSessionAuthenticator::new(
            session_repository,
            auth.session_signing_key().map(|key| key.expose()),
            auth.session_ttl_secs(),
        )
        .map_err(BootstrapError::SessionAuthentication)?;
        let initial_setup_repository = InitialSetupRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::InitialSetupRepository)?;
        let registration_repository = RegistrationRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::RegistrationRepository)?;
        let auth_challenge_repository = AuthChallengeRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AuthChallengeRepository)?;
        let auth_challenge_rate_limit_repository = AuthChallengeRateLimitRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AuthChallengeRateLimitRepository)?;
        let email_settings_repository = EmailSettingsRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::EmailSettingsRepository)?;
        let network_settings_repository = NetworkSettingsRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::NetworkSettingsRepository)?;
        let payment_settings_repository = PaymentSettingsRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::PaymentSettingsRepository)?;
        let credential_proxy_repository = CredentialProxyRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::CredentialProxyRepository)?;
        let site_settings_repository = SiteSettingsRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::SiteSettingsRepository)?;
        let announcement_repository = Arc::new(
            AnnouncementRepository::new(
                self.stage.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::AnnouncementRepository)?,
        );
        let custom_oauth2_provider_repository = CustomOAuth2ProviderRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::CustomOAuth2ProviderRepository)?;
        let model_provider_catalog_repository = ModelProviderCatalogRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::ModelProviderCatalogRepository)?;
        let balance_alert_settings_repository = BalanceAlertSettingsRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::BalanceAlertSettingsRepository)?;
        let balance_alert_repository = BalanceAlertRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::BalanceAlertRepository)?;
        let subscription_balance_alert_repository = SubscriptionBalanceAlertRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::SubscriptionBalanceAlertRepository)?;
        let password_reset_repository = PasswordResetRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::PasswordResetRepository)?;
        let playground_share_repository = PlaygroundShareRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::PlaygroundShareRepository)?;
        let playground_conversation_repository = PlaygroundConversationRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::PlaygroundConversationRepository)?;
        let user_token_repository = UserTokenRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::UserTokenRepository)?;
        let admin_user_repository = AdminUserRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminUserRepository)?;
        let wallet_ledger_repository = WalletLedgerRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::WalletLedgerRepository)?;
        let redemption_repository = RedemptionRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::RedemptionRepository)?;
        let subscription_repository = SubscriptionRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::SubscriptionRepository)?;
        let user_profile_repository = UserProfileRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::UserProfileRepository)?;
        let passkey_repository = PasskeyRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        );
        let user_invitation_repository = UserInvitationRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::UserInvitationRepository)?;
        let system_secret_cipher = SystemSecretCipher::new(self.config.credential_encryption())
            .map_err(BootstrapError::SystemSecret)?;
        let session_authenticator =
            session_authenticator.with_two_factor_cipher(system_secret_cipher.clone());
        let admin_credential_proxy_service = Arc::new(DatabaseAdminCredentialProxyService::new(
            credential_proxy_repository,
            system_secret_cipher.clone(),
        ));
        let smtp_delivery: Arc<dyn EmailDelivery> = Arc::new(SmtpEmailDelivery);
        let email_binding_service = match EmailBindingService::new(
            auth_challenge_repository.clone(),
            auth_challenge_rate_limit_repository.clone(),
            email_settings_repository.clone(),
            system_secret_cipher.clone(),
            Arc::clone(&smtp_delivery),
            auth.session_signing_key().map(|key| key.expose()),
        ) {
            Ok(service) => Some(service),
            Err(error) => {
                tracing::warn!(error = %error, "邮箱绑定服务未启用");
                None
            }
        };
        let user_notification_repository = Arc::new(
            UserNotificationRepository::new(
                self.stage.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::UserNotificationRepository)?,
        );
        let balance_alert_task = BalanceAlertTask::new(
            balance_alert_settings_repository.clone(),
            balance_alert_repository,
            subscription_balance_alert_repository,
            email_settings_repository.clone(),
            site_settings_repository.clone(),
            system_secret_cipher.clone(),
            Arc::clone(&smtp_delivery),
        );
        let admin_balance_alert_settings_service = DatabaseAdminBalanceAlertSettingsService::new(
            balance_alert_settings_repository.clone(),
        );
        let password_reset_service = DatabasePasswordResetService::new(
            password_reset_repository,
            auth_challenge_repository.clone(),
            auth_challenge_rate_limit_repository.clone(),
            email_settings_repository.clone(),
            site_settings_repository.clone(),
            system_secret_cipher.clone(),
            Arc::clone(&smtp_delivery),
            auth.session_signing_key().map(|key| key.expose()),
        )
        .map_err(BootstrapError::PasswordResetService)?;
        let registration_service = DatabaseRegistrationService::new(
            registration_repository,
            admin_user_repository.clone(),
            auth_challenge_repository,
            auth_challenge_rate_limit_repository.clone(),
            email_settings_repository.clone(),
            system_secret_cipher.clone(),
            Arc::clone(&smtp_delivery),
            auth.session_signing_key().map(|key| key.expose()),
        )
        .map_err(BootstrapError::RegistrationService)?;
        let registration_service: Arc<dyn RegistrationService> = Arc::new(registration_service);
        // Passkey 登录与注册只复用数据库公开地址派生的唯一 Origin。
        let passkey_authentication_service =
            build_passkey_runtime(self.stage.passkey_public_base_url.as_deref())
                .map(|webauthn| {
                    DatabasePasskeyAuthenticationService::new(
                        passkey_repository.clone(),
                        system_secret_cipher.clone(),
                        webauthn,
                        Arc::clone(&registration_service),
                        auth_challenge_rate_limit_repository,
                        auth.session_signing_key().map(|key| key.expose()),
                    )
                    .map(|service| Arc::new(service) as Arc<dyn PasskeyAuthenticationService>)
                })
                .transpose()
                .map_err(BootstrapError::PasskeyAuthenticationService)?;
        let admin_email_settings_service = DatabaseAdminEmailSettingsService::new(
            email_settings_repository,
            system_secret_cipher.clone(),
            smtp_delivery,
        );
        let admin_group_repository = AdminGroupRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminGroupRepository)?;
        let admin_route_repository = AdminRouteRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminRouteRepository)?;
        let admin_model_repository = AdminModelRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminModelRepository)?;
        let admin_token_repository = AdminTokenRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminTokenRepository)?;
        let admin_channel_repository = AdminChannelRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminChannelRepository)?;
        let admin_usage_log_repository = AdminUsageLogRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminUsageLogRepository)?;
        let request_outcome_read_repository =
            RequestOutcomeRepository::new(self.stage.database.clone());
        let admin_dashboard_repository = AdminDashboardRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminDashboardRepository)?;
        let admin_dashboard_channel_storage: Arc<dyn AdminDashboardChannelStorage> = Arc::new(
            crate::admin_dashboard_storage::DatabaseAdminDashboardChannelStorage::new(
                admin_dashboard_repository.clone(),
            ),
        );
        let credential_encryptor = CredentialEncryptor::new(self.config.credential_encryption())
            .map_err(BootstrapError::CredentialEncryption)?;
        let credential_decryptor = CredentialDecryptor::new(self.config.credential_encryption())
            .map_err(BootstrapError::CredentialDecryption)?;
        let query_api_key_policy = if auth.allow_query_api_key() {
            tracing::warn!(
                security_setting = "query_api_key_enabled",
                "已启用查询参数 API Key，前置代理必须关闭或脱敏查询串日志"
            );
            QueryApiKeyPolicy::Allow
        } else {
            QueryApiKeyPolicy::Deny
        };
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage,
        } = self;
        let user_profile_service: Arc<dyn UserProfileService> =
            Arc::new(build_user_profile_service(
                user_profile_repository,
                balance_alert_settings_repository,
                passkey_repository,
                system_secret_cipher.clone(),
                stage.passkey_public_base_url.as_deref(),
                email_binding_service,
            ));
        let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(session_authenticator);
        let extensions = build_extensions(RuntimeExtensionContext {
            config: &config,
            database: &stage.database,
            session_authenticator: &session_authenticator,
            user_profile_service: &user_profile_service,
            passkey_authentication_service: passkey_authentication_service.as_ref(),
            system_secret_cipher: &system_secret_cipher,
            usage_log_repository: &admin_usage_log_repository,
            request_outcome_repository: &request_outcome_read_repository,
        })?;
        let repository = match extensions.organization_token_validator.as_ref() {
            Some(validator) => repository.with_organization_validator(Arc::clone(validator)),
            None => repository,
        };
        let database_authenticator: Arc<dyn TokenAuthenticator> =
            Arc::new(DatabaseTokenAuthenticator::new(repository));
        let platform_audit_service = Arc::new(af_admin::DatabasePlatformAuditService::new(
            af_db::PlatformAuditRepository::new(stage.database.clone()),
        ));
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: AuthenticationReady {
                infrastructure: stage,
                authenticator: Arc::new(TokenRequestAdmissionAuthenticator::new(
                    database_authenticator,
                    Arc::new(token_request_admission_repository),
                )),
                session_authenticator,
                initial_setup: Arc::new(DatabaseInitialSetup::new(initial_setup_repository)),
                registration_service,
                passkey_authentication_service,
                password_reset_service: Arc::new(password_reset_service),
                user_profile_service,
                user_wallet_service: Arc::new(DatabaseUserWalletService::new(
                    wallet_ledger_repository.clone(),
                )),
                account_verification_provider: Arc::new(
                    af_admin::ManualAccountVerificationProvider,
                ),
                verification_settings_service: None,
                user_notification_service: Arc::new(DatabaseUserNotificationService::new(
                    user_notification_repository,
                )),
                extensions,
                platform_audit_service,
                redemption_service: Arc::new(DatabaseRedemptionService::new(redemption_repository)),
                subscription_service: Arc::new(DatabaseSubscriptionService::new(
                    subscription_repository.clone(),
                )),
                subscription_repository,
                user_invitation_service: Arc::new(DatabaseUserInvitationService::new(
                    user_invitation_repository,
                )),
                site_settings_service: Arc::new(DatabaseSiteSettingsService::new(
                    site_settings_repository.clone(),
                )),
                announcement_service: Arc::new(DatabaseAnnouncementService::new(
                    announcement_repository,
                )),
                site_settings_repository,
                admin_email_settings_service: Arc::new(admin_email_settings_service),
                admin_credential_proxy_service,
                custom_oauth2_provider_repository,
                model_provider_catalog_repository,
                network_settings_repository,
                payment_settings_repository,
                system_secret_cipher,
                admin_balance_alert_settings_service: Arc::new(
                    admin_balance_alert_settings_service,
                ),
                balance_alert_task,
                playground_share_service: Arc::new(DatabasePlaygroundShareService::new(
                    playground_share_repository,
                )),
                playground_conversation_service: Arc::new(
                    DatabasePlaygroundConversationService::new(playground_conversation_repository),
                ),
                user_token_service: Arc::new(DatabaseUserTokenService::new(user_token_repository)),
                admin_channel_reader: Arc::new(DatabaseAdminChannelReader::new(
                    admin_channel_repository.clone(),
                )),
                admin_channel_writer: Arc::new(
                    DatabaseAdminChannelWriter::new(admin_channel_repository, credential_encryptor)
                        .with_decryptor(credential_decryptor),
                ),
                admin_dashboard_reader: Arc::new(
                    StorageAdminDashboardReader::new(Arc::new(
                        crate::admin_dashboard_storage::DatabaseAdminDashboardStorage::new(
                            admin_dashboard_repository.clone(),
                        ),
                    ))
                    .with_service_level_storage(Arc::new(
                        crate::admin_dashboard_storage::DatabaseAdminDashboardStorage::new(
                            admin_dashboard_repository,
                        ),
                    )),
                ),
                admin_dashboard_channel_storage,
                admin_group_reader: Arc::new(DatabaseAdminGroupReader::new(
                    admin_group_repository.clone(),
                )),
                admin_group_writer: Arc::new(DatabaseAdminGroupWriter::new(admin_group_repository)),
                admin_route_reader: Arc::new(DatabaseAdminRouteReader::new(
                    admin_route_repository.clone(),
                )),
                admin_route_writer: Arc::new(DatabaseAdminRouteWriter::new(admin_route_repository)),
                admin_model_reader: Arc::new(DatabaseAdminModelReader::new(
                    admin_model_repository.clone(),
                )),
                admin_model_writer: Arc::new(DatabaseAdminModelWriter::new(admin_model_repository)),
                admin_token_reader: Arc::new(DatabaseAdminTokenReader::new(
                    admin_token_repository.clone(),
                )),
                admin_token_writer: Arc::new(DatabaseAdminTokenWriter::new(admin_token_repository)),
                admin_usage_log_reader: Arc::new(
                    DatabaseAdminUsageLogReader::new(admin_usage_log_repository)
                        .with_request_outcomes(request_outcome_read_repository),
                ),
                admin_user_reader: Arc::new(DatabaseAdminUserReader::new(
                    admin_user_repository.clone(),
                )),
                admin_user_writer: Arc::new(DatabaseAdminUserWriter::new(admin_user_repository)),
                admin_wallet_service: Arc::new(DatabaseAdminWalletService::new(
                    wallet_ledger_repository,
                )),
                query_api_key_policy,
                turnstile_verifier: None,
            },
        })
    }
}

impl Bootstrap<AuthenticationReady> {
    /// 使用启动配置建立生产转发共用的受控 HTTP Client。
    pub fn init_relay(self) -> Result<Bootstrap<RelayReady>, BootstrapError> {
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage: mut authentication,
        } = self;
        let (startup_config, max_cached_clients) = build_http_client_parts(&config)?;
        let upstream_clients = HttpClientProvider::new(startup_config.clone(), max_cached_clients)
            .map_err(BootstrapError::HttpClient)?;
        let alipay_provider = af_admin::AlipayAccountVerificationProvider::from_config(
            config.account_verification().alipay(),
            upstream_clients.clone(),
        )
        .map_err(BootstrapError::AlipayVerificationProvider)?;
        drop(alipay_provider);
        let verification_settings_service = Arc::new(
            af_admin::DatabaseVerificationSettingsService::new_with_site_settings(
                af_db::AccountVerificationSettingsRepository::new(
                    authentication.infrastructure.database.clone(),
                    Duration::from_secs(config.auth().lookup_timeout_secs()),
                ),
                authentication.system_secret_cipher.clone(),
                config.account_verification().alipay().clone(),
                upstream_clients.clone(),
                Some(Arc::new(authentication.site_settings_repository.clone())),
            ),
        );
        authentication.account_verification_provider = verification_settings_service.clone();
        authentication.verification_settings_service = Some(verification_settings_service);
        if let Some(initialize) = authentication.extensions.initialize_relay.as_ref() {
            initialize(&upstream_clients)?;
        }
        if let Some(settings) = config.clickhouse_analytics() {
            let clickhouse_config =
                crate::admin_dashboard_storage::ClickHouseAdminDashboardConfig::new(
                    settings.endpoint().expose(),
                    settings.query().expose(),
                    settings.username().clone(),
                    settings.password().clone(),
                    Duration::from_secs(settings.timeout_secs()),
                    settings.max_response_bytes(),
                )
                .map_err(BootstrapError::ClickHouseAdminDashboardConfig)?;
            let storage = SplitAdminDashboardStorage::new(
                Arc::new(
                    crate::admin_dashboard_storage::ClickHouseAdminDashboardStorage::new(
                        clickhouse_config,
                        upstream_clients.clone(),
                    ),
                ),
                Arc::clone(&authentication.admin_dashboard_channel_storage),
            );
            let sla_repository = AdminDashboardRepository::new(
                authentication.infrastructure.database.clone(),
                Duration::from_secs(config.auth().lookup_timeout_secs()),
            )
            .map_err(BootstrapError::AdminDashboardRepository)?;
            authentication.admin_dashboard_reader = Arc::new(
                StorageAdminDashboardReader::new(Arc::new(storage)).with_service_level_storage(
                    Arc::new(
                        crate::admin_dashboard_storage::DatabaseAdminDashboardStorage::new(
                            sla_repository,
                        ),
                    ),
                ),
            );
            tracing::info!("已启用 ClickHouse 管理看板分析事实读取");
        }
        authentication.turnstile_verifier = config.turnstile().secret_key().map(|secret| {
            Arc::new(CloudflareTurnstileVerifier::new(
                secret,
                upstream_clients.clone(),
                Duration::from_secs(config.turnstile().timeout_secs()),
            )) as Arc<dyn TurnstileVerifier>
        });
        let runtime = Arc::new(HttpClientNetworkSettingsApplier::new(
            upstream_clients.clone(),
            startup_config,
        ));
        let admin_network_settings_service = Arc::new(DatabaseAdminNetworkSettingsService::new(
            authentication.network_settings_repository.clone(),
            authentication.system_secret_cipher.clone(),
            runtime,
        ));
        let oauth_login_repository = OAuthLoginRepository::new(
            authentication.infrastructure.database.clone(),
            Duration::from_secs(config.auth().lookup_timeout_secs()),
        )
        .map_err(BootstrapError::OAuthLoginRepository)?;
        let custom_oauth2_login_repository = CustomOAuth2LoginRepository::new(
            authentication.infrastructure.database.clone(),
            Duration::from_secs(config.auth().lookup_timeout_secs()),
        )
        .map_err(BootstrapError::CustomOAuth2LoginRepository)?;
        let oauth_login_service: Arc<dyn OAuthLoginService> =
            Arc::new(DatabaseOAuthLoginService::new(
                oauth_login_repository,
                authentication.custom_oauth2_provider_repository.clone(),
                custom_oauth2_login_repository,
                authentication.site_settings_repository.clone(),
                authentication.system_secret_cipher.clone(),
                OAuthLoginClient::new(upstream_clients.clone()),
            ));
        let oauth_runtime = initialize_oauth_runtime(
            &config,
            authentication.infrastructure.database.clone(),
            Arc::clone(&authentication.admin_channel_reader),
            upstream_clients.clone(),
        )
        .map_err(BootstrapError::OAuthRuntime)?;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: RelayReady {
                infrastructure: authentication,
                upstream_clients,
                oauth_login_service,
                admin_network_settings_service,
                admin_oauth_connection_service: oauth_runtime.admin_connection_service,
                oauth_connection_coordinator: oauth_runtime.connection_coordinator,
                oauth_loopback_servers: oauth_runtime.loopback_servers,
                oauth_profiles: oauth_runtime.profiles,
            },
        })
    }
}

impl Bootstrap<RelayReady> {
    /// 打开计费 WAL、加载定价缓存并装配生产预扣、结算与内部补偿链路。
    pub async fn init_billing(self) -> Result<Bootstrap<BillingReady>, BootstrapError> {
        self.stage
            .admin_network_settings_service
            .apply_current()
            .await
            .map_err(BootstrapError::NetworkSettingsRuntime)?;
        let decryptor = CredentialDecryptor::new(self.config.credential_encryption())
            .map_err(BootstrapError::CredentialDecryption)?;
        let oauth_refresh_supervisor = initialize_oauth_refresh_supervisor(
            &self.config,
            self.stage.infrastructure.infrastructure.database.clone(),
            self.stage.oauth_profiles.clone(),
            self.stage.upstream_clients.clone(),
            decryptor.clone(),
        )
        .await
        .map_err(BootstrapError::OAuthRuntime)?;
        let model_sync_repository =
            ModelSyncRepository::new(self.stage.infrastructure.infrastructure.database.clone());
        let model_discoverer = Arc::new(DatabaseUpstreamModelDiscoverer::new(
            model_sync_repository.clone(),
            decryptor.clone(),
            self.stage.upstream_clients.clone(),
        ));
        let admin_model_sync_service: Arc<dyn AdminModelSyncService> = Arc::new(
            DatabaseAdminModelSyncService::new(model_sync_repository, model_discoverer),
        );
        let request_timeout = self
            .stage
            .upstream_clients
            .base_config()
            .map_err(BootstrapError::HttpClient)?
            .timeouts()
            .request();
        let topup_repository = TopupRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
            Duration::from_secs(self.config.auth().lookup_timeout_secs()),
        )
        .map_err(BootstrapError::TopupRepository)?;
        let topup_repository = match self.stage.infrastructure.extensions.topup.as_ref() {
            Some(extension) => topup_repository.with_extension(Arc::clone(extension)),
            None => topup_repository,
        };
        let refund_repository = RefundRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
            Duration::from_secs(self.config.auth().lookup_timeout_secs()),
        )
        .map_err(BootstrapError::RefundRepository)?;
        let admin_refund_repository = refund_repository.clone();
        import_legacy_stripe_payment_settings(
            &self.stage.infrastructure.payment_settings_repository,
            &self.stage.infrastructure.system_secret_cipher,
            self.config.payment(),
        )
        .await?;
        let payment_runtime = Arc::new(PaymentRuntime::new(
            topup_repository,
            refund_repository,
            self.stage.infrastructure.subscription_repository.clone(),
            self.stage.infrastructure.site_settings_repository.clone(),
            self.stage.upstream_clients.clone(),
            request_timeout,
        ));
        let admin_refund_service: Arc<dyn AdminRefundService> = Arc::new(
            DatabaseAdminRefundService::new(admin_refund_repository, Some(payment_runtime.clone())),
        );
        let admin_payment_settings_service = Arc::new(DatabaseAdminPaymentSettingsService::new(
            self.stage
                .infrastructure
                .payment_settings_repository
                .clone(),
            self.stage.infrastructure.system_secret_cipher.clone(),
            payment_runtime.clone(),
        ));
        admin_payment_settings_service
            .apply_current()
            .await
            .map_err(|_| BootstrapError::PaymentRuntime)?;
        let admin_payment_settings_service: Arc<dyn af_admin::AdminPaymentSettingsService> =
            admin_payment_settings_service;
        let billing = BillingRuntime::open(
            self.config.billing(),
            self.stage.infrastructure.infrastructure.database.clone(),
        )
        .map_err(BootstrapError::Billing)?;
        let flush_timeout = Duration::from_secs(self.config.billing().flush_timeout_secs());
        tokio::time::timeout(flush_timeout, billing.flush_all())
            .await
            .map_err(|_| BootstrapError::BillingStartupFlushTimeout)?
            .map_err(BootstrapError::Billing)?;
        let (usage_record_queue, usage_record_receiver) =
            UsageRecordQueue::bounded(self.config.billing().usage_record_queue_capacity())
                .map_err(BootstrapError::UsageRecordQueue)?;
        let (refund_signal_queue, refund_signal_receiver) =
            RefundSignalQueue::bounded(self.config.billing().usage_record_queue_capacity())
                .map_err(BootstrapError::RefundSignalQueue)?;
        let model_prices = ModelPriceCache::load_from_database(
            self.stage.infrastructure.infrastructure.database.clone(),
        )
        .await
        .map_err(BootstrapError::ModelPriceCache)?;
        let group_pricing = GroupPricingCache::load_from_database(
            self.stage.infrastructure.infrastructure.database.clone(),
        )
        .await
        .map_err(BootstrapError::GroupPricingCache)?;
        let group_pricing_refresher: Arc<dyn af_admin::GroupPricingRuntimeRefresher> =
            Arc::new(RuntimeGroupPricingRefresher::new(group_pricing.clone()));
        let admin_group_writer: Arc<dyn AdminGroupWriter> =
            Arc::new(RuntimeRefreshingAdminGroupWriter::new(
                Arc::clone(&self.stage.infrastructure.admin_group_writer),
                Arc::clone(&group_pricing_refresher),
            ));
        let model_catalog_metadata = AdminModelRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
            Duration::from_secs(self.config.auth().lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminModelRepository)?;
        let admin_model_price_service: Arc<dyn AdminModelPriceService> =
            Arc::new(DatabaseAdminModelPriceService::new(
                ModelPriceRepository::new(
                    self.stage.infrastructure.infrastructure.database.clone(),
                ),
                model_catalog_metadata.clone(),
                Arc::new(ModelPriceDiscoverer::new(
                    self.stage.upstream_clients.clone(),
                )),
                Arc::new(RuntimeModelPriceRefresher::new(model_prices.clone())),
            ));
        let channel_index = InMemoryChannelIndex::load_from_database(
            self.stage.infrastructure.infrastructure.database.clone(),
        )
        .await
        .map_err(BootstrapError::ChannelIndex)?;
        let debug_trace_snapshot_cipher = Arc::new(
            DiagnosticSnapshotCipher::new(self.config.credential_encryption())
                .map_err(BootstrapError::DebugTraceSnapshotCipher)?,
        );
        let debug_trace_repository = DebugTraceRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
            Duration::from_secs(self.config.auth().lookup_timeout_secs()),
            debug_trace_snapshot_cipher,
        )
        .map_err(BootstrapError::DebugTraceRepository)?;
        let debug_trace_settings = debug_trace_repository
            .settings()
            .await
            .map_err(BootstrapError::DebugTraceSettings)?;
        let (debug_trace_runtime, debug_trace_worker) =
            DebugTraceRuntime::new(debug_trace_repository.clone(), debug_trace_settings);
        let scheduler_invalidation = prepare_scheduler_invalidation_runtime(
            &self.config,
            self.stage.infrastructure.infrastructure.database.clone(),
            channel_index.clone(),
        )
        .await
        .map_err(BootstrapError::SchedulerInvalidation)?;
        let sticky_store = if let Some(redis_url) = self.config.redis().url() {
            let config = RedisStickySessionConfig::new(
                RedisConfig::new(redis_url.expose().to_owned()),
                "anyflows.scheduler.sticky.v1",
            )
            .map_err(BootstrapError::StickySession)?;
            let store = RedisStickySessionStore::connect(config)
                .await
                .map_err(BootstrapError::StickySession)?;
            Some(Arc::new(store) as Arc<dyn crate::sticky_session::StickySessionStore>)
        } else {
            None
        };
        let concurrency = if let Some(redis_url) = self.config.redis().url() {
            let config = RedisConcurrencyConfig::new(
                RedisConfig::new(redis_url.expose().to_owned()),
                "anyflows.scheduler.concurrency.v1",
            )
            .map_err(BootstrapError::Concurrency)?;
            let store = RedisConcurrencyStore::connect(config)
                .await
                .map_err(BootstrapError::Concurrency)?;
            Some(ConcurrencyRuntime::new(store))
        } else {
            None
        };
        let scheduler_health = if let Some(redis_url) = self.config.redis().url() {
            let config = RedisHealthConfig::new(
                RedisConfig::new(redis_url.expose().to_owned()),
                "anyflows.scheduler.health.v1",
            )
            .map_err(BootstrapError::SchedulerHealth)?;
            let store = RedisHealthStore::connect(config)
                .await
                .map_err(BootstrapError::SchedulerHealth)?;
            Some(Arc::new(store) as Arc<dyn crate::health_runtime::SchedulerHealthStore>)
        } else {
            None
        };
        let request_rate_limit_store = if let Some(redis_url) = self.config.redis().url() {
            let config = RedisRequestRateLimitConfig::new(
                RedisConfig::new(redis_url.expose().to_owned()),
                self.config
                    .redis()
                    .request_rate_limit_namespace()
                    .unwrap_or("anyflows.gateway.rpm.v1"),
            )
            .map_err(BootstrapError::RequestRateLimit)?;
            let store = RedisRequestRateLimitStore::connect(config)
                .await
                .map_err(BootstrapError::RequestRateLimit)?;
            Some(Arc::new(store) as Arc<dyn RequestRateLimitStore>)
        } else {
            None
        };
        let (concurrency_runtime, concurrency_release_worker, concurrency_cleanup_worker) =
            match concurrency {
                Some((runtime, release_worker, cleanup_worker)) => {
                    (Some(runtime), Some(release_worker), Some(cleanup_worker))
                }
                None => (None, None, None),
            };
        let route_service = ScheduledChatService::new(
            IndexedWeightedScheduler::new(channel_index.clone()),
            decryptor,
            self.stage.upstream_clients.clone(),
            CredentialStateRepository::new(
                self.stage.infrastructure.infrastructure.database.clone(),
            ),
        )
        .with_proxy_cipher(self.stage.infrastructure.system_secret_cipher.clone())
        .with_group_repository(
            AdminGroupRepository::new(
                self.stage.infrastructure.infrastructure.database.clone(),
                Duration::from_secs(self.config.auth().lookup_timeout_secs()),
            )
            .map_err(BootstrapError::AdminGroupRepository)?,
        )
        .with_smart_route_repository(
            SmartRouteRuntimeRepository::new(
                self.stage.infrastructure.infrastructure.database.clone(),
                Duration::from_secs(self.config.auth().lookup_timeout_secs()),
            )
            .map_err(BootstrapError::SmartRouteRuntimeRepository)?,
        )
        .with_channel_state_repository(ChannelStateRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
        ))
        .with_debug_trace_runtime(debug_trace_runtime.clone());
        let route_service = match sticky_store {
            Some(store) => route_service.with_sticky_sessions(store, StickyWaitPolicy::default()),
            None => route_service,
        };
        let route_service = match concurrency_runtime {
            Some(runtime) => route_service.with_concurrency(runtime),
            None => route_service,
        };
        let route_service = match scheduler_health {
            Some(store) => route_service.with_scheduler_health(store),
            None => route_service,
        };
        let embedding_route_planner: Arc<dyn EmbeddingRoutePlanner> =
            Arc::new(route_service.clone());
        let image_route_planner: Arc<dyn ImageRoutePlanner> = Arc::new(route_service.clone());
        let rerank_route_planner: Arc<dyn RerankRoutePlanner> = Arc::new(route_service.clone());
        let audio_route_planner: Arc<dyn AudioRoutePlanner> = Arc::new(route_service.clone());
        let speech_route_planner: Arc<dyn SpeechRoutePlanner> = Arc::new(route_service.clone());
        let responses_compact_route_planner: Arc<dyn ResponsesCompactRoutePlanner> =
            Arc::new(route_service.clone());
        let video_task_runtime = route_service.clone();
        let route_planner: Arc<dyn ChatRoutePlanner> = Arc::new(route_service);
        let pricing_source =
            CachedRequestPricingSnapshotSource::new(model_prices.clone(), group_pricing.clone());
        let pricing_source = match self
            .stage
            .infrastructure
            .extensions
            .contract_prices
            .as_ref()
        {
            Some(source) => pricing_source.with_contract_price_source(Arc::clone(source)),
            None => pricing_source,
        };
        let pricing_source = Arc::new(pricing_source);
        let model_catalog_reader: Arc<dyn ModelCatalogReader> =
            Arc::new(RuntimeModelCatalogReader::new(
                channel_index.clone(),
                model_prices,
                pricing_source.clone(),
                model_catalog_metadata,
            ));
        let quota_repository =
            QuotaRepository::new(self.stage.infrastructure.infrastructure.database.clone());
        let quota_repository = match self.stage.infrastructure.extensions.quota.as_ref() {
            Some(extension) => quota_repository.with_extension(Arc::clone(extension)),
            None => quota_repository,
        };
        let quota_repository = Arc::new(quota_repository);
        let precharge_port: Arc<dyn BillingPrechargePort> = quota_repository.clone();
        let settlement_port: Arc<dyn BillingSettlementPort> = quota_repository.clone();
        let task_reserve_port: Arc<dyn TaskBillingReservePort> = quota_repository.clone();
        let task_settlement_port: Arc<dyn TaskBillingSettlementPort> = quota_repository.clone();
        let task_release_sink: Arc<dyn TaskBillingReleaseSink> = quota_repository.clone();
        let refund_sink: Arc<dyn RefundSignalSink> = quota_repository.clone();
        let refund_port: Arc<dyn RefundSignalPort> = Arc::new(refund_signal_queue.clone());
        let usage_port: Arc<dyn UsageRecordPort> = Arc::new(usage_record_queue.clone());
        let task_operation_timeout = Duration::from_secs(self.config.auth().lookup_timeout_secs());
        let analytics_export_repository = self
            .config
            .clickhouse_export()
            .map(|_| {
                AnalyticsExportRepository::new(
                    self.stage.infrastructure.infrastructure.database.clone(),
                    task_operation_timeout,
                )
                .map_err(BootstrapError::AnalyticsExportRepository)
            })
            .transpose()?;
        let usage_repository =
            UsageLogRepository::new(self.stage.infrastructure.infrastructure.database.clone());
        let usage_repository =
            analytics_export_repository
                .as_ref()
                .map_or(usage_repository.clone(), |repository| {
                    usage_repository
                        .clone()
                        .with_analytics_export(repository.clone())
                });
        let usage_record_sink = DatabaseUsageRecordSink::new(usage_repository);
        let usage_record_sink = match self
            .stage
            .infrastructure
            .extensions
            .usage_projection
            .as_ref()
        {
            Some(projection) => usage_record_sink.with_projection(Arc::clone(projection)),
            None => usage_record_sink,
        };
        let usage_record_sink: Arc<dyn UsageRecordSink> = Arc::new(usage_record_sink);
        let usage_record_consumer = UsageRecordConsumer::new(
            usage_record_receiver,
            Arc::clone(&usage_record_sink),
            BILLING_QUEUE_RETRY_DELAY,
        )
        .map_err(BootstrapError::UsageRecordConsumer)?;
        let request_repository = RequestOutcomeRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
        );
        let request_repository =
            analytics_export_repository
                .as_ref()
                .map_or(request_repository.clone(), |repository| {
                    request_repository
                        .clone()
                        .with_analytics_export(repository.clone())
                });
        let request_outcomes = RequestOutcomeRuntime::new(request_repository);
        let refund_signal_consumer = RefundSignalConsumer::new(
            refund_signal_receiver,
            refund_sink,
            BILLING_QUEUE_RETRY_DELAY,
        )
        .map_err(BootstrapError::RefundSignalConsumer)?;
        let usage_record_worker_count = self.config.billing().usage_record_worker_count();
        let task_database = self.stage.infrastructure.infrastructure.database.clone();
        let video_task: Arc<dyn VideoTaskService> = Arc::new(DatabaseVideoTaskService::new(
            crate::scheduled_chat::PersistentVideoTaskCoordinator::new(
                video_task_runtime,
                AsyncTaskRepository::new(task_database.clone(), task_operation_timeout)
                    .expect("认证查询超时已校验为正数，异步任务仓储必须可装配"),
                AsyncTaskSubmissionRepository::new(task_database.clone(), task_operation_timeout)
                    .expect("认证查询超时已校验为正数，异步任务提交仓储必须可装配"),
                AsyncTaskBillingRepository::new(task_database, task_operation_timeout)
                    .expect("认证查询超时已校验为正数，异步任务计费仓储必须可装配"),
                group_pricing,
                crate::scheduled_chat::PersistentVideoTaskBillingPorts::new(
                    task_reserve_port,
                    task_settlement_port,
                    task_release_sink,
                    usage_record_sink,
                ),
            ),
        ));
        let embedding: Arc<dyn EmbeddingService> = Arc::new(
            BillingEmbeddingService::new(
                embedding_route_planner,
                pricing_source.clone(),
                Some(precharge_port.clone()),
                Some(refund_port.clone()),
                Some(settlement_port.clone()),
                usage_port.clone(),
            )
            .with_request_outcomes(request_outcomes.clone()),
        );
        let image: Arc<dyn ImageService> = Arc::new(
            BillingImageService::new(
                image_route_planner,
                pricing_source.clone(),
                Some(precharge_port.clone()),
                Some(refund_port.clone()),
                Some(settlement_port.clone()),
                usage_port.clone(),
            )
            .with_request_outcomes(request_outcomes.clone()),
        );
        let rerank: Arc<dyn RerankService> = Arc::new(
            BillingRerankService::new(
                rerank_route_planner,
                pricing_source.clone(),
                Some(precharge_port.clone()),
                Some(refund_port.clone()),
                Some(settlement_port.clone()),
                usage_port.clone(),
            )
            .with_request_outcomes(request_outcomes.clone()),
        );
        let audio: Arc<dyn AudioService> = Arc::new(
            BillingAudioService::new(
                audio_route_planner,
                pricing_source.clone(),
                Some(precharge_port.clone()),
                Some(refund_port.clone()),
                Some(settlement_port.clone()),
                usage_port.clone(),
            )
            .with_request_outcomes(request_outcomes.clone()),
        );
        let speech: Arc<dyn SpeechService> = Arc::new(
            BillingSpeechService::new(
                speech_route_planner,
                pricing_source.clone(),
                Some(precharge_port.clone()),
                Some(refund_port.clone()),
                Some(settlement_port.clone()),
                usage_port.clone(),
            )
            .with_request_outcomes(request_outcomes.clone()),
        );
        let responses_compact: Arc<dyn ResponsesCompactService> = Arc::new(
            BillingResponsesCompactService::new(
                responses_compact_route_planner,
                pricing_source.clone(),
                Some(precharge_port.clone()),
                Some(refund_port.clone()),
                Some(settlement_port.clone()),
                usage_port.clone(),
            )
            .with_request_outcomes(request_outcomes.clone()),
        );
        let chat: Arc<dyn ChatService> = Arc::new(
            BillingChatService::new(
                route_planner,
                pricing_source,
                Some(precharge_port),
                Some(refund_port),
                Some(settlement_port),
                usage_port,
            )
            .with_request_outcomes(request_outcomes),
        );
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage,
        } = self;
        let mut infrastructure = stage.infrastructure;
        infrastructure.admin_group_writer = admin_group_writer;
        infrastructure.initial_setup = Arc::new(RuntimeRefreshingInitialSetup::new(
            infrastructure.initial_setup,
            group_pricing_refresher,
        ));
        let authenticator = Arc::clone(&infrastructure.authenticator);
        infrastructure.authenticator = Arc::new(RuntimeTokenAuthenticator::new(
            authenticator,
            request_rate_limit_store,
        ));
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: BillingReady {
                infrastructure,
                chat,
                responses_compact,
                audio,
                speech,
                embedding,
                image,
                rerank,
                video_task,
                billing,
                usage_record_queue,
                usage_record_consumer,
                usage_record_worker_count,
                refund_signal_queue,
                refund_signal_consumer,
                upstream_clients: stage.upstream_clients,
                oauth_login_service: stage.oauth_login_service,
                admin_network_settings_service: stage.admin_network_settings_service,
                admin_oauth_connection_service: stage.admin_oauth_connection_service,
                oauth_connection_coordinator: stage.oauth_connection_coordinator,
                oauth_loopback_servers: stage.oauth_loopback_servers,
                oauth_refresh_supervisor,
                channel_index,
                scheduler_invalidation,
                model_catalog_reader,
                admin_model_sync_service,
                admin_model_price_service,
                payment_runtime,
                admin_payment_settings_service,
                admin_refund_service,
                concurrency_release_worker,
                concurrency_cleanup_worker,
                debug_trace_repository,
                debug_trace_runtime,
                debug_trace_worker,
                analytics_export_repository,
            },
        })
    }
}

impl Bootstrap<BillingReady> {
    /// 建立后台任务监督域；批量模式在该域注册周期 flush 并共享关闭信号。
    #[must_use]
    pub fn start_supervisor(self) -> Bootstrap<Supervised> {
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage,
        } = self;
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        let BillingReady {
            infrastructure,
            chat,
            responses_compact,
            audio,
            speech,
            embedding,
            image,
            rerank,
            video_task,
            billing,
            usage_record_queue,
            usage_record_consumer,
            usage_record_worker_count,
            refund_signal_queue,
            refund_signal_consumer,
            upstream_clients,
            oauth_login_service,
            admin_network_settings_service,
            admin_oauth_connection_service,
            oauth_connection_coordinator,
            oauth_loopback_servers,
            oauth_refresh_supervisor,
            channel_index,
            scheduler_invalidation,
            model_catalog_reader,
            admin_model_sync_service,
            admin_model_price_service,
            payment_runtime,
            admin_payment_settings_service,
            admin_refund_service,
            concurrency_release_worker,
            concurrency_cleanup_worker,
            debug_trace_repository,
            debug_trace_runtime,
            debug_trace_worker,
            analytics_export_repository,
        } = stage;
        let analytics_export_control = analytics_export_repository.as_ref().map(|repository| {
            Arc::new(crate::analytics_export::ClickHouseExportControl::new(
                repository.clone(),
            )) as Arc<dyn af_analytics::AnalyticsExportControl>
        });
        spawn_channel_index_refresh_task(
            &mut supervisor,
            channel_index.clone(),
            ChannelIndexRefreshConfig::default(),
        );
        register_scheduler_invalidation_tasks(&mut supervisor, scheduler_invalidation);
        register_oauth_loopback_servers(&mut supervisor, oauth_loopback_servers);
        if oauth_connection_coordinator.profile_count() > 0 {
            spawn_oauth_session_cleanup_task(&mut supervisor, oauth_connection_coordinator);
        }
        if let Some(runtime) = oauth_refresh_supervisor {
            spawn_oauth_refresh_task(&mut supervisor, runtime);
        }
        if let Some(worker) = concurrency_release_worker {
            supervisor.spawn_drainable("concurrency-release", move |task_shutdown| {
                let worker = worker.clone();
                async move {
                    worker.run_until(task_shutdown.cancelled()).await;
                }
            });
        }
        if let Some(worker) = concurrency_cleanup_worker {
            supervisor.spawn("concurrency-cleanup", move |task_shutdown| {
                let worker = worker.clone();
                async move {
                    worker.run_until(task_shutdown.cancelled()).await;
                }
            });
        }
        supervisor.spawn_drainable("debug-trace-persist", move |task_shutdown| {
            let worker = debug_trace_worker.clone();
            async move {
                worker.run(task_shutdown).await;
            }
        });
        if let (Some(settings), Some(repository)) =
            (config.clickhouse_export(), analytics_export_repository)
        {
            let runtime = crate::analytics_export::ClickHouseExportRuntime::new(
                settings,
                upstream_clients.clone(),
                repository,
            )
            .expect("已校验的 ClickHouse 事实投递配置必须可装配");
            supervisor.spawn("clickhouse-fact-export", move |task_shutdown| {
                let runtime = runtime.clone();
                async move {
                    runtime.run_until(task_shutdown.cancelled()).await;
                }
            });
            tracing::info!("已启用 ClickHouse 异步事实投递");
        }
        supervisor.spawn("balance-alert", {
            let runtime = BalanceAlertRuntime::new(infrastructure.balance_alert_task.clone());
            move |task_shutdown| {
                let runtime = runtime.clone();
                async move {
                    runtime.run_until(task_shutdown.cancelled()).await;
                }
            }
        });
        if let Some(register_tasks) = infrastructure.extensions.background_tasks.as_ref() {
            register_tasks(
                &mut supervisor,
                &config,
                &infrastructure.infrastructure.database,
            );
        }
        if config.subscription().enabled() {
            spawn_subscription_cycle_task(
                &mut supervisor,
                infrastructure.subscription_repository.clone(),
                subscription_cycle_config(&config),
            );
        }
        if billing.batch_enabled() {
            supervisor.spawn("billing-flush", {
                let billing = billing.clone();
                move |task_shutdown| {
                    let billing = billing.clone();
                    async move {
                        if let Err(error) = billing.run_periodic_flush(task_shutdown).await {
                            tracing::error!(error_kind = error.error_kind(), "计费周期刷新失败");
                        }
                    }
                }
            });
        }
        for _ in 0..usage_record_worker_count {
            supervisor.spawn_drainable("usage-record-persist", {
                let usage_record_consumer = usage_record_consumer.clone();
                move |_| {
                    let usage_record_consumer = usage_record_consumer.clone();
                    async move {
                        usage_record_consumer.run().await;
                    }
                }
            });
        }
        supervisor.spawn_drainable("refund-signal-consume", {
            let refund_signal_consumer = refund_signal_consumer.clone();
            move |_| {
                let refund_signal_consumer = refund_signal_consumer.clone();
                async move {
                    refund_signal_consumer.run().await;
                }
            }
        });
        let decryptor = CredentialDecryptor::new(config.credential_encryption())
            .expect("已校验的渠道凭据配置必须包含有效解密密钥");
        let probe = DatabaseChannelProbe::new(
            infrastructure.infrastructure.database.clone(),
            decryptor,
            upstream_clients.clone(),
            Duration::from_secs(config.channel_probe().probe_timeout_secs()),
        );
        let probe_config = channel_probe_config(&config);
        let admin_channel_probe: Arc<dyn AdminChannelProbe> = Arc::new(
            BoundedAdminChannelProbe::new(probe.clone(), probe_config.probe_timeout()),
        );
        if config.channel_probe().enabled() {
            spawn_channel_probe_task(
                &mut supervisor,
                infrastructure.infrastructure.database.clone(),
                probe_config,
                probe,
            );
        }
        Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: Supervised {
                infrastructure,
                chat,
                responses_compact,
                audio,
                speech,
                embedding,
                image,
                rerank,
                video_task,
                billing,
                usage_record_queue,
                refund_signal_queue,
                _channel_index: channel_index,
                model_catalog_reader,
                admin_model_sync_service,
                admin_model_price_service,
                payment_runtime,
                admin_payment_settings_service,
                admin_refund_service,
                admin_network_settings_service,
                admin_oauth_connection_service,
                oauth_login_service,
                admin_channel_probe,
                upstream_clients,
                debug_trace_repository,
                debug_trace_runtime,
                analytics_export_control,
                supervisor,
            },
        }
    }
}

/// 注册可重建且响应统一关闭信号的渠道索引周期任务。
fn spawn_channel_index_refresh_task(
    supervisor: &mut BackgroundTaskSupervisor,
    index: InMemoryChannelIndex,
    refresh_config: ChannelIndexRefreshConfig,
) {
    supervisor.spawn("channel-index-refresh", move |task_shutdown| {
        let runner = ChannelIndexRefreshSupervisor::new(index.clone(), refresh_config);
        async move {
            runner.run_periodic_until(task_shutdown.cancelled()).await;
        }
    });
}

/// 注册启动即扫描、可重建且响应统一关闭信号的 OAuth 刷新任务。
fn spawn_oauth_refresh_task(
    supervisor: &mut BackgroundTaskSupervisor,
    runtime: OAuthRefreshSupervisor,
) {
    supervisor.spawn("oauth-refresh", move |task_shutdown| {
        let runtime = runtime.clone();
        async move {
            runtime.run_periodic_until(task_shutdown.cancelled()).await;
        }
    });
}

/// 注册启动即扫描、按固定周期清理且响应统一关闭信号的 OAuth 会话任务。
fn spawn_oauth_session_cleanup_task(
    supervisor: &mut BackgroundTaskSupervisor,
    coordinator: Arc<OAuthConnectionCoordinator>,
) {
    supervisor.spawn("oauth-session-cleanup", move |task_shutdown| {
        let coordinator = Arc::clone(&coordinator);
        async move {
            let mut cleanup_ticks = tokio::time::interval(OAUTH_SESSION_CLEANUP_INTERVAL);
            cleanup_ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = task_shutdown.cancelled() => return,
                    _ = cleanup_ticks.tick() => {
                        match coordinator.cleanup_expired_sessions() {
                            Ok(removed) if removed > 0 => {
                                tracing::debug!(removed, "OAuth 过期授权会话已清理");
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(
                                    error_kind = oauth_session_cleanup_error_kind(&error),
                                    "OAuth 过期授权会话清理失败"
                                );
                            }
                        }
                    }
                }
            }
        }
    });
}

const fn oauth_session_cleanup_error_kind(
    error: &af_account::OAuthConnectionError,
) -> &'static str {
    match error {
        af_account::OAuthConnectionError::Authorization(
            af_account::OAuthAuthorizationError::StoreUnavailable,
        ) => "store_unavailable",
        af_account::OAuthConnectionError::Authorization(_) => "authorization_state",
        _ => "internal",
    }
}

/// 注册启动即扫描、可重建且响应统一关闭信号的订阅周期任务。
fn spawn_subscription_cycle_task(
    supervisor: &mut BackgroundTaskSupervisor,
    repository: SubscriptionRepository,
    cycle_config: SubscriptionCycleSupervisorConfig,
) {
    supervisor.spawn("subscription-cycle", move |task_shutdown| {
        let repository = repository.clone();
        async move {
            SubscriptionCycleSupervisor::new(repository, cycle_config)
                .run_periodic_until(task_shutdown.cancelled())
                .await;
        }
    });
}

/// 注册可重建且响应统一关闭信号的渠道探活周期任务。
fn spawn_channel_probe_task<P>(
    supervisor: &mut BackgroundTaskSupervisor,
    database: DatabasePool,
    probe_config: ChannelProbeSupervisorConfig,
    probe: P,
) where
    P: ChannelProbe + Clone + Send + Sync + 'static,
{
    supervisor.spawn("channel-probe", move |task_shutdown| {
        let store = ChannelStateRepository::new(database.clone());
        let probe = probe.clone();
        async move {
            ChannelProbeSupervisor::new(store, probe, probe_config)
                .run_periodic_until(task_shutdown.cancelled())
                .await;
        }
    });
}

/// 将已校验的启动配置转换为调度层的强类型探活边界。
fn channel_probe_config(config: &AppConfig) -> ChannelProbeSupervisorConfig {
    let settings = config.channel_probe();
    let configured_default = Duration::from_secs(settings.probe_timeout_secs());
    let maximum_channel_timeout = Duration::from_secs(af_domain::MAX_CHANNEL_TIMEOUT_SECS);
    // 原生 Responses 会顺序执行普通健康请求与 Compact 能力请求，外层按两次最坏请求兜底。
    let hard_timeout = std::cmp::max(configured_default, maximum_channel_timeout)
        .checked_mul(2)
        .expect("渠道双请求探活硬截止必须可表示")
        .checked_add(CHANNEL_PROBE_HARD_TIMEOUT_PADDING)
        .expect("渠道探活硬截止必须可表示");
    ChannelProbeSupervisorConfig::new(
        settings.batch_size(),
        Duration::from_secs(settings.interval_secs()),
        hard_timeout,
    )
    .expect("已校验的渠道探活配置必须满足调度层边界")
}

/// 将已校验的启动配置转换为调度层订阅周期边界。
fn subscription_cycle_config(config: &AppConfig) -> SubscriptionCycleSupervisorConfig {
    let settings = config.subscription();
    SubscriptionCycleSupervisorConfig::new(
        settings.batch_size(),
        Duration::from_secs(settings.interval_secs()),
        settings.max_batches_per_run(),
    )
    .expect("已校验的订阅周期配置必须满足调度层边界")
}

/// 首次升级时把旧 TOML Stripe 配置加密导入数据库；初始化后绝不再读取旧值。
async fn import_legacy_stripe_payment_settings(
    repository: &PaymentSettingsRepository,
    cipher: &SystemSecretCipher,
    legacy: &af_config::PaymentSettings,
) -> Result<(), BootstrapError> {
    let current = repository
        .settings()
        .await
        .map_err(|_| BootstrapError::PaymentRuntime)?;
    if current.initialized() {
        return Ok(());
    }
    let stripe_secret_key = legacy
        .stripe_secret_key()
        .map(|secret| {
            encrypt_legacy_payment_secret(
                cipher,
                SystemSecretKind::StripeSecretKey,
                secret.expose(),
            )
        })
        .transpose()?;
    let stripe_webhook_secret = legacy
        .stripe_webhook_secret()
        .map(|secret| {
            encrypt_legacy_payment_secret(
                cipher,
                SystemSecretKind::StripeWebhookSecret,
                secret.expose(),
            )
        })
        .transpose()?;
    let stripe_enabled = stripe_secret_key.is_some()
        && stripe_webhook_secret.is_some()
        && legacy.stripe_publishable_key().is_some();
    let update = repository
        .update(PaymentSettingsWriteRecord::new(
            current.version(),
            stripe_enabled,
            legacy.stripe_publishable_key().map(str::to_owned),
            stripe_secret_key.map_or(PaymentSecretUpdate::Keep, PaymentSecretUpdate::Replace),
            stripe_webhook_secret.map_or(PaymentSecretUpdate::Keep, PaymentSecretUpdate::Replace),
            u16::try_from(legacy.stripe_signature_tolerance_secs())
                .map_err(|_| BootstrapError::PaymentRuntime)?,
            false,
            None,
            None,
            PaymentSecretUpdate::Keep,
            true,
            true,
            false,
            false,
            false,
            500_000,
        ))
        .await;
    match update {
        Ok(_) => Ok(()),
        Err(PaymentSettingsRepositoryError::ConcurrentUpdate) => {
            // 多副本首次启动时允许另一实例先完成接管，但不能吞掉未初始化的异常竞争。
            let current = repository
                .settings()
                .await
                .map_err(|_| BootstrapError::PaymentRuntime)?;
            if current.initialized() {
                Ok(())
            } else {
                Err(BootstrapError::PaymentRuntime)
            }
        }
        Err(_) => Err(BootstrapError::PaymentRuntime),
    }
}

fn encrypt_legacy_payment_secret(
    cipher: &SystemSecretCipher,
    kind: SystemSecretKind,
    value: &str,
) -> Result<af_db::EncryptedCredentialEnvelope, BootstrapError> {
    let secret = PlainSystemSecret::new(value.to_owned()).map_err(BootstrapError::SystemSecret)?;
    cipher
        .encrypt(kind, &secret)
        .map_err(BootstrapError::SystemSecret)
}

impl Bootstrap<Supervised> {
    /// 构建完整 HTTP Router；此后只允许进入监听阶段。
    #[must_use]
    pub fn build_router(self) -> Bootstrap<RouterReady> {
        let probe =
            DatabaseReadinessProbe::new(self.stage.infrastructure.infrastructure.database.clone());
        let readiness = ReadinessHandle::new(probe);
        let debug_trace_service: Arc<dyn af_admin::AdminDebugTraceService> =
            Arc::new(af_admin::DatabaseAdminDebugTraceService::new(
                self.stage.debug_trace_repository.clone(),
                Arc::new(AdminDebugTraceRuntimeApplier::new(
                    self.stage.debug_trace_runtime.clone(),
                )),
            ));
        let user_topup_service: Arc<dyn UserTopupService> = self.stage.payment_runtime.clone();
        let custom_oauth2_provider_service: Arc<dyn AdminCustomOAuth2ProviderService> =
            Arc::new(DatabaseAdminCustomOAuth2ProviderService::new(
                self.stage
                    .infrastructure
                    .custom_oauth2_provider_repository
                    .clone(),
                self.stage.infrastructure.system_secret_cipher.clone(),
            ));
        let model_provider_catalog_service: Arc<dyn AdminModelProviderCatalogService> =
            Arc::new(DatabaseAdminModelProviderCatalogService::new(
                self.stage
                    .infrastructure
                    .model_provider_catalog_repository
                    .clone(),
            ));
        let payment_webhook_registry: Arc<dyn PaymentWebhookProcessorRegistry> =
            self.stage.payment_runtime.clone();
        let refund_receipt_registry: Arc<dyn RefundReceiptProcessorRegistry> =
            self.stage.payment_runtime.clone();
        let payment_webhook_routes = build_payment_webhook_router(payment_webhook_registry)
            .merge(build_refund_receipt_webhook_router(refund_receipt_registry));
        let embedded_frontend: Arc<dyn FrontendAssetSource> = Arc::new(EmbeddedFrontendAssets);
        let frontend_template_manager = FrontendTemplateManager::new(
            self.config
                .server()
                .frontend_template_directory()
                .to_path_buf(),
            Arc::clone(&embedded_frontend),
            Arc::new(EmbeddedNextFrontendAssets),
            self.stage.infrastructure.site_settings_repository.clone(),
            self.stage
                .infrastructure
                .infrastructure
                .frontend_template_id
                .clone(),
        );
        let mut http_extensions = self.http_extensions.clone();
        if let Some(factory) = self.stage.infrastructure.extensions.http.as_ref() {
            http_extensions.extend(factory(RuntimeHttpContext {
                config: &self.config,
                database: &self.stage.infrastructure.infrastructure.database,
                session_authenticator: &self.stage.infrastructure.session_authenticator,
                user_topup_service: &user_topup_service,
                upstream_clients: &self.stage.upstream_clients,
            }));
        }
        let router = build_router(
            self.config.server(),
            self.stage.chat.clone(),
            self.stage.audio.clone(),
            self.stage.embedding.clone(),
            self.stage.image.clone(),
            self.stage.rerank.clone(),
            Some(self.stage.video_task.clone()),
            self.stage.speech.clone(),
            self.stage.responses_compact.clone(),
            readiness.clone(),
            Arc::clone(&self.stage.infrastructure.authenticator),
            Arc::clone(&self.stage.infrastructure.session_authenticator),
            Arc::clone(&self.stage.model_catalog_reader),
            Arc::clone(&self.stage.infrastructure.playground_share_service),
            Arc::clone(&self.stage.infrastructure.playground_conversation_service),
            Arc::clone(&self.stage.infrastructure.user_token_service),
            Arc::clone(&self.stage.infrastructure.initial_setup),
            Arc::clone(&self.stage.infrastructure.registration_service),
            self.stage
                .infrastructure
                .passkey_authentication_service
                .clone(),
            Arc::clone(&self.stage.infrastructure.password_reset_service),
            Arc::clone(&self.stage.infrastructure.user_profile_service),
            Arc::clone(&self.stage.infrastructure.user_wallet_service),
            Arc::clone(&self.stage.infrastructure.user_notification_service),
            Some(Arc::clone(
                &self.stage.infrastructure.platform_audit_service,
            )),
            Some(user_topup_service),
            Arc::clone(&self.stage.infrastructure.redemption_service),
            Arc::clone(&self.stage.infrastructure.subscription_service),
            Arc::clone(&self.stage.infrastructure.user_invitation_service),
            Arc::clone(&self.stage.infrastructure.site_settings_service),
            Arc::clone(&self.stage.infrastructure.announcement_service),
            Some(custom_oauth2_provider_service),
            Some(model_provider_catalog_service),
            Some(Arc::clone(&self.stage.oauth_login_service)),
            self.stage.infrastructure.turnstile_verifier.clone(),
            self.config.turnstile().site_key(),
            Arc::clone(&self.stage.infrastructure.admin_email_settings_service),
            self.stage.admin_network_settings_service.clone(),
            Arc::clone(&self.stage.admin_payment_settings_service),
            Arc::clone(&self.stage.infrastructure.admin_credential_proxy_service),
            Some(Arc::clone(&self.stage.admin_oauth_connection_service)),
            Arc::clone(
                &self
                    .stage
                    .infrastructure
                    .admin_balance_alert_settings_service,
            ),
            debug_trace_service,
            Arc::clone(&self.stage.infrastructure.admin_channel_reader),
            Arc::clone(&self.stage.infrastructure.admin_channel_writer),
            Some(Arc::clone(&self.stage.admin_channel_probe)),
            Some(Arc::new(
                crate::credential_usage::CodexCredentialUsageProbe::new(
                    Arc::clone(&self.stage.infrastructure.admin_channel_writer),
                    self.stage.upstream_clients.clone(),
                ),
            )),
            Arc::clone(&self.stage.infrastructure.admin_group_reader),
            Arc::clone(&self.stage.infrastructure.admin_group_writer),
            Some(Arc::clone(&self.stage.infrastructure.admin_model_reader)),
            Some(Arc::clone(&self.stage.infrastructure.admin_model_writer)),
            Some(Arc::clone(&self.stage.admin_model_sync_service)),
            Some(Arc::clone(&self.stage.admin_model_price_service)),
            Some(Arc::clone(&self.stage.infrastructure.admin_route_reader)),
            Some(Arc::clone(&self.stage.infrastructure.admin_route_writer)),
            Arc::clone(&self.stage.infrastructure.admin_token_reader),
            Arc::clone(&self.stage.infrastructure.admin_token_writer),
            Arc::clone(&self.stage.infrastructure.admin_dashboard_reader),
            self.stage.analytics_export_control.clone(),
            Arc::clone(&self.stage.infrastructure.admin_usage_log_reader),
            Arc::clone(&self.stage.infrastructure.admin_user_reader),
            Arc::clone(&self.stage.infrastructure.admin_user_writer),
            Some(Arc::clone(&self.stage.infrastructure.admin_wallet_service)),
            Some(Arc::clone(&self.stage.admin_refund_service)),
            self.stage.infrastructure.query_api_key_policy,
            Some(Arc::clone(&frontend_template_manager) as Arc<dyn FrontendAssetSource>),
            Some(http_extensions),
            Some(payment_webhook_routes),
        );
        let router = router
            .merge(af_http::build_account_verification_router(
                Arc::new(
                    af_admin::AccountVerificationService::new(
                        af_db::AccountVerificationRepository::new(
                            self.stage.infrastructure.infrastructure.database.clone(),
                        ),
                        Arc::clone(&self.stage.infrastructure.platform_audit_service),
                    )
                    .with_provider(Arc::new(
                        af_admin::DatabaseManualAccountVerificationProvider::new(Arc::clone(
                            self.stage
                                .infrastructure
                                .verification_settings_service
                                .as_ref()
                                .expect("实名认证设置已在 init_relay 阶段装配"),
                        )),
                    ))
                    .with_provider(Arc::clone(
                        &self.stage.infrastructure.account_verification_provider,
                    ))
                    .with_settings(Arc::clone(
                        self.stage
                            .infrastructure
                            .verification_settings_service
                            .as_ref()
                            .expect("实名认证设置已在 init_relay 阶段装配"),
                    )),
                ),
                Arc::clone(&self.stage.infrastructure.session_authenticator),
            ))
            .merge(af_http::build_verification_settings_router(
                Arc::clone(
                    self.stage
                        .infrastructure
                        .verification_settings_service
                        .as_ref()
                        .expect("实名认证设置已在 init_relay 阶段装配"),
                ),
                Arc::clone(&self.stage.infrastructure.session_authenticator),
            ))
            .merge(build_frontend_template_router(
                frontend_template_manager.clone() as Arc<dyn af_http::FrontendTemplateService>,
                Arc::clone(&self.stage.infrastructure.session_authenticator),
            ));
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage,
        } = self;
        Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: RouterReady {
                runtime: stage,
                router,
                readiness,
            },
        }
    }
}

impl Bootstrap<RouterReady> {
    /// 绑定配置的监听地址；端口为零时可在下一阶段读取系统分配的实际端口。
    pub fn bind(self) -> Result<Bootstrap<ListenerReady>, BootstrapError> {
        let listener =
            HttpListener::bind(self.config.server().bind()).map_err(BootstrapError::Http)?;
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: _,
            stage,
        } = self;
        let RouterReady {
            runtime,
            router,
            readiness,
        } = stage;
        readiness.mark_ready();
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            http_extensions,
            database_migration_extension: None,
            stage: ListenerReady {
                runtime,
                router,
                listener,
                readiness,
            },
        })
    }
}

impl Bootstrap<ListenerReady> {
    /// 返回已绑定监听器的实际地址。
    #[must_use]
    pub const fn local_addr(&self) -> std::net::SocketAddr {
        self.stage.listener.local_addr()
    }

    /// 运行 HTTP 服务，并按顺序排空请求、停止任务、关闭数据库。
    ///
    /// 若强制停止后仍无法确认 HTTP 连接或后台任务已经退出，本方法会记录固定脱敏
    /// 分类并直接以状态码 1 结束进程，不会返回 `Result`。该终态无法在嵌入宿主内
    /// 安全恢复；需要自定义策略的宿主不应调用本服务进程入口。
    pub async fn serve<F>(self, shutdown_signal: F) -> Result<ShutdownReport, BootstrapError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let billing_flush_timeout = Duration::from_secs(self.config.billing().flush_timeout_secs());
        let Bootstrap {
            config: _,
            shutdown_timeout,
            shutdown,
            http_extensions: _,
            database_migration_extension: _,
            stage,
        } = self;
        let ListenerReady {
            runtime,
            router,
            listener,
            readiness,
        } = stage;
        let Supervised {
            infrastructure,
            supervisor,
            billing,
            usage_record_queue,
            refund_signal_queue,
            chat: _,
            responses_compact: _,
            embedding: _,
            image: _,
            rerank: _,
            audio: _,
            speech: _,
            _channel_index: _,
            model_catalog_reader: _,
            admin_model_sync_service: _,
            admin_model_price_service: _,
            payment_runtime: _,
            admin_payment_settings_service: _,
            admin_refund_service: _,
            admin_network_settings_service: _,
            admin_oauth_connection_service: _,
            oauth_login_service: _,
            admin_channel_probe: _,
            upstream_clients: _,
            debug_trace_repository: _,
            debug_trace_runtime: _,
            analytics_export_control: _,
            video_task: _,
        } = runtime;
        let shutdown_signal = begin_draining_on_signal(shutdown_signal, readiness);
        let http =
            serve_with_graceful_shutdown(listener, router, shutdown_signal, shutdown_timeout);

        run_http_and_shutdown(
            http,
            RuntimeShutdownResources {
                shutdown,
                supervisor,
                billing,
                usage_record_queue,
                refund_signal_queue,
                database: infrastructure.infrastructure.database,
            },
            shutdown_timeout,
            billing_flush_timeout,
        )
        .await
    }
}

async fn begin_draining_on_signal<F>(signal: F, readiness: ReadinessHandle)
where
    F: Future<Output = ()>,
{
    signal.await;
    readiness.begin_draining();
}

/// HTTP 停止后仍需按顺序排空并关闭的运行期资源。
struct RuntimeShutdownResources {
    shutdown: ShutdownController,
    supervisor: BackgroundTaskSupervisor,
    billing: BillingRuntime,
    usage_record_queue: UsageRecordQueue,
    refund_signal_queue: RefundSignalQueue,
    database: DatabasePool,
}

async fn run_http_and_shutdown<H>(
    http: H,
    resources: RuntimeShutdownResources,
    shutdown_timeout: Duration,
    billing_flush_timeout: Duration,
) -> Result<ShutdownReport, BootstrapError>
where
    H: Future<Output = Result<ServeOutcome, ServeError>>,
{
    let RuntimeShutdownResources {
        shutdown,
        supervisor,
        billing,
        usage_record_queue,
        refund_signal_queue,
        database,
    } = resources;
    let http_result = http.await;

    // HTTP 排空后不再有新的 lifecycle 完成回调；先封闭两个入口，让消费者可靠排空已接收事实。
    usage_record_queue.close();
    refund_signal_queue.close();
    // 队列关闭后再停止其他后台任务，消费者会忽略该信号直到队列返回空。
    let _ = shutdown.trigger();
    let background_result = supervisor.shutdown(shutdown_timeout).await;
    let usage_records_drained = usage_record_queue.is_drained();
    let refund_signals_drained = refund_signal_queue.is_drained();

    // 无法确认请求或后台任务已经终止时，禁止析构仍可能被使用的运行期资源。
    let http_result = match http_result {
        Err(ServeError::ForceStopTimeout) => {
            terminate_unconfirmed_shutdown("http_force_stop_timeout");
        }
        result => result,
    };
    let background = match background_result {
        Ok(outcome) => outcome,
        Err(SupervisorError::ForceStopTimeout) => {
            terminate_unconfirmed_shutdown("background_force_stop_timeout");
        }
    };
    // 周期任务已确认退出后再执行最终 flush；超时取消只会留下可安全重放的 WAL。
    let billing_result =
        match tokio::time::timeout(billing_flush_timeout, billing.flush_all()).await {
            Ok(result) => result.map_err(BootstrapError::Billing),
            Err(_) => Err(BootstrapError::BillingShutdownFlushTimeout),
        };
    let database_result = database.close().await;
    match (http_result, billing_result, database_result) {
        (Ok(_), Ok(_), Ok(())) if !usage_records_drained => {
            Err(BootstrapError::UsageRecordDrainIncomplete)
        }
        (Ok(_), Ok(_), Ok(())) if !refund_signals_drained => {
            Err(BootstrapError::RefundSignalDrainIncomplete)
        }
        (Ok(http), Ok(billing), Ok(())) => Ok(ShutdownReport {
            http,
            background,
            billing,
        }),
        (Ok(_), Ok(_), Err(error)) => Err(BootstrapError::Database(error)),
        (Ok(_), Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error), Err(_)) => {
            tracing::error!(
                error_kind = "database_close_after_billing_error",
                "计费收尾失败后的数据库关闭也失败"
            );
            Err(error)
        }
        (Err(error), billing_result, database_result) => {
            // 主 HTTP 错误决定服务失败原因，两个清理错误只记录固定分类。
            if billing_result.is_err() {
                tracing::error!(
                    error_kind = "billing_flush_after_http_error",
                    "HTTP 失败后的计费收尾也失败"
                );
            }
            if database_result.is_err() {
                tracing::error!(
                    error_kind = "database_close_after_http_error",
                    "HTTP 失败后的数据库关闭也失败"
                );
            }
            Err(BootstrapError::Http(error))
        }
    }
}

impl<S> Bootstrap<S> {
    fn transition<T>(self, stage: T) -> Bootstrap<T> {
        Bootstrap {
            config: self.config,
            shutdown_timeout: self.shutdown_timeout,
            shutdown: self.shutdown,
            http_extensions: self.http_extensions,
            database_migration_extension: self.database_migration_extension,
            stage,
        }
    }
}

fn database_options(config: &AppConfig) -> Result<DatabaseOptions, BootstrapError> {
    let options = DatabaseOptions::new(config.database().url().expose().to_owned())
        .map_err(BootstrapError::DatabaseOptions)?;
    Ok(
        if let Some(seconds) = config.database().health_check_timeout_secs() {
            options.with_health_check_timeout(Duration::from_secs(seconds))
        } else {
            options
        },
    )
}

fn migration_options(config: &AppConfig) -> Result<MigrationOptions, BootstrapError> {
    config.database().migration_timeout_secs().map_or_else(
        || Ok(MigrationOptions::default()),
        |seconds| {
            MigrationOptions::new(Duration::from_secs(seconds))
                .map_err(BootstrapError::DatabaseOptions)
        },
    )
}

/// 把数据库网络快照转换为共享 HTTP Client 基线的运行时适配器。
struct HttpClientNetworkSettingsApplier {
    provider: HttpClientProvider,
    startup_config: HttpClientConfig,
}

impl HttpClientNetworkSettingsApplier {
    fn new(provider: HttpClientProvider, startup_config: HttpClientConfig) -> Self {
        Self {
            provider,
            startup_config,
        }
    }

    fn apply_inner(
        &self,
        record: &NetworkSettingsRecord,
        cipher: &SystemSecretCipher,
    ) -> Result<(), NetworkSettingsRuntimeError> {
        let config = match record.mode() {
            NetworkSettingsMode::Inherit => self.startup_config.clone(),
            NetworkSettingsMode::Direct => self
                .startup_config
                .clone()
                .with_proxy(ProxyConfig::direct())
                .with_remote_dns_policy(RemoteDnsPolicy::Deny),
            mode if mode.is_proxy() => {
                let password = record
                    .password_secret()
                    .map(|secret| {
                        cipher.decrypt(af_account::SystemSecretKind::ProxyPassword, secret)
                    })
                    .transpose()
                    .map_err(|_| NetworkSettingsRuntimeError::Failed)?;
                let proxy = ProxyConfig::from_parts(
                    mode.scheme().ok_or(NetworkSettingsRuntimeError::Failed)?,
                    record
                        .proxy_host()
                        .ok_or(NetworkSettingsRuntimeError::Failed)?,
                    record
                        .proxy_port()
                        .ok_or(NetworkSettingsRuntimeError::Failed)?,
                    record.username(),
                    password
                        .as_ref()
                        .map(af_account::DecryptedSystemSecret::expose_secret),
                )
                .map_err(|_| NetworkSettingsRuntimeError::Failed)?;
                self.startup_config
                    .clone()
                    .with_proxy(proxy)
                    .with_remote_dns_policy(if record.trust_proxy_dns() {
                        RemoteDnsPolicy::TrustProxy
                    } else {
                        RemoteDnsPolicy::Deny
                    })
            }
            _ => return Err(NetworkSettingsRuntimeError::Failed),
        };
        self.provider
            .replace_base_config(config)
            .map_err(|_| NetworkSettingsRuntimeError::Failed)
    }
}

impl NetworkSettingsRuntimeApplier for HttpClientNetworkSettingsApplier {
    fn apply<'a>(
        &'a self,
        record: &'a NetworkSettingsRecord,
        cipher: &'a SystemSecretCipher,
    ) -> af_admin::NetworkSettingsApplyFuture<'a> {
        Box::pin(async move { self.apply_inner(record, cipher) })
    }
}

fn build_http_client_parts(
    config: &AppConfig,
) -> Result<(HttpClientConfig, usize), BootstrapError> {
    let settings = config.http_client();
    let timeouts = HttpTimeouts::new(
        settings
            .connect_timeout_secs()
            .map_or(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs),
        settings
            .read_timeout_secs()
            .map_or(DEFAULT_READ_TIMEOUT, Duration::from_secs),
        settings
            .request_timeout_secs()
            .map_or(DEFAULT_REQUEST_TIMEOUT, Duration::from_secs),
    )
    .map_err(BootstrapError::HttpClient)?;
    let proxy = settings
        .proxy_url()
        .map_or_else(
            || Ok(ProxyConfig::direct()),
            |url| ProxyConfig::parse(url.expose()),
        )
        .map_err(BootstrapError::HttpClient)?;
    let remote_dns_policy = if settings.trust_proxy_dns() {
        RemoteDnsPolicy::TrustProxy
    } else {
        RemoteDnsPolicy::Deny
    };
    let client_config =
        HttpClientConfig::new(proxy, timeouts).with_remote_dns_policy(remote_dns_policy);
    Ok((
        client_config,
        settings
            .max_cached_clients()
            .unwrap_or(DEFAULT_MAX_CACHED_CLIENTS),
    ))
}

fn terminate_unconfirmed_shutdown(error_kind: &'static str) -> ! {
    tracing::error!(
        error_kind,
        "无法确认运行期任务已经退出，立即终止进程以保护共享资源"
    );
    std::process::exit(1)
}

/// 加载默认配置并运行当前已实现的完整启动链。
pub async fn run() -> Result<(), BootstrapError> {
    let config = af_config::load().map_err(BootstrapError::Config)?;
    let report = Bootstrap::new(config)?
        .init_telemetry()?
        .connect_database()
        .await?
        .init_authentication()?
        .init_relay()?
        .init_billing()
        .await?
        .start_supervisor()
        .build_router()
        .bind()?
        .serve(system_shutdown_signal())
        .await?;
    log_shutdown_report(report);
    Ok(())
}

fn log_shutdown_report(report: ShutdownReport) {
    match report.http {
        ServeOutcome::Drained => {
            tracing::info!(shutdown_kind = "http_drained", "HTTP 在途请求已排空");
        }
        ServeOutcome::DrainTimedOut => {
            tracing::warn!(
                shutdown_kind = "http_forced",
                "HTTP 排空超时，剩余连接已强制终止"
            );
        }
        ServeOutcome::Stopped => {
            tracing::warn!(shutdown_kind = "http_stopped", "HTTP 服务在关闭信号前停止");
        }
        _ => {
            tracing::warn!(shutdown_kind = "http_unknown", "HTTP 服务以未知状态停止");
        }
    }
    match report.background {
        SupervisorShutdown::Drained => {
            tracing::info!(shutdown_kind = "tasks_drained", "后台任务已完成收尾");
        }
        SupervisorShutdown::TimedOut => {
            tracing::warn!(
                shutdown_kind = "tasks_forced",
                "后台任务收尾超时，剩余任务已取消"
            );
        }
    }
    tracing::info!(
        confirmed_batches = report.billing.confirmed_batches(),
        confirmed_events = report.billing.confirmed_events(),
        "关闭前计费 WAL 已完成最终刷新"
    );
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
    };

    use af_admin::{SessionPrincipal, SessionRole};
    use af_billing::{
        BillingBatch, BillingBatchApplyOutcome, BillingBatchEvent, BillingBatchSink,
        BillingBatchSinkFuture, BillingRequestPlan, BillingUsageRecord, FileBillingBatcher,
        RatioPricingResolver, RefundSignalConsumer, RefundSignalOutcome, RefundSignalPort,
        RefundSignalQueue, RefundSignalSink, RefundSignalSinkFuture, UsageRecordConsumer,
        UsageRecordQueue, UsageRecordSink, UsageRecordSinkFuture, UserBillingDelta,
    };
    use af_config::CREDENTIAL_ENCRYPTION_KEY_BYTES;
    use af_db::connect_and_migrate;
    use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, QuotaDelta, TokenId, UserId};
    use af_http::{ReadinessFuture, ReadinessProbe};
    use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use tokio::sync::{Notify, oneshot};

    use super::*;

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct AlwaysReadyProbe;

    #[derive(Clone, Copy)]
    struct TestUnhealthyChannelProbe;

    struct OrderedBillingSink {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    struct OrderedUsageSink {
        events: Arc<Mutex<Vec<&'static str>>>,
        release: Arc<Notify>,
    }

    struct OrderedRefundSink {
        events: Arc<Mutex<Vec<&'static str>>>,
        release: Arc<Notify>,
    }

    impl BillingBatchSink for OrderedBillingSink {
        fn apply<'a>(&'a self, _batch: &'a BillingBatch) -> BillingBatchSinkFuture<'a> {
            Box::pin(async move {
                self.events.lock().unwrap().push("billing_flushed");
                Ok(BillingBatchApplyOutcome::Applied)
            })
        }
    }

    impl UsageRecordSink for OrderedUsageSink {
        fn persist<'a>(&'a self, _record: BillingUsageRecord) -> UsageRecordSinkFuture<'a> {
            Box::pin(async move {
                self.release.notified().await;
                self.events.lock().unwrap().push("usage_persisted");
                Ok(())
            })
        }
    }

    impl RefundSignalSink for OrderedRefundSink {
        fn refund<'a>(
            &'a self,
            _reservation_id: BillingReservationId,
        ) -> RefundSignalSinkFuture<'a> {
            Box::pin(async move {
                self.release.notified().await;
                self.events.lock().unwrap().push("refund_persisted");
                Ok(())
            })
        }
    }

    impl ReadinessProbe for AlwaysReadyProbe {
        fn check(&self) -> ReadinessFuture<'_> {
            Box::pin(async { true })
        }
    }

    impl ChannelProbe for TestUnhealthyChannelProbe {
        async fn check(
            &self,
            _channel_id: af_domain::ChannelId,
        ) -> af_scheduler::ChannelProbeStatus {
            af_scheduler::ChannelProbeStatus::Unhealthy
        }
    }

    fn config_file(database_url: &str) -> PathBuf {
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-bootstrap-{}-{serial}.toml",
            std::process::id()
        ));
        let wal_directory = billing_wal_directory(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let credential_key = URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES]);
        fs::write(
            &path,
            format!(
                "[server]\nbind = '127.0.0.1:0'\nshutdown_timeout_secs = 1\n[telemetry]\nlevel = 'off'\n[database]\nurl = '{database_url}'\n[billing]\nwal_directory = '{wal_directory}'\n[credential_encryption]\nkey_id = 'bootstrap-test'\nkey = '{credential_key}'\n[auth]\nsession_signing_key = '{credential_key}'\n"
            ),
        )
        .unwrap();
        path
    }

    fn billing_wal_directory(config_path: &std::path::Path) -> PathBuf {
        config_path.with_extension("billing-wal")
    }

    #[test]
    fn invalid_database_scheme_fails_before_any_connection_attempt() {
        let path = config_file("redis://private-host/secret");
        let config = af_config::load_from(&path).unwrap();
        let _ = fs::remove_file(path);
        let error = database_options(&config).unwrap_err();
        assert!(matches!(error, BootstrapError::DatabaseOptions(_)));
        let rendered = format!("{error:?}\n{error}");
        assert!(!rendered.contains("private-host"));
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn top_level_display_never_renders_nested_error_details() {
        const SECRET_CANARY: &str = "startup-error-secret-canary";
        let error = BootstrapError::Http(ServeError::Io(std::io::Error::other(SECRET_CANARY)));

        assert!(!error.to_string().contains(SECRET_CANARY));
        assert!(!error.requires_immediate_exit());
        assert!(BootstrapError::Http(ServeError::ForceStopTimeout).requires_immediate_exit());
        assert!(
            BootstrapError::Supervisor(SupervisorError::ForceStopTimeout).requires_immediate_exit()
        );

        let error =
            BootstrapError::RuntimeExtension(Box::new(std::io::Error::other(SECRET_CANARY)));
        assert!(!error.to_string().contains(SECRET_CANARY));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn passkey_origin_uses_the_single_database_public_address() {
        let (rp_id, origin) = passkey_origin(Some("https://console.example/account")).unwrap();
        assert_eq!(rp_id, "console.example");
        assert_eq!(origin.as_str(), "https://console.example/");
        assert!(build_passkey_runtime(Some("https://console.example/account")).is_some());

        let (rp_id, origin) = passkey_origin(Some("http://localhost:5185/preview")).unwrap();
        assert_eq!(rp_id, "localhost");
        assert_eq!(origin.as_str(), "http://localhost:5185/");
        assert!(build_passkey_runtime(Some("http://localhost:5185/preview")).is_some());
    }

    #[test]
    fn passkey_origin_rejects_insecure_or_non_domain_public_addresses() {
        for value in [
            None,
            Some("http://console.example"),
            Some("http://127.0.0.1:5185"),
            Some("https://127.0.0.1"),
            Some("https://console.example/?source=untrusted"),
        ] {
            assert!(passkey_origin(value).is_none(), "不应接受 {value:?}");
            assert!(build_passkey_runtime(value).is_none(), "不应启用 {value:?}");
        }
    }

    #[tokio::test]
    async fn shutdown_signal_marks_readiness_draining_before_completion() {
        let readiness = ReadinessHandle::new(AlwaysReadyProbe);
        readiness.mark_ready();
        let (signal, signal_rx) = oneshot::channel();
        let task = tokio::spawn(begin_draining_on_signal(
            async move {
                let _ = signal_rx.await;
            },
            readiness.clone(),
        ));

        assert!(!readiness.is_draining());
        signal.send(()).unwrap();
        task.await.unwrap();
        assert!(readiness.is_draining());
        readiness.mark_ready();
        assert!(readiness.is_draining());
    }

    #[tokio::test]
    async fn current_stages_start_and_shutdown_in_order() {
        let path = config_file("sqlite::memory:");
        let config = af_config::load_from(&path).unwrap();
        let wal_directory = billing_wal_directory(&path);
        let _ = fs::remove_file(path);

        let supervised = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap()
            .init_authentication()
            .unwrap()
            .init_relay()
            .unwrap()
            .init_billing()
            .await
            .unwrap();
        // Codex 使用固定的 1455 回调端口；端口被并行测试占用时，启动器会
        // 保留手动回调路径并跳过对应监听任务，因此这里只把实际注册的监听任务计入基线。
        let expected_task_count = 9 + supervised.stage.oauth_loopback_servers.len();
        let supervised = supervised.start_supervisor();
        assert_eq!(
            supervised.stage.supervisor.task_count(),
            expected_task_count
        );
        let report = supervised
            .build_router()
            .bind()
            .unwrap()
            .serve(async {})
            .await
            .unwrap();
        assert_eq!(report.http, ServeOutcome::Drained);
        assert_eq!(report.background, SupervisorShutdown::Drained);
        assert_eq!(report.billing, BillingFlushReport::default());
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn runtime_extension_factory_reuses_public_auth_services() {
        let path = config_file("sqlite::memory:");
        let config = af_config::load_from(&path).unwrap();
        let _ = fs::remove_file(&path);
        let mut database_ready = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap();
        database_ready.stage.passkey_public_base_url = Some("https://console.example".to_owned());
        let mut calls = 0;
        let mut shared_auth = None;
        let authenticated = database_ready
            .init_authentication_with_extensions(|context| {
                calls += 1;
                shared_auth = Some((
                    Arc::clone(context.session_authenticator),
                    Arc::clone(context.user_profile_service),
                    Arc::clone(context.passkey_authentication_service.unwrap()),
                ));
                Ok(RuntimeExtensions::default())
            })
            .unwrap();

        assert_eq!(calls, 1);
        let (session, profile, passkey) = shared_auth.unwrap();
        assert!(Arc::ptr_eq(
            &session,
            &authenticated.stage.session_authenticator
        ));
        assert!(Arc::ptr_eq(
            &profile,
            &authenticated.stage.user_profile_service
        ));
        assert!(Arc::ptr_eq(
            &passkey,
            authenticated
                .stage
                .passkey_authentication_service
                .as_ref()
                .unwrap(),
        ));
        let database = authenticated.stage.infrastructure.database.clone();
        drop(authenticated);
        database.close().await.unwrap();
        let _ = fs::remove_dir_all(billing_wal_directory(&path));
    }

    #[tokio::test]
    async fn runtime_extension_factory_error_aborts_authentication_without_fallback() {
        let path = config_file("sqlite::memory:");
        let config = af_config::load_from(&path).unwrap();
        let _ = fs::remove_file(&path);
        let bootstrap = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap();
        let database = bootstrap.stage.database.clone();
        let mut calls = 0;

        let result = bootstrap.init_authentication_with_extensions(|_| {
            calls += 1;
            Err(BootstrapError::RuntimeExtension(Box::new(
                std::io::Error::other("enterprise-factory-error-canary"),
            )))
        });
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("企业服务装配失败时必须终止启动"),
        };
        assert_eq!(calls, 1);
        assert!(matches!(error, BootstrapError::RuntimeExtension(_)));
        assert!(
            !error
                .to_string()
                .contains("enterprise-factory-error-canary")
        );
        database.close().await.unwrap();
        let _ = fs::remove_dir_all(billing_wal_directory(&path));
    }

    #[tokio::test]
    async fn configured_redis_failure_rejects_scim_rate_limit_startup() {
        let path = config_file("sqlite::memory:");
        let contents = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            format!("{contents}[redis]\nurl = 'not-a-redis-url'\n"),
        )
        .unwrap();
        let config = af_config::load_from(&path).unwrap();
        let wal_directory = billing_wal_directory(&path);
        let _ = fs::remove_file(path);

        let Err(error) = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap()
            .init_authentication()
            .unwrap()
            .init_relay()
            .unwrap()
            .init_billing()
            .await
        else {
            panic!("已配置但不可用的 Redis 必须阻止计费启动");
        };
        let rendered = format!("{error:?}{error}");
        assert!(!rendered.contains("not-a-redis-url"));
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn configured_redis_failure_rejects_scim_rate_limit_startup_when_oauth_refresh_is_disabled()
     {
        let path = config_file("sqlite::memory:");
        let contents = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            format!(
                "{contents}[oauth]\nrefresh_enabled = false\n[redis]\nurl = 'not-a-redis-url'\n"
            ),
        )
        .unwrap();
        let config = af_config::load_from(&path).unwrap();
        let wal_directory = billing_wal_directory(&path);
        let _ = fs::remove_file(path);

        let Err(error) = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap()
            .init_authentication()
            .unwrap()
            .init_relay()
            .unwrap()
            .init_billing()
            .await
        else {
            panic!("已配置但不可用的 Redis 必须阻止计费启动");
        };
        let rendered = format!("{error:?}{error}");
        assert!(!rendered.contains("not-a-redis-url"));
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn oauth_main_port_conflict_keeps_manual_completion_available() {
        let path = config_file("sqlite::memory:");
        let contents = fs::read_to_string(&path)
            .unwrap()
            .replace("bind = '127.0.0.1:0'", "bind = '127.0.0.1:1455'");
        fs::write(
            &path,
            format!("{contents}[oauth.codex]\nclient_id = 'bootstrap-codex-client'\n"),
        )
        .unwrap();
        let config = af_config::load_from(&path).unwrap();
        let wal_directory = billing_wal_directory(&path);
        let _ = fs::remove_file(path);

        let relay = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap()
            .init_authentication()
            .unwrap()
            .init_relay()
            .unwrap();
        assert!(relay.stage.oauth_loopback_servers.is_empty());
        let statuses = relay
            .stage
            .admin_oauth_connection_service
            .provider_statuses(SessionPrincipal::new(
                UserId::new(1).unwrap(),
                SessionRole::Admin,
            ))
            .unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].provider(), af_http::AdminOAuthProvider::Codex);
        assert!(!statuses[0].loopback_listener_ready());
        assert_eq!(
            statuses[0].redirect_uri(),
            "http://localhost:1455/auth/callback"
        );
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn claude_code_profile_is_exposed_with_ready_loopback_listener() {
        let path = config_file("sqlite::memory:");
        let contents = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            format!("{contents}[oauth.claude_code]\nclient_id = 'bootstrap-claude-client'\n"),
        )
        .unwrap();
        let config = af_config::load_from(&path).unwrap();
        let wal_directory = billing_wal_directory(&path);
        let _ = fs::remove_file(path);

        let relay = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap()
            .init_authentication()
            .unwrap()
            .init_relay()
            .unwrap();
        let statuses = relay
            .stage
            .admin_oauth_connection_service
            .provider_statuses(SessionPrincipal::new(
                UserId::new(1).unwrap(),
                SessionRole::Admin,
            ))
            .unwrap();
        assert_eq!(statuses.len(), 2);
        let claude = statuses
            .iter()
            .find(|status| status.provider() == af_http::AdminOAuthProvider::ClaudeCode)
            .unwrap();
        assert!(claude.loopback_listener_ready());
        assert_eq!(claude.redirect_uri(), "http://localhost:54545/callback");
        assert!(
            statuses
                .iter()
                .any(|status| status.provider() == af_http::AdminOAuthProvider::Codex)
        );
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn missing_management_session_key_rejects_authentication_stage() {
        let path = config_file("sqlite::memory:");
        let contents = fs::read_to_string(&path).unwrap();
        let without_session_key = contents
            .lines()
            .filter(|line| !line.starts_with("session_signing_key"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, without_session_key).unwrap();
        let config = af_config::load_from(&path).unwrap();
        let _ = fs::remove_file(&path);

        let bootstrap = Bootstrap::new(config)
            .unwrap()
            .init_telemetry()
            .unwrap()
            .connect_database()
            .await
            .unwrap();
        let error = match bootstrap.init_authentication_with_extensions(|_| {
            panic!("公共认证尚未初始化时不得调用企业装配函数")
        }) {
            Ok(_) => panic!("缺失管理会话密钥时不应进入认证就绪阶段"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            BootstrapError::SessionAuthentication(
                SessionAuthenticatorConfigError::MissingSigningKey
            )
        ));
        let _ = fs::remove_dir_all(billing_wal_directory(&path));
    }

    #[tokio::test]
    async fn channel_probe_task_is_registered_and_cancellable() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let shutdown = ShutdownController::new();
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown);
        spawn_channel_probe_task(
            &mut supervisor,
            database.clone(),
            ChannelProbeSupervisorConfig::new(
                1,
                Duration::from_secs(3_600),
                Duration::from_secs(1),
            )
            .unwrap(),
            TestUnhealthyChannelProbe,
        );

        assert_eq!(supervisor.task_count(), 1);
        assert_eq!(
            supervisor.shutdown(Duration::from_secs(1)).await.unwrap(),
            SupervisorShutdown::Drained
        );
        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn subscription_cycle_task_is_registered_and_cancellable() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repository =
            SubscriptionRepository::new(database.clone(), Duration::from_secs(1)).unwrap();
        let shutdown = ShutdownController::new();
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown);
        spawn_subscription_cycle_task(
            &mut supervisor,
            repository,
            SubscriptionCycleSupervisorConfig::new(1, Duration::from_secs(3_600), 1).unwrap(),
        );

        assert_eq!(supervisor.task_count(), 1);
        assert_eq!(
            supervisor.shutdown(Duration::from_secs(1)).await.unwrap(),
            SupervisorShutdown::Drained
        );
        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn channel_index_refresh_task_is_registered_and_cancellable() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let index = InMemoryChannelIndex::load_from_database(database.clone())
            .await
            .unwrap();
        let shutdown = ShutdownController::new();
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown);
        spawn_channel_index_refresh_task(
            &mut supervisor,
            index,
            ChannelIndexRefreshConfig::new(Duration::from_secs(3_600)).unwrap(),
        );

        assert_eq!(supervisor.task_count(), 1);
        assert_eq!(
            supervisor.shutdown(Duration::from_secs(1)).await.unwrap(),
            SupervisorShutdown::Drained
        );
        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn http_finishes_before_background_shutdown_and_database_close() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let observer = database.clone();
        let shutdown = ShutdownController::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let wal_directory = std::env::temp_dir().join(format!(
            "anyflows-bootstrap-order-{}-{serial}",
            std::process::id()
        ));
        let batcher = FileBillingBatcher::open(
            &wal_directory,
            Arc::new(OrderedBillingSink {
                events: Arc::clone(&events),
            }),
        )
        .unwrap();
        let billing = BillingRuntime::from_batcher(batcher, true, Duration::from_secs(1));
        let (usage_record_queue, _) = UsageRecordQueue::bounded(1).unwrap();
        let (refund_signal_queue, _) = RefundSignalQueue::bounded(1).unwrap();
        billing.record(billing_event()).await.unwrap();
        let started = Arc::new(Notify::new());
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        supervisor.spawn("order-test", {
            let events = Arc::clone(&events);
            let started = Arc::clone(&started);
            move |task_shutdown| {
                let events = Arc::clone(&events);
                let started = Arc::clone(&started);
                async move {
                    started.notify_one();
                    task_shutdown.cancelled().await;
                    events.lock().unwrap().push("background_stopped");
                }
            }
        });
        started.notified().await;

        let http_shutdown = shutdown.clone();
        let http_events = Arc::clone(&events);
        let result = run_http_and_shutdown(
            async move {
                assert!(!http_shutdown.is_triggered());
                http_events.lock().unwrap().push("http_finished");
                Err(ServeError::Io(std::io::Error::other("primary-http-secret")))
            },
            RuntimeShutdownResources {
                shutdown,
                supervisor,
                billing,
                usage_record_queue,
                refund_signal_queue,
                database,
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .await;

        assert!(matches!(result, Err(BootstrapError::Http(_))));
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["http_finished", "background_stopped", "billing_flushed"]
        );
        assert!(observer.ping().await.is_err());
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn shutdown_final_flush_reports_pending_events_before_database_close() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let observer = database.clone();
        let shutdown = ShutdownController::new();
        let supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        let events = Arc::new(Mutex::new(Vec::new()));
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let wal_directory = std::env::temp_dir().join(format!(
            "anyflows-bootstrap-final-flush-{}-{serial}",
            std::process::id()
        ));
        let batcher = FileBillingBatcher::open(
            &wal_directory,
            Arc::new(OrderedBillingSink {
                events: Arc::clone(&events),
            }),
        )
        .unwrap();
        let billing = BillingRuntime::from_batcher(batcher, true, Duration::from_secs(1));
        billing.record(billing_event()).await.unwrap();
        let (usage_record_queue, _) = UsageRecordQueue::bounded(1).unwrap();
        let (refund_signal_queue, _) = RefundSignalQueue::bounded(1).unwrap();

        let report = run_http_and_shutdown(
            async { Ok(ServeOutcome::Drained) },
            RuntimeShutdownResources {
                shutdown,
                supervisor,
                billing,
                usage_record_queue,
                refund_signal_queue,
                database,
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert_eq!(report.billing.confirmed_batches(), 1);
        assert_eq!(report.billing.confirmed_events(), 1);
        assert_eq!(events.lock().unwrap().as_slice(), ["billing_flushed"]);
        assert!(observer.ping().await.is_err());
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn shutdown_drains_usage_queue_after_http_and_before_final_billing_flush() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let observer = database.clone();
        let shutdown = ShutdownController::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let release = Arc::new(Notify::new());
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let wal_directory = std::env::temp_dir().join(format!(
            "anyflows-bootstrap-usage-order-{}-{serial}",
            std::process::id()
        ));
        let batcher = FileBillingBatcher::open(
            &wal_directory,
            Arc::new(OrderedBillingSink {
                events: Arc::clone(&events),
            }),
        )
        .unwrap();
        let billing = BillingRuntime::from_batcher(batcher, true, Duration::from_secs(1));
        billing.record(billing_event()).await.unwrap();

        let (usage_record_queue, receiver) = UsageRecordQueue::bounded(1).unwrap();
        let (refund_signal_queue, _) = RefundSignalQueue::bounded(1).unwrap();
        let usage = usage();
        let mut lifecycle =
            BillingRequestPlan::prepare(Arc::new(RatioPricingResolver::free()), &usage)
                .unwrap()
                .start_free(
                    GatewayPrincipal::new(
                        TokenId::new(1).unwrap(),
                        UserId::new(2).unwrap(),
                        GroupId::new(3).unwrap(),
                    ),
                    BillingReservationId::new([7; 16]).unwrap(),
                    Arc::new(usage_record_queue.clone()),
                )
                .unwrap();
        let _ = lifecycle.complete(usage).await.unwrap();

        let consumer = UsageRecordConsumer::new(
            receiver,
            Arc::new(OrderedUsageSink {
                events: Arc::clone(&events),
                release: Arc::clone(&release),
            }),
            Duration::from_millis(1),
        )
        .unwrap();
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        supervisor.spawn_drainable("usage-order-test", move |_| {
            let consumer = consumer.clone();
            async move { consumer.run().await }
        });

        let http_events = Arc::clone(&events);
        let report = run_http_and_shutdown(
            async move {
                http_events.lock().unwrap().push("http_finished");
                release.notify_one();
                Ok(ServeOutcome::Drained)
            },
            RuntimeShutdownResources {
                shutdown,
                supervisor,
                billing,
                usage_record_queue,
                refund_signal_queue,
                database,
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert_eq!(report.background, SupervisorShutdown::Drained);
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["http_finished", "usage_persisted", "billing_flushed"]
        );
        assert!(observer.ping().await.is_err());
        let _ = fs::remove_dir_all(wal_directory);
    }

    #[tokio::test]
    async fn shutdown_drains_refund_queue_after_http_and_before_final_billing_flush() {
        let database = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let observer = database.clone();
        let shutdown = ShutdownController::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let release = Arc::new(Notify::new());
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let wal_directory = std::env::temp_dir().join(format!(
            "anyflows-bootstrap-refund-order-{}-{serial}",
            std::process::id()
        ));
        let batcher = FileBillingBatcher::open(
            &wal_directory,
            Arc::new(OrderedBillingSink {
                events: Arc::clone(&events),
            }),
        )
        .unwrap();
        let billing = BillingRuntime::from_batcher(batcher, true, Duration::from_secs(1));
        billing.record(billing_event()).await.unwrap();

        let (usage_record_queue, _) = UsageRecordQueue::bounded(1).unwrap();
        let (refund_signal_queue, receiver) = RefundSignalQueue::bounded(1).unwrap();
        assert_eq!(
            refund_signal_queue.try_signal_refund(BillingReservationId::new([8; 16]).unwrap()),
            RefundSignalOutcome::Accepted
        );
        let consumer = RefundSignalConsumer::new(
            receiver,
            Arc::new(OrderedRefundSink {
                events: Arc::clone(&events),
                release: Arc::clone(&release),
            }),
            Duration::from_millis(1),
        )
        .unwrap();
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        supervisor.spawn_drainable("refund-order-test", move |_| {
            let consumer = consumer.clone();
            async move { consumer.run().await }
        });

        let http_events = Arc::clone(&events);
        let report = run_http_and_shutdown(
            async move {
                http_events.lock().unwrap().push("http_finished");
                release.notify_one();
                Ok(ServeOutcome::Drained)
            },
            RuntimeShutdownResources {
                shutdown,
                supervisor,
                billing,
                usage_record_queue,
                refund_signal_queue,
                database,
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert_eq!(report.background, SupervisorShutdown::Drained);
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["http_finished", "refund_persisted", "billing_flushed"]
        );
        assert!(observer.ping().await.is_err());
        let _ = fs::remove_dir_all(wal_directory);
    }

    fn billing_event() -> BillingBatchEvent {
        let mut event_id = [0_u8; 16];
        event_id[15] = 1;
        BillingBatchEvent::new(
            BillingReservationId::new(event_id).unwrap(),
            Some(
                UserBillingDelta::new(
                    UserId::new(1).unwrap(),
                    QuotaDelta::new(-1).unwrap(),
                    QuotaDelta::new(1).unwrap(),
                    1,
                )
                .unwrap(),
            ),
            None,
            None,
        )
        .unwrap()
    }

    fn usage() -> Usage {
        Usage::new(
            TokenCount::new(3).unwrap(),
            TokenCount::new(2).unwrap(),
            UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Estimated,
            UsageSemantics::Inclusive,
        )
        .unwrap()
    }
}
