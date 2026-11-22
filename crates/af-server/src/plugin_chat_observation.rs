//! Host-controlled plugin stages and Provider adapters for authenticated requests.

mod processing;

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_adapter::{Bytes, HeaderName, HeaderValue, Method, StatusCode, UpstreamResponse};
use af_admin::AdminPluginSidecarRuntime;
use af_admin::PluginProviderUpstreamResponseMode;
use af_db::{
    PluginArtifactSourceKind, PluginCatalogRepository, PluginDeploymentQuery,
    PluginDeploymentRecord,
};
use af_domain::{Operation, Protocol};
use af_http::{ChatService, ChatServiceFuture};
use af_plugin_contract::{
    PROVIDER_LIFECYCLE_CAPABILITY, PluginManifest, PluginProviderFailureClass,
    PluginProviderOperation, PluginProviderResult, REQUEST_PROCESSING_CAPABILITY,
    RequestProcessingAction, RequestProcessingInput, RequestProcessingOutput,
    RequestProcessingStage,
};
use af_protocol::CanonicalRequestEnvelope;
use af_relay::RelayDiagnosticInput;
use af_relay::{
    ProviderRelayBinding, ProviderRelayFailureAction, ProviderRelayHook, ProviderRelayRequest,
    ProviderRelayRequestFuture, ProviderRelayResponse, ProviderRelayResponseFuture,
    ProviderRelayTransformFuture,
};
use futures_util::future::join_all;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, Semaphore},
    time::Instant,
};
use url::Url;

const OBSERVER_CAPABILITY_VERSION: u16 = 1;
const OBSERVER_CACHE_TTL: Duration = Duration::from_secs(30);
const OBSERVER_REFRESH_BUDGET: Duration = Duration::from_secs(10);
const OBSERVER_REQUEST_BUDGET: Duration = Duration::from_millis(120);
const OBSERVER_PLUGIN_BUDGET: Duration = Duration::from_millis(120);
const OBSERVER_MAX_PLUGINS_PER_REQUEST: usize = 16;
const OBSERVER_MAX_CONCURRENT_REQUESTS: usize = 32;
const DEPLOYMENT_PAGE_SIZE: u32 = 100;
const DEPLOYMENT_PAGE_LIMIT: usize = 100;

type ObservationFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Observes only a host-built, content-free summary before billing and routing.
pub trait ChatIngressObserver: Send + Sync {
    fn observe<'a>(
        &'a self,
        request_id: &'a str,
        protocol: Protocol,
        request: &'a CanonicalRequestEnvelope,
    ) -> ObservationFuture<'a>;
}

/// Synchronous host-controlled request stage. Unlike the legacy observer this
/// hook runs before billing and may apply only contract-validated effects.
pub trait ChatRequestProcessor: Send + Sync {
    fn process<'a>(
        &'a self,
        request_id: &'a str,
        protocol: Protocol,
        request: CanonicalRequestEnvelope,
    ) -> Pin<
        Box<dyn Future<Output = Result<CanonicalRequestEnvelope, af_domain::AfError>> + Send + 'a>,
    >;
}

/// Wraps the billed Chat service while leaving its request, response and errors intact.
pub struct PluginObservedChatService {
    inner: Arc<dyn ChatService>,
    observer: Arc<dyn ChatIngressObserver>,
    processor: Option<Arc<dyn ChatRequestProcessor>>,
}

impl PluginObservedChatService {
    #[must_use]
    pub fn new(inner: Arc<dyn ChatService>, observer: Arc<dyn ChatIngressObserver>) -> Self {
        Self {
            inner,
            observer,
            processor: None,
        }
    }

    #[must_use]
    pub fn with_processor(mut self, processor: Arc<dyn ChatRequestProcessor>) -> Self {
        self.processor = Some(processor);
        self
    }
}

