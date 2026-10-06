use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use af_account::{
    CredentialDecryptor, CredentialEncryptionError, CredentialEncryptor,
    OAUTH_REFRESH_LEASE_NAMESPACE, OAuthAuthorizationCallback, OAuthAuthorizationContext,
    OAuthAuthorizationError, OAuthAuthorizationSessionStore, OAuthConnectionCoordinator,
    OAuthConnectionError, OAuthConnectionOutcome, OAuthProviderProfile, OAuthProviderProfileError,
    OAuthRefreshCoordinator, OAuthRefreshCoordinatorError, OAuthRefreshPersistenceService,
    OAuthRefreshSupervisor, OAuthRefreshSupervisorConfig, OAuthRefreshSupervisorConfigError,
    OAuthTokenExchangeError, OAuthTokenPersistenceError, OAuthTokenPersistenceService,
    UpstreamOAuthProvider,
};
use af_admin::{AdminChannelReadError, AdminChannelReader, SessionPrincipal, SessionRole};
use af_cache::{CacheError, DistributedLeaseConfig, DistributedLeaseManager, RedisConfig};
use af_config::AppConfig;
use af_db::{DatabasePool, OAuthCredentialRepository};
use af_domain::CredentialKind;
use af_http::{
    AdminOAuthAuthorizationStart, AdminOAuthBeginFuture, AdminOAuthCompleteFuture,
    AdminOAuthCompletionOutcome, AdminOAuthConnectionError, AdminOAuthConnectionService,
    AdminOAuthProvider, AdminOAuthProviderStatus, HttpListener, HttpRouter, OAuthLoopbackBinding,
    OAuthLoopbackBindingError, ServeError, build_oauth_loopback_callback_router,
    serve_with_graceful_shutdown,
};
use af_httpclient::HttpClientProvider;
use thiserror::Error;
use url::Url;
use zeroize::Zeroizing;

use crate::{BackgroundTaskSupervisor, ShutdownController};

