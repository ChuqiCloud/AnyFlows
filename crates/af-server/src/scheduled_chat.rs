use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Instant,
};

use crate::{
    ChatRoutePlanFuture, ChatRoutePlanner, PlannedChatExecution, PlannedChatExecutionFuture,
    adaptor_credential::build_adaptor_credential,
    auto_ban_feedback::{ChannelAutoBanAttemptTarget, persist_channel_auto_ban_feedback},
    concurrency_runtime::{
        ConcurrencyRuntime, ConcurrencyRuntimeError, RuntimeConcurrencyPermit,
        user_permit_completion_hook,
    },
    credential_feedback::{CredentialAttemptTarget, persist_credential_feedback},
    credential_order::{
        CredentialLoad, order_credentials, order_credentials_by_load,
        order_credentials_by_load_and_health, order_credentials_by_load_with_session_affinity,
        order_credentials_with_health,
    },
    debug_trace_runtime::{DebugTraceCapture, DebugTraceRuntime},
    health_runtime::{
        RuntimeHealthState, SchedulerHealthStore, load_channel_health, load_credential_health,
        persist_scheduler_health_feedback,
    },
    responses_websocket_runtime::ResponsesWebSocketRuntime,
    sticky_session::{
        StableSessionScope, StickySessionStore, stable_session_scope, sticky_cache_error_kind,
    },
};
use af_account::{CredentialDecryptor, SystemSecretCipher, SystemSecretKind};
use af_adapter::{
    AdaptorSettings, AnthropicAdaptorSettings, BuiltInClientSimulation, CohereAdaptorSettings,
    GeminiAdaptorSettings, HeaderMap, HeaderName, HeaderValue, JinaAdaptorSettings,
    OpenAiAdaptorSettings, RelayContext, ResponsesWebSocketPoolKey, TransportDispatcher,
    get_adaptor,
};
use af_db::{
    AdminGroupLookupOutcome, AdminGroupRepository, ChannelStateRepository,
    CredentialStateRepository, SmartRouteRuntimeRepository,
};
use af_domain::{
    AfError, ChannelType, ClientSimulationBodyPatchResult, ClientSimulationBodyProfile,
    ConcurrencyLimit, CredentialId, CredentialQuotaDimension, GatewayPrincipal, GroupId, Operation,
    Protocol,
};
use af_httpclient::{HttpClientProvider, ProxyConfig, RemoteDnsPolicy};
use af_protocol::{CanonicalRequestEnvelope, ClientSimulationBodyPatch, UtcDate};
use af_relay::{
    AnthropicRequestOverrides, ChatResponse, GeminiRequestOverrides, OpenAiChatRequestOverrides,
    OpenAiResponsesRequestOverrides, RelayCandidate, RelayCandidateRequest, RelayDiagnosticInput,
    RelayStateMachine, relay_anthropic_with_report, relay_gemini_with_report,
    relay_openai_chat_with_report, relay_openai_responses_with_report,
};
use af_scheduler::{
    ChannelRoutingHealth, IndexedRoutePlan, IndexedWeightedScheduler,
    IndexedWeightedSchedulerError, RouteWaitKind, StickyRouteOutcome, StickyWaitPolicy,
};
use af_telemetry::{
    MetricClientSimulationBodyProfile, MetricClientSimulationBodyResult,
    MetricClientSimulationProfile, MetricClientSimulationResult, record_client_simulation_attempt,
    record_client_simulation_body_patch,
};

mod audio;
mod compact;
mod embedding;
mod image;
mod rerank;
mod smart_route;
mod speech;
mod video_task;
mod video_task_persistence;

const MAX_PLAYGROUND_GROUP_DEPTH: usize = 4;

/// 正文档案只能使用进程受信 UTC 时钟，测试可注入固定日期以复验跨午夜重试边界。
type UtcDateClock = dyn Fn() -> Option<UtcDate> + Send + Sync;

#[cfg(test)]
use smart_route::SmartRouteRulesFuture;
use smart_route::{SmartRouteResolver, bound_candidates, persist_feedback};

pub use video_task::{
    BoundVideoTaskSubmission, VideoTaskBinding, VideoTaskPollFuture, VideoTaskRuntime,
    VideoTaskSubmissionFuture, VideoTaskSubmissionRuntimeError,
};
pub(crate) use video_task_persistence::{
    PersistentVideoTaskBillingPorts, PersistentVideoTaskCoordinator, PersistentVideoTaskError,
    PersistentVideoTaskPollOutcome,
};

/// 路由计划在同一次 Redis 快照中取得的账号负载与用户许可。
struct PreparedConcurrencyPlan<'a> {
    account_loads: Option<&'a BTreeMap<i64, CredentialLoad>>,
    credential_health: Option<&'a BTreeMap<i64, RuntimeHealthState>>,
    user_permit: Option<RuntimeConcurrencyPermit>,
}

struct PreparedRouteSelection {
    route: IndexedRoutePlan,
    upstream_protocol: Protocol,
    routed_model: String,
    credential_health: Option<BTreeMap<i64, RuntimeHealthState>>,
}

struct PreparedExecutionRoute {
    route: IndexedRoutePlan,
    upstream_protocol: Protocol,
    routed_model: String,
}

/// 区分静态能力缺失与运行时健康故障，避免向客户端返回错误的过载语义。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RouteAvailability {
    Available,
    Unsupported,
    Unhealthy,
}

impl PreparedConcurrencyPlan<'_> {
    #[cfg(test)]
    const fn disabled() -> Self {
        Self {
            account_loads: None,
            credential_health: None,
            user_permit: None,
        }
    }
}

/// 基于内存运行时快照装配生产原生协议候选的服务。
#[derive(Clone)]
pub(crate) struct ScheduledChatService {
    scheduler: IndexedWeightedScheduler,
    decryptor: CredentialDecryptor,
    clients: HttpClientProvider,
    proxy_cipher: Option<SystemSecretCipher>,
    credential_states: CredentialStateRepository,
    responses_websocket: std::sync::Arc<ResponsesWebSocketRuntime>,
    sticky_sessions: Option<Arc<dyn StickySessionStore>>,
    sticky_wait_policy: StickyWaitPolicy,
    concurrency: Option<ConcurrencyRuntime>,
    health: Option<Arc<dyn SchedulerHealthStore>>,
    channel_states: Option<ChannelStateRepository>,
    debug_trace: Option<DebugTraceRuntime>,
    groups: Option<AdminGroupRepository>,
    smart_routes: Option<Arc<dyn SmartRouteResolver>>,
    smart_route_feedback: Option<SmartRouteRuntimeRepository>,
    utc_date_clock: Arc<UtcDateClock>,
}

impl ScheduledChatService {
    /// 使用已加载渠道索引、凭据解密器和受控 HTTP Client 创建生产路由器。
    #[must_use]
    pub(crate) fn new(
        scheduler: IndexedWeightedScheduler,
        decryptor: CredentialDecryptor,
        clients: HttpClientProvider,
        credential_states: CredentialStateRepository,
    ) -> Self {
        Self {
            scheduler,
            decryptor,
            clients,
            proxy_cipher: None,
            credential_states,
            responses_websocket: std::sync::Arc::new(ResponsesWebSocketRuntime::default()),
            sticky_sessions: None,
            sticky_wait_policy: StickyWaitPolicy::default(),
            concurrency: None,
            health: None,
            channel_states: None,
            debug_trace: None,
            groups: None,
            smart_routes: None,
            smart_route_feedback: None,
            utc_date_clock: Arc::new(crate::utc_time::current_utc_date),
        }
    }

    /// 注入专属代理密码解密器；未注入时任何代理绑定都失败关闭。
    pub(crate) fn with_proxy_cipher(mut self, cipher: SystemSecretCipher) -> Self {
        self.proxy_cipher = Some(cipher);
        self
    }

    /// 注入可选 Redis 粘性存储；未配置时继续使用非粘性路由。
    pub(crate) fn with_sticky_sessions(
        mut self,
        store: Arc<dyn StickySessionStore>,
        policy: StickyWaitPolicy,
    ) -> Self {
        self.sticky_sessions = Some(store);
        self.sticky_wait_policy = policy;
        self
    }

    /// 注入已通过启动健康检查的三级 Redis 并发运行时。
    pub(crate) fn with_concurrency(mut self, concurrency: ConcurrencyRuntime) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    /// 注入已通过启动健康检查的 Redis 熔断存储。
    pub(crate) fn with_scheduler_health(mut self, health: Arc<dyn SchedulerHealthStore>) -> Self {
        self.health = Some(health);
        self
    }

    /// 注入渠道自动禁用状态仓储；未注入时保持纯路由测试模式。
    pub(crate) fn with_channel_state_repository(
        mut self,
        repository: ChannelStateRepository,
    ) -> Self {
        self.channel_states = Some(repository);
        self
    }

    /// 注入脱敏调试追踪运行时；未注入时不执行采样或分配追踪上下文。
    pub(crate) fn with_debug_trace_runtime(mut self, runtime: DebugTraceRuntime) -> Self {
        self.debug_trace = Some(runtime);
        self
    }

    /// 注入分组只读仓储，仅供试炼场托管主体解析显式回退链。
    pub(crate) fn with_group_repository(mut self, repository: AdminGroupRepository) -> Self {
        self.groups = Some(repository);
        self
    }

    /// 注入管理员智能路由运行时仓储；普通 API Key 不消费该额外规则源。
    pub(crate) fn with_smart_route_repository(
        mut self,
        repository: SmartRouteRuntimeRepository,
    ) -> Self {
        self.smart_routes = Some(Arc::new(repository.clone()));
        self.smart_route_feedback = Some(repository);
        self
    }

    #[cfg(test)]
    fn with_smart_route_resolver(mut self, resolver: Arc<dyn SmartRouteResolver>) -> Self {
        self.smart_routes = Some(resolver);
        self
    }