impl ChatService for PluginObservedChatService {
    fn service_account_chat_completions<'a>(
        &'a self,
        authentication: &'a af_admin::ServiceAccountModelAuthentication,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        diagnostic: RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        Box::pin(async move {
            let request = match &self.processor {
                Some(processor) => {
                    processor
                        .process(request_id, response_protocol, request)
                        .await?
                }
                None => request,
            };
            self.observer
                .observe(request_id, response_protocol, &request)
                .await;
            self.inner
                .service_account_chat_completions(
                    authentication,
                    request,
                    response_protocol,
                    request_id,
                    diagnostic,
                )
                .await
        })
    }

    fn chat_completions<'a>(
        &'a self,
        principal: &'a af_domain::GatewayPrincipal,
        user_concurrency: Option<af_domain::ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        diagnostic: RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        Box::pin(async move {
            let request = match &self.processor {
                Some(processor) => {
                    processor
                        .process(request_id, response_protocol, request)
                        .await?
                }
                None => request,
            };
            self.observer
                .observe(request_id, response_protocol, &request)
                .await;
            self.inner
                .chat_completions(
                    principal,
                    user_concurrency,
                    request,
                    response_protocol,
                    request_id,
                    diagnostic,
                )
                .await
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ObserverTarget {
    plugin_id: String,
    generation: i64,
}

struct CachedTargets {
    refreshed_at: Instant,
    targets: Vec<ObserverTarget>,
}

#[derive(Clone, Debug)]
struct ProviderTarget {
    plugin_id: String,
    generation: i64,
    capability_version: u16,
    priority: i32,
    upstream_response_mode: PluginProviderUpstreamResponseMode,
    models: Vec<String>,
    channels: Vec<i64>,
}

/// Connects the versioned Provider adapter to the actual Relay candidate
/// request and response points. Discovery is cached; the deployment and grant
/// are rechecked by the admin runtime for every invocation.
#[derive(Clone)]
pub struct DatabasePluginProviderRelayHook {
    repository: PluginCatalogRepository,
    runtime: Arc<AdminPluginSidecarRuntime>,
    target: Arc<Mutex<Option<(Instant, Vec<ProviderTarget>)>>>,
    lifecycle: Option<Arc<af_admin::DatabaseAdminPluginLifecycleService>>,
    processor: DatabasePluginChatIngressObserver,
}

impl DatabasePluginProviderRelayHook {
    #[must_use]
    pub fn new(
        repository: PluginCatalogRepository,
        runtime: Arc<AdminPluginSidecarRuntime>,
    ) -> Self {
        Self {
            processor: DatabasePluginChatIngressObserver::new(repository.clone(), runtime.clone()),
            repository,
            runtime,
            target: Arc::new(Mutex::new(None)),
            lifecycle: None,
        }
    }

    #[must_use]
    pub fn with_lifecycle(
        mut self,
        lifecycle: Arc<af_admin::DatabaseAdminPluginLifecycleService>,
    ) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }

    async fn provider_targets(&self) -> Vec<ProviderTarget> {
        if let Some((at, targets)) = self.target.lock().await.clone()
            && at.elapsed() < OBSERVER_CACHE_TTL
        {
            return targets;
        }
        let mut before = None;
        let mut targets = Vec::new();
        for _ in 0..DEPLOYMENT_PAGE_LIMIT {
            let Some(query) = PluginDeploymentQuery::new(before.clone(), DEPLOYMENT_PAGE_SIZE).ok()
            else {
                break;
            };
            let Ok(page) = self.repository.list_deployments(query).await else {
                break;
            };
            if page.is_empty() {
                break;
            }
            for deployment in &page {
                if !deployment.enabled
                    || deployment.artifact.approval.is_none()
                    || deployment.artifact.revocation.is_some()
                    || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
                {
                    continue;
                }
                let manifest: PluginManifest =
                    match serde_json::from_value(deployment.artifact.manifest.clone()) {
                        Ok(manifest) => manifest,
                        Err(_) => continue,
                    };
                let Some(capability) = manifest
                    .capabilities
                    .iter()
                    .find(|capability| capability.id.as_str() == PROVIDER_LIFECYCLE_CAPABILITY)
                else {
                    continue;
                };
                if deployment.granted_capabilities.iter().any(|grant| {
                    grant.id == PROVIDER_LIFECYCLE_CAPABILITY && grant.version == capability.version
                }) {
                    let selection = deployment.config.get("providerSelection");
                    let priority = selection
                        .and_then(|value| value.get("priority"))
                        .and_then(Value::as_i64)
                        .and_then(|value| i32::try_from(value).ok())
                        .unwrap_or_default();
                    let models = selection
                        .and_then(|value| value.get("models"))
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    let channels = selection
                        .and_then(|value| value.get("channelIds"))
                        .and_then(Value::as_array)
                        .map(|items| items.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default();
                    let upstream_response_mode = selection
                        .and_then(|value| value.get("upstreamResponseMode"))
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default();
                    targets.push(ProviderTarget {
                        plugin_id: deployment.plugin_id.clone(),
                        generation: deployment.generation,
                        capability_version: capability.version,
                        priority,
                        upstream_response_mode,
                        models,
                        channels,
                    });
                }
            }
            before = page.last().map(|deployment| deployment.plugin_id.clone());
            if page.len() < DEPLOYMENT_PAGE_SIZE as usize {
                break;
            }
        }
        targets.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.plugin_id.cmp(&right.plugin_id))
        });
        *self.target.lock().await = Some((Instant::now(), targets.clone()));
        targets
    }

    async fn transform(
        &self,
        request: ProviderRelayRequest,
    ) -> Result<ProviderRelayRequest, af_domain::UpstreamError> {
        let targets = self.provider_targets().await;
        let candidates: Vec<_> = targets
            .into_iter()
            .filter(|target| target_matches(target, &request))
            .take(8)
            .collect();
        if candidates.is_empty() {
            return Ok(request);
        }
        let body = request
            .body
            .as_ref()
            .and_then(|body| serde_json::from_slice::<Value>(body).ok())
            .ok_or(af_domain::UpstreamError::ProtocolError)?;
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut selected = None;
        for target in candidates {
            let applies_response_mode = target.upstream_response_mode
                != PluginProviderUpstreamResponseMode::Auto
                && request.operation == Operation::Chat
                && request.upstream_protocol == Protocol::OpenAiChat;
            let upstream_response_mode = if applies_response_mode {
                match target.upstream_response_mode {
                    PluginProviderUpstreamResponseMode::Auto => request.response_mode,
                    PluginProviderUpstreamResponseMode::NonStreaming => {
                        af_adapter::ResponseMode::Full
                    }
                    PluginProviderUpstreamResponseMode::Streaming => {
                        af_adapter::ResponseMode::Stream
                    }
                }
            } else {
                request.response_mode
            };
            let mut plugin_request = body.clone();
            if applies_response_mode {
                set_chat_stream_mode(&mut plugin_request, upstream_response_mode)?;
            }
            let invocation = self.invocation(&target, &request.model, "provider-request");
            let result = tokio::time::timeout_at(
                deadline,
                self.runtime.invoke_provider(
                    &self.repository,
                    &target.plugin_id,
                    target.generation,
                    &invocation,
                    target.capability_version,
                    PluginProviderOperation::Request {
                        model: request.model.clone(),
                        request: plugin_request,
                    },
                ),
            )
            .await;
            if let Ok(Ok(result @ PluginProviderResult::Request { .. })) = result {
                selected = Some((
                    target,
                    result,
                    upstream_response_mode,
                    applies_response_mode,
                ));
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        let (selected, result, upstream_response_mode, applies_response_mode) =
            selected.ok_or(af_domain::UpstreamError::ProtocolError)?;
        let PluginProviderResult::Request {
            method,
            path,
            headers,
            body,
        } = result
        else {
            unreachable!()
        };
        let mut body = body;
        if applies_response_mode {
            set_chat_stream_mode(&mut body, upstream_response_mode)?;
        }
        let mut target_url =
            Url::parse(&request.target).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let (path_only, query) = path
            .split_once('?')
            .map_or((path.as_str(), None), |(path, query)| (path, Some(query)));
        target_url.set_path(path_only);
        target_url.set_query(query);
        let method = Method::from_bytes(method.as_bytes())
            .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let mut header_map = request.headers.clone();
        for (name, value) in headers {
            if protected_provider_header(&name) {
                continue;
            }
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
            let value = HeaderValue::from_str(&value)
                .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
            header_map.insert(name, value);
        }
        if applies_response_mode {
            set_chat_accept_header(&mut header_map, upstream_response_mode);
        }
        let projected = ProviderRelayRequest {
            request_id: request.request_id,
            credential_id: request.credential_id,
            channel_id: request.channel_id,
            binding: Some(ProviderRelayBinding {
                plugin_id: selected.plugin_id,
                generation: selected.generation,
                capability_version: selected.capability_version,
            }),
            model: request.model,
            operation: request.operation,
            method,
            target: target_url.to_string(),
            headers: header_map,
            body: serde_json::to_vec(&body).ok().map(Bytes::from),
            response_mode: request.response_mode,
            upstream_response_mode,
            upstream_response_mode_overridden: applies_response_mode,
            upstream_protocol: request.upstream_protocol,
            response_body_limit: request.response_body_limit,
        };
        Ok(projected)
    }

    async fn sign_request(
        &self,
        mut projected: ProviderRelayRequest,
    ) -> Result<ProviderRelayRequest, af_domain::UpstreamError> {
        let url =
            Url::parse(&projected.target).map_err(|_| af_domain::UpstreamError::ProtocolError)?;
        let sign_path = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };
        let sign_body = projected
            .body
            .as_ref()
            .and_then(|body| serde_json::from_slice(body).ok());
        if let (Some(lifecycle), Some(channel), Some(credential)) = (
            &self.lifecycle,
            projected.channel_id,
            projected.credential_id,
        ) && let Some(af_plugin_contract::PluginCredentialResult::Signed {
            headers,
            query,
            body,
        }) = lifecycle
            .sign_bound_plugin_request(
                channel.get(),
                credential,
                projected.method.as_str(),
                &sign_path,
                sign_body,
            )
            .await
            .map_err(|_| af_domain::UpstreamError::ProtocolError)?
        {
            for (name, value) in headers {
                let name = HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                let value = HeaderValue::from_str(&value)
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                projected.headers.insert(name, value);
            }
            if let Some(query) = query {
                let mut url = Url::parse(&projected.target)
                    .map_err(|_| af_domain::UpstreamError::ProtocolError)?;
                url.query_pairs_mut().extend_pairs(query);
                projected.target = url.to_string();
            }
            if let Some(body) = body {
                projected.body = Some(Bytes::from(
                    serde_json::to_vec(&body)
                        .map_err(|_| af_domain::UpstreamError::ProtocolError)?,
                ));
            }
        }
        Ok(projected)
    }

    async fn classify(
        &self,
        request: &ProviderRelayRequest,
        status: Option<StatusCode>,
        error: af_domain::UpstreamError,
    ) -> ProviderRelayFailureAction {
        let Some(binding) = &request.binding else {
            return ProviderRelayFailureAction::Continue;
        };
        let target = ProviderTarget::from(binding);
        let invocation = self.invocation(&target, &request.model, "provider-failure");
        let Ok(result) = self
            .runtime
            .invoke_provider(
                &self.repository,
                &target.plugin_id,
                target.generation,
                &invocation,
                target.capability_version,
                PluginProviderOperation::ClassifyFailure {
                    status: status.map(|value| value.as_u16()),
                    error: json!({ "kind": format!("{error:?}") }),
                },
            )
            .await
        else {
            return ProviderRelayFailureAction::Continue;
        };
        match result {
            PluginProviderResult::Retry {
                retryable: true,
                delay_ms,
                ..
            } => ProviderRelayFailureAction::RetryCurrent {
                delay: Duration::from_millis(u64::from(delay_ms)),
            },
            PluginProviderResult::Failure {
                class:
                    PluginProviderFailureClass::Unavailable
                    | PluginProviderFailureClass::Timeout
                    | PluginProviderFailureClass::RateLimited,
                ..
            } => ProviderRelayFailureAction::RouteToNext,
            PluginProviderResult::Failure { .. }
            | PluginProviderResult::Retry {
                retryable: false, ..
            } => ProviderRelayFailureAction::Abort,
            _ => ProviderRelayFailureAction::Continue,
        }
    }

    fn invocation(
        &self,
        target: &ProviderTarget,
        model: &str,
        suffix: &str,
    ) -> af_plugin_runtime::CapabilityExecutionContext {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        af_plugin_runtime::CapabilityExecutionContext::new(
            format!("provider-relay:{}:{}", target.plugin_id, suffix),
            af_plugin_contract::InvocationMetadata {
                request_id: format!("provider-relay:{}", model),
                deadline_unix_ms: now_ms.saturating_add(120),
            },
            serde_json::json!({}),
            256 * 1024,
        )
        .expect("provider relay invocation metadata is bounded")
    }
}

