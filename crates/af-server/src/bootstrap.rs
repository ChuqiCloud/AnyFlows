use std::{future::Future, sync::Arc, time::Duration};

use af_account::{
    CredentialDecryptionError, CredentialDecryptor, CredentialEncryptionError, CredentialEncryptor,
    SystemSecretCipher, SystemSecretError,
};
use af_admin::{
    AdminChannelReader, AdminChannelWriter, AdminEmailSettingsService, AdminGroupReader,
    AdminGroupWriter, AdminModelReader, AdminModelSyncService, AdminModelWriter, AdminTokenReader,
    AdminTokenWriter, AdminUsageLogReader, AdminUserReader, AdminUserWriter,
    DatabaseAdminChannelReader, DatabaseAdminChannelWriter, DatabaseAdminEmailSettingsService,
    DatabaseAdminGroupReader, DatabaseAdminGroupWriter, DatabaseAdminModelReader,
    DatabaseAdminModelSyncService, DatabaseAdminModelWriter, DatabaseAdminTokenReader,
    DatabaseAdminTokenWriter, DatabaseAdminUsageLogReader, DatabaseAdminUserReader,
    DatabaseAdminUserWriter, DatabaseInitialSetup, DatabasePasswordResetService,
    DatabasePlaygroundConversationService, DatabasePlaygroundShareService,
    DatabaseRegistrationService, DatabaseSessionAuthenticator, DatabaseSiteSettingsService,
    DatabaseTokenAuthenticator, DatabaseUserProfileService, DatabaseUserTokenService,
    EmailDelivery, InitialSetup, ModelCatalogReader, PasswordResetService,
    PasswordResetServiceConfigError, PlaygroundConversationService, PlaygroundShareService,
    RegistrationService, RegistrationServiceConfigError, SessionAuthenticator,
    SessionAuthenticatorConfigError, SiteSettingsService, TokenAuthenticator, UserProfileService,
    UserTokenService,
};
use af_analytics::{AdminDashboardReader, DatabaseAdminDashboardReader};
use af_billing::{
    BillingPrechargePort, BillingSettlementPort, CachedRequestPricingSnapshotSource,
    DatabaseUsageRecordSink, GroupPricingCache, GroupPricingCacheError, ModelPriceCache,
    ModelPriceCacheError, RefundSignalConsumer, RefundSignalConsumerError, RefundSignalPort,
    RefundSignalQueue, RefundSignalQueueError, RefundSignalSink, UsageRecordConsumer,
    UsageRecordConsumerError, UsageRecordPort, UsageRecordQueue, UsageRecordQueueError,
};
use af_config::{AppConfig, ConfigError};
use af_db::{
    AdminChannelRepository, AdminChannelRepositoryConfigError, AdminDashboardRepository,
    AdminDashboardRepositoryConfigError, AdminGroupRepository, AdminGroupRepositoryConfigError,
    AdminModelRepository, AdminModelRepositoryConfigError, AdminTokenRepository,
    AdminTokenRepositoryConfigError, AdminUsageLogRepository, AdminUsageLogRepositoryConfigError,
    AdminUserRepository, AdminUserRepositoryConfigError, AuthChallengeRateLimitRepository,
    AuthChallengeRateLimitRepositoryConfigError, AuthChallengeRepository,
    AuthChallengeRepositoryConfigError, ChannelStateRepository, CredentialStateRepository,
    DatabaseError, DatabaseOptions, DatabaseOptionsError, DatabasePool, EmailSettingsRepository,
    EmailSettingsRepositoryConfigError, InitialSetupRepository, InitialSetupRepositoryConfigError,
    MigrationOptions, ModelSyncRepository, PasswordResetRepository,
    PasswordResetRepositoryConfigError, PlaygroundConversationRepository,
    PlaygroundConversationRepositoryConfigError, PlaygroundShareRepository,
    PlaygroundShareRepositoryConfigError, QuotaRepository, RegistrationRepository,
    RegistrationRepositoryConfigError, SiteSettingsRepository, SiteSettingsRepositoryConfigError,
    TokenAuthRepository, TokenAuthRepositoryConfigError, UsageLogRepository, UserProfileRepository,
    UserProfileRepositoryConfigError, UserSessionRepository, UserSessionRepositoryConfigError,
    UserTokenRepository, UserTokenRepositoryConfigError, connect_and_migrate,
};
use af_http::{
    AdminChannelProbe, ChatService, HttpListener, HttpRouter, QueryApiKeyPolicy, ReadinessHandle,
    ServeError, ServeOutcome, build_router, serve_with_graceful_shutdown,
};
use af_httpclient::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_CACHED_CLIENTS, DEFAULT_READ_TIMEOUT,
    DEFAULT_REQUEST_TIMEOUT, HttpClientConfig, HttpClientError, HttpClientProvider, HttpTimeouts,
    ProxyConfig, RemoteDnsPolicy,
};
use af_scheduler::{
    ChannelIndexCacheError, ChannelIndexRefreshConfig, ChannelIndexRefreshSupervisor, ChannelProbe,
    ChannelProbeSupervisor, ChannelProbeSupervisorConfig, InMemoryChannelIndex,
    IndexedWeightedScheduler,
};
use af_telemetry::{MetricsHandle, TelemetryError, TracingHandle, init_metrics, init_tracing};
use thiserror::Error;