    fn build_plan(
        &self,
        prepared_route: PreparedExecutionRoute,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &str,
        session_scope: Option<StableSessionScope>,
        concurrency: PreparedConcurrencyPlan<'_>,
    ) -> Result<ScheduledChatExecution, AfError> {
        let PreparedExecutionRoute {
            route,
            upstream_protocol,
            routed_model,
        } = prepared_route;
        let PreparedConcurrencyPlan {
            account_loads,
            credential_health,
            user_permit,
        } = concurrency;
        let requested_model = request.canonical().model.clone();
        let quota_dimension = CredentialQuotaDimension::for_canonical_model(&routed_model);
        let responses_websocket_session = session_scope.as_ref().map(StableSessionScope::as_str);
        let target_group_id = route.target_group_id();
        let mut relay_candidates = Vec::with_capacity(
            route
                .candidates()
                .len()
                .min(RelayStateMachine::MAX_CANDIDATES),
        );
        let mut attempt_targets = Vec::with_capacity(relay_candidates.capacity());
        let mut auto_ban_targets = Vec::with_capacity(relay_candidates.capacity());
        let mut route_channel_ids = Vec::with_capacity(relay_candidates.capacity());
        let mut health_filtered = false;
        'channels: for candidate in route.candidates() {
            let target = candidate.runtime_target();
            let wait_plan = candidate.wait_plan();
            if !target_matches_protocol(target, upstream_protocol) {
                continue;
            }
            let upstream_model = target
                .mapped_model(&routed_model)
                .unwrap_or(&routed_model)
                .to_owned();
            let candidate_request =
                candidate_request(target, &request, &requested_model, &upstream_model)?;
            let adaptor = get_adaptor(
                target.channel_type(),
                adaptor_settings(target, upstream_model)?,
            )
            .map_err(|_| AfError::Internal)?;
            if adaptor.default_protocol() != target.protocol() {
                return Err(AfError::Internal);
            }

            let headers = header_overrides(target)?;
            let eligible_credentials = target
                .credentials()
                .iter()
                .filter(|credential| credential.quota_dimension() == quota_dimension)
                .cloned()
                .collect::<Vec<_>>();
            let order_scope = session_scope
                .as_ref()
                .map_or(request_id, StableSessionScope::as_str);
            let ordered_credentials = match (wait_plan.kind(), account_loads, credential_health) {
                (RouteWaitKind::Fallback, Some(loads), health) if session_scope.is_some() => {
                    order_credentials_by_load_with_session_affinity(
                        order_scope,
                        target.channel_id(),
                        &eligible_credentials,
                        loads,
                        health,
                    )
                }
                (RouteWaitKind::Fallback, Some(loads), Some(health)) => {
                    order_credentials_by_load_and_health(
                        order_scope,
                        target.channel_id(),
                        &eligible_credentials,
                        loads,
                        health,
                    )
                }
                (RouteWaitKind::Fallback, Some(loads), None) => order_credentials_by_load(
                    order_scope,
                    target.channel_id(),
                    &eligible_credentials,
                    loads,
                ),
                (_, _, Some(health)) => order_credentials_with_health(
                    order_scope,
                    target.channel_id(),
                    &eligible_credentials,
                    health,
                ),
                _ => order_credentials(order_scope, target.channel_id(), &eligible_credentials),
            };
            health_filtered |= !eligible_credentials.is_empty() && ordered_credentials.is_empty();
            for runtime_credential in ordered_credentials {
                if relay_candidates.len() == RelayStateMachine::MAX_CANDIDATES {
                    break 'channels;
                }
                let credential_id = CredentialId::new(runtime_credential.credential_id())
                    .map_err(|_| AfError::Internal)?;
                let secret_owner_id = runtime_credential.secret_owner_id();
                let concurrency_owner_id = runtime_credential.concurrency_owner_id();
                let client = self.client_for_credential(target, runtime_credential)?;
                let responses_websocket_pool = if target.responses_websocket_enabled()
                    && responses_websocket_session.is_some()
                {
                    Some(
                        self.responses_websocket
                            .pool_for(&client)
                            .map_err(|_| AfError::Internal)?,
                    )
                } else {
                    None
                };
                let mut context = RelayContext::new(client);
                if let Some(base_url) = target.base_url() {
                    context = context
                        .with_base_url(base_url)
                        .map_err(|_| AfError::Internal)?;
                }
                context = context.with_oauth_identity(
                    runtime_credential.oauth_provider(),
                    runtime_credential.oauth_account_key(),
                );
                context = context
                    .with_request_id(request_id.to_owned())
                    .map_err(|_| AfError::Internal)?;
                let decrypted = self
                    .decryptor
                    .decrypt_envelope(
                        target.channel_id(),
                        secret_owner_id.get(),
                        runtime_credential.credential_kind(),
                        runtime_credential.envelope(),
                    )
                    .map_err(|_| AfError::Internal)?;
                let oauth_has_refresh_token = decrypted.oauth_has_refresh_token();
                let credential =
                    build_adaptor_credential(&decrypted).map_err(|_| AfError::Internal)?;
                let mut relay_candidate =
                    RelayCandidate::new(adaptor.clone(), context.clone(), credential)
                        .with_header_overrides(headers.clone())
                        .with_channel_group(target.channel_id());
                if let Some(profile) = target.client_simulation_profile() {
                    relay_candidate = relay_candidate
                        .with_client_simulation(Arc::new(BuiltInClientSimulation::new(profile)));
                }
                if let Some(concurrency) = &self.concurrency {
                    relay_candidate = relay_candidate.with_attempt_gate(concurrency.account_gate(
                        concurrency_owner_id,
                        runtime_credential.concurrency(),
                        wait_plan.timeout(),
                    ));
                }
                if let (Some(pool), Some(session_scope)) = (
                    responses_websocket_pool.as_ref(),
                    responses_websocket_session,
                ) {
                    let key = ResponsesWebSocketPoolKey::new(
                        target.channel_id(),
                        secret_owner_id,
                        runtime_credential.credential_revision(),
                        session_scope,
                    )
                    .map_err(|_| AfError::Internal)?;
                    relay_candidate = relay_candidate.with_transport_dispatcher(
                        TransportDispatcher::responses_websocket(pool.clone(), key),
                    );
                }
                if let Some(candidate_request) = &candidate_request {
                    relay_candidate = relay_candidate.with_request(candidate_request.clone());
                }
                relay_candidates.push(relay_candidate);
                attempt_targets.push(CredentialAttemptTarget::with_runtime_identity(
                    target.channel_id(),
                    credential_id,
                    secret_owner_id,
                    runtime_credential.shared_health_id(),
                    runtime_credential.credential_kind(),
                    oauth_has_refresh_token,
                    target.pool_mode(),
                )?);
                auto_ban_targets.push(ChannelAutoBanAttemptTarget::new(
                    target.channel_id(),
                    target.auto_ban_rules(),
                    target.pool_mode(),
                )?);
                route_channel_ids.push(candidate.route_channel_id(credential_id));
            }
        }
        if relay_candidates.is_empty() {
            return Err(if health_filtered {
                af_domain::UpstreamError::overloaded().into()
            } else {
                af_domain::UpstreamError::ModelUnsupported.into()
            });
        }
        let machine = RelayStateMachine::new(relay_candidates).map_err(|_| AfError::Internal)?;
        Ok(ScheduledChatExecution {
            target_group_id,
            pricing_model: routed_model,
            machine,
            attempt_targets,
            auto_ban_targets,
            route_channel_ids,
            credential_states: self.credential_states.clone(),
            scheduler: self.scheduler.clone(),
            request,
            response_protocol,
            upstream_protocol,
            request_id: request_id.to_owned(),
            sticky_sessions: self.sticky_sessions.clone(),
            session_scope,
            user_permit,
            health: self.health.clone(),
            channel_states: self.channel_states.clone(),
            debug_trace: None,
            smart_route_feedback: self.smart_route_feedback.clone(),
        })
    }

    /// 为单条凭据选择全局出口或强制专属代理，任何专属代理异常都不允许回退。
    fn client_for_credential(
        &self,
        target: &af_db::SchedulerRuntimeTargetRecord,
        credential: &af_db::SchedulerRuntimeCredentialRecord,
    ) -> Result<af_httpclient::PooledClient, AfError> {
        let timeout = target.timeout().map(af_domain::ChannelTimeout::duration);
        if !credential.proxy_required() {
            return self.clients.get(timeout).map_err(|_| AfError::Internal);
        }
        let proxy = credential.proxy().ok_or(AfError::Internal)?;
        let cipher = self.proxy_cipher.as_ref().ok_or(AfError::Internal)?;
        let password = proxy
            .password_secret()
            .map(|secret| {
                cipher.decrypt(
                    SystemSecretKind::CredentialProxyPassword(proxy.proxy_id()),
                    secret,
                )
            })
            .transpose()
            .map_err(|_| AfError::Internal)?;
        let proxy_config = ProxyConfig::from_parts(
            proxy.scheme().as_str(),
            proxy.host(),
            proxy.port(),
            proxy.username(),
            password
                .as_ref()
                .map(af_account::DecryptedSystemSecret::expose_secret),
        )
        .map_err(|_| AfError::Internal)?;
        self.clients
            .get_with_proxy(
                proxy_config,
                if proxy.trust_proxy_dns() {
                    RemoteDnsPolicy::TrustProxy
                } else {
                    RemoteDnsPolicy::Deny
                },
                timeout,
            )
            .map_err(|_| AfError::Internal)
    }

    /// 读取粘性绑定并只在当前协议候选中应用；Redis 故障降级为普通路由。
    async fn apply_sticky_binding(
        &self,
        route: &mut IndexedRoutePlan,
        upstream_protocol: Protocol,
        session_scope: Option<&StableSessionScope>,
    ) {
        let (Some(store), Some(scope)) = (self.sticky_sessions.as_ref(), session_scope) else {
            return;
        };
        let binding = match store.get_and_refresh(scope.as_str()).await {
            Ok(binding) => binding,
            Err(error) => {
                tracing::warn!(
                    target: "af_server::sticky_session",
                    error_kind = sticky_cache_error_kind(&error),
                    "读取粘性会话失败，当前请求降级为普通路由"
                );
                return;
            }
        };
        let Some(channel_id) = binding.and_then(|value| af_domain::ChannelId::new(value).ok())
        else {
            return;
        };
        let available = route.candidates().iter().any(|candidate| {
            candidate.channel_id() == channel_id
                && target_matches_protocol(candidate.runtime_target(), upstream_protocol)
        });
        if !available {
            if self.sticky_wait_policy.clear_unavailable_binding()
                && let Err(error) = store
                    .delete_if_channel(scope.as_str(), channel_id.get())
                    .await
            {
                tracing::warn!(
                    target: "af_server::sticky_session",
                    error_kind = sticky_cache_error_kind(&error),
                    "清理失效粘性会话失败，当前请求继续使用普通路由"
                );
            }
            return;
        }
        if matches!(
            route.prefer_sticky_channel(channel_id, self.sticky_wait_policy),
            StickyRouteOutcome::Unavailable
        ) {
            // 当前快照在检查与重排之间只能被本地值修改；失败时保持普通候选顺序。
            if self.sticky_wait_policy.clear_unavailable_binding()
                && let Err(error) = store
                    .delete_if_channel(scope.as_str(), channel_id.get())
                    .await
            {
                tracing::warn!(
                    target: "af_server::sticky_session",
                    error_kind = sticky_cache_error_kind(&error),
                    "条件删除失效粘性会话失败"
                );
            }
        }
    }

    async fn acquire_user_permit(
        &self,
        principal: GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
    ) -> Result<Option<RuntimeConcurrencyPermit>, AfError> {
        let Some(concurrency) = &self.concurrency else {
            return Ok(None);
        };
        concurrency
            .acquire_user(
                principal,
                user_concurrency,
                self.sticky_wait_policy.fallback_wait_timeout(),
            )
            .await
            .map(Some)
            .map_err(map_concurrency_runtime_error)
    }

    async fn load_account_concurrency(
        &self,
        route: &IndexedRoutePlan,
        upstream_protocol: Protocol,
        model: &str,
    ) -> Result<Option<BTreeMap<i64, CredentialLoad>>, AfError> {
        let Some(concurrency) = &self.concurrency else {
            return Ok(None);
        };
        let accounts = collect_route_accounts(route, upstream_protocol, model)?;
        concurrency
            .account_loads(&accounts)
            .await
            .map(Some)
            .map_err(|_| AfError::Internal)
    }

    async fn load_channel_health(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<
        (
            Vec<af_domain::ChannelId>,
            BTreeMap<af_domain::ChannelId, ChannelRoutingHealth>,
        ),
        AfError,
    > {
        let channel_ids = self
            .scheduler
            .route_channel_ids(group_id, model)
            .map_err(map_scheduler_error)?;
        let Some(store) = &self.health else {
            return Ok((channel_ids, BTreeMap::new()));
        };
        let health = load_channel_health(store, &channel_ids)
            .await
            .map_err(|_| AfError::Internal)?;
        Ok((channel_ids, health))
    }

    async fn load_credential_health(
        &self,
        route: &IndexedRoutePlan,
        upstream_protocol: Protocol,
        model: &str,
    ) -> Result<Option<BTreeMap<i64, RuntimeHealthState>>, AfError> {
        let Some(store) = &self.health else {
            return Ok(None);
        };
        let credential_ids = collect_route_credentials(route, upstream_protocol, model)?;
        load_credential_health(store, &credential_ids)
            .await
            .map(Some)
            .map_err(|_| AfError::Internal)
    }

    async fn route_group_chain(
        &self,
        principal: GatewayPrincipal,
    ) -> Result<Vec<GroupId>, AfError> {
        let mut groups = vec![principal.group_id()];
        if !principal.is_playground() {
            return Ok(groups);
        }
        let repository = self.groups.as_ref().ok_or_else(|| {
            tracing::error!(
                target: "af_server::scheduled_chat",
                error_kind = "playground_group_repository_missing",
                "试炼场分组回退仓储未完成启动装配"
            );
            AfError::Internal
        })?;
        let mut visited = BTreeSet::from([principal.group_id()]);
        let mut current = principal.group_id();
        while groups.len() < MAX_PLAYGROUND_GROUP_DEPTH {
            let record = match repository
                .get(current)
                .await
                .map_err(|_| AfError::Internal)?
            {
                AdminGroupLookupOutcome::Found(record) => record,
                AdminGroupLookupOutcome::NotFound => return Err(AfError::Internal),
            };
            let Some(fallback_group_id) = record.fallback_group_id() else {
                break;
            };
            if !visited.insert(fallback_group_id) {
                tracing::warn!(
                    target: "af_server::scheduled_chat",
                    error_kind = "playground_group_fallback_cycle",
                    "试炼场分组回退链存在循环，已在发出上游请求前截断"
                );
                break;
            }
            groups.push(fallback_group_id);
            current = fallback_group_id;
        }
        Ok(groups)
    }

    async fn prepare_route_selection(
        &self,
        principal: GatewayPrincipal,
        request: &CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &str,
        session_scope: Option<&StableSessionScope>,
    ) -> Result<PreparedRouteSelection, AfError> {
        let mut unhealthy_route_seen = false;
        let group_chain = self.route_group_chain(principal).await?;
        for group_id in group_chain.iter().copied() {
            let Some(unfiltered_route) = self
                .scheduler
                .route_plan(group_id, &request.canonical().model)
                .map_err(map_scheduler_error)?
            else {
                continue;
            };
            let unfiltered_protocol =
                select_upstream_protocol(&unfiltered_route, request, response_protocol)?;
            if route_availability(
                &unfiltered_route,
                unfiltered_protocol,
                &request.canonical().model,
                request_id,
                None,
            ) == RouteAvailability::Unsupported
            {
                // 模型存在但协议不匹配或没有静态凭据时属于不支持，不能伪装成运行时过载。
                continue;
            }

            let route = if self.health.is_some() {
                let (channel_ids, channel_health) = self
                    .load_channel_health(group_id, &request.canonical().model)
                    .await?;
                let route = self
                    .scheduler
                    .route_plan_with_health(group_id, &request.canonical().model, &channel_health)
                    .map_err(map_scheduler_error)?;
                if route.is_none() && !channel_ids.is_empty() {
                    unhealthy_route_seen = true;
                }
                route
            } else {
                Some(unfiltered_route)
            };
            let Some(mut route) = route else {
                continue;
            };
            let upstream_protocol = select_upstream_protocol(&route, request, response_protocol)?;
            route.configure_wait_policy(self.sticky_wait_policy);
            self.apply_sticky_binding(&mut route, upstream_protocol, session_scope)
                .await;
            let credential_health = self
                .load_credential_health(&route, upstream_protocol, &request.canonical().model)
                .await?;
            let order_scope = session_scope.map_or(request_id, StableSessionScope::as_str);
            match route_availability(
                &route,
                upstream_protocol,
                &request.canonical().model,
                order_scope,
                credential_health.as_ref(),
            ) {
                RouteAvailability::Available => {
                    return Ok(PreparedRouteSelection {
                        route,
                        upstream_protocol,
                        routed_model: request.canonical().model.clone(),
                        credential_health,
                    });
                }
                RouteAvailability::Unhealthy => unhealthy_route_seen = true,
                RouteAvailability::Unsupported => {
                    // 健康过滤可能只留下其他协议；未过滤计划可用时仍应归类为健康故障。
                    unhealthy_route_seen |= self.health.is_some();
                }
            }
        }

        if principal.is_playground()
            && let Some(resolver) = self.smart_routes.as_ref()
        {
            let rules = resolver
                .matching_rules(&request.canonical().model)
                .await
                .map_err(|_| AfError::Internal)?;
            for group_id in group_chain {
                for rule in &rules {
                    let bindings = bound_candidates(rule)?;
                    let Some(unfiltered_route) = self
                        .scheduler
                        .bound_route_plan(
                            group_id,
                            rule.routed_model(),
                            rule.strategy(),
                            &bindings,
                            &BTreeMap::new(),
                        )
                        .map_err(map_scheduler_error)?
                    else {
                        continue;
                    };
                    let unfiltered_protocol =
                        select_upstream_protocol(&unfiltered_route, request, response_protocol)?;
                    if route_availability(
                        &unfiltered_route,
                        unfiltered_protocol,
                        rule.routed_model(),
                        request_id,
                        None,
                    ) == RouteAvailability::Unsupported
                    {
                        continue;
                    }

                    let route = if self.health.is_some() {
                        let (_, channel_health) = self
                            .load_channel_health(group_id, rule.routed_model())
                            .await?;
                        self.scheduler
                            .bound_route_plan(
                                group_id,
                                rule.routed_model(),
                                rule.strategy(),
                                &bindings,
                                &channel_health,
                            )
                            .map_err(map_scheduler_error)?
                    } else {
                        Some(unfiltered_route)
                    };
                    let Some(mut route) = route else {
                        unhealthy_route_seen = true;
                        break;
                    };
                    let upstream_protocol =
                        select_upstream_protocol(&route, request, response_protocol)?;
                    route.configure_wait_policy(self.sticky_wait_policy);
                    self.apply_sticky_binding(&mut route, upstream_protocol, session_scope)
                        .await;
                    let credential_health = self
                        .load_credential_health(&route, upstream_protocol, rule.routed_model())
                        .await?;
                    let order_scope = session_scope.map_or(request_id, StableSessionScope::as_str);
                    match route_availability(
                        &route,
                        upstream_protocol,
                        rule.routed_model(),
                        order_scope,
                        credential_health.as_ref(),
                    ) {
                        RouteAvailability::Available => {
                            return Ok(PreparedRouteSelection {
                                route,
                                upstream_protocol,
                                routed_model: rule.routed_model().to_owned(),
                                credential_health,
                            });
                        }
                        RouteAvailability::Unhealthy => {
                            unhealthy_route_seen = true;
                            break;
                        }
                        RouteAvailability::Unsupported => {
                            // 静态计划已经锁定规则优先级；健康过滤后的协议缺口只能降到下一分组。
                            unhealthy_route_seen = true;
                            break;
                        }
                    }
                }
            }
        }
        Err(if unhealthy_route_seen {
            af_domain::UpstreamError::overloaded().into()
        } else {
            af_domain::UpstreamError::ModelUnsupported.into()
        })
    }
}