impl ProviderRelayHook for DatabasePluginProviderRelayHook {
    fn transform_request<'a>(
        &'a self,
        request: ProviderRelayRequest,
    ) -> ProviderRelayRequestFuture<'a> {
        Box::pin(async move {
            let request = self.process_outbound(request).await?;
            let request = self.transform(request).await?;
            let request = self.sign_request(request).await?;
            if request.upstream_response_mode_overridden {
                validate_chat_stream_mode(&request)?;
            }
            Ok(request)
        })
    }

    fn observe_response<'a>(
        &'a self,
        response: ProviderRelayResponse<'a>,
    ) -> ProviderRelayResponseFuture<'a> {
        Box::pin(async move {
            let _ = response;
        })
    }

    fn classify_failure<'a>(
        &'a self,
        request: &'a ProviderRelayRequest,
        status: Option<StatusCode>,
        error: af_domain::UpstreamError,
    ) -> af_relay::ProviderRelayFailureFuture<'a> {
        Box::pin(async move { self.classify(request, status, error).await })
    }
    fn transform_response<'a>(
        &'a self,
        request: &'a ProviderRelayRequest,
        response: UpstreamResponse,
    ) -> ProviderRelayTransformFuture<'a> {
        Box::pin(async move { self.process_response(request, response).await })
    }
}