use crate::readiness::DatabaseReadinessProbe;
use crate::{
    BackgroundTaskSupervisor, BillingChatService, BillingFlushReport, BillingRuntime,
    BillingRuntimeError, ChatRoutePlanner, ShutdownController, SupervisorError, SupervisorShutdown,
    channel_probe::{BoundedAdminChannelProbe, DatabaseChannelProbe},
    frontend::EmbeddedFrontendAssets,
    model_catalog::RuntimeModelCatalogReader,
    model_discovery::DatabaseUpstreamModelDiscoverer,
    scheduled_chat::ScheduledChatService,
    smtp_delivery::SmtpEmailDelivery,
    system_shutdown_signal,
};

const BILLING_QUEUE_RETRY_DELAY: Duration = Duration::from_millis(100);
/// 为数据库读取和任务调度保留的探活硬截止余量。
const CHANNEL_PROBE_HARD_TIMEOUT_PADDING: Duration = Duration::from_secs(10);

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
}

/// 已使用数据库连接池装配令牌认证服务。
pub struct AuthenticationReady {
    infrastructure: DatabaseReady,
    authenticator: Arc<dyn TokenAuthenticator>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    initial_setup: Arc<dyn InitialSetup>,
    registration_service: Arc<dyn RegistrationService>,
    password_reset_service: Arc<dyn PasswordResetService>,
    user_profile_service: Arc<dyn UserProfileService>,
    site_settings_service: Arc<dyn SiteSettingsService>,
    admin_email_settings_service: Arc<dyn AdminEmailSettingsService>,
    playground_share_service: Arc<dyn PlaygroundShareService>,
    playground_conversation_service: Arc<dyn PlaygroundConversationService>,
    user_token_service: Arc<dyn UserTokenService>,
    admin_channel_reader: Arc<dyn AdminChannelReader>,
    admin_channel_writer: Arc<dyn AdminChannelWriter>,
    admin_dashboard_reader: Arc<dyn AdminDashboardReader>,
    admin_group_reader: Arc<dyn AdminGroupReader>,
    admin_group_writer: Arc<dyn AdminGroupWriter>,
    admin_model_reader: Arc<dyn AdminModelReader>,
    admin_model_writer: Arc<dyn AdminModelWriter>,
    admin_token_reader: Arc<dyn AdminTokenReader>,
    admin_token_writer: Arc<dyn AdminTokenWriter>,
    admin_usage_log_reader: Arc<dyn AdminUsageLogReader>,
    admin_user_reader: Arc<dyn AdminUserReader>,
    admin_user_writer: Arc<dyn AdminUserWriter>,
    query_api_key_policy: QueryApiKeyPolicy,
}