impl ChatRoutePlanner for ScheduledChatService {
    fn plan<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        diagnostic: RelayDiagnosticInput,
    ) -> ChatRoutePlanFuture<'a> {
        Box::pin(async move {
            let session_scope = stable_session_scope(*principal, request.canonical());
            let PreparedRouteSelection {
                route,
                upstream_protocol,
                routed_model,
                credential_health,
            } = self
                .prepare_route_selection(
                    *principal,
                    &request,
                    response_protocol,
                    request_id,
                    session_scope.as_ref(),
                )
                .await?;
            let body_profile = route_client_simulation_body_profile(
                &route,
                upstream_protocol,
                &routed_model,
                session_scope
                    .as_ref()
                    .map_or(request_id, StableSessionScope::as_str),
                credential_health.as_ref(),
            )?;
            // 只在候选档案完全一致时冻结一次日期；任何混合或不可重建形状都在预扣前失败。
            let request = match body_profile {
                Some(profile) => {
                    let Some(date) = (self.utc_date_clock)() else {
                        record_client_simulation_body_patch(
                            metric_client_simulation_body_profile(profile),
                            MetricClientSimulationBodyResult::Rejected,
                        );
                        return Err(AfError::InvalidRequest);
                    };
                    let patched = ClientSimulationBodyPatch::new(profile).apply(request, date);
                    match patched {
                        Ok(request) => {
                            record_client_simulation_body_patch(
                                metric_client_simulation_body_profile(profile),
                                MetricClientSimulationBodyResult::Applied,
                            );
                            request
                        }
                        Err(_) => {
                            record_client_simulation_body_patch(
                                metric_client_simulation_body_profile(profile),
                                MetricClientSimulationBodyResult::Rejected,
                            );
                            return Err(AfError::InvalidRequest);
                        }
                    }
                }
                None => request,
            };
            let debug_trace = self.debug_trace.as_ref().and_then(|runtime| {
                runtime.capture(
                    request_id,
                    *principal,
                    &request.canonical().model,
                    response_protocol,
                    upstream_protocol,
                    request.canonical().operation,
                    body_profile,
                    body_profile.map(|_| ClientSimulationBodyPatchResult::Applied),
                    diagnostic,
                )
            });
            let user_permit = self
                .acquire_user_permit(*principal, user_concurrency)
                .await?;
            let account_loads = self
                .load_account_concurrency(&route, upstream_protocol, &routed_model)
                .await?;
            let mut plan = self.build_plan(
                PreparedExecutionRoute {
                    route,
                    upstream_protocol,
                    routed_model,
                },
                request,
                response_protocol,
                request_id,
                session_scope,
                PreparedConcurrencyPlan {
                    account_loads: account_loads.as_ref(),
                    credential_health: credential_health.as_ref(),
                    user_permit,
                },
            )?;
            plan.debug_trace = debug_trace;
            Ok(Box::new(plan) as Box<dyn PlannedChatExecution>)
        })
    }
}