impl From<&ProviderRelayBinding> for ProviderTarget {
    fn from(binding: &ProviderRelayBinding) -> Self {
        Self {
            plugin_id: binding.plugin_id.clone(),
            generation: binding.generation,
            capability_version: binding.capability_version,
            priority: 0,
            upstream_response_mode: PluginProviderUpstreamResponseMode::Auto,
            models: vec![],
            channels: vec![],
        }
    }
}

fn target_matches(target: &ProviderTarget, request: &ProviderRelayRequest) -> bool {
    (target.models.is_empty() || target.models.iter().any(|model| model == &request.model))
        && (!target.channels.is_empty()
            && request
                .channel_id
                .is_some_and(|channel| target.channels.contains(&channel.get())))
}

fn protected_provider_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "host" | "content-length" | "cookie" | "proxy-authorization"
    )
}

fn set_chat_stream_mode(
    body: &mut Value,
    mode: af_adapter::ResponseMode,
) -> Result<(), af_domain::UpstreamError> {
    let object = body
        .as_object_mut()
        .ok_or(af_domain::UpstreamError::ProtocolError)?;
    object.insert(
        "stream".to_owned(),
        Value::Bool(mode == af_adapter::ResponseMode::Stream),
    );
    if mode == af_adapter::ResponseMode::Full {
        object.remove("stream_options");
    }
    Ok(())
}