/// 生产转发共用的受控 HTTP Client 已完成安全配置与装配。
pub struct RelayReady {
    infrastructure: AuthenticationReady,
    upstream_clients: HttpClientProvider,
}

/// 计费 WAL、定价缓存、额度仓储及内部可靠队列均已完成装配。
pub struct BillingReady {
    infrastructure: AuthenticationReady,
    chat: Arc<dyn ChatService>,
    billing: BillingRuntime,
    usage_record_queue: UsageRecordQueue,
    usage_record_consumer: UsageRecordConsumer,
    usage_record_worker_count: usize,
    refund_signal_queue: RefundSignalQueue,
    refund_signal_consumer: RefundSignalConsumer,
    upstream_clients: HttpClientProvider,
    channel_index: InMemoryChannelIndex,
    model_catalog_reader: Arc<dyn ModelCatalogReader>,
    admin_model_sync_service: Arc<dyn AdminModelSyncService>,
}

/// 后台任务监督域已经建立，周期 flush、渠道索引、渠道探活与可靠队列 worker 已注册。
pub struct Supervised {
    infrastructure: AuthenticationReady,
    chat: Arc<dyn ChatService>,
    billing: BillingRuntime,
    usage_record_queue: UsageRecordQueue,
    refund_signal_queue: RefundSignalQueue,
    _channel_index: InMemoryChannelIndex,
    model_catalog_reader: Arc<dyn ModelCatalogReader>,
    admin_model_sync_service: Arc<dyn AdminModelSyncService>,
    admin_channel_probe: Arc<dyn AdminChannelProbe>,
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
    /// 用户会话仓储的启动参数无效。
    #[error("用户会话启动参数无效")]
    SessionRepository(#[source] UserSessionRepositoryConfigError),
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
    /// SMTP 设置仓储的启动参数无效。
    #[error("SMTP 设置仓储启动参数无效")]
    EmailSettingsRepository(#[source] EmailSettingsRepositoryConfigError),
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
    /// 普通用户资料仓储的启动参数无效。
    #[error("用户资料仓储启动参数无效")]
    UserProfileRepository(#[source] UserProfileRepositoryConfigError),
    /// 管理员分组仓储的启动参数无效。
    #[error("管理员分组仓储启动参数无效")]
    AdminGroupRepository(#[source] AdminGroupRepositoryConfigError),
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
    /// 管理员渠道仓储的启动参数无效。
    #[error("管理员渠道仓储启动参数无效")]
    AdminChannelRepository(#[source] AdminChannelRepositoryConfigError),
    /// HTTP 监听、服务或强制关闭失败。
    #[error("HTTP 服务生命周期操作失败")]
    Http(#[source] ServeError),
    /// 受控上游 HTTP Client 配置或构造失败。
    #[error("初始化上游 HTTP Client 失败")]
    HttpClient(#[source] HttpClientError),
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

impl Bootstrap<Configured> {
    /// 从已加载配置创建第一阶段；仍会重新校验，避免嵌入式调用绕过约束。
    pub fn new(config: AppConfig) -> Result<Self, BootstrapError> {
        config.validate().map_err(BootstrapError::Config)?;
        let shutdown_timeout = Duration::from_secs(config.server().shutdown_timeout_secs());
        Ok(Self {
            config,
            shutdown_timeout,
            shutdown: ShutdownController::new(),
            stage: Configured,
        })
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
        let database = connect_and_migrate(&database_options, migration_options)
            .await
            .map_err(BootstrapError::Database)?;
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: DatabaseReady {
                _telemetry: stage,
                database,
            },
        })
    }
}

impl Bootstrap<DatabaseReady> {
    /// 使用同一数据库连接池装配令牌认证，并固定查询截止时间与 carrier 策略。
    pub fn init_authentication(self) -> Result<Bootstrap<AuthenticationReady>, BootstrapError> {
        let auth = self.config.auth();
        let repository = TokenAuthRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::Authentication)?;
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
        let site_settings_repository = SiteSettingsRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::SiteSettingsRepository)?;
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
        let user_profile_repository = UserProfileRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::UserProfileRepository)?;
        let system_secret_cipher = SystemSecretCipher::new(self.config.credential_encryption())
            .map_err(BootstrapError::SystemSecret)?;
        let smtp_delivery: Arc<dyn EmailDelivery> = Arc::new(SmtpEmailDelivery);
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
            auth_challenge_rate_limit_repository,
            email_settings_repository.clone(),
            system_secret_cipher.clone(),
            Arc::clone(&smtp_delivery),
            auth.session_signing_key().map(|key| key.expose()),
        )
        .map_err(BootstrapError::RegistrationService)?;
        let admin_email_settings_service = DatabaseAdminEmailSettingsService::new(
            email_settings_repository,
            system_secret_cipher,
            smtp_delivery,
        );
        let admin_group_repository = AdminGroupRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminGroupRepository)?;
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
        let admin_dashboard_repository = AdminDashboardRepository::new(
            self.stage.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::AdminDashboardRepository)?;
        let credential_encryptor = CredentialEncryptor::new(self.config.credential_encryption())
            .map_err(BootstrapError::CredentialEncryption)?;
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
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: AuthenticationReady {
                infrastructure: stage,
                authenticator: Arc::new(DatabaseTokenAuthenticator::new(repository)),
                session_authenticator: Arc::new(session_authenticator),
                initial_setup: Arc::new(DatabaseInitialSetup::new(initial_setup_repository)),
                registration_service: Arc::new(registration_service),
                password_reset_service: Arc::new(password_reset_service),
                user_profile_service: Arc::new(DatabaseUserProfileService::new(
                    user_profile_repository,
                )),
                site_settings_service: Arc::new(DatabaseSiteSettingsService::new(
                    site_settings_repository,
                )),
                admin_email_settings_service: Arc::new(admin_email_settings_service),
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
                admin_channel_writer: Arc::new(DatabaseAdminChannelWriter::new(
                    admin_channel_repository,
                    credential_encryptor,
                )),
                admin_dashboard_reader: Arc::new(DatabaseAdminDashboardReader::new(
                    admin_dashboard_repository,
                )),
                admin_group_reader: Arc::new(DatabaseAdminGroupReader::new(
                    admin_group_repository.clone(),
                )),
                admin_group_writer: Arc::new(DatabaseAdminGroupWriter::new(admin_group_repository)),
                admin_model_reader: Arc::new(DatabaseAdminModelReader::new(
                    admin_model_repository.clone(),
                )),
                admin_model_writer: Arc::new(DatabaseAdminModelWriter::new(admin_model_repository)),
                admin_token_reader: Arc::new(DatabaseAdminTokenReader::new(
                    admin_token_repository.clone(),
                )),
                admin_token_writer: Arc::new(DatabaseAdminTokenWriter::new(admin_token_repository)),
                admin_usage_log_reader: Arc::new(DatabaseAdminUsageLogReader::new(
                    admin_usage_log_repository,
                )),
                admin_user_reader: Arc::new(DatabaseAdminUserReader::new(
                    admin_user_repository.clone(),
                )),
                admin_user_writer: Arc::new(DatabaseAdminUserWriter::new(admin_user_repository)),
                query_api_key_policy,
            },
        })
    }
}

