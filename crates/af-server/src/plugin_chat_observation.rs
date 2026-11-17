//! Bounded, read-only plugin observation for authenticated Chat requests.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_admin::AdminPluginSidecarRuntime;
use af_db::{
    PluginArtifactSourceKind, PluginCatalogRepository, PluginDeploymentQuery,
    PluginDeploymentRecord,
};
use af_domain::Protocol;
use af_http::{ChatService, ChatServiceFuture};
use af_plugin_contract::{
    REQUEST_PROCESSING_CAPABILITY, RequestProcessingInput, RequestProcessingOutput,
    RequestProcessingStage,
};
use af_protocol::CanonicalRequestEnvelope;
use af_relay::RelayDiagnosticInput;
use futures_util::future::join_all;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, Semaphore},
    time::Instant,
};

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

/// Wraps the billed Chat service while leaving its request, response and errors intact.
pub struct PluginObservedChatService {
    inner: Arc<dyn ChatService>,
    observer: Arc<dyn ChatIngressObserver>,
}

impl PluginObservedChatService {
    #[must_use]
    pub fn new(inner: Arc<dyn ChatService>, observer: Arc<dyn ChatIngressObserver>) -> Self {
        Self { inner, observer }
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
        let result = tokio::time::timeout(
            OBSERVER_PLUGIN_BUDGET,
            self.runtime.process_request_stage_with_deadline(
                &self.repository,
                &target.plugin_id,
                target.generation,
                &request.request_id,
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