fn set_chat_accept_header(headers: &mut af_adapter::HeaderMap, mode: af_adapter::ResponseMode) {
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static(match mode {
            af_adapter::ResponseMode::Full => "application/json",
            af_adapter::ResponseMode::Stream => "text/event-stream",
        }),
    );
}

fn validate_chat_stream_mode(
    request: &ProviderRelayRequest,
) -> Result<(), af_domain::UpstreamError> {
    if request.operation != Operation::Chat || request.upstream_protocol != Protocol::OpenAiChat {
        return Ok(());
    }
    let body = request
        .body
        .as_ref()
        .and_then(|body| serde_json::from_slice::<Value>(body).ok())
        .ok_or(af_domain::UpstreamError::ProtocolError)?;
    let mut expected = body.clone();
    set_chat_stream_mode(&mut expected, request.upstream_response_mode)?;
    if expected != body {
        return Err(af_domain::UpstreamError::ProtocolError);
    }
    let expected_accept = match request.upstream_response_mode {
        af_adapter::ResponseMode::Full => "application/json",
        af_adapter::ResponseMode::Stream => "text/event-stream",
    };
    if request
        .headers
        .get("accept")
        .and_then(|value| value.to_str().ok())
        != Some(expected_accept)
    {
        return Err(af_domain::UpstreamError::ProtocolError);
    }
    Ok(())
}

/// Selects explicitly enabled, approved and granted request observers.
/// Invocation rechecks the current generation and grant in the admin runtime.
#[derive(Clone)]
pub struct DatabasePluginChatIngressObserver {
    repository: PluginCatalogRepository,
    runtime: Arc<AdminPluginSidecarRuntime>,
    targets: Arc<Mutex<Option<CachedTargets>>>,
    refreshing: Arc<AtomicBool>,
    target_cursor: Arc<AtomicUsize>,
    request_slots: Arc<Semaphore>,
    invocation_slots: Arc<Semaphore>,
}

impl DatabasePluginChatIngressObserver {
    #[must_use]
    pub fn new(
        repository: PluginCatalogRepository,
        runtime: Arc<AdminPluginSidecarRuntime>,
    ) -> Self {
        Self {
            repository,
            runtime,
            targets: Arc::new(Mutex::new(None)),
            refreshing: Arc::new(AtomicBool::new(false)),
            target_cursor: Arc::new(AtomicUsize::new(0)),
            request_slots: Arc::new(Semaphore::new(OBSERVER_MAX_CONCURRENT_REQUESTS)),
            invocation_slots: Arc::new(Semaphore::new(OBSERVER_MAX_CONCURRENT_REQUESTS)),
        }
    }