impl fmt::Debug for ScheduledChatService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScheduledChatService")
            .field("scheduler", &self.scheduler)
            .field("decryptor", &"<受控>")
            .field("clients", &"<受控>")
            .field("credential_states", &self.credential_states)
            .field("responses_websocket", &self.responses_websocket)
            .field("sticky_sessions", &self.sticky_sessions.is_some())
            .field("sticky_wait_policy", &self.sticky_wait_policy)
            .field("concurrency", &self.concurrency.is_some())
            .field("health", &self.health.is_some())
            .field("debug_trace", &self.debug_trace.is_some())
            .field("groups", &self.groups.is_some())
            .field("smart_routes", &self.smart_routes.is_some())
            .finish()
    }
}

struct ScheduledChatExecution {
    target_group_id: GroupId,
    pricing_model: String,
    machine: RelayStateMachine,
    attempt_targets: Vec<CredentialAttemptTarget>,
    auto_ban_targets: Vec<ChannelAutoBanAttemptTarget>,
    route_channel_ids: Vec<Option<af_domain::RouteChannelId>>,
    credential_states: CredentialStateRepository,
    scheduler: IndexedWeightedScheduler,
    request: CanonicalRequestEnvelope,
    response_protocol: Protocol,
    upstream_protocol: Protocol,
    request_id: String,
    sticky_sessions: Option<Arc<dyn StickySessionStore>>,
    session_scope: Option<StableSessionScope>,
    user_permit: Option<RuntimeConcurrencyPermit>,
    health: Option<Arc<dyn SchedulerHealthStore>>,
    channel_states: Option<ChannelStateRepository>,
    debug_trace: Option<DebugTraceCapture>,
    smart_route_feedback: Option<SmartRouteRuntimeRepository>,
}

impl PlannedChatExecution for ScheduledChatExecution {
    fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }

    fn pricing_model(&self) -> &str {
        &self.pricing_model
    }

    fn canonical_request(&self) -> &CanonicalRequestEnvelope {
        &self.request
    }

    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
        let Self {
            target_group_id: _,
            pricing_model: _,
            machine,
            attempt_targets,
            auto_ban_targets,
            route_channel_ids,
            credential_states,
            scheduler,
            request,
            response_protocol,
            upstream_protocol,
            request_id,
            sticky_sessions,
            session_scope,
            user_permit,
            health,
            channel_states,
            debug_trace,
            smart_route_feedback,
        } = *self;
        Box::pin(async move {
            let machine = match debug_trace.as_ref() {
                Some(capture) => machine.with_diagnostic_policy(capture.relay_policy()),
                None => machine,
            };
            let operation = request.canonical().operation;
            let routing_started_at = Instant::now();
            let (result, report) = match (operation, upstream_protocol) {
                (Operation::Chat, Protocol::OpenAiChat) => {
                    relay_openai_chat_with_report(&machine, request, response_protocol, &request_id)
                        .await
                        .into_parts()
                }
                (Operation::Chat, Protocol::Anthropic) => {
                    relay_anthropic_with_report(&machine, request, &request_id)
                        .await
                        .into_parts()
                }
                (Operation::Chat, Protocol::Gemini) => {
                    relay_gemini_with_report(&machine, request, &request_id)
                        .await
                        .into_parts()
                }
                (Operation::Responses, Protocol::OpenAiResponses) => {
                    relay_openai_responses_with_report(&machine, request, &request_id)
                        .await
                        .into_parts()
                }
                _ => return Err(AfError::InvalidRequest),
            };
            record_client_simulation_metrics(&report);
            if let Some(capture) = debug_trace {
                capture.submit(
                    &report,
                    &attempt_targets,
                    result.is_ok(),
                    routing_started_at.elapsed(),
                );
            }
            let successful_channel_id = report
                .successful_candidate_index()
                .and_then(|index| attempt_targets.get(index))
                .map(CredentialAttemptTarget::channel_id);
            let ((), (), (), ()) = tokio::join!(
                persist_credential_feedback(
                    &credential_states,
                    &scheduler,
                    &attempt_targets,
                    &report,
                ),
                persist_scheduler_health_feedback(health.as_ref(), &attempt_targets, &report),
                persist_channel_auto_ban_feedback(
                    channel_states.as_ref(),
                    &scheduler,
                    &auto_ban_targets,
                    &report,
                ),
                persist_feedback(smart_route_feedback.as_ref(), &route_channel_ids, &report,),
            );
            if result.is_ok()
                && let (Some(store), Some(scope), Some(channel_id)) = (
                    sticky_sessions.as_ref(),
                    session_scope.as_ref(),
                    successful_channel_id.map(af_domain::ChannelId::get),
                )
                && let Err(error) = store.bind(scope.as_str(), channel_id).await
            {
                tracing::warn!(
                    target: "af_server::sticky_session",
                    error_kind = sticky_cache_error_kind(&error),
                    "写入粘性会话绑定失败，当前响应继续返回"
                );
            }
            match result {
                Ok(ChatResponse::Full { body, usage }) => {
                    if let Some(permit) = user_permit {
                        permit.release().await;
                    }
                    successful_channel_id
                        .map(|channel_id| {
                            crate::RoutedExecution::new(
                                ChatResponse::Full { body, usage },
                                channel_id,
                            )
                        })
                        .ok_or(AfError::Internal)
                }
                Ok(ChatResponse::Stream { body, usage }) => {
                    let body = match user_permit {
                        Some(permit) => {
                            body.with_completion_hook(user_permit_completion_hook(permit))
                        }
                        None => body,
                    };
                    successful_channel_id
                        .map(|channel_id| {
                            crate::RoutedExecution::new(
                                ChatResponse::Stream { body, usage },
                                channel_id,
                            )
                        })
                        .ok_or(AfError::Internal)
                }
                Err(error) => {
                    if let Some(permit) = user_permit {
                        permit.release().await;
                    }
                    Err(error)
                }
            }
        })
    }
}

/// 只把 Relay 生成的闭合档案和结果写入低基数指标。
fn record_client_simulation_metrics(report: &af_relay::RelayAttemptReport) {
    for failure in report.failures() {
        if let Some(client_simulation) = failure.client_simulation() {
            record_client_simulation_attempt(
                metric_client_simulation_profile(client_simulation.profile()),
                metric_client_simulation_result(client_simulation.result()),
            );
        }
    }
    if let Some(client_simulation) = report.successful_client_simulation() {
        record_client_simulation_attempt(
            metric_client_simulation_profile(client_simulation.profile()),
            metric_client_simulation_result(client_simulation.result()),
        );
    }
}

const fn metric_client_simulation_profile(
    profile: af_domain::ClientSimulationProfile,
) -> MetricClientSimulationProfile {
    match profile {
        af_domain::ClientSimulationProfile::AnthropicCliHeadersV1 => {
            MetricClientSimulationProfile::AnthropicCliHeadersV1
        }
    }
}

const fn metric_client_simulation_body_profile(
    profile: ClientSimulationBodyProfile,
) -> MetricClientSimulationBodyProfile {
    match profile {
        ClientSimulationBodyProfile::AnthropicCliSystemDateV1 => {
            MetricClientSimulationBodyProfile::AnthropicCliSystemDateV1
        }
    }
}

const fn metric_client_simulation_result(
    result: af_domain::ClientSimulationResult,
) -> MetricClientSimulationResult {
    match result {
        af_domain::ClientSimulationResult::NotApplied => MetricClientSimulationResult::NotApplied,
        af_domain::ClientSimulationResult::Applied => MetricClientSimulationResult::Applied,
        af_domain::ClientSimulationResult::Failed => MetricClientSimulationResult::Failed,
    }
}

fn collect_route_accounts(
    route: &IndexedRoutePlan,
    upstream_protocol: Protocol,
    model: &str,
) -> Result<Vec<(CredentialId, Option<ConcurrencyLimit>)>, AfError> {
    let mut accounts = Vec::with_capacity(RelayStateMachine::MAX_CANDIDATES);
    let mut unique = BTreeSet::new();
    let quota_dimension = CredentialQuotaDimension::for_canonical_model(model);
    'channels: for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !target_matches_protocol(target, upstream_protocol) {
            continue;
        }
        for credential in target.credentials() {
            if credential.quota_dimension() != quota_dimension {
                continue;
            }
            if accounts.len() == RelayStateMachine::MAX_CANDIDATES {
                break 'channels;
            }
            let owner = credential.concurrency_owner_id();
            if unique.insert(owner) {
                accounts.push((owner, credential.concurrency()));
            }
        }
    }
    Ok(accounts)
}

