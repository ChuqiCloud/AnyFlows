use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use crate::{
    ChatRoutePlanFuture, ChatRoutePlanner, PlannedChatExecution, PlannedChatExecutionFuture,
    adaptor_credential::build_adaptor_credential,
    concurrency_runtime::{
        ConcurrencyRuntime, ConcurrencyRuntimeError, RuntimeConcurrencyPermit,
        user_permit_completion_hook,
    },
    credential_feedback::{CredentialAttemptTarget, persist_credential_feedback},
    credential_order::{
        CredentialLoad, order_credentials, order_credentials_by_load,
        order_credentials_by_load_and_health, order_credentials_with_health,
    },
    health_runtime::{
        RuntimeHealthState, SchedulerHealthStore, load_channel_health, load_credential_health,
        persist_scheduler_health_feedback,
    },
    responses_websocket_runtime::ResponsesWebSocketRuntime,
    sticky_session::{
        StableSessionScope, StickySessionStore, stable_session_scope, sticky_cache_error_kind,
    },
};
use af_account::CredentialDecryptor;
use af_adapter::{
    AdaptorSettings, AnthropicAdaptorSettings, GeminiAdaptorSettings, HeaderMap, HeaderName,
    HeaderValue, OpenAiAdaptorSettings, RelayContext, ResponsesWebSocketPoolKey,
    TransportDispatcher, get_adaptor,
};
use af_db::CredentialStateRepository;
use af_domain::{
    AfError, ChannelType, ConcurrencyLimit, CredentialId, GatewayPrincipal, GroupId, Operation,
    Protocol,
};
use af_httpclient::HttpClientProvider;
use af_protocol::CanonicalRequestEnvelope;
use af_relay::{
    AnthropicRequestOverrides, ChatResponse, GeminiRequestOverrides, OpenAiChatRequestOverrides,
    OpenAiResponsesRequestOverrides, RelayCandidate, RelayCandidateRequest, RelayStateMachine,
    relay_anthropic_with_report, relay_gemini_with_report, relay_openai_chat_with_report,
    relay_openai_responses_with_report,
};
use af_scheduler::{
    ChannelRoutingHealth, IndexedRoutePlan, IndexedWeightedScheduler,
    IndexedWeightedSchedulerError, RouteWaitKind, StickyRouteOutcome, StickyWaitPolicy,
};