    async fn targets_for_request(&self) -> Vec<ObserverTarget> {
        let cache = self.targets.lock().await;
        let stale = cache
            .as_ref()
            .is_none_or(|cached| cached.refreshed_at.elapsed() >= OBSERVER_CACHE_TTL);
        if stale && !self.refreshing.swap(true, Ordering::AcqRel) {
            let observer = self.clone();
            tokio::spawn(async move {
                let refreshed =
                    tokio::time::timeout(OBSERVER_REFRESH_BUDGET, observer.load_targets()).await;
                match refreshed {
                    Ok(Ok(targets)) => {
                        *observer.targets.lock().await = Some(CachedTargets {
                            refreshed_at: Instant::now(),
                            targets,
                        });
                    }
                    Ok(Err(_)) | Err(_) => tracing::warn!(
                        target: "anyflows::plugin_observation",
                        "plugin observer discovery unavailable"
                    ),
                }
                observer.refreshing.store(false, Ordering::Release);
            });
        }
        cache
            .as_ref()
            .map_or_else(Vec::new, |cached| cached.targets.clone())
    }

    async fn load_targets(&self) -> Result<Vec<ObserverTarget>, af_db::PluginCatalogError> {
        let mut targets = Vec::new();
        let mut before_plugin_id = None;
        for _ in 0..DEPLOYMENT_PAGE_LIMIT {
            let query = PluginDeploymentQuery::new(before_plugin_id.clone(), DEPLOYMENT_PAGE_SIZE)?;
            let page = self.repository.list_deployments(query).await?;
            let page_len = page.len();
            if page_len == 0 {
                break;
            }
            for deployment in &page {
                if deployment_supports_chat_observation(deployment) {
                    targets.push(ObserverTarget {
                        plugin_id: deployment.plugin_id.clone(),
                        generation: deployment.generation,
                    });
                }
            }
            before_plugin_id = page.last().map(|deployment| deployment.plugin_id.clone());
            if page_len < DEPLOYMENT_PAGE_SIZE as usize {
                break;
            }
        }
        Ok(targets)
    }

    async fn invoke_one(&self, target: ObserverTarget, request: RequestProcessingInput) {
        let Ok(_permit) = self.invocation_slots.clone().try_acquire_owned() else {
            return;
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok());
        let Some(deadline_unix_ms) = now_ms.and_then(|now| {
            u64::try_from(OBSERVER_PLUGIN_BUDGET.as_millis())
                .ok()
                .and_then(|budget| now.checked_add(budget))
        }) else {
            return;
        };
        let request_id = request.request_id.clone();
        let result = tokio::time::timeout(
            OBSERVER_PLUGIN_BUDGET,
            self.runtime.process_request_stage_with_deadline(
                &self.repository,
                &target.plugin_id,
                target.generation,
                &request_id,
                deadline_unix_ms,
                request,
            ),
        )
        .await;
        match result {
            Ok(Ok(output)) => record_observer_findings(&target, &output),
            Ok(Err(_)) | Err(_) => {
                tracing::debug!(
                    target: "anyflows::plugin_observation",
                    plugin_id = %target.plugin_id,
                    generation = target.generation,
                    "plugin ingress observer unavailable; request continues"
                );
            }
        }
    }
}

impl ChatIngressObserver for DatabasePluginChatIngressObserver {
    fn observe<'a>(
        &'a self,
        request_id: &'a str,
        protocol: Protocol,
        request: &'a CanonicalRequestEnvelope,
    ) -> ObservationFuture<'a> {
        let observer = self.clone();
        Box::pin(async move {
            let Ok(permit) = observer.request_slots.clone().try_acquire_owned() else {
                return;
            };
            let targets = observer.targets_for_request().await;
            if targets.is_empty() {
                return;
            }
            let input = chat_observer_input(request_id, protocol, request);
            tokio::spawn(async move {
                let _permit = permit;
                let start = observer
                    .target_cursor
                    .fetch_add(OBSERVER_MAX_PLUGINS_PER_REQUEST, Ordering::Relaxed)
                    % targets.len();
                let selected = (0..targets.len().min(OBSERVER_MAX_PLUGINS_PER_REQUEST))
                    .map(|index| targets[(start + index) % targets.len()].clone());
                let invocations = selected.map(|target| observer.invoke_one(target, input.clone()));
                if tokio::time::timeout(OBSERVER_REQUEST_BUDGET, join_all(invocations))
                    .await
                    .is_err()
                {
                    tracing::debug!(target: "anyflows::plugin_observation", "plugin ingress observer request budget exhausted; request continues");
                }
            });
        })
    }
}