const LOOPBACK_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// OAuth 生产装配错误；不携带客户端材料或回调查询。
#[derive(Debug, Error)]
pub enum OAuthRuntimeConfigError {
    /// provider profile 配置无法形成闭合合约。
    #[error("OAuth provider profile 配置无效")]
    Profile(#[source] OAuthProviderProfileError),
    /// 待授权会话容量或 TTL 无效。
    #[error("OAuth 授权会话配置无效")]
    Session(#[source] OAuthAuthorizationError),
    /// 凭据加密密钥无法初始化。
    #[error("OAuth 凭据加密配置无效")]
    Encryption(#[source] CredentialEncryptionError),
    /// provider 集合或协调器装配失败。
    #[error("OAuth 协调器配置无效")]
    Coordinator(#[source] OAuthConnectionError),
    /// account 与 HTTP 层的 loopback 合约不一致。
    #[error("OAuth loopback 合约无效")]
    Loopback(#[source] OAuthLoopbackBindingError),
    /// token 刷新周期、批次或并发配置无法形成有界任务。
    #[error("OAuth 刷新任务配置无效")]
    RefreshSupervisor(#[source] OAuthRefreshSupervisorConfigError),
    /// 刷新协调器无法使用共享 Provider profile 完成装配。
    #[error("OAuth 刷新协调器配置无效")]
    RefreshCoordinator(#[source] OAuthRefreshCoordinatorError),
    /// Redis 或本地刷新租约管理器无法在启动期完成装配。
    #[error("OAuth 刷新租约配置不可用")]
    RefreshLease(#[source] CacheError),
}

/// 已经预绑定端口、等待注册到后台监督域的回调服务。
pub(crate) struct PreparedOAuthLoopbackServer {
    provider: AdminOAuthProvider,
    listener: HttpListener,
    router: HttpRouter,
}

/// 已装配的连接向导运行时，以及供刷新任务复用的同一批 Provider profile。
pub(crate) struct PreparedOAuthRuntime {
    pub(crate) admin_connection_service: Arc<dyn AdminOAuthConnectionService>,
    pub(crate) connection_coordinator: Arc<OAuthConnectionCoordinator>,
    pub(crate) loopback_servers: Vec<PreparedOAuthLoopbackServer>,
    pub(crate) profiles: Vec<Arc<OAuthProviderProfile>>,
}

/// 生产管理 OAuth 服务；目标预检和管理员边界在创建授权会话前完成。
struct RuntimeAdminOAuthConnectionService {
    coordinator: Arc<OAuthConnectionCoordinator>,
    channel_reader: Arc<dyn AdminChannelReader>,
    bindings: Vec<OAuthLoopbackBinding>,
    listener_ready: AtomicU8,
}

impl RuntimeAdminOAuthConnectionService {
    fn new(
        coordinator: Arc<OAuthConnectionCoordinator>,
        channel_reader: Arc<dyn AdminChannelReader>,
        bindings: Vec<OAuthLoopbackBinding>,
    ) -> Self {
        Self {
            coordinator,
            channel_reader,
            bindings,
            listener_ready: AtomicU8::new(0),
        }
    }

    fn bindings(&self) -> Vec<OAuthLoopbackBinding> {
        self.bindings.clone()
    }

    fn mark_listener_ready(&self, provider: AdminOAuthProvider) {
        self.listener_ready
            .fetch_or(provider_bit(provider), Ordering::Release);
    }

    fn listener_ready(&self, provider: AdminOAuthProvider) -> bool {
        self.listener_ready.load(Ordering::Acquire) & provider_bit(provider) != 0
    }

    fn binding(&self, provider: AdminOAuthProvider) -> Option<&OAuthLoopbackBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.provider() == provider)
    }
}

impl AdminOAuthConnectionService for RuntimeAdminOAuthConnectionService {
    fn provider_statuses(
        &self,
        principal: SessionPrincipal,
    ) -> Result<Vec<AdminOAuthProviderStatus>, AdminOAuthConnectionError> {
        require_admin(principal)?;
        Ok(self
            .bindings
            .iter()
            .map(|binding| {
                AdminOAuthProviderStatus::new(
                    binding.provider(),
                    binding.redirect_uri().to_string(),
                    binding.bind_address().port(),
                    binding.callback_path().to_owned(),
                    self.listener_ready(binding.provider()),
                )
            })
            .collect())
    }

    fn begin<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: af_domain::ChannelId,
        credential_id: af_domain::CredentialId,
        provider: AdminOAuthProvider,
    ) -> AdminOAuthBeginFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            if self.binding(provider).is_none() {
                return Err(report_oauth_failure(
                    provider,
                    "begin",
                    AdminOAuthConnectionError::ProviderNotConfigured,
                ));
            }
            let credential = self
                .channel_reader
                .get_credential(principal, channel_id, credential_id)
                .await
                .map_err(|error| {
                    report_oauth_failure(provider, "begin", map_channel_read_error(error))
                })?;
            if credential.kind() != CredentialKind::Oauth {
                return Err(report_oauth_failure(
                    provider,
                    "begin",
                    AdminOAuthConnectionError::TargetKindMismatch,
                ));
            }
            let account_provider = account_provider(provider);
            if credential
                .oauth_provider()
                .is_some_and(|bound| bound != account_provider.as_str())
            {
                return Err(report_oauth_failure(
                    provider,
                    "begin",
                    AdminOAuthConnectionError::CredentialProviderMismatch,
                ));
            }
            let context = OAuthAuthorizationContext::new(
                principal.user_id(),
                Some(channel_id),
                Some(credential_id),
            )
            .map_err(|_| {
                report_oauth_failure(provider, "begin", AdminOAuthConnectionError::Internal)
            })?;
            let start = self
                .coordinator
                .begin(account_provider, context)
                .map_err(|error| {
                    report_oauth_failure(provider, "begin", map_connection_error(error))
                })?;
            let expires_in_seconds = start
                .expires_at()
                .saturating_duration_since(Instant::now())
                .as_secs()
                .max(1);
            Ok(AdminOAuthAuthorizationStart::new(
                provider,
                start.authorization_url().to_string(),
                start.redirect_uri().to_string(),
                expires_in_seconds,
                self.listener_ready(provider),
            ))
        })
    }

    fn complete_manual<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: AdminOAuthProvider,
        callback_url: String,
    ) -> AdminOAuthCompleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            if self.binding(provider).is_none() {
                return Err(report_oauth_failure(
                    provider,
                    "manual_callback",
                    AdminOAuthConnectionError::ProviderNotConfigured,
                ));
            }
            let callback = parse_callback(provider, callback_url)
                .map_err(|error| report_oauth_failure(provider, "manual_callback", error))?;
            let outcome = self
                .coordinator
                .complete_for(principal.user_id(), callback)
                .await
                .map_err(|error| {
                    report_oauth_failure(provider, "manual_callback", map_connection_error(error))
                })?;
            Ok(map_connection_outcome(outcome))
        })
    }

    fn complete_loopback<'a>(
        &'a self,
        provider: AdminOAuthProvider,
        callback_url: String,
    ) -> AdminOAuthCompleteFuture<'a> {
        Box::pin(async move {
            if self.binding(provider).is_none() {
                return Err(report_oauth_failure(
                    provider,
                    "loopback_callback",
                    AdminOAuthConnectionError::ProviderNotConfigured,
                ));
            }
            let callback = parse_callback(provider, callback_url)
                .map_err(|error| report_oauth_failure(provider, "loopback_callback", error))?;
            let outcome = self.coordinator.complete(callback).await.map_err(|error| {
                report_oauth_failure(provider, "loopback_callback", map_connection_error(error))
            })?;
            Ok(map_connection_outcome(outcome))
        })
    }
}