/// 路由计划在同一次 Redis 快照中取得的账号负载与用户许可。
struct PreparedConcurrencyPlan<'a> {
    account_loads: Option<&'a BTreeMap<i64, CredentialLoad>>,
    credential_health: Option<&'a BTreeMap<i64, RuntimeHealthState>>,
    user_permit: Option<RuntimeConcurrencyPermit>,
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
    credential_states: CredentialStateRepository,
    responses_websocket: std::sync::Arc<ResponsesWebSocketRuntime>,
    sticky_sessions: Option<Arc<dyn StickySessionStore>>,
    sticky_wait_policy: StickyWaitPolicy,
    concurrency: Option<ConcurrencyRuntime>,
    health: Option<Arc<dyn SchedulerHealthStore>>,
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
            credential_states,
            responses_websocket: std::sync::Arc::new(ResponsesWebSocketRuntime::default()),
            sticky_sessions: None,
            sticky_wait_policy: StickyWaitPolicy::default(),
            concurrency: None,
            health: None,
        }
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

    fn build_plan(
        &self,
        route: IndexedRoutePlan,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &str,
        upstream_protocol: Protocol,
        session_scope: Option<StableSessionScope>,
        concurrency: PreparedConcurrencyPlan<'_>,
    ) -> Result<ScheduledChatExecution, AfError> {
        let PreparedConcurrencyPlan {
            account_loads,
            credential_health,
            user_permit,
        } = concurrency;
        let model = request.canonical().model.clone();
        let responses_websocket_session = session_scope.as_ref().map(StableSessionScope::as_str);
        let target_group_id = route.target_group_id();
        let mut relay_candidates = Vec::with_capacity(
            route
                .candidates()
                .len()
                .min(RelayStateMachine::MAX_CANDIDATES),
        );
        let mut attempt_targets = Vec::with_capacity(relay_candidates.capacity());
        let mut health_filtered = false;
        'channels: for candidate in route.candidates() {
            let target = candidate.runtime_target();
            let wait_plan = candidate.wait_plan();
            if !target_matches_protocol(target, upstream_protocol) {
                continue;
            }
            if target
                .credentials()
                .iter()
                .any(af_db::SchedulerRuntimeCredentialRecord::proxy_required)
            {
                // 专属代理仓储尚未落地，绑定代理的凭据绝不能回退共享 Client 或直连。
                return Err(AfError::Internal);
            }
            let upstream_model = target.mapped_model(&model).unwrap_or(&model).to_owned();
            let candidate_request = candidate_request(target, &request, &model, &upstream_model)?;
            let adaptor = get_adaptor(
                target.channel_type(),
                adaptor_settings(target, upstream_model)?,
            )
            .map_err(|_| AfError::Internal)?;
            if adaptor.default_protocol() != target.protocol() {
                return Err(AfError::Internal);
            }

            let client = self
                .clients
                .get(target.timeout().map(af_domain::ChannelTimeout::duration))
                .map_err(|_| AfError::Internal)?;
            let responses_websocket_pool =
                if target.responses_websocket_enabled() && responses_websocket_session.is_some() {
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
            context = context
                .with_request_id(request_id.to_owned())
                .map_err(|_| AfError::Internal)?;
            let headers = header_overrides(target)?;
            let order_scope = session_scope
                .as_ref()
                .map_or(request_id, StableSessionScope::as_str);
            let ordered_credentials = match (wait_plan.kind(), account_loads, credential_health) {
                (RouteWaitKind::Fallback, Some(loads), Some(health)) => {
                    order_credentials_by_load_and_health(
                        order_scope,
                        target.channel_id(),
                        target.credentials(),
                        loads,
                        health,
                    )
                }
                (RouteWaitKind::Fallback, Some(loads), None) => order_credentials_by_load(
                    order_scope,
                    target.channel_id(),
                    target.credentials(),
                    loads,
                ),
                (_, _, Some(health)) => order_credentials_with_health(
                    order_scope,
                    target.channel_id(),
                    target.credentials(),
                    health,
                ),
                _ => order_credentials(order_scope, target.channel_id(), target.credentials()),
            };
            health_filtered |= !target.credentials().is_empty() && ordered_credentials.is_empty();
            for runtime_credential in ordered_credentials {
                if relay_candidates.len() == RelayStateMachine::MAX_CANDIDATES {
                    break 'channels;
                }
                let credential_id = CredentialId::new(runtime_credential.credential_id())
                    .map_err(|_| AfError::Internal)?;
                let decrypted = self
                    .decryptor
                    .decrypt_envelope(
                        target.channel_id(),
                        runtime_credential.credential_id(),
                        runtime_credential.credential_kind(),
                        runtime_credential.envelope(),
                    )
                    .map_err(|_| AfError::Internal)?;
                let credential =
                    build_adaptor_credential(&decrypted).map_err(|_| AfError::Internal)?;
                let mut relay_candidate =
                    RelayCandidate::new(adaptor.clone(), context.clone(), credential)
                        .with_header_overrides(headers.clone())
                        .with_channel_group(target.channel_id());
                if let Some(concurrency) = &self.concurrency {
                    relay_candidate = relay_candidate.with_attempt_gate(concurrency.account_gate(
                        credential_id,
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
                        credential_id,
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
                attempt_targets.push(CredentialAttemptTarget::new(
                    target.channel_id(),
                    runtime_credential.credential_id(),
                    runtime_credential.credential_kind(),
                )?);
            }
        }
        if relay_candidates.is_empty() {
            return Err(if health_filtered {
                af_domain::UpstreamError::Overloaded.into()
            } else {
                af_domain::UpstreamError::ModelUnsupported.into()
            });
        }
        let machine = RelayStateMachine::new(relay_candidates).map_err(|_| AfError::Internal)?;
        Ok(ScheduledChatExecution {
            target_group_id,
            machine,
            attempt_targets,
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
        })
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
    ) -> Result<Option<BTreeMap<i64, CredentialLoad>>, AfError> {
        let Some(concurrency) = &self.concurrency else {
            return Ok(None);
        };
        let accounts = collect_route_accounts(route, upstream_protocol)?;
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
    ) -> Result<Option<BTreeMap<i64, RuntimeHealthState>>, AfError> {
        let Some(store) = &self.health else {
            return Ok(None);
        };
        let credential_ids = collect_route_credentials(route, upstream_protocol)?;
        load_credential_health(store, &credential_ids)
            .await
            .map(Some)
            .map_err(|_| AfError::Internal)
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
    ) -> ChatRoutePlanFuture<'a> {
        Box::pin(async move {
            let mut route = if self.health.is_some() {
                let (channel_ids, channel_health) = self
                    .load_channel_health(principal.group_id(), &request.canonical().model)
                    .await?;
                self.scheduler
                    .route_plan_with_health(
                        principal.group_id(),
                        &request.canonical().model,
                        &channel_health,
                    )
                    .map_err(map_scheduler_error)?
                    .ok_or_else(|| {
                        AfError::from(if channel_ids.is_empty() {
                            af_domain::UpstreamError::ModelUnsupported
                        } else {
                            af_domain::UpstreamError::Overloaded
                        })
                    })?
            } else {
                self.scheduler
                    .route_plan(principal.group_id(), &request.canonical().model)
                    .map_err(map_scheduler_error)?
                    .ok_or_else(|| AfError::from(af_domain::UpstreamError::ModelUnsupported))?
            };
            let upstream_protocol = select_upstream_protocol(&route, &request, response_protocol)?;
            route.configure_wait_policy(self.sticky_wait_policy);
            let session_scope = stable_session_scope(*principal, request.canonical());
            self.apply_sticky_binding(&mut route, upstream_protocol, session_scope.as_ref())
                .await;
            let credential_health = self
                .load_credential_health(&route, upstream_protocol)
                .await?;
            let user_permit = self
                .acquire_user_permit(*principal, user_concurrency)
                .await?;
            let account_loads = self
                .load_account_concurrency(&route, upstream_protocol)
                .await?;
            self.build_plan(
                route,
                request,
                response_protocol,
                request_id,
                upstream_protocol,
                session_scope,
                PreparedConcurrencyPlan {
                    account_loads: account_loads.as_ref(),
                    credential_health: credential_health.as_ref(),
                    user_permit,
                },
            )
            .map(|plan| Box::new(plan) as Box<dyn PlannedChatExecution>)
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
            .finish()
    }
}

struct ScheduledChatExecution {
    target_group_id: GroupId,
    machine: RelayStateMachine,
    attempt_targets: Vec<CredentialAttemptTarget>,
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
}

impl PlannedChatExecution for ScheduledChatExecution {
    fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }

    fn execute(self: Box<Self>) -> PlannedChatExecutionFuture {
        let Self {
            target_group_id: _,
            machine,
            attempt_targets,
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
        } = *self;
        Box::pin(async move {
            let operation = request.canonical().operation;
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
            let successful_channel_id = report
                .successful_candidate_index()
                .and_then(|index| attempt_targets.get(index))
                .map(CredentialAttemptTarget::channel_id)
                .map(af_domain::ChannelId::get);
            let ((), ()) = tokio::join!(
                persist_credential_feedback(
                    &credential_states,
                    &scheduler,
                    &attempt_targets,
                    &report,
                ),
                persist_scheduler_health_feedback(health.as_ref(), &attempt_targets, &report),
            );
            if result.is_ok()
                && let (Some(store), Some(scope), Some(channel_id)) = (
                    sticky_sessions.as_ref(),
                    session_scope.as_ref(),
                    successful_channel_id,
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
                    Ok(ChatResponse::Full { body, usage })
                }
                Ok(ChatResponse::Stream { body, usage }) => {
                    let body = match user_permit {
                        Some(permit) => {
                            body.with_completion_hook(user_permit_completion_hook(permit))
                        }
                        None => body,
                    };
                    Ok(ChatResponse::Stream { body, usage })
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

fn collect_route_accounts(
    route: &IndexedRoutePlan,
    upstream_protocol: Protocol,
) -> Result<Vec<(CredentialId, Option<ConcurrencyLimit>)>, AfError> {
    let mut accounts = Vec::with_capacity(RelayStateMachine::MAX_CANDIDATES);
    'channels: for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !target_matches_protocol(target, upstream_protocol) {
            continue;
        }
        for credential in target.credentials() {
            if accounts.len() == RelayStateMachine::MAX_CANDIDATES {
                break 'channels;
            }
            accounts.push((
                CredentialId::new(credential.credential_id()).map_err(|_| AfError::Internal)?,
                credential.concurrency(),
            ));
        }
    }
    Ok(accounts)
}

fn collect_route_credentials(
    route: &IndexedRoutePlan,
    upstream_protocol: Protocol,
) -> Result<Vec<CredentialId>, AfError> {
    let mut credential_ids = Vec::with_capacity(RelayStateMachine::MAX_CANDIDATES);
    let mut unique = BTreeSet::new();
    'channels: for candidate in route.candidates() {
        let target = candidate.runtime_target();
        if !target_matches_protocol(target, upstream_protocol) {
            continue;
        }
        for credential in target.credentials() {
            if credential_ids.len() == RelayStateMachine::MAX_CANDIDATES {
                break 'channels;
            }
            let credential_id =
                CredentialId::new(credential.credential_id()).map_err(|_| AfError::Internal)?;
            if !unique.insert(credential_id) {
                return Err(AfError::Internal);
            }
            credential_ids.push(credential_id);
        }
    }
    Ok(credential_ids)
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
                Protocol::OpenAiChat | Protocol::OpenAiResponses
            ) | (ChannelType::Anthropic, Protocol::Anthropic)
                | (ChannelType::Gemini, Protocol::Gemini)
        )
}

fn adaptor_settings(
    target: &af_db::SchedulerRuntimeTargetRecord,
    upstream_model: String,
) -> Result<AdaptorSettings, AfError> {
    match (target.channel_type(), target.protocol()) {
        (ChannelType::OpenAi, Protocol::OpenAiChat | Protocol::OpenAiResponses) => {
            Ok(AdaptorSettings::OpenAi(
                OpenAiAdaptorSettings::for_protocol(target.protocol(), vec![upstream_model])
                    .map_err(|_| AfError::Internal)?,
            ))
        }
        (ChannelType::Anthropic, Protocol::Anthropic) => Ok(AdaptorSettings::Anthropic(
            AnthropicAdaptorSettings::new(vec![upstream_model]),
        )),
        (ChannelType::Gemini, Protocol::Gemini) => {
            Ok(AdaptorSettings::Gemini(GeminiAdaptorSettings::new(vec![
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
        | IndexedWeightedSchedulerError::RetryPlan(_) => AfError::Internal,
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
    use std::sync::Arc;

    use af_account::credential_plaintext_aad;
    use af_adapter::Bytes;
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_db::{
        ChannelModelMappings, ChannelParameterOverrides, CredentialStateRepository,
        DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions,
        SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord, connect_and_migrate,
    };
    use af_domain::{ChannelId, CredentialKind, Operation, UpstreamError};
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
            )
            .await
            .unwrap();

        assert_eq!(plan.target_group_id(), GroupId::new(7).unwrap());
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
                route,
                request(),
                Protocol::OpenAiChat,
                "request-key-pool",
                Protocol::OpenAiChat,
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();

        assert_eq!(execution.attempt_targets.len(), 2);
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
                route,
                request(),
                Protocol::Anthropic,
                "request-anthropic",
                Protocol::Anthropic,
                None,
                PreparedConcurrencyPlan::disabled(),
            )
            .unwrap();

        assert_eq!(execution.response_protocol, Protocol::Anthropic);
        assert_eq!(execution.upstream_protocol, Protocol::Anthropic);
        assert_eq!(execution.attempt_targets.len(), 1);
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
                route,
                request(),
                Protocol::Anthropic,
                "request-anthropic-fallback",
                Protocol::OpenAiChat,
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
                route,
                request(),
                Protocol::Gemini,
                "request-gemini-native",
                Protocol::Gemini,
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
                route,
                request(),
                Protocol::Gemini,
                "request-gemini-fallback",
                Protocol::OpenAiChat,
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
                route,
                responses_request(),
                Protocol::OpenAiResponses,
                "request-responses",
                Protocol::OpenAiResponses,
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
        let target = SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(CHANNEL_ID).unwrap(),
            channel_type,
            protocol,
            Some("https://scheduled-chat.example".to_owned()),
            credentials,
            vec![("x-scheduled-test".to_owned(), "test-value".to_owned())],
        )
        .unwrap();
        ChannelIndexSourceRecord::with_runtime_target(
            GroupId::new(7).unwrap(),
            "gpt-scheduled",
            10,
            0,
            Arc::new(target),
        )
        .unwrap()
    }

    fn request() -> CanonicalRequestEnvelope {
        CanonicalRequest::new(
            Operation::Chat,
            "gpt-scheduled".to_owned(),
            Vec::new(),
            false,
        )
        .into()
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
        let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let nonce = [marker; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: br#"{"kind":"api_key","api_key":"scheduled-secret"}"#,
                    aad: &credential_plaintext_aad(
                        ChannelId::new(CHANNEL_ID).unwrap(),
                        credential_id,
                        CredentialKind::ApiKey,
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
}