impl ChatRequestProcessor for DatabasePluginChatIngressObserver {
    fn process<'a>(
        &'a self,
        request_id: &'a str,
        protocol: Protocol,
        request: CanonicalRequestEnvelope,
    ) -> Pin<
        Box<dyn Future<Output = Result<CanonicalRequestEnvelope, af_domain::AfError>> + Send + 'a>,
    > {
        let processor = self.clone();
        Box::pin(async move {
            let targets = processor.targets_for_request().await;
            if targets.is_empty() {
                return Ok(request);
            }
            let mut input = chat_observer_input(request_id, protocol, &request);
            input.stage = RequestProcessingStage::Request;
            input.body["model"] = Value::String(request.canonical().model.clone());
            input.body["stream"] = Value::Bool(request.canonical().stream);
            let total_deadline = Instant::now() + OBSERVER_REQUEST_BUDGET;
            for target in targets.into_iter().take(OBSERVER_MAX_PLUGINS_PER_REQUEST) {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| af_domain::AfError::Internal)?
                    .as_millis() as u64;
                let remaining = total_deadline.saturating_duration_since(Instant::now());
                let deadline = now_ms.saturating_add(remaining.as_millis() as u64);
                let output = tokio::time::timeout_at(
                    total_deadline,
                    processor.runtime.process_request_stage_with_deadline(
                        &processor.repository,
                        &target.plugin_id,
                        target.generation,
                        request_id,
                        deadline,
                        input.clone(),
                    ),
                )
                .await
                .map_err(|_| af_domain::AfError::Internal)?
                .map_err(|_| af_domain::AfError::Internal)?;
                if matches!(
                    output.action,
                    RequestProcessingAction::Reject | RequestProcessingAction::Stop
                ) {
                    return Err(af_domain::AfError::InvalidRequest);
                }
                output
                    .apply_to(&mut input)
                    .map_err(|_| af_domain::AfError::Internal)?;
            }
            let Some(model) = input.body.get("model").and_then(Value::as_str) else {
                return Ok(request);
            };
            if model.is_empty() || model.len() > 256 {
                return Err(af_domain::AfError::InvalidRequest);
            }
            if model == request.canonical().model {
                Ok(request)
            } else {
                Err(af_domain::AfError::InvalidRequest)
            }
        })
    }
}

fn deployment_supports_chat_observation(deployment: &PluginDeploymentRecord) -> bool {
    deployment.enabled
        && deployment.artifact.approval.is_some()
        && deployment.artifact.revocation.is_none()
        && deployment.artifact.source_kind == PluginArtifactSourceKind::LocalProcess
        && deployment.granted_capabilities.iter().any(|grant| {
            grant.id == REQUEST_PROCESSING_CAPABILITY
                && grant.version == OBSERVER_CAPABILITY_VERSION
        })
        && deployment
            .artifact
            .manifest
            .get("capabilities")
            .is_some_and(observer_capability_declared)
}

fn chat_observer_input(
    request_id: &str,
    protocol: Protocol,
    request: &CanonicalRequestEnvelope,
) -> RequestProcessingInput {
    let canonical = request.canonical();
    RequestProcessingInput {
        stage: RequestProcessingStage::Ingress,
        request_id: request_id.to_owned(),
        format: protocol.as_str().to_owned(),
        path: "/chat".to_owned(),
        model: Some(canonical.model.clone()),
        upstream_model: None,
        headers: Default::default(),
        body: json!({
            "operation": canonical.operation.as_str(),
            "stream": canonical.stream,
            "messageCount": canonical.messages.len(),
            "toolCount": canonical.tools.len(),
            "attachmentCount": canonical.attachments.len(),
        }),
        response_status: None,
        response_body: None,
        usage: None,
    }
}

fn record_observer_findings(target: &ObserverTarget, output: &RequestProcessingOutput) {
    for finding in &output.findings {
        tracing::debug!(
            target: "anyflows::plugin_observation",
            plugin_id = %target.plugin_id,
            generation = target.generation,
            finding_code = %finding.code,
            finding_status = ?finding.status,
            "plugin reported a Chat ingress finding"
        );
    }
    // This stage is strictly observational. Ignore action, message and effects even when valid.
}