/// 装配协调器、管理端口并预绑定所有未冲突的 provider 回调端口。
pub(crate) fn initialize_oauth_runtime(
    config: &AppConfig,
    database: DatabasePool,
    channel_reader: Arc<dyn AdminChannelReader>,
    http_clients: HttpClientProvider,
) -> Result<PreparedOAuthRuntime, OAuthRuntimeConfigError> {
    let profiles = build_profiles(config)?;
    let sessions = OAuthAuthorizationSessionStore::new(
        config.oauth().max_pending_authorizations(),
        Duration::from_secs(config.oauth().session_ttl_secs()),
    )
    .map_err(OAuthRuntimeConfigError::Session)?;
    let persistence = OAuthTokenPersistenceService::new(
        CredentialEncryptor::new(config.credential_encryption())
            .map_err(OAuthRuntimeConfigError::Encryption)?,
        OAuthCredentialRepository::new(database),
    );
    let coordinator = OAuthConnectionCoordinator::new(
        profiles.iter().cloned(),
        sessions,
        http_clients,
        persistence,
    )
    .map_err(OAuthRuntimeConfigError::Coordinator)?;
    let coordinator = Arc::new(coordinator);
    let bindings = coordinator
        .configured_loopback_redirects()
        .into_iter()
        .map(|redirect| {
            OAuthLoopbackBinding::new(
                http_provider(redirect.provider()),
                redirect.bind_address(),
                redirect.redirect_uri().to_string(),
            )
            .map_err(OAuthRuntimeConfigError::Loopback)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let service = Arc::new(RuntimeAdminOAuthConnectionService::new(
        Arc::clone(&coordinator),
        channel_reader,
        bindings,
    ));
    let servers = prepare_loopback_servers(config.server().bind(), Arc::clone(&service));
    let service: Arc<dyn AdminOAuthConnectionService> = service;
    Ok(PreparedOAuthRuntime {
        admin_connection_service: service,
        connection_coordinator: coordinator,
        loopback_servers: servers,
        profiles,
    })
}

/// 使用共享 Provider profile 装配 OAuth 周期刷新、持久化与分布式租约。
pub(crate) async fn initialize_oauth_refresh_supervisor(
    config: &AppConfig,
    database: DatabasePool,
    profiles: Vec<Arc<OAuthProviderProfile>>,
    http_clients: HttpClientProvider,
    decryptor: CredentialDecryptor,
) -> Result<Option<OAuthRefreshSupervisor>, OAuthRuntimeConfigError> {
    if !config.oauth().refresh_enabled() {
        return Ok(None);
    }
    let settings = config.oauth();
    let supervisor_config = OAuthRefreshSupervisorConfig::new(
        settings.refresh_batch_size(),
        settings.refresh_concurrency(),
        Duration::from_secs(settings.refresh_interval_secs()),
        Duration::from_secs(settings.refresh_before_expiry_secs()),
    )
    .map_err(OAuthRuntimeConfigError::RefreshSupervisor)?;
    let persistence = Arc::new(OAuthRefreshPersistenceService::new(
        CredentialEncryptor::new(config.credential_encryption())
            .map_err(OAuthRuntimeConfigError::Encryption)?,
        decryptor,
        OAuthCredentialRepository::new(database),
    ));
    let mut lease_config = DistributedLeaseConfig::new(OAUTH_REFRESH_LEASE_NAMESPACE)
        .expect("固定 OAuth 刷新租约命名空间必须有效");
    if let Some(redis_url) = config.redis().url() {
        lease_config = lease_config.with_redis(RedisConfig::new(redis_url.expose().to_owned()));
    }
    let leases = DistributedLeaseManager::new(lease_config)
        .await
        .map_err(OAuthRuntimeConfigError::RefreshLease)?;
    let coordinator = Arc::new(
        OAuthRefreshCoordinator::new_with_distributed_lease(
            profiles,
            http_clients,
            Arc::clone(&persistence),
            leases,
        )
        .map_err(OAuthRuntimeConfigError::RefreshCoordinator)?,
    );
    Ok(Some(OAuthRefreshSupervisor::new(
        persistence,
        coordinator,
        supervisor_config,
    )))
}

fn build_profiles(
    config: &AppConfig,
) -> Result<Vec<Arc<OAuthProviderProfile>>, OAuthRuntimeConfigError> {
    let mut profiles = Vec::with_capacity(config.oauth().configured_provider_count() + 1);
    if let Some(client) = config.oauth().claude_code() {
        profiles.push(Arc::new(
            OAuthProviderProfile::new(
                UpstreamOAuthProvider::ClaudeCode,
                client.client_id().expose().to_owned(),
                None,
            )
            .map_err(OAuthRuntimeConfigError::Profile)?,
        ));
    }
    // Codex 的端点、客户端和回调均属于 OpenAI 固定协议，始终可用；旧配置字段
    // 仅为兼容保留，不再决定 provider 是否注册。
    profiles.push(Arc::new(
        OAuthProviderProfile::new(
            UpstreamOAuthProvider::Codex,
            af_account::CODEX_CLIENT_ID.to_owned(),
            None,
        )
        .map_err(OAuthRuntimeConfigError::Profile)?,
    ));
    for (provider, client) in [
        (UpstreamOAuthProvider::Gemini, config.oauth().gemini()),
        (
            UpstreamOAuthProvider::Antigravity,
            config.oauth().antigravity(),
        ),
    ] {
        if let Some(client) = client {
            profiles.push(Arc::new(
                OAuthProviderProfile::new(
                    provider,
                    client.client_id().expose().to_owned(),
                    Some(client.client_secret().expose().to_owned()),
                )
                .map_err(OAuthRuntimeConfigError::Profile)?,
            ));
        }
    }
    Ok(profiles)
}

fn prepare_loopback_servers(
    main_bind: SocketAddr,
    service: Arc<RuntimeAdminOAuthConnectionService>,
) -> Vec<PreparedOAuthLoopbackServer> {
    let mut servers = Vec::with_capacity(service.bindings.len());
    for binding in service.bindings() {
        let provider = binding.provider();
        if binding.bind_address() == main_bind {
            tracing::warn!(
                provider = provider.as_str(),
                listener_state = "unavailable",
                error_kind = "main_listener_conflict",
                "OAuth 自动回调端口与主服务冲突，保留手动回调路径"
            );
            continue;
        }
        let listener = match HttpListener::bind(binding.bind_address()) {
            Ok(listener) => listener,
            Err(_) => {
                tracing::warn!(
                    provider = provider.as_str(),
                    listener_state = "unavailable",
                    error_kind = "bind_failed",
                    "OAuth 自动回调监听失败，保留手动回调路径"
                );
                continue;
            }
        };
        service.mark_listener_ready(provider);
        let service_port: Arc<dyn AdminOAuthConnectionService> = service.clone();
        let router = build_oauth_loopback_callback_router(service_port, binding);
        servers.push(PreparedOAuthLoopbackServer {
            provider,
            listener,
            router,
        });
    }
    servers
}

/// 把预绑定 listener 注册到统一监督与关闭域。
pub(crate) fn register_oauth_loopback_servers(
    supervisor: &mut BackgroundTaskSupervisor,
    servers: Vec<PreparedOAuthLoopbackServer>,
) {
    for server in servers {
        let provider = server.provider;
        let listener = Arc::new(server.listener);
        let router = server.router;
        supervisor.spawn("oauth-loopback-callback", move |task_shutdown| {
            let listener = listener.try_clone();
            let router = router.clone();
            async move {
                let listener = match listener {
                    Ok(listener) => listener,
                    Err(_) => {
                        tracing::error!(
                            provider = provider.as_str(),
                            error_kind = "listener_clone_failed",
                            "OAuth 回调监听器重建失败"
                        );
                        return;
                    }
                };
                if let Err(error) = serve_with_graceful_shutdown(
                    listener,
                    router,
                    shutdown_future(task_shutdown),
                    LOOPBACK_DRAIN_TIMEOUT,
                )
                .await
                {
                    tracing::error!(
                        provider = provider.as_str(),
                        error_kind = serve_error_kind(&error),
                        "OAuth 回调服务循环失败"
                    );
                }
            }
        });
    }
}

async fn shutdown_future(shutdown: ShutdownController) {
    shutdown.cancelled().await;
}

fn parse_callback(
    provider: AdminOAuthProvider,
    callback_url: String,
) -> Result<OAuthAuthorizationCallback, AdminOAuthConnectionError> {
    let callback_url = Zeroizing::new(callback_url);
    if callback_url.is_empty() || callback_url.trim() != callback_url.as_str() {
        return Err(AdminOAuthConnectionError::InvalidInput);
    }
    let url = Url::parse(&callback_url).map_err(|_| AdminOAuthConnectionError::InvalidInput)?;
    OAuthAuthorizationCallback::from_redirect_url(account_provider(provider), url)
        .map_err(map_authorization_error)
}

const fn account_provider(provider: AdminOAuthProvider) -> UpstreamOAuthProvider {
    match provider {
        AdminOAuthProvider::ClaudeCode => UpstreamOAuthProvider::ClaudeCode,
        AdminOAuthProvider::Codex => UpstreamOAuthProvider::Codex,
        AdminOAuthProvider::Gemini => UpstreamOAuthProvider::Gemini,
        AdminOAuthProvider::Antigravity => UpstreamOAuthProvider::Antigravity,
    }
}

const fn http_provider(provider: UpstreamOAuthProvider) -> AdminOAuthProvider {
    match provider {
        UpstreamOAuthProvider::ClaudeCode => AdminOAuthProvider::ClaudeCode,
        UpstreamOAuthProvider::Codex => AdminOAuthProvider::Codex,
        UpstreamOAuthProvider::Gemini => AdminOAuthProvider::Gemini,
        UpstreamOAuthProvider::Antigravity => AdminOAuthProvider::Antigravity,
    }
}

const fn provider_bit(provider: AdminOAuthProvider) -> u8 {
    match provider {
        AdminOAuthProvider::ClaudeCode => 1 << 0,
        AdminOAuthProvider::Codex => 1 << 1,
        AdminOAuthProvider::Gemini => 1 << 2,
        AdminOAuthProvider::Antigravity => 1 << 3,
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminOAuthConnectionError> {
    if principal.role() != SessionRole::Admin {
        return Err(AdminOAuthConnectionError::Forbidden);
    }
    Ok(())
}

fn map_channel_read_error(error: AdminChannelReadError) -> AdminOAuthConnectionError {
    match error {
        AdminChannelReadError::Forbidden => AdminOAuthConnectionError::Forbidden,
        AdminChannelReadError::ChannelNotFound | AdminChannelReadError::CredentialNotFound => {
            AdminOAuthConnectionError::TargetNotFound
        }
        AdminChannelReadError::InvalidPagination | AdminChannelReadError::Internal => {
            AdminOAuthConnectionError::Internal
        }
    }
}

fn map_connection_outcome(outcome: OAuthConnectionOutcome) -> AdminOAuthCompletionOutcome {
    match outcome {
        OAuthConnectionOutcome::Connected => AdminOAuthCompletionOutcome::Connected,
        OAuthConnectionOutcome::TargetNotFound => AdminOAuthCompletionOutcome::TargetNotFound,
        OAuthConnectionOutcome::CredentialProviderMismatch => {
            AdminOAuthCompletionOutcome::CredentialProviderMismatch
        }
    }
}

fn map_connection_error(error: OAuthConnectionError) -> AdminOAuthConnectionError {
    match error {
        OAuthConnectionError::ProviderProfileNotConfigured { .. } => {
            AdminOAuthConnectionError::ProviderNotConfigured
        }
        OAuthConnectionError::HttpClientUnavailable => AdminOAuthConnectionError::Unavailable,
        OAuthConnectionError::Authorization(error) => map_authorization_error(error),
        OAuthConnectionError::TokenExchange(error) => map_exchange_error(error),
        OAuthConnectionError::Persistence(error) => map_persistence_error(error),
        OAuthConnectionError::DuplicateProviderProfile { .. } => {
            AdminOAuthConnectionError::Internal
        }
        _ => AdminOAuthConnectionError::Internal,
    }
}

fn map_authorization_error(error: OAuthAuthorizationError) -> AdminOAuthConnectionError {
    match error {
        OAuthAuthorizationError::CapacityExceeded => {
            AdminOAuthConnectionError::AuthorizationCapacityExceeded
        }
        OAuthAuthorizationError::SessionNotFound | OAuthAuthorizationError::PrincipalMismatch => {
            AdminOAuthConnectionError::AuthorizationNotFound
        }
        OAuthAuthorizationError::SessionExpired => AdminOAuthConnectionError::AuthorizationExpired,
        OAuthAuthorizationError::ProviderDenied => AdminOAuthConnectionError::AuthorizationDenied,
        OAuthAuthorizationError::InvalidState
        | OAuthAuthorizationError::MalformedCallback
        | OAuthAuthorizationError::ProviderMismatch
        | OAuthAuthorizationError::RedirectUriMismatch
        | OAuthAuthorizationError::InvalidAuthorizationCode => {
            AdminOAuthConnectionError::InvalidInput
        }
        OAuthAuthorizationError::Entropy | OAuthAuthorizationError::StoreUnavailable => {
            AdminOAuthConnectionError::Unavailable
        }
        OAuthAuthorizationError::InvalidContext
        | OAuthAuthorizationError::InvalidClientId
        | OAuthAuthorizationError::InvalidAuthorizationEndpoint
        | OAuthAuthorizationError::AuthorizationUrlTooLong
        | OAuthAuthorizationError::InvalidRedirectUri
        | OAuthAuthorizationError::InvalidScope
        | OAuthAuthorizationError::ReservedParameter
        | OAuthAuthorizationError::InvalidAuthorizationParameter
        | OAuthAuthorizationError::InvalidSessionTtl
        | OAuthAuthorizationError::InvalidSessionCapacity => AdminOAuthConnectionError::Internal,
        _ => AdminOAuthConnectionError::Internal,
    }
}

fn map_exchange_error(error: OAuthTokenExchangeError) -> AdminOAuthConnectionError {
    match error {
        OAuthTokenExchangeError::Transport => AdminOAuthConnectionError::Unavailable,
        OAuthTokenExchangeError::EndpointRejected { .. } => {
            AdminOAuthConnectionError::UpstreamRejected
        }
        OAuthTokenExchangeError::ResponseTooLarge
        | OAuthTokenExchangeError::MalformedResponse
        | OAuthTokenExchangeError::InvalidTokenResponse => {
            AdminOAuthConnectionError::UpstreamInvalidResponse
        }
        OAuthTokenExchangeError::InvalidClientId
        | OAuthTokenExchangeError::InvalidTokenEndpoint
        | OAuthTokenExchangeError::InvalidClientAuthentication
        | OAuthTokenExchangeError::ProviderMismatch
        | OAuthTokenExchangeError::RequestTooLarge
        | OAuthTokenExchangeError::RequestEncoding => AdminOAuthConnectionError::Internal,
        _ => AdminOAuthConnectionError::Internal,
    }
}

const fn map_persistence_error(error: OAuthTokenPersistenceError) -> AdminOAuthConnectionError {
    match error {
        OAuthTokenPersistenceError::RepositoryUnavailable
        | OAuthTokenPersistenceError::RepositoryTimeout => AdminOAuthConnectionError::Unavailable,
        OAuthTokenPersistenceError::InvalidContext
        | OAuthTokenPersistenceError::InvalidExpiration
        | OAuthTokenPersistenceError::InvalidTokenSet
        | OAuthTokenPersistenceError::Encryption
        | OAuthTokenPersistenceError::Invariant => AdminOAuthConnectionError::Internal,
        _ => AdminOAuthConnectionError::Internal,
    }
}

/// 记录 OAuth 管理流程的固定失败分类；日志不携带回调 URL、state、授权码或 token。
fn report_oauth_failure(
    provider: AdminOAuthProvider,
    stage: &'static str,
    error: AdminOAuthConnectionError,
) -> AdminOAuthConnectionError {
    tracing::warn!(
        provider = provider.as_str(),
        stage,
        error_kind = admin_oauth_error_kind(error),
        "OAuth provider 操作失败"
    );
    error
}

const fn admin_oauth_error_kind(error: AdminOAuthConnectionError) -> &'static str {
    match error {
        AdminOAuthConnectionError::InvalidInput => "invalid_input",
        AdminOAuthConnectionError::Forbidden => "forbidden",
        AdminOAuthConnectionError::ProviderNotConfigured => "provider_not_configured",
        AdminOAuthConnectionError::TargetNotFound => "target_not_found",
        AdminOAuthConnectionError::TargetKindMismatch => "target_kind_mismatch",
        AdminOAuthConnectionError::CredentialProviderMismatch => "provider_mismatch",
        AdminOAuthConnectionError::AuthorizationCapacityExceeded => "authorization_capacity",
        AdminOAuthConnectionError::AuthorizationNotFound => "authorization_not_found",
        AdminOAuthConnectionError::AuthorizationExpired => "authorization_expired",
        AdminOAuthConnectionError::AuthorizationDenied => "authorization_denied",
        AdminOAuthConnectionError::UpstreamTimeout => "upstream_timeout",
        AdminOAuthConnectionError::UpstreamRejected => "upstream_rejected",
        AdminOAuthConnectionError::UpstreamInvalidResponse => "upstream_invalid_response",
        AdminOAuthConnectionError::Unavailable => "unavailable",
        AdminOAuthConnectionError::Internal => "internal",
    }
}

const fn serve_error_kind(error: &ServeError) -> &'static str {
    match error {
        ServeError::ZeroDrainTimeout => "zero_drain_timeout",
        ServeError::ForceStopTimeout => "force_stop_timeout",
        ServeError::Io(_) => "io",
        _ => "unknown",
    }
}