impl Bootstrap<AuthenticationReady> {
    /// 使用启动配置建立生产转发共用的受控 HTTP Client。
    pub fn init_relay(self) -> Result<Bootstrap<RelayReady>, BootstrapError> {
        let upstream_clients = build_http_clients(&self.config)?;
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: RelayReady {
                infrastructure: stage,
                upstream_clients,
            },
        })
    }
}

impl Bootstrap<RelayReady> {
    /// 打开计费 WAL、加载定价缓存并装配生产预扣、结算与内部补偿链路。
    pub async fn init_billing(self) -> Result<Bootstrap<BillingReady>, BootstrapError> {
        let decryptor = CredentialDecryptor::new(self.config.credential_encryption())
            .map_err(BootstrapError::CredentialDecryption)?;
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
        let channel_index = InMemoryChannelIndex::load_from_database(
            self.stage.infrastructure.infrastructure.database.clone(),
        )
        .await
        .map_err(BootstrapError::ChannelIndex)?;
        let route_planner: Arc<dyn ChatRoutePlanner> = Arc::new(ScheduledChatService::new(
            IndexedWeightedScheduler::new(channel_index.clone()),
            decryptor,
            self.stage.upstream_clients.clone(),
            CredentialStateRepository::new(
                self.stage.infrastructure.infrastructure.database.clone(),
            ),
        ));
        let pricing_source = Arc::new(CachedRequestPricingSnapshotSource::new(
            model_prices.clone(),
            group_pricing,
        ));
        let model_catalog_reader: Arc<dyn ModelCatalogReader> =
            Arc::new(RuntimeModelCatalogReader::new(
                channel_index.clone(),
                model_prices,
                pricing_source.clone(),
            ));
        let quota_repository = Arc::new(QuotaRepository::new(
            self.stage.infrastructure.infrastructure.database.clone(),
        ));
        let precharge_port: Arc<dyn BillingPrechargePort> = quota_repository.clone();
        let settlement_port: Arc<dyn BillingSettlementPort> = quota_repository.clone();
        let refund_sink: Arc<dyn RefundSignalSink> = quota_repository.clone();
        let refund_port: Arc<dyn RefundSignalPort> = Arc::new(refund_signal_queue.clone());
        let usage_port: Arc<dyn UsageRecordPort> = Arc::new(usage_record_queue.clone());
        let usage_record_consumer = UsageRecordConsumer::new(
            usage_record_receiver,
            Arc::new(DatabaseUsageRecordSink::new(UsageLogRepository::new(
                self.stage.infrastructure.infrastructure.database.clone(),
            ))),
            BILLING_QUEUE_RETRY_DELAY,
        )
        .map_err(BootstrapError::UsageRecordConsumer)?;
        let refund_signal_consumer = RefundSignalConsumer::new(
            refund_signal_receiver,
            refund_sink,
            BILLING_QUEUE_RETRY_DELAY,
        )
        .map_err(BootstrapError::RefundSignalConsumer)?;
        let usage_record_worker_count = self.config.billing().usage_record_worker_count();
        let chat: Arc<dyn ChatService> = Arc::new(BillingChatService::new(
            route_planner,
            pricing_source,
            Some(precharge_port),
            Some(refund_port),
            Some(settlement_port),
            usage_port,
        ));
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Ok(Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage: BillingReady {
                infrastructure: stage.infrastructure,
                chat,
                billing,
                usage_record_queue,
                usage_record_consumer,
                usage_record_worker_count,
                refund_signal_queue,
                refund_signal_consumer,
                upstream_clients: stage.upstream_clients,
                channel_index,
                model_catalog_reader,
                admin_model_sync_service,
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
            stage,
        } = self;
        let mut supervisor = BackgroundTaskSupervisor::new(shutdown.clone());
        let BillingReady {
            infrastructure,
            chat,
            billing,
            usage_record_queue,
            usage_record_consumer,
            usage_record_worker_count,
            refund_signal_queue,
            refund_signal_consumer,
            upstream_clients,
            channel_index,
            model_catalog_reader,
            admin_model_sync_service,
        } = stage;
        spawn_channel_index_refresh_task(
            &mut supervisor,
            channel_index.clone(),
            ChannelIndexRefreshConfig::default(),
        );
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
            stage: Supervised {
                infrastructure,
                chat,
                billing,
                usage_record_queue,
                refund_signal_queue,
                _channel_index: channel_index,
                model_catalog_reader,
                admin_model_sync_service,
                admin_channel_probe,
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
    // 渠道覆盖由探活实现自身执行；外层只兜底失控实现和数据库读取停滞。
    let hard_timeout = std::cmp::max(configured_default, maximum_channel_timeout)
        .checked_add(CHANNEL_PROBE_HARD_TIMEOUT_PADDING)
        .expect("渠道探活硬截止必须可表示");
    ChannelProbeSupervisorConfig::new(
        settings.batch_size(),
        Duration::from_secs(settings.interval_secs()),
        hard_timeout,
    )
    .expect("已校验的渠道探活配置必须满足调度层边界")
}

impl Bootstrap<Supervised> {
    /// 构建完整 HTTP Router；此后只允许进入监听阶段。
    #[must_use]
    pub fn build_router(self) -> Bootstrap<RouterReady> {
        let probe =
            DatabaseReadinessProbe::new(self.stage.infrastructure.infrastructure.database.clone());
        let readiness = ReadinessHandle::new(probe);
        let router = build_router(
            self.config.server(),
            self.stage.chat.clone(),
            readiness.clone(),
            Arc::clone(&self.stage.infrastructure.authenticator),
            Arc::clone(&self.stage.infrastructure.session_authenticator),
            Arc::clone(&self.stage.model_catalog_reader),
            Arc::clone(&self.stage.infrastructure.playground_share_service),
            Arc::clone(&self.stage.infrastructure.playground_conversation_service),
            Arc::clone(&self.stage.infrastructure.user_token_service),
            Arc::clone(&self.stage.infrastructure.initial_setup),
            Arc::clone(&self.stage.infrastructure.registration_service),
            Arc::clone(&self.stage.infrastructure.password_reset_service),
            Arc::clone(&self.stage.infrastructure.user_profile_service),
            Arc::clone(&self.stage.infrastructure.site_settings_service),
            Arc::clone(&self.stage.infrastructure.admin_email_settings_service),
            Arc::clone(&self.stage.infrastructure.admin_channel_reader),
            Arc::clone(&self.stage.infrastructure.admin_channel_writer),
            Some(Arc::clone(&self.stage.admin_channel_probe)),
            Arc::clone(&self.stage.infrastructure.admin_group_reader),
            Arc::clone(&self.stage.infrastructure.admin_group_writer),
            Some(Arc::clone(&self.stage.infrastructure.admin_model_reader)),
            Some(Arc::clone(&self.stage.infrastructure.admin_model_writer)),
            Some(Arc::clone(&self.stage.admin_model_sync_service)),
            Arc::clone(&self.stage.infrastructure.admin_token_reader),
            Arc::clone(&self.stage.infrastructure.admin_token_writer),
            Arc::clone(&self.stage.infrastructure.admin_dashboard_reader),
            Arc::clone(&self.stage.infrastructure.admin_usage_log_reader),
            Arc::clone(&self.stage.infrastructure.admin_user_reader),
            Arc::clone(&self.stage.infrastructure.admin_user_writer),
            self.stage.infrastructure.query_api_key_policy,
            Some(Arc::new(EmbeddedFrontendAssets)),
        );
        let Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
            stage,
        } = self;
        Bootstrap {
            config,
            shutdown_timeout,
            shutdown,
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
            _channel_index: _,
            model_catalog_reader: _,
            admin_channel_probe: _,
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

fn build_http_clients(config: &AppConfig) -> Result<HttpClientProvider, BootstrapError> {
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
    HttpClientProvider::new(
        client_config,
        settings
            .max_cached_clients()
            .unwrap_or(DEFAULT_MAX_CACHED_CLIENTS),
    )
    .map_err(BootstrapError::HttpClient)
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

    use af_billing::{
        BillingBatch, BillingBatchApplyOutcome, BillingBatchEvent, BillingBatchSink,
        BillingBatchSinkFuture, BillingRequestPlan, BillingUsageRecord, FileBillingBatcher,
        RatioPricingResolver, RefundSignalConsumer, RefundSignalOutcome, RefundSignalPort,
        RefundSignalQueue, RefundSignalSink, RefundSignalSinkFuture, UsageRecordConsumer,
        UsageRecordQueue, UsageRecordSink, UsageRecordSinkFuture, UserBillingDelta,
    };
    use af_config::CREDENTIAL_ENCRYPTION_KEY_BYTES;
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

        let report = Bootstrap::new(config)
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
            .unwrap()
            .start_supervisor()
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
        let error = match bootstrap.init_authentication() {
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