fn observer_capability_declared(capabilities: &Value) -> bool {
    capabilities.as_array().is_some_and(|capabilities| {
        capabilities.iter().any(|capability| {
            capability.get("id").and_then(Value::as_str) == Some(REQUEST_PROCESSING_CAPABILITY)
                && capability.get("version").and_then(Value::as_u64)
                    == Some(u64::from(OBSERVER_CAPABILITY_VERSION))
                && capability.get("access").and_then(Value::as_str) == Some("read")
                && capability.get("sideEffect").and_then(Value::as_str) == Some("none")
                && capability.get("inputSchema").is_some_and(Value::is_object)
                && capability.get("outputSchema").is_some_and(Value::is_object)
        })
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use af_adapter::Bytes;
    use af_domain::{ConcurrencyLimit, GroupId, Operation, Role, TokenId, UserId};
    use af_httpclient::HeaderMap;
    use af_protocol::{CanonicalRequest, ContentBlock, Message};
    use af_relay::ChatResponse;
    use serde_json::json;

    use super::*;

    #[test]
    fn ingress_projection_contains_request_shape_but_no_prompt_content() {
        let request: CanonicalRequestEnvelope = CanonicalRequest::new(
            Operation::Chat,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("private prompt".to_owned())],
            )],
            true,
        )
        .into();
        let projection = chat_observer_input("req-1", Protocol::OpenAiChat, &request);
        let encoded = serde_json::to_string(&projection).unwrap();
        assert!(encoded.contains("messageCount"));
        assert!(encoded.contains("public-model"));
        assert!(!encoded.contains("private prompt"));
        assert_eq!(projection.body["messageCount"], json!(1));
    }

    #[test]
    fn request_processor_declaration_requires_read_only_schemas() {
        assert!(observer_capability_declared(&json!([{
            "id": REQUEST_PROCESSING_CAPABILITY,
            "version": 1,
            "access": "read",
            "sideEffect": "none",
            "inputSchema": {},
            "outputSchema": {}
        }])));
        assert!(!observer_capability_declared(&json!([{
            "id": REQUEST_PROCESSING_CAPABILITY,
            "version": 1,
            "access": "write",
            "sideEffect": "none",
            "inputSchema": {},
            "outputSchema": {}
        }])));
    }

    struct RecordingObserver(Arc<StdMutex<Vec<&'static str>>>);

    impl ChatIngressObserver for RecordingObserver {
        fn observe<'a>(
            &'a self,
            _request_id: &'a str,
            _protocol: Protocol,
            _request: &'a CanonicalRequestEnvelope,
        ) -> ObservationFuture<'a> {
            Box::pin(async move {
                self.0.lock().unwrap().push("observer");
            })
        }
    }

    struct RecordingChatService(Arc<StdMutex<Vec<&'static str>>>);

    impl ChatService for RecordingChatService {
        fn chat_completions<'a>(
            &'a self,
            _principal: &'a af_domain::GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            request: CanonicalRequestEnvelope,
            _response_protocol: Protocol,
            _request_id: &'a str,
            _diagnostic: RelayDiagnosticInput,
        ) -> ChatServiceFuture<'a> {
            Box::pin(async move {
                assert_eq!(request.canonical().model, "public-model");
                assert!(request.canonical().stream);
                self.0.lock().unwrap().push("chat");
                Ok(ChatResponse::Full {
                    body: Bytes::from_static(b"{}"),
                    usage: Err(af_relay::UsageResolutionError::UnsupportedContent),
                })
            })
        }
    }

    #[tokio::test]
    async fn chat_observation_runs_once_before_billing_service_without_changing_request() {
        let order = Arc::new(StdMutex::new(Vec::new()));
        let service = PluginObservedChatService::new(
            Arc::new(RecordingChatService(Arc::clone(&order))),
            Arc::new(RecordingObserver(Arc::clone(&order))),
        );
        let principal = af_domain::GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(2).unwrap(),
            GroupId::new(3).unwrap(),
        );
        let request =
            CanonicalRequest::new(Operation::Chat, "public-model".to_owned(), vec![], true).into();
        let diagnostic = RelayDiagnosticInput::capture(
            "POST",
            "/v1/chat/completions",
            &HeaderMap::new(),
            &Bytes::from_static(b"{}"),
        );

        service
            .chat_completions(
                &principal,
                None,
                request,
                Protocol::OpenAiChat,
                "request-1",
                diagnostic,
            )
            .await
            .unwrap();

        assert_eq!(order.lock().unwrap().as_slice(), &["observer", "chat"]);
    }
}