fn route_availability(
    route: &IndexedRoutePlan,
    upstream_protocol: Protocol,
    model: &str,
    order_scope: &str,
    credential_health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> RouteAvailability {
    let quota_dimension = CredentialQuotaDimension::for_canonical_model(model);
    let mut matching_protocol = false;
    let mut static_credentials = false;
    for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !target_matches_protocol(target, upstream_protocol) {
            continue;
        }
        matching_protocol = true;
        let eligible_credentials = target
            .credentials()
            .iter()
            .filter(|credential| credential.quota_dimension() == quota_dimension)
            .cloned()
            .collect::<Vec<_>>();
        if eligible_credentials.is_empty() {
            continue;
        }
        static_credentials = true;
        let available = match credential_health {
            None => true,
            Some(health) => !order_credentials_with_health(
                order_scope,
                target.channel_id(),
                &eligible_credentials,
                health,
            )
            .is_empty(),
        };
        if available {
            return RouteAvailability::Available;
        }
    }
    if matching_protocol && static_credentials {
        RouteAvailability::Unhealthy
    } else {
        RouteAvailability::Unsupported
    }
}

fn collect_route_credentials(
    route: &IndexedRoutePlan,
    upstream_protocol: Protocol,
    model: &str,
) -> Result<Vec<(CredentialId, CredentialId)>, AfError> {
    let mut credential_ids = Vec::with_capacity(RelayStateMachine::MAX_CANDIDATES);
    let mut unique = BTreeSet::new();
    let quota_dimension = CredentialQuotaDimension::for_canonical_model(model);
    'channels: for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !target_matches_protocol(target, upstream_protocol) {
            continue;
        }
        for credential in target.credentials() {
            if credential.quota_dimension() != quota_dimension {
                continue;
            }
            if credential_ids.len() == RelayStateMachine::MAX_CANDIDATES {
                break 'channels;
            }
            let credential_id =
                CredentialId::new(credential.credential_id()).map_err(|_| AfError::Internal)?;
            if !unique.insert(credential_id) {
                return Err(AfError::Internal);
            }
            credential_ids.push((credential_id, credential.shared_health_id()));
        }
    }
    Ok(credential_ids)
}

/// 从可发送候选收集正文档案；混合关闭与启用、或不同版本档案一律在预扣前拒绝。
fn route_client_simulation_body_profile(
    route: &IndexedRoutePlan,
    upstream_protocol: Protocol,
    model: &str,
    order_scope: &str,
    credential_health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> Result<Option<ClientSimulationBodyProfile>, AfError> {
    let quota_dimension = CredentialQuotaDimension::for_canonical_model(model);
    let mut resolved = None;
    for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !target_matches_protocol(target, upstream_protocol) {
            continue;
        }
        let credentials = target
            .credentials()
            .iter()
            .filter(|credential| credential.quota_dimension() == quota_dimension)
            .cloned()
            .collect::<Vec<_>>();
        if credentials.is_empty() {
            continue;
        }
        if credential_health.is_some_and(|health| {
            order_credentials_with_health(order_scope, target.channel_id(), &credentials, health)
                .is_empty()
        }) {
            continue;
        }
        let profile = target.client_simulation_body_profile();
        match resolved {
            None => resolved = Some(profile),
            Some(current) if current == profile => {}
            Some(_) => return Err(AfError::InvalidRequest),
        }
    }
    Ok(resolved.flatten())
}

const fn map_concurrency_runtime_error(error: ConcurrencyRuntimeError) -> AfError {
    match error {
        ConcurrencyRuntimeError::Limited => AfError::ConcurrencyLimited,
        ConcurrencyRuntimeError::Internal => AfError::Internal,
    }
}

fn header_overrides(target: &af_db::SchedulerRuntimeTargetRecord) -> Result<HeaderMap, AfError> {
    let mut headers = HeaderMap::with_capacity(target.headers().len());
    for header in target.headers() {
        let name =
            HeaderName::from_bytes(header.name().as_bytes()).map_err(|_| AfError::Internal)?;
        let mut value = HeaderValue::from_str(header.value()).map_err(|_| AfError::Internal)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

fn candidate_request(
    target: &af_db::SchedulerRuntimeTargetRecord,
    request: &CanonicalRequestEnvelope,
    requested_model: &str,
    upstream_model: &str,
) -> Result<Option<RelayCandidateRequest>, AfError> {
    let parameters = target.parameter_overrides();
    if upstream_model == requested_model && parameters.is_empty() {
        return Ok(None);
    }
    match target.protocol() {
        Protocol::OpenAiChat => OpenAiChatRequestOverrides::new(
            upstream_model,
            parameters.temperature(),
            parameters.top_p(),
            parameters.max_output_tokens(),
            parameters.stop_sequences().map(<[String]>::to_vec),
        )
        .and_then(|overrides| overrides.prepare(request.clone()))
        .map(Some)
        .map_err(|_| AfError::Internal),
        Protocol::OpenAiResponses => {
            if parameters.stop_sequences().is_some() {
                return Err(AfError::Internal);
            }
            OpenAiResponsesRequestOverrides::new(
                upstream_model,
                parameters.temperature(),
                parameters.top_p(),
                parameters.max_output_tokens(),
            )
            .and_then(|overrides| overrides.prepare(request.clone()))
            .map(Some)
            .map_err(|_| AfError::Internal)
        }
        Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech
        | Protocol::JinaRerank
        | Protocol::CohereRerank
        | Protocol::XaiVideo => Err(AfError::Internal),
        Protocol::Anthropic => AnthropicRequestOverrides::new(
            upstream_model,
            parameters.temperature(),
            parameters.top_p(),
            parameters.max_output_tokens(),
            parameters.stop_sequences().map(<[String]>::to_vec),
        )
        .and_then(|overrides| overrides.prepare(request.clone()))
        .map(Some)
        .map_err(|_| AfError::Internal),
        Protocol::Gemini => GeminiRequestOverrides::new(
            upstream_model,
            parameters.temperature(),
            parameters.top_p(),
            parameters.max_output_tokens(),
            parameters.stop_sequences().map(<[String]>::to_vec),
        )
        .and_then(|overrides| overrides.prepare(request.clone()))
        .map(Some)
        .map_err(|_| AfError::Internal),
    }
}

fn target_matches_protocol(
    target: &af_db::SchedulerRuntimeTargetRecord,
    protocol: Protocol,
) -> bool {
    target.protocol() == protocol
        && matches!(
            (target.channel_type(), protocol),
            (
                ChannelType::OpenAi,
                Protocol::OpenAiChat
                    | Protocol::OpenAiResponses
                    | Protocol::OpenAiEmbeddings
                    | Protocol::OpenAiImages
                    | Protocol::OpenAiAudio
                    | Protocol::OpenAiSpeech
            ) | (ChannelType::Anthropic, Protocol::Anthropic)
                | (ChannelType::Gemini, Protocol::Gemini)
                | (ChannelType::Jina, Protocol::JinaRerank)
                | (ChannelType::Cohere, Protocol::CohereRerank)
                | (ChannelType::Xai, Protocol::XaiVideo)
        )
}

fn adaptor_settings(
    target: &af_db::SchedulerRuntimeTargetRecord,
    upstream_model: String,
) -> Result<AdaptorSettings, AfError> {
    match (target.channel_type(), target.protocol()) {
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
            | Protocol::OpenAiResponses
            | Protocol::OpenAiEmbeddings
            | Protocol::OpenAiImages
            | Protocol::OpenAiAudio
            | Protocol::OpenAiSpeech,
        ) => Ok(AdaptorSettings::OpenAi(
            OpenAiAdaptorSettings::for_protocol(target.protocol(), vec![upstream_model])
                .map_err(|_| AfError::Internal)?,
        )),
        (ChannelType::Anthropic, Protocol::Anthropic) => Ok(AdaptorSettings::Anthropic(
            AnthropicAdaptorSettings::new(vec![upstream_model]),
        )),
        (ChannelType::Gemini, Protocol::Gemini) => {
            Ok(AdaptorSettings::Gemini(GeminiAdaptorSettings::new(vec![
                upstream_model,
            ])))
        }
        (ChannelType::Jina, Protocol::JinaRerank) => {
            Ok(AdaptorSettings::Jina(JinaAdaptorSettings::new(vec![
                upstream_model,
            ])))
        }
        (ChannelType::Cohere, Protocol::CohereRerank) => {
            Ok(AdaptorSettings::Cohere(CohereAdaptorSettings::new(vec![
                upstream_model,
            ])))
        }
        _ => Err(AfError::Internal),
    }
}

fn select_upstream_protocol(
    route: &IndexedRoutePlan,
    request: &CanonicalRequestEnvelope,
    response_protocol: Protocol,
) -> Result<Protocol, AfError> {
    match (request.canonical().operation, response_protocol) {
        (Operation::Chat, Protocol::OpenAiChat) => Ok(Protocol::OpenAiChat),
        (Operation::Chat, Protocol::Anthropic) => {
            // 原生 Anthropic 同协议优先；没有标准 Anthropic 渠道时保留既有 OpenAI 跨协议链路。
            if route.candidates().iter().any(|candidate| {
                target_matches_protocol(candidate.runtime_target(), Protocol::Anthropic)
            }) {
                Ok(Protocol::Anthropic)
            } else {
                Ok(Protocol::OpenAiChat)
            }
        }
        (Operation::Chat, Protocol::Gemini) => {
            // 原生 Gemini 同协议优先；没有标准 Gemini 渠道时保留既有 OpenAI 跨协议链路。
            if route.candidates().iter().any(|candidate| {
                target_matches_protocol(candidate.runtime_target(), Protocol::Gemini)
            }) {
                Ok(Protocol::Gemini)
            } else {
                Ok(Protocol::OpenAiChat)
            }
        }
        (Operation::Responses, Protocol::OpenAiResponses) => Ok(Protocol::OpenAiResponses),
        _ => Err(AfError::InvalidRequest),
    }
}

fn map_scheduler_error(error: IndexedWeightedSchedulerError) -> AfError {
    match error {
        IndexedWeightedSchedulerError::InvalidModel => AfError::InvalidRequest,
        IndexedWeightedSchedulerError::Cache(_)
        | IndexedWeightedSchedulerError::Selection(_)
        | IndexedWeightedSchedulerError::RetryPlan(_)
        | IndexedWeightedSchedulerError::StableFirst(_) => AfError::Internal,
        _ => AfError::Internal,
    }
}

#[cfg(test)]
fn responses_websocket_session_scope(
    principal: GatewayPrincipal,
    request: &af_protocol::CanonicalRequest,
) -> Option<String> {
    stable_session_scope(principal, request).map(|scope| scope.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use af_account::credential_plaintext_aad;
    use af_adapter::{Bytes, HeaderMap};
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_db::{
        AdminGroupRepository, AdminGroupWriteRecord, ChannelModelMappings,
        ChannelParameterOverrides, CredentialStateRepository, DatabaseOptions,
        EncryptedCredentialEnvelope, MigrationOptions, SchedulerRuntimeCredentialRecord,
        SchedulerRuntimeTargetRecord, SmartRouteRuntimeCandidate, SmartRouteRuntimeRule,
        connect_and_migrate,
    };
    use af_domain::{
        ChannelId, CredentialId, CredentialKind, Operation, RouteChannelId, RouteId, RouteMode,
        RouteStrategy, UpstreamError,
    };
    use af_httpclient::{HttpClientConfig, HttpClientProvider};
    use af_protocol::{CanonicalRequest, RequestContinuation, RequestMetadata, openai_chat};
    use af_scheduler::{
        ChannelIndexSource, ChannelIndexSourceFuture, ChannelIndexSourceRecord,
        InMemoryChannelIndex,
    };
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use chacha20poly1305::{
        XChaCha20Poly1305, XNonce,
        aead::{Aead as _, KeyInit as _, Payload},
    };
    use serde_json::json;

    use super::*;

    const CHANNEL_ID: i64 = 41;
    const CREDENTIAL_ID: i64 = 51;
    const KEY_ID: &str = "scheduled-chat-test";

    fn diagnostic() -> RelayDiagnosticInput {
        RelayDiagnosticInput::capture(
            "POST",
            "/v1/chat/completions",
            &HeaderMap::new(),
            &Bytes::new(),
        )
    }

    #[test]
    fn websocket_session_scope_requires_stable_metadata_and_isolates_tokens() {
        let empty = CanonicalRequest::new(
            Operation::Responses,
            "gpt-scheduled".to_owned(),
            Vec::new(),
            true,
        );
        assert!(responses_websocket_session_scope(principal(7), &empty).is_none());

        let mut request = empty;
        request.metadata = RequestMetadata::new(
            Some("private-user-canary".to_owned()),
            Some("private-session-canary".to_owned()),
        );
        request.continuation = RequestContinuation::new(
            None,
            Some("private-conversation-canary".to_owned()),
            Some("private-cache-canary".to_owned()),
        );
        let first = responses_websocket_session_scope(principal(7), &request).unwrap();
        let second_principal = GatewayPrincipal::new(
            af_domain::TokenId::new(2).unwrap(),
            af_domain::UserId::new(2).unwrap(),
            GroupId::new(7).unwrap(),
        );
        let second = responses_websocket_session_scope(second_principal, &request).unwrap();

        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
        for canary in [
            "private-user-canary",
            "private-session-canary",
            "private-conversation-canary",
            "private-cache-canary",
        ] {
            assert!(!first.contains(canary));
        }
    }

    #[tokio::test]
    async fn plan_pins_the_runtime_target_group_before_execution() {
        let service = service(vec![record(false, valid_envelope())]).await;
        let principal = principal(7);

        let plan = service
            .plan(
                &principal,
                None,
                request(),
                Protocol::OpenAiChat,
                "request-plan",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(plan.target_group_id(), GroupId::new(7).unwrap());
    }

    #[tokio::test]
    async fn playground_plan_uses_configured_fallback_without_changing_regular_keys() {
        let (service, source_group_id, fallback_group_id) =
            service_with_group_fallback(valid_envelope()).await;
        let playground = GatewayPrincipal::playground(
            af_domain::TokenId::new(1).unwrap(),
            af_domain::UserId::new(1).unwrap(),
            source_group_id,
        );

        let plan = service
            .plan(
                &playground,
                None,
                request(),
                Protocol::OpenAiChat,
                "request-playground-fallback",
                diagnostic(),
            )
            .await
            .unwrap();
        assert_eq!(plan.target_group_id(), fallback_group_id);

        let regular = GatewayPrincipal::new(
            af_domain::TokenId::new(2).unwrap(),
            af_domain::UserId::new(1).unwrap(),
            source_group_id,
        );
        assert!(matches!(
            service
                .plan(
                    &regular,
                    None,
                    request(),
                    Protocol::OpenAiChat,
                    "request-regular-no-fallback",
                    diagnostic(),
                )
                .await,
            Err(AfError::Upstream(UpstreamError::ModelUnsupported))
        ));
    }

    #[tokio::test]
    async fn playground_smart_route_maps_model_and_preserves_regular_key_boundary() {
        let (service, group_id) = service_with_smart_route(valid_envelope()).await;
        let playground = GatewayPrincipal::playground(
            af_domain::TokenId::new(1).unwrap(),
            af_domain::UserId::new(1).unwrap(),
            group_id,
        );

        let plan = service
            .plan(
                &playground,
                None,
                valid_request_for_model("smart-chat"),
                Protocol::OpenAiChat,
                "request-playground-smart-route",
                diagnostic(),
            )
            .await
            .unwrap();
        assert_eq!(plan.target_group_id(), group_id);
        assert_eq!(plan.pricing_model(), "gpt-routed");

        let regular = GatewayPrincipal::new(
            af_domain::TokenId::new(2).unwrap(),
            af_domain::UserId::new(1).unwrap(),
            group_id,
        );
        assert!(matches!(
            service
                .plan(
                    &regular,
                    None,
                    valid_request_for_model("smart-chat"),
                    Protocol::OpenAiChat,
                    "request-regular-smart-route",
                    diagnostic(),
                )
                .await,
            Err(AfError::Upstream(UpstreamError::ModelUnsupported))
        ));
    }

    #[tokio::test]
    async fn playground_smart_route_keeps_the_explicit_group_chain_as_billing_authority() {
        let (service, source_group_id, fallback_group_id) =
            service_with_smart_route_fallback(valid_envelope()).await;
        let playground = GatewayPrincipal::playground(
            af_domain::TokenId::new(1).unwrap(),
            af_domain::UserId::new(1).unwrap(),
            source_group_id,
        );

        let plan = service
            .plan(
                &playground,
                None,
                valid_request_for_model("smart-chat"),
                Protocol::OpenAiChat,
                "request-playground-smart-route-fallback",
                diagnostic(),
            )
            .await
            .unwrap();

        assert_eq!(plan.target_group_id(), fallback_group_id);
        assert_eq!(plan.pricing_model(), "gpt-routed");
    }

    #[tokio::test]
    async fn plan_expands_multiple_credentials_inside_one_channel_candidate() {
        let credentials = vec![
            SchedulerRuntimeCredentialRecord::with_scheduling(
                CREDENTIAL_ID,
                CredentialKind::ApiKey,
                valid_envelope_for(CREDENTIAL_ID, 0x24),
                false,
                10,
                10,
            )
            .unwrap(),
            SchedulerRuntimeCredentialRecord::with_scheduling(
                CREDENTIAL_ID + 1,
                CredentialKind::ApiKey,
                valid_envelope_for(CREDENTIAL_ID + 1, 0x25),
                false,
                0,
                10,
            )
            .unwrap(),
        ];
        let service = service(vec![record_pool(credentials)]).await;
        let route = service
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();

        let execution = service
            .build_plan(
                prepared_execution_route(route, Protocol::OpenAiChat),
                request(),
                Protocol::OpenAiChat,
                "request-key-pool",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();

        assert_eq!(execution.attempt_targets.len(), 2);
    }

    #[tokio::test]
    async fn spark_plan_uses_parent_aad_but_keeps_shadow_attempt_identity() {
        let parent_id = CredentialId::new(CREDENTIAL_ID).unwrap();
        let shadow_id = CredentialId::new(CREDENTIAL_ID + 1).unwrap();
        let parent_envelope = valid_oauth_envelope(true, 0x28);
        let parent = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::Oauth,
            parent_envelope.clone(),
            false,
        )
        .unwrap();
        let shadow = SchedulerRuntimeCredentialRecord::with_runtime_identity(
            shadow_id,
            parent_id,
            parent_id,
            parent_id,
            CredentialQuotaDimension::Spark,
            CredentialKind::Oauth,
            parent_envelope,
            false,
            0,
            10,
            None,
        )
        .unwrap();
        assert_eq!(shadow.concurrency_owner_id(), parent_id);

        let model = "gpt-5.3-codex-spark";
        let service = service(vec![record_pool_with_target_for_group_and_model(
            GroupId::new(7).unwrap(),
            model,
            vec![parent, shadow],
            ChannelType::OpenAi,
            Protocol::OpenAiResponses,
        )])
        .await;
        let route = service
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), model)
            .unwrap()
            .unwrap();

        assert_eq!(
            collect_route_credentials(&route, Protocol::OpenAiResponses, "gpt-5.3-codex").unwrap(),
            vec![(parent_id, parent_id)]
        );
        assert_eq!(
            collect_route_credentials(&route, Protocol::OpenAiResponses, model).unwrap(),
            vec![(shadow_id, parent_id)]
        );

        let execution = service
            .build_plan(
                PreparedExecutionRoute {
                    route,
                    upstream_protocol: Protocol::OpenAiResponses,
                    routed_model: model.to_owned(),
                },
                CanonicalRequest::new(Operation::Responses, model.to_owned(), Vec::new(), false)
                    .into(),
                Protocol::OpenAiResponses,
                "request-spark-identity",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();

        assert_eq!(execution.attempt_targets.len(), 1);
        assert_eq!(execution.attempt_targets[0].credential_id(), shadow_id);
        assert_eq!(execution.attempt_targets[0].secret_owner_id(), parent_id);
    }

    #[tokio::test]
    async fn plan_carries_only_oauth_refresh_capability_into_feedback() {
        for (oauth_has_refresh_token, marker) in [(false, 0x26), (true, 0x27)] {
            let credential = SchedulerRuntimeCredentialRecord::new(
                CREDENTIAL_ID,
                CredentialKind::Oauth,
                valid_oauth_envelope(oauth_has_refresh_token, marker),
                false,
            )
            .unwrap();
            let service = service(vec![record_pool(vec![credential])]).await;
            let route = service
                .scheduler
                .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
                .unwrap()
                .unwrap();

            let execution = service
                .build_plan(
                    prepared_execution_route(route, Protocol::OpenAiChat),
                    request(),
                    Protocol::OpenAiChat,
                    "request-oauth-refresh-capability",
                    None,
                    PreparedConcurrencyPlan::disabled(),
                )
                .unwrap();

            assert_eq!(execution.attempt_targets.len(), 1);
            let context = execution.attempt_targets[0].failure_context();
            assert_eq!(context.credential_kind(), CredentialKind::Oauth);
            assert_eq!(context.oauth_has_refresh_token(), oauth_has_refresh_token);
            let rendered = format!("{:?}", execution.attempt_targets);
            assert!(!rendered.contains("access-private"));
            assert!(!rendered.contains("refresh-private"));
        }
    }

    #[tokio::test]
    async fn plan_pins_the_client_response_protocol_for_execution() {
        let service = service(vec![anthropic_record(valid_envelope())]).await;
        let route = service
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();

        let execution = service
            .build_plan(
                prepared_execution_route(route, Protocol::Anthropic),
                request(),
                Protocol::Anthropic,
                "request-anthropic",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();

        assert_eq!(execution.response_protocol, Protocol::Anthropic);
        assert_eq!(execution.upstream_protocol, Protocol::Anthropic);
        assert_eq!(execution.attempt_targets.len(), 1);
    }

    #[tokio::test]
    async fn body_simulation_profile_requires_all_sendable_candidates_to_match() {
        let profile = ClientSimulationBodyProfile::AnthropicCliSystemDateV1;
        let target = |channel_id, body_profile| {
            SchedulerRuntimeTargetRecord::new_pool(
                ChannelId::new(channel_id).unwrap(),
                ChannelType::Anthropic,
                Protocol::Anthropic,
                Some("https://scheduled-chat.example".to_owned()),
                vec![
                    SchedulerRuntimeCredentialRecord::new(
                        channel_id + 100,
                        CredentialKind::Oauth,
                        valid_envelope(),
                        false,
                    )
                    .unwrap(),
                ],
                Vec::new(),
            )
            .unwrap()
            .with_client_simulation_profile(Some(
                af_domain::ClientSimulationProfile::AnthropicCliHeadersV1,
            ))
            .unwrap()
            .with_client_simulation_body_profile(body_profile)
            .unwrap()
        };
        let group_id = GroupId::new(7).unwrap();
        let record = |channel_id, body_profile| {
            ChannelIndexSourceRecord::with_runtime_target(
                group_id,
                "gpt-scheduled",
                10,
                0,
                Arc::new(target(channel_id, body_profile)),
            )
            .unwrap()
        };

        let matching_index = InMemoryChannelIndex::load(Arc::new(StaticSource {
            records: vec![
                record(CHANNEL_ID, Some(profile)),
                record(CHANNEL_ID + 1, Some(profile)),
            ],
        }))
        .await
        .unwrap();
        let matching_route = IndexedWeightedScheduler::new(matching_index)
            .route_plan(group_id, "gpt-scheduled")
            .unwrap()
            .unwrap();
        assert_eq!(
            route_client_simulation_body_profile(
                &matching_route,
                Protocol::Anthropic,
                "gpt-scheduled",
                "body-profile-matching",
                None,
            ),
            Ok(Some(profile))
        );

        let mixed_index = InMemoryChannelIndex::load(Arc::new(StaticSource {
            records: vec![
                record(CHANNEL_ID, Some(profile)),
                record(CHANNEL_ID + 1, None),
            ],
        }))
        .await
        .unwrap();
        let mixed_route = IndexedWeightedScheduler::new(mixed_index)
            .route_plan(group_id, "gpt-scheduled")
            .unwrap()
            .unwrap();
        assert_eq!(
            route_client_simulation_body_profile(
                &mixed_route,
                Protocol::Anthropic,
                "gpt-scheduled",
                "body-profile-mixed",
                None,
            ),
            Err(AfError::InvalidRequest)
        );
    }

    #[tokio::test]
    async fn anthropic_client_keeps_openai_chat_fallback_without_native_channel() {
        let service = service(vec![record(false, valid_envelope())]).await;
        let route = service
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();

        let execution = service
            .build_plan(
                prepared_execution_route(route, Protocol::OpenAiChat),
                request(),
                Protocol::Anthropic,
                "request-anthropic-fallback",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();

        assert_eq!(execution.response_protocol, Protocol::Anthropic);
        assert_eq!(execution.upstream_protocol, Protocol::OpenAiChat);
        assert_eq!(execution.attempt_targets.len(), 1);
    }

    #[tokio::test]
    async fn gemini_client_prefers_native_channel_and_keeps_openai_fallback() {
        let native = service(vec![gemini_record(valid_envelope_for(
            CREDENTIAL_ID + 1,
            0x26,
        ))])
        .await;
        let route = native
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();
        let execution = native
            .build_plan(
                prepared_execution_route(route, Protocol::Gemini),
                request(),
                Protocol::Gemini,
                "request-gemini-native",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();
        assert_eq!(execution.response_protocol, Protocol::Gemini);
        assert_eq!(execution.upstream_protocol, Protocol::Gemini);
        assert_eq!(execution.attempt_targets.len(), 1);

        let fallback = service(vec![record(false, valid_envelope())]).await;
        let route = fallback
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();
        let execution = fallback
            .build_plan(
                prepared_execution_route(route, Protocol::OpenAiChat),
                request(),
                Protocol::Gemini,
                "request-gemini-fallback",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();
        assert_eq!(execution.response_protocol, Protocol::Gemini);
        assert_eq!(execution.upstream_protocol, Protocol::OpenAiChat);
        assert_eq!(execution.attempt_targets.len(), 1);
    }

    #[tokio::test]
    async fn openai_client_does_not_schedule_anthropic_upstream() {
        let service = service(vec![anthropic_record(valid_envelope())]).await;

        assert_eq!(
            service
                .plan(
                    &principal(7),
                    None,
                    request(),
                    Protocol::OpenAiChat,
                    "request-openai-client",
                    diagnostic(),
                )
                .await
                .err(),
            Some(AfError::Upstream(UpstreamError::ModelUnsupported))
        );
    }

    #[tokio::test]
    async fn openai_client_does_not_schedule_gemini_upstream() {
        let service = service(vec![gemini_record(valid_envelope())]).await;

        assert_eq!(
            service
                .plan(
                    &principal(7),
                    None,
                    request(),
                    Protocol::OpenAiChat,
                    "request-openai-gemini-upstream",
                    diagnostic(),
                )
                .await
                .err(),
            Some(AfError::Upstream(UpstreamError::ModelUnsupported))
        );
    }

    #[tokio::test]
    async fn responses_plan_requires_a_native_responses_channel() {
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            valid_envelope(),
            false,
        )
        .unwrap();
        let responses = service(vec![record_pool_with_protocol(
            vec![credential],
            Protocol::OpenAiResponses,
        )])
        .await;
        let route = responses
            .scheduler
            .route_plan(GroupId::new(7).unwrap(), "gpt-scheduled")
            .unwrap()
            .unwrap();
        let execution = responses
            .build_plan(
                prepared_execution_route(route, Protocol::OpenAiResponses),
                responses_request(),
                Protocol::OpenAiResponses,
                "request-responses",
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();
        assert_eq!(execution.response_protocol, Protocol::OpenAiResponses);
        assert_eq!(execution.attempt_targets.len(), 1);

        let chat_only = service(vec![record(false, valid_envelope())]).await;
        assert_eq!(
            chat_only
                .plan(
                    &principal(7),
                    None,
                    responses_request(),
                    Protocol::OpenAiResponses,
                    "request-wrong-protocol",
                    diagnostic(),
                )
                .await
                .err(),
            Some(AfError::Upstream(UpstreamError::ModelUnsupported))
        );
    }

    #[tokio::test]
    async fn missing_candidates_proxy_binding_and_decryption_fail_closed() {
        let principal = principal(7);
        let empty = service(Vec::new()).await;
        assert_eq!(
            empty
                .plan(
                    &principal,
                    None,
                    request(),
                    Protocol::OpenAiChat,
                    "request-empty",
                    diagnostic(),
                )
                .await
                .err(),
            Some(AfError::Upstream(UpstreamError::ModelUnsupported))
        );

        let proxy_required = service(vec![record(true, valid_envelope())]).await;
        assert_eq!(
            proxy_required
                .plan(
                    &principal,
                    None,
                    request(),
                    Protocol::OpenAiChat,
                    "request-proxy",
                    diagnostic(),
                )
                .await
                .err(),
            Some(AfError::Internal)
        );

        let broken = EncryptedCredentialEnvelope::new(KEY_ID, [0x11; 24], vec![0x22; 32]).unwrap();
        let decrypt_failure = service(vec![record(false, broken)]).await;
        assert_eq!(
            decrypt_failure
                .plan(
                    &principal,
                    None,
                    request(),
                    Protocol::OpenAiChat,
                    "request-decrypt",
                    diagnostic(),
                )
                .await
                .err(),
            Some(AfError::Internal)
        );
    }

    #[test]
    fn runtime_policy_builds_candidate_request_and_empty_policy_preserves_passthrough() {
        let request = openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"gpt-scheduled","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            valid_envelope(),
            false,
        )
        .unwrap();
        let configured = SchedulerRuntimeTargetRecord::new_pool_with_request_policy(
            ChannelId::new(CHANNEL_ID).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://scheduled-chat.example".to_owned()),
            vec![credential.clone()],
            ChannelModelMappings::parse(&json!({
                "gpt-scheduled": "private-upstream-model"
            }))
            .unwrap(),
            ChannelParameterOverrides::parse(&json!({
                "temperature": 0.25,
                "stop_sequences": ["private-stop-canary"]
            }))
            .unwrap(),
            Vec::new(),
        )
        .unwrap();

        let candidate = candidate_request(
            &configured,
            &request,
            "gpt-scheduled",
            "private-upstream-model",
        )
        .unwrap()
        .unwrap();
        let rendered = format!("{configured:?}\n{candidate:?}");
        assert!(!rendered.contains("private-upstream-model"));
        assert!(!rendered.contains("private-stop-canary"));

        let passthrough = SchedulerRuntimeTargetRecord::new(
            ChannelId::new(CHANNEL_ID).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://scheduled-chat.example".to_owned()),
            credential,
            vec![("x-provider-feature".to_owned(), "enabled".to_owned())],
        )
        .unwrap();
        assert!(
            candidate_request(&passthrough, &request, "gpt-scheduled", "gpt-scheduled")
                .unwrap()
                .is_none()
        );
    }

    async fn service(records: Vec<ChannelIndexSourceRecord>) -> ScheduledChatService {
        let index = InMemoryChannelIndex::load(Arc::new(StaticSource { records }))
            .await
            .unwrap();
        ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            decryptor(),
            HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap(),
            CredentialStateRepository::new(
                connect_and_migrate(
                    &DatabaseOptions::new("sqlite::memory:").unwrap(),
                    MigrationOptions::default(),
                )
                .await
                .unwrap(),
            ),
        )
    }

    async fn service_with_group_fallback(
        envelope: EncryptedCredentialEnvelope,
    ) -> (ScheduledChatService, GroupId, GroupId) {
        let pool = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repository = AdminGroupRepository::new(pool.clone(), Duration::from_secs(1)).unwrap();
        let fallback = repository
            .create(group_record("playground-fallback", None))
            .await
            .unwrap();
        let source = repository
            .create(group_record("playground-source", Some(fallback.group_id())))
            .await
            .unwrap();
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            envelope,
            false,
        )
        .unwrap();
        let record = record_pool_with_target_for_group(
            fallback.group_id(),
            vec![credential],
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
        );
        let index = InMemoryChannelIndex::load(Arc::new(StaticSource {
            records: vec![record],
        }))
        .await
        .unwrap();
        let service = ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            decryptor(),
            HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap(),
            CredentialStateRepository::new(pool),
        )
        .with_group_repository(repository);
        (service, source.group_id(), fallback.group_id())
    }

    async fn service_with_smart_route(
        envelope: EncryptedCredentialEnvelope,
    ) -> (ScheduledChatService, GroupId) {
        let pool = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let groups = AdminGroupRepository::new(pool.clone(), Duration::from_secs(1)).unwrap();
        let group = groups
            .create(group_record("playground-smart-route", None))
            .await
            .unwrap();
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            envelope,
            false,
        )
        .unwrap();
        let record = record_pool_with_target_for_group_and_model(
            group.group_id(),
            "gpt-routed",
            vec![credential],
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
        );
        let index = InMemoryChannelIndex::load(Arc::new(StaticSource {
            records: vec![record],
        }))
        .await
        .unwrap();
        let service = ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            decryptor(),
            HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap(),
            CredentialStateRepository::new(pool),
        )
        .with_group_repository(groups)
        .with_smart_route_resolver(Arc::new(StaticSmartRouteResolver));
        (service, group.group_id())
    }

    async fn service_with_smart_route_fallback(
        envelope: EncryptedCredentialEnvelope,
    ) -> (ScheduledChatService, GroupId, GroupId) {
        let pool = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let groups = AdminGroupRepository::new(pool.clone(), Duration::from_secs(1)).unwrap();
        let fallback = groups
            .create(group_record("smart-route-fallback", None))
            .await
            .unwrap();
        let source = groups
            .create(group_record(
                "smart-route-source",
                Some(fallback.group_id()),
            ))
            .await
            .unwrap();
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            envelope,
            false,
        )
        .unwrap();
        let record = record_pool_with_target_for_group_and_model(
            fallback.group_id(),
            "gpt-routed",
            vec![credential],
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
        );
        let index = InMemoryChannelIndex::load(Arc::new(StaticSource {
            records: vec![record],
        }))
        .await
        .unwrap();
        let service = ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            decryptor(),
            HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap(),
            CredentialStateRepository::new(pool),
        )
        .with_group_repository(groups)
        .with_smart_route_resolver(Arc::new(StaticSmartRouteResolver));
        (service, source.group_id(), fallback.group_id())
    }

    fn group_record(name: &str, fallback_group_id: Option<GroupId>) -> AdminGroupWriteRecord {
        AdminGroupWriteRecord::new(
            name.to_owned(),
            name.to_owned(),
            1_000_000,
            None,
            false,
            None,
            None,
            None,
            None,
            fallback_group_id,
            json!({}),
        )
    }

    fn record(
        proxy_required: bool,
        envelope: EncryptedCredentialEnvelope,
    ) -> ChannelIndexSourceRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            envelope,
            proxy_required,
        )
        .unwrap();
        record_pool(vec![credential])
    }

    fn record_pool(credentials: Vec<SchedulerRuntimeCredentialRecord>) -> ChannelIndexSourceRecord {
        record_pool_with_protocol(credentials, Protocol::OpenAiChat)
    }

    fn record_pool_with_protocol(
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        protocol: Protocol,
    ) -> ChannelIndexSourceRecord {
        record_pool_with_target(credentials, ChannelType::OpenAi, protocol)
    }

    fn anthropic_record(envelope: EncryptedCredentialEnvelope) -> ChannelIndexSourceRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID,
            CredentialKind::ApiKey,
            envelope,
            false,
        )
        .unwrap();
        record_pool_with_target(
            vec![credential],
            ChannelType::Anthropic,
            Protocol::Anthropic,
        )
    }

    fn gemini_record(envelope: EncryptedCredentialEnvelope) -> ChannelIndexSourceRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            CREDENTIAL_ID + 1,
            CredentialKind::ApiKey,
            envelope,
            false,
        )
        .unwrap();
        record_pool_with_target(vec![credential], ChannelType::Gemini, Protocol::Gemini)
    }

    fn record_pool_with_target(
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        channel_type: ChannelType,
        protocol: Protocol,
    ) -> ChannelIndexSourceRecord {
        record_pool_with_target_for_group(
            GroupId::new(7).unwrap(),
            credentials,
            channel_type,
            protocol,
        )
    }

    fn record_pool_with_target_for_group(
        group_id: GroupId,
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        channel_type: ChannelType,
        protocol: Protocol,
    ) -> ChannelIndexSourceRecord {
        record_pool_with_target_for_group_and_model(
            group_id,
            "gpt-scheduled",
            credentials,
            channel_type,
            protocol,
        )
    }

    fn record_pool_with_target_for_group_and_model(
        group_id: GroupId,
        model: &str,
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        channel_type: ChannelType,
        protocol: Protocol,
    ) -> ChannelIndexSourceRecord {
        let target = SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(CHANNEL_ID).unwrap(),
            channel_type,
            protocol,
            Some("https://scheduled-chat.example".to_owned()),
            credentials,
            vec![("x-scheduled-test".to_owned(), "test-value".to_owned())],
        )
        .unwrap();
        ChannelIndexSourceRecord::with_runtime_target(group_id, model, 10, 0, Arc::new(target))
            .unwrap()
    }

    fn request() -> CanonicalRequestEnvelope {
        request_for_model("gpt-scheduled")
    }

    fn prepared_execution_route(
        route: IndexedRoutePlan,
        upstream_protocol: Protocol,
    ) -> PreparedExecutionRoute {
        PreparedExecutionRoute {
            route,
            upstream_protocol,
            routed_model: "gpt-scheduled".to_owned(),
        }
    }

    fn request_for_model(model: &str) -> CanonicalRequestEnvelope {
        CanonicalRequest::new(Operation::Chat, model.to_owned(), Vec::new(), false).into()
    }

    fn valid_request_for_model(model: &str) -> CanonicalRequestEnvelope {
        openai_chat::parse_request_envelope(Bytes::from(
            serde_json::to_vec(&json!({
                "model": model,
                "messages": [{"role": "user", "content": "hello"}]
            }))
            .unwrap(),
        ))
        .unwrap()
    }

    fn responses_request() -> CanonicalRequestEnvelope {
        CanonicalRequest::new(
            Operation::Responses,
            "gpt-scheduled".to_owned(),
            Vec::new(),
            false,
        )
        .into()
    }

    fn principal(group_id: i64) -> GatewayPrincipal {
        GatewayPrincipal::new(
            af_domain::TokenId::new(1).unwrap(),
            af_domain::UserId::new(2).unwrap(),
            GroupId::new(group_id).unwrap(),
        )
    }

    fn decryptor() -> CredentialDecryptor {
        let settings = serde_json::from_value::<CredentialEncryptionSettings>(json!({
            "key_id": KEY_ID,
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES]),
        }))
        .unwrap();
        CredentialDecryptor::new(&settings).unwrap()
    }

    fn valid_envelope() -> EncryptedCredentialEnvelope {
        valid_envelope_for(CREDENTIAL_ID, 0x24)
    }

    fn valid_envelope_for(credential_id: i64, marker: u8) -> EncryptedCredentialEnvelope {
        encrypted_envelope_for(
            credential_id,
            marker,
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"scheduled-secret"}"#,
        )
    }

    fn valid_oauth_envelope(
        oauth_has_refresh_token: bool,
        marker: u8,
    ) -> EncryptedCredentialEnvelope {
        let plaintext: &[u8] = if oauth_has_refresh_token {
            br#"{"kind":"oauth","access_token":"access-private","refresh_token":"refresh-private"}"#
        } else {
            br#"{"kind":"oauth","access_token":"access-private"}"#
        };
        encrypted_envelope_for(CREDENTIAL_ID, marker, CredentialKind::Oauth, plaintext)
    }

    fn encrypted_envelope_for(
        credential_id: i64,
        marker: u8,
        credential_kind: CredentialKind,
        plaintext: &[u8],
    ) -> EncryptedCredentialEnvelope {
        let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let nonce = [marker; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &credential_plaintext_aad(
                        ChannelId::new(CHANNEL_ID).unwrap(),
                        credential_id,
                        credential_kind,
                    ),
                },
            )
            .unwrap();
        EncryptedCredentialEnvelope::new(KEY_ID, nonce, ciphertext).unwrap()
    }

    #[derive(Clone)]
    struct StaticSource {
        records: Vec<ChannelIndexSourceRecord>,
    }

    impl ChannelIndexSource for StaticSource {
        fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
            let records = self.records.clone();
            Box::pin(async move { Ok(records) })
        }
    }

    struct StaticSmartRouteResolver;

    impl SmartRouteResolver for StaticSmartRouteResolver {
        fn matching_rules<'a>(&'a self, requested_model: &'a str) -> SmartRouteRulesFuture<'a> {
            Box::pin(async move {
                if requested_model != "smart-chat" {
                    return Ok(Vec::new());
                }
                let candidate = SmartRouteRuntimeCandidate::new(
                    RouteChannelId::new(1).unwrap(),
                    ChannelId::new(CHANNEL_ID).unwrap(),
                    CredentialId::new(CREDENTIAL_ID).unwrap(),
                    10,
                    10,
                    None,
                )?;
                SmartRouteRuntimeRule::new(
                    RouteId::new(1).unwrap(),
                    RouteMode::ExplicitGroup,
                    RouteStrategy::Weighted,
                    "gpt-routed".to_owned(),
                    vec![candidate],
                )
                .map(|rule| vec![rule])
            })
        }
    }
}
