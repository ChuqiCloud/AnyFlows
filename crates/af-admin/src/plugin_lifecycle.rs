//! Administrative lifecycle operations for verified plugin artifacts.
//!
//! This service owns authorization, persistent approvals/deployments and audit
//! attribution. Runtime activation is deliberately handled by a separate host
//! runtime coordinator.

mod data_plane;
mod git_install;

pub use data_plane::PluginNativeTaskProtocol;

use std::{
    fmt,
    sync::Arc,
    time::{Instant, SystemTime},
};

use af_db::{
    MAX_PLUGIN_DEPLOYMENT_HISTORY, PlatformAuditOutcome, PluginArtifactApproval,
    PluginArtifactCreate, PluginArtifactQuery, PluginArtifactRecord, PluginArtifactRevocation,
    PluginArtifactSourceKind, PluginCapabilityGrant, PluginCatalogError, PluginCatalogRepository,
    PluginDeploymentQuery, PluginDeploymentRecord, PluginDeploymentWrite,
    PluginEventDeliveryClaimOutcome, PluginEventDeliveryCompletionOutcome,
    PluginEventDeliveryLease, PluginEventDeliveryRepository, PluginEventDeliveryResultQuery,
    PluginEventFailureKind,
};
use af_domain::PlatformPermission;
use af_httpclient::{
    HeaderMap, HeaderName, HttpClientProvider, HttpResponse, Method, PooledClient,
};
use af_plugin_contract::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, InvocationMetadata,
    PluginCredentialOperation, PluginCredentialResult, PluginId, PluginManifest, PluginMarketIndex,
    PluginMarketSource, PluginRouteMethod, PluginTaskOperation, PluginTaskResult, SideEffectClass,
};
use af_plugin_runtime::{
    PluginStorageDispatcher, VerifiedPluginArtifact, validate_capability_schemas,
    validate_public_plugin_configuration,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::Duration;

use crate::{
    AdminPluginSidecarRuntime, PlatformAuditEntry, PlatformAuditError, PlatformAuditService,
    PlatformPolicy, SessionPrincipal, SessionRole,
};

pub const DEFAULT_PLUGIN_CATALOG_PAGE_SIZE: u32 = 50;
const PLUGIN_PERMISSION: PlatformPermission = PlatformPermission::PluginsManage;
const PLUGIN_SETUP_PENDING_FIELD: &str = "pluginSetupPending";
const PLUGIN_EVENT_CALL_BUDGET: Duration = Duration::from_secs(5);
const PLUGIN_CUSTOM_CAPABILITY_CALL_BUDGET: Duration = Duration::from_secs(30);
const PLUGIN_CUSTOM_CAPABILITY_MAX_BYTES: u32 = 1024 * 1024;
const PLUGIN_CUSTOM_CAPABILITY_MAX_CONCURRENCY: usize = 16;
const PLUGIN_EVENT_MAX_BYTES: u32 = 32 * 1024;
const PLUGIN_EVENT_POLL_INTERVAL: Duration = Duration::from_secs(2);
const PLUGIN_EVENT_BATCH_SIZE: usize = 8;
const MAX_PLUGIN_EVENT_RESULT_SUMMARY_BYTES: usize = 256;
pub const MAX_PLUGIN_PACKAGE_UPLOAD_BYTES: usize = 128 * 1024 * 1024;
const STALE_PLUGIN_UPLOAD_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const PLUGIN_CAPABILITY_ACTION_TTL_SECONDS: i64 = 300;
const MAX_PLUGIN_MARKET_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MARKET_REDIRECTS: usize = 3;

/// Host metadata shares the deployment record so routing and config update
/// atomically. Preserve legacy manifests that explicitly declare these fields.
fn plugin_public_configuration(configuration: &Value, schema: &Value) -> Value {
    if plugin_configuration_contains_sensitive_fields(configuration) {
        return json!({});
    }
    let mut public = configuration.clone();
    if let Some(fields) = public.as_object_mut() {
        for key in [
            "providerSelection",
            "taskPricing",
            PLUGIN_SETUP_PENDING_FIELD,
        ] {
            if key == PLUGIN_SETUP_PENDING_FIELD || schema["properties"].get(key).is_none() {
                fields.remove(key);
            }
        }
    }
    public
}

fn plugin_configuration_contains_sensitive_fields(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(name, value)| {
            let normalized = name
                .bytes()
                .filter(|byte| byte.is_ascii_alphanumeric())
                .map(char::from)
                .collect::<String>()
                .to_ascii_lowercase();
            [
                "password",
                "secret",
                "credential",
                "authorization",
                "apikey",
                "accesstoken",
                "refreshtoken",
                "idtoken",
                "privatekey",
                "cookie",
                "bearer",
                "clientsecret",
            ]
            .iter()
            .any(|marker| normalized.contains(marker))
                || normalized == "token"
                || normalized.ends_with("token")
                || plugin_configuration_contains_sensitive_fields(value)
        }),
        Value::Array(items) => items
            .iter()
            .any(plugin_configuration_contains_sensitive_fields),
        _ => false,
    }
}

pub(crate) fn deployment_public_configuration(deployment: &PluginDeploymentRecord) -> Value {
    plugin_public_configuration(
        &deployment.config,
        &deployment.artifact.manifest["configSchema"],
    )
}

async fn execute_market_get(client: &PooledClient, initial_url: &str) -> Result<HttpResponse, ()> {
    execute_market_get_with_headers(client, initial_url, HeaderMap::new()).await
}

async fn execute_market_get_with_headers(
    client: &PooledClient,
    initial_url: &str,
    headers: HeaderMap,
) -> Result<HttpResponse, ()> {
    let initial = url::Url::parse(initial_url).map_err(|_| ())?;
    let initial_host = initial.host_str().ok_or(())?.to_owned();
    let mut current = initial;
    for _ in 0..=MAX_MARKET_REDIRECTS {
        let response = client
            .execute(Method::GET, current.as_str(), headers.clone(), None)
            .await
            .map_err(|_| ())?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get(HeaderName::from_static("location"))
            .and_then(|value| value.to_str().ok())
            .ok_or(())?;
        let target = url::Url::parse(location).map_err(|_| ())?;
        if target.scheme() != "https"
            || !target.username().is_empty()
            || target.password().is_some()
            || !market_redirect_host_allowed(&initial_host, target.host_str().ok_or(())?)
        {
            return Err(());
        }
        current = target;
    }
    Err(())
}

fn market_redirect_host_allowed(initial: &str, target: &str) -> bool {
    let initial = initial.to_ascii_lowercase();
    let target = target.to_ascii_lowercase();
    target == initial
        || (initial == "github.com"
            && (target == "github.com"
                || target.ends_with(".githubusercontent.com")
                || target.ends_with(".githubassets.com")))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginCapabilityActionProposal {
    pub proposal_key: String,
    pub expires_at: i64,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginCapabilityActionResult {
    pub proposal_key: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome_code: Option<String>,
}

fn capability_action_input_digest(input: &Value) -> Result<String, AdminPluginLifecycleError> {
    let bytes = serde_json::to_vec(input).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
    if bytes.len() > 256 * 1024 {
        return Err(AdminPluginLifecycleError::PayloadTooLarge);
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn capability_action_status(status: af_db::PluginCapabilityActionStatus) -> &'static str {
    match status {
        af_db::PluginCapabilityActionStatus::Pending => "pending",
        af_db::PluginCapabilityActionStatus::Executing => "executing",
        af_db::PluginCapabilityActionStatus::Succeeded => "succeeded",
        af_db::PluginCapabilityActionStatus::Unknown => "unknown",
        af_db::PluginCapabilityActionStatus::Expired => "expired",
        af_db::PluginCapabilityActionStatus::Revoked => "revoked",
    }
}

/// A capability is eligible for the one-step installer only when its verified
/// declaration is schema-bound, read-only, and has no external side effect.
/// This semantic check deliberately applies to any namespaced capability ID;
/// external reads and writes remain on the explicit review path.
fn simple_install_capability(capability: &CapabilityDeclaration) -> bool {
    capability.validate().is_ok()
        && capability.access == CapabilityAccess::Read
        && capability.side_effect == SideEffectClass::None
        && capability.input_schema.as_ref().is_some_and(|input| {
            capability
                .output_schema
                .as_ref()
                .is_some_and(|output| validate_capability_schemas(input, output).is_ok())
        })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginEventResult {
    pub delivery_id: i64,
    pub plugin_id: String,
    pub event_kind: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub attempt_count: u16,
    pub status: String,
    pub completed_at: i64,
    pub summary: String,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginEventResultPage {
    pub results: Vec<AdminPluginEventResult>,
    pub next_before_id: Option<i64>,
}

/// Result from invoking a plugin-defined read capability through the generic
/// administrator extension entry point.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginCapabilityInvocationResult {
    pub plugin_id: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub output: Value,
}

/// Result returned by a manifest-declared administrator route.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginRouteInvocationResult {
    pub plugin_id: String,
    pub route_id: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub output: Value,
}

/// Public discovery projection used by the plugin market. It contains only
/// approved artifact metadata and never exposes host paths or configuration.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginMarketEntry {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub digest: String,
    pub source_kind: String,
    pub capabilities: Vec<String>,
    pub icon: Option<String>,
    pub repository: Option<String>,
    pub published_at: i64,
    pub official: bool,
}

/// A validated remote artifact selected from a market index. The archive is
/// downloaded by the host and then passed through the normal package verifier.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginMarketInstallRequest {
    pub plugin_id: String,
    pub plugin_version: String,
    pub source: PluginMarketInstallSource,
    pub config: Option<Value>,
    pub expected_generation: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginMarketInstallSource {
    pub kind: String,
    pub archive_url: String,
    pub sha256: String,
    pub repository: Option<String>,
    pub commit: Option<String>,
    pub path: Option<String>,
}

impl PluginMarketInstallSource {
    fn into_contract(self) -> PluginMarketSource {
        PluginMarketSource {
            kind: self.kind,
            archive_url: self.archive_url,
            sha256: self.sha256,
            repository: self.repository,
            commit: self.commit,
            path: self.path,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginCredentialInvocationResult {
    pub plugin_id: String,
    pub deployment_generation: i64,
    pub credential_id: String,
    pub result: af_plugin_contract::PluginCredentialResult,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginTaskInvocationResult {
    pub plugin_id: String,
    pub deployment_generation: i64,
    pub result: PluginTaskResult,
}

/// A bounded upload in host-owned storage. Dropping it removes unfinished data.
pub struct PluginPackageUpload {
    file: tokio::fs::File,
    temporary: tempfile::NamedTempFile,
    artifact_root: std::path::PathBuf,
    uploaded: usize,
    expected_plugin_id: Option<String>,
    expected_plugin_version: Option<String>,
    market_source: Option<PluginMarketSource>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PluginPackageUpload {
    pub fn bind_market_source(
        mut self,
        plugin_id: String,
        plugin_version: String,
        source: PluginMarketSource,
    ) -> Self {
        self.expected_plugin_id = Some(plugin_id);
        self.expected_plugin_version = Some(plugin_version);
        self.market_source = Some(source);
        self
    }

    pub async fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), AdminPluginLifecycleError> {
        use tokio::io::AsyncWriteExt as _;
        self.uploaded = self
            .uploaded
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_PLUGIN_PACKAGE_UPLOAD_BYTES)
            .ok_or(AdminPluginLifecycleError::PayloadTooLarge)?;
        self.file
            .write_all(bytes)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)
    }
}

/// A safe artifact view. The source reference and signature are intentionally
/// not represented here, so future HTTP adapters cannot accidentally serialize them.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifact {
    id: i64,
    plugin_id: String,
    plugin_version: String,
    digest: String,
    manifest: Value,
    source_kind: &'static str,
    signer_key_id: String,
    official: bool,
    created_at: i64,
    approval: Option<AdminPluginArtifactApproval>,
    revocation: Option<AdminPluginArtifactRevocation>,
}

impl AdminPluginArtifact {
    fn from_record(record: PluginArtifactRecord) -> Self {
        let source_kind = match record.source_kind {
            PluginArtifactSourceKind::Builtin => "builtin",
            PluginArtifactSourceKind::LocalProcess => "local_process",
            PluginArtifactSourceKind::RemoteEndpoint => "remote_endpoint",
        };
        Self {
            id: record.id,
            plugin_id: record.plugin_id,
            plugin_version: record.plugin_version,
            digest: record.digest,
            manifest: record.manifest,
            source_kind,
            signer_key_id: record.signer_key_id,
            official: false,
            created_at: record.created_at,
            approval: record.approval.map(AdminPluginArtifactApproval::from),
            revocation: record.revocation.map(AdminPluginArtifactRevocation::from),
        }
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    #[must_use]
    pub fn plugin_version(&self) -> &str {
        &self.plugin_version
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    #[must_use]
    pub fn manifest(&self) -> &Value {
        &self.manifest
    }

    #[must_use]
    pub const fn source_kind(&self) -> &'static str {
        self.source_kind
    }

    #[must_use]
    pub fn signer_key_id(&self) -> &str {
        &self.signer_key_id
    }

    #[must_use]
    pub const fn is_official(&self) -> bool {
        self.official
    }

    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    #[must_use]
    pub const fn approval(&self) -> Option<&AdminPluginArtifactApproval> {
        self.approval.as_ref()
    }

    #[must_use]
    pub const fn revocation(&self) -> Option<&AdminPluginArtifactRevocation> {
        self.revocation.as_ref()
    }
}

impl fmt::Debug for AdminPluginArtifact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminPluginArtifact")
            .field("id", &self.id)
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("digest", &self.digest)
            .field("manifest", &"<redacted>")
            .field("source_kind", &self.source_kind)
            .field("signer_key_id", &self.signer_key_id)
            .field("created_at", &self.created_at)
            .field("approved", &self.approval.is_some())
            .field("revoked", &self.revocation.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactApproval {
    approved_by: i64,
    approved_at: i64,
    request_id: String,
}

impl From<PluginArtifactApproval> for AdminPluginArtifactApproval {
    fn from(value: PluginArtifactApproval) -> Self {
        Self {
            approved_by: value.approved_by.get(),
            approved_at: value.approved_at,
            request_id: value.request_id,
        }
    }
}

impl AdminPluginArtifactApproval {
    #[must_use]
    pub const fn approved_by(&self) -> i64 {
        self.approved_by
    }

    #[must_use]
    pub const fn approved_at(&self) -> i64 {
        self.approved_at
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactRevocation {
    revoked_by: i64,
    revoked_at: i64,
    request_id: String,
}

impl From<PluginArtifactRevocation> for AdminPluginArtifactRevocation {
    fn from(value: PluginArtifactRevocation) -> Self {
        Self {
            revoked_by: value.revoked_by.get(),
            revoked_at: value.revoked_at,
            request_id: value.request_id,
        }
    }
}

impl AdminPluginArtifactRevocation {
    #[must_use]
    pub const fn revoked_by(&self) -> i64 {
        self.revoked_by
    }

    #[must_use]
    pub const fn revoked_at(&self) -> i64 {
        self.revoked_at
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

impl fmt::Debug for AdminPluginArtifactRevocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminPluginArtifactRevocation")
            .field("revoked_by", &self.revoked_by)
            .field("revoked_at", &self.revoked_at)
            .field("request_id", &self.request_id)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactPage {
    artifacts: Vec<AdminPluginArtifact>,
    next_before_id: Option<i64>,
}

impl AdminPluginArtifactPage {
    #[must_use]
    pub fn artifacts(&self) -> &[AdminPluginArtifact] {
        &self.artifacts
    }

    #[must_use]
    pub const fn next_before_id(&self) -> Option<i64> {
        self.next_before_id
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginDeployment {
    plugin_id: String,
    generation: i64,
    artifact: AdminPluginArtifact,
    enabled: bool,
    needs_configuration: bool,
    granted_capabilities: Vec<AdminPluginCapabilityGrant>,
    provider_selection: Option<PluginProviderSelection>,
    updated_by: i64,
    updated_at: i64,
    request_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginPublicConfiguration {
    plugin_id: String,
    generation: i64,
    configuration: Value,
}

impl AdminPluginPublicConfiguration {
    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    #[must_use]
    pub const fn generation(&self) -> i64 {
        self.generation
    }

    #[must_use]
    pub const fn configuration(&self) -> &Value {
        &self.configuration
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProviderSelection {
    pub channel_ids: Vec<i64>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub upstream_response_mode: PluginProviderUpstreamResponseMode,
    #[serde(default)]
    pub session_enabled: bool,
    #[serde(default = "default_provider_session_timeout_ms")]
    pub session_timeout_ms: u32,
    #[serde(default, alias = "webSocketEndpoints", alias = "websocketEndpoints")]
    pub endpoints: Vec<PluginProviderEndpoint>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProviderEndpoint {
    pub id: String,
    pub url: String,
    #[serde(default = "default_provider_endpoint_paths")]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub allowed_methods: Vec<String>,
    #[serde(default)]
    pub websocket_enabled: bool,
}

fn default_provider_endpoint_paths() -> Vec<String> {
    vec!["/".to_owned()]
}

pub type PluginProviderWebSocketEndpoint = PluginProviderEndpoint;

fn default_provider_session_timeout_ms() -> u32 {
    30_000
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginProviderUpstreamResponseMode {
    #[default]
    Auto,
    NonStreaming,
    Streaming,
}

impl PluginProviderSelection {
    pub fn from_config(config: &Value) -> Option<Self> {
        let selection = config.get("providerSelection")?.as_object()?;
        let endpoints = selection.get("endpoints").cloned().unwrap_or_else(|| {
            selection
                .get("webSocketEndpoints")
                .or_else(|| selection.get("websocketEndpoints"))
                .and_then(Value::as_array)
                .map(|legacy| {
                    Value::Array(
                        legacy
                            .iter()
                            .map(|endpoint| {
                                let id = endpoint.get("id").cloned().unwrap_or(Value::Null);
                                let (url, websocket_enabled) = endpoint
                                    .get("url")
                                    .and_then(Value::as_str)
                                    .map(|url| (json!(url), true))
                                    .unwrap_or((Value::Null, true));
                                json!({
                                    "id": id,
                                    "url": url,
                                    "allowedPaths": ["/"],
                                    "allowedMethods": [],
                                    "websocketEnabled": websocket_enabled
                                })
                            })
                            .collect(),
                    )
                })
                .unwrap_or_else(|| json!([]))
        });
        serde_json::from_value(json!({
            "channelIds": selection.get("channelIds").cloned().unwrap_or_else(|| json!([])),
            "models": selection.get("models").cloned().unwrap_or_else(|| json!([])),
            "priority": selection.get("priority").cloned().unwrap_or_else(|| json!(0)),
            "upstreamResponseMode": selection.get("upstreamResponseMode").cloned().unwrap_or_else(|| json!("auto")),
            "sessionEnabled": selection.get("sessionEnabled").cloned().unwrap_or(json!(false)),
            "sessionTimeoutMs": selection.get("sessionTimeoutMs").cloned().unwrap_or_else(|| json!(default_provider_session_timeout_ms())),
            "endpoints": endpoints
        }))
        .ok()
    }

    fn validate(&self) -> Result<(), AdminPluginLifecycleError> {
        let mut channels = std::collections::BTreeSet::new();
        let mut models = std::collections::BTreeSet::new();
        if self.channel_ids.len() > 128
            || !self
                .channel_ids
                .iter()
                .all(|id| *id > 0 && channels.insert(id))
            || self.models.len() > 128
            || !self.models.iter().all(|model| {
                !model.is_empty()
                    && model.len() <= 255
                    && model.trim() == model
                    && !model.chars().any(char::is_control)
                    && models.insert(model)
            })
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        if !(1_000..=360_000).contains(&self.session_timeout_ms) || self.endpoints.len() > 16 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let mut endpoint_ids = std::collections::BTreeSet::new();
        for endpoint in &self.endpoints {
            if endpoint.id.is_empty()
                || endpoint.id.len() > 64
                || !endpoint.id.as_bytes()[0].is_ascii_lowercase()
                || !endpoint.id.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
                })
                || !endpoint_ids.insert(&endpoint.id)
            {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
            if endpoint.allowed_paths.is_empty()
                || endpoint.allowed_paths.len() > 64
                || endpoint
                    .allowed_paths
                    .iter()
                    .any(|path| validate_provider_endpoint_path(path).is_err())
                || endpoint.allowed_methods.len() > 7
                || !endpoint.allowed_methods.iter().all(|method| {
                    matches!(
                        method.as_str(),
                        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
                    )
                })
                || endpoint
                    .allowed_methods
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != endpoint.allowed_methods.len()
                || (endpoint.allowed_methods.is_empty() && !endpoint.websocket_enabled)
            {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
            let url = url::Url::parse(&endpoint.url)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            if !matches!(url.scheme(), "https" | "wss")
                || url.host_str().is_none()
                || url.username() != ""
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !matches!(url.path(), "" | "/")
                || url.port() == Some(0)
                || matches!(url.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))
                || (url.scheme() == "wss"
                    && (!endpoint.websocket_enabled || !endpoint.allowed_methods.is_empty()))
            {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
        }
        Ok(())
    }
}

fn validate_provider_endpoint_path(path: &str) -> Result<(), AdminPluginLifecycleError> {
    if path.is_empty()
        || path.len() > 2_048
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\', '%'])
        || path
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || path.split('/').any(|segment| matches!(segment, "." | ".."))
    {
        Err(AdminPluginLifecycleError::InvalidInput)
    } else {
        Ok(())
    }
}

impl AdminPluginDeployment {
    fn from_record(record: PluginDeploymentRecord) -> Self {
        let needs_configuration = record
            .config
            .get(PLUGIN_SETUP_PENDING_FIELD)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Self {
            plugin_id: record.plugin_id,
            generation: record.generation,
            artifact: AdminPluginArtifact::from_record(record.artifact),
            enabled: record.enabled,
            needs_configuration,
            provider_selection: PluginProviderSelection::from_config(&record.config),
            granted_capabilities: record
                .granted_capabilities
                .into_iter()
                .map(AdminPluginCapabilityGrant::from)
                .collect(),
            updated_by: record.updated_by.get(),
            updated_at: record.updated_at,
            request_id: record.request_id,
        }
    }

    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    #[must_use]
    pub const fn generation(&self) -> i64 {
        self.generation
    }

    #[must_use]
    pub const fn artifact(&self) -> &AdminPluginArtifact {
        &self.artifact
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn needs_configuration(&self) -> bool {
        self.needs_configuration
    }

    #[must_use]
    pub fn granted_capabilities(&self) -> &[AdminPluginCapabilityGrant] {
        &self.granted_capabilities
    }

    #[must_use]
    pub const fn provider_selection(&self) -> Option<&PluginProviderSelection> {
        self.provider_selection.as_ref()
    }

    #[must_use]
    pub const fn updated_by(&self) -> i64 {
        self.updated_by
    }

    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

/// Safe public projection of one explicitly granted plugin capability.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginCapabilityGrant {
    id: String,
    version: u16,
}

impl From<PluginCapabilityGrant> for AdminPluginCapabilityGrant {
    fn from(value: PluginCapabilityGrant) -> Self {
        Self {
            id: value.id,
            version: value.version,
        }
    }
}

impl AdminPluginCapabilityGrant {
    pub fn new(id: String, version: u16) -> Result<Self, AdminPluginLifecycleError> {
        if id.is_empty() || id.len() > 128 || version == 0 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self { id, version })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }
}

impl fmt::Debug for AdminPluginDeployment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminPluginDeployment")
            .field("plugin_id", &self.plugin_id)
            .field("generation", &self.generation)
            .field("artifact", &self.artifact)
            .field("enabled", &self.enabled)
            .field("config", &"<redacted>")
            .field("granted_capabilities", &self.granted_capabilities)
            .field("updated_by", &self.updated_by)
            .field("updated_at", &self.updated_at)
            .field("request_id", &self.request_id)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginDeploymentPage {
    deployments: Vec<AdminPluginDeployment>,
    next_before_plugin_id: Option<String>,
}

impl AdminPluginDeploymentPage {
    #[must_use]
    pub fn deployments(&self) -> &[AdminPluginDeployment] {
        &self.deployments
    }

    #[must_use]
    pub fn next_before_plugin_id(&self) -> Option<&str> {
        self.next_before_plugin_id.as_deref()
    }
}

/// Validated desired deployment state. Callers cannot set the actor, timestamp,
/// or request id; the service derives those from the authenticated request.
#[derive(Clone)]
pub struct PluginDeploymentCommand {
    plugin_id: String,
    artifact_digest: String,
    expected_generation: Option<i64>,
    enabled: bool,
    granted_capabilities: Vec<AdminPluginCapabilityGrant>,
    config: Value,
}

/// Administrator supplied artifact locator and optional publisher signature.
/// Integrity is always checked; configured publisher keys enable signature verification.
#[derive(Clone)]
pub struct PluginArtifactRegistrationCommand {
    plugin_id: String,
    plugin_version: String,
    manifest: Value,
    package_path: String,
    executable_path: String,
    sha256: String,
    signer_key_id: String,
    signature_base64: String,
    source_reference: Option<String>,
}

impl PluginArtifactRegistrationCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: String,
        plugin_version: String,
        manifest: Value,
        package_path: String,
        executable_path: String,
        sha256: String,
        signer_key_id: String,
        signature_base64: String,
    ) -> Result<Self, AdminPluginLifecycleError> {
        if plugin_id.is_empty()
            || plugin_id.len() > 128
            || plugin_version.is_empty()
            || plugin_version.len() > 128
            || !manifest.is_object()
            || package_path.is_empty()
            || package_path.len() > 900
            || executable_path.is_empty()
            || executable_path.len() > 900
            || sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || signer_key_id.len() > 128
            || signer_key_id.is_empty() != signature_base64.is_empty()
            || (!signature_base64.is_empty() && signature_base64.len() != 88)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self {
            plugin_id,
            plugin_version,
            manifest,
            package_path,
            executable_path,
            sha256,
            signer_key_id,
            signature_base64,
            source_reference: None,
        })
    }

    pub fn with_source_reference(
        mut self,
        source_reference: String,
    ) -> Result<Self, AdminPluginLifecycleError> {
        if source_reference.is_empty()
            || source_reference.len() > 2048
            || source_reference.chars().any(char::is_control)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        self.source_reference = Some(source_reference);
        Ok(self)
    }
}

impl fmt::Debug for PluginArtifactRegistrationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginArtifactRegistrationCommand")
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("manifest", &"<redacted>")
            .field("package_path", &"<relative path>")
            .field("executable_path", &"<relative path>")
            .field("sha256", &self.sha256)
            .field("signer_key_id", &self.signer_key_id)
            .field("signature_base64", &"<redacted>")
            .field("source_reference", &"<redacted>")
            .finish()
    }
}

impl PluginDeploymentCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: String,
        artifact_digest: String,
        expected_generation: Option<i64>,
        enabled: bool,
        granted_capabilities: Vec<AdminPluginCapabilityGrant>,
        config: Value,
    ) -> Result<Self, AdminPluginLifecycleError> {
        if plugin_id.is_empty()
            || plugin_id.len() > 128
            || artifact_digest.len() != 64
            || !artifact_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || expected_generation.is_some_and(|generation| generation <= 0)
            || !config.is_object()
            || granted_capabilities.len() > af_db::MAX_PLUGIN_CAPABILITIES
            || (enabled && granted_capabilities.is_empty())
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let mut seen = std::collections::BTreeSet::new();
        if granted_capabilities
            .iter()
            .any(|grant| !seen.insert(grant.id.clone()))
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self {
            plugin_id,
            artifact_digest,
            expected_generation,
            enabled,
            granted_capabilities,
            config,
        })
    }

    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

impl fmt::Debug for PluginDeploymentCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginDeploymentCommand")
            .field("plugin_id", &self.plugin_id)
            .field("artifact_digest", &self.artifact_digest)
            .field("expected_generation", &self.expected_generation)
            .field("enabled", &self.enabled)
            .field("granted_capabilities", &self.granted_capabilities)
            .field("config", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AdminPluginLifecycleError {
    #[error("plugin lifecycle access is forbidden")]
    Forbidden,
    #[error("plugin lifecycle request is invalid")]
    InvalidInput,
    #[error("the simple installer only grants the standard public catalog capabilities")]
    CapabilityReviewRequired,
    #[error("plugin package upload exceeds the size limit")]
    PayloadTooLarge,
    #[error("plugin lifecycle record was not found")]
    NotFound,
    #[error("plugin lifecycle state conflicts with the request")]
    Conflict,
    #[error("plugin sidecar runtime is not configured or available")]
    RuntimeUnavailable,
    #[error("plugin lifecycle service is unavailable")]
    Internal,
}

/// Admin-only access to the durable plugin catalog and its optional sidecar runtime.
pub struct DatabaseAdminPluginLifecycleService {
    data_plane: Option<data_plane::DataPlane>,
    repository: PluginCatalogRepository,
    audit: Arc<dyn PlatformAuditService>,
    market_clients: Option<HttpClientProvider>,
    sidecar_runtime: Option<Arc<AdminPluginSidecarRuntime>>,
    storage_dispatcher: Option<Arc<dyn PluginStorageDispatcher>>,
    event_delivery_repository: Option<PluginEventDeliveryRepository>,
    custom_capability_slots: Arc<tokio::sync::Semaphore>,
    pub(crate) catalog_admission: crate::plugin_catalog_extension::PluginCatalogAdmission,
}

impl DatabaseAdminPluginLifecycleService {
    pub(crate) fn catalog_repository(&self) -> &PluginCatalogRepository {
        &self.repository
    }

    pub(crate) fn storage_dispatcher(&self) -> Option<Arc<dyn PluginStorageDispatcher>> {
        self.storage_dispatcher.clone()
    }

    fn artifact_view(&self, record: PluginArtifactRecord) -> AdminPluginArtifact {
        let official = self
            .sidecar_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.is_official_artifact(&record));
        let mut view = AdminPluginArtifact::from_record(record);
        view.official = official;
        view
    }

    fn deployment_view(&self, record: PluginDeploymentRecord) -> AdminPluginDeployment {
        let official = self
            .sidecar_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.is_official_artifact(&record.artifact));
        let mut view = AdminPluginDeployment::from_record(record);
        view.artifact.official = official;
        view
    }
    #[must_use]
    pub fn new(repository: PluginCatalogRepository, audit: Arc<dyn PlatformAuditService>) -> Self {
        Self {
            repository,
            audit,
            data_plane: None,
            market_clients: None,
            sidecar_runtime: None,
            storage_dispatcher: None,
            event_delivery_repository: None,
            custom_capability_slots: Arc::new(tokio::sync::Semaphore::new(
                PLUGIN_CUSTOM_CAPABILITY_MAX_CONCURRENCY,
            )),
            catalog_admission: crate::plugin_catalog_extension::PluginCatalogAdmission::default(),
        }
    }

    #[must_use]
    pub fn with_market_clients(mut self, clients: HttpClientProvider) -> Self {
        self.market_clients = Some(clients);
        self
    }

    #[must_use]
    pub fn with_sidecar_runtime(mut self, runtime: Option<Arc<AdminPluginSidecarRuntime>>) -> Self {
        self.sidecar_runtime = runtime;
        self
    }

    #[must_use]
    pub fn with_storage_dispatcher(
        mut self,
        dispatcher: Option<Arc<dyn PluginStorageDispatcher>>,
    ) -> Self {
        self.storage_dispatcher = dispatcher;
        self
    }

    #[must_use]
    pub fn with_event_delivery_repository(
        mut self,
        repository: Option<PluginEventDeliveryRepository>,
    ) -> Self {
        self.event_delivery_repository = repository;
        self
    }

    /// Return the narrowly scoped host capability port for trusted business
    /// extensions. It does not authorize callers or expose platform secrets.
    #[must_use]
    pub fn capability_runtime(&self) -> Option<Arc<AdminPluginSidecarRuntime>> {
        self.sidecar_runtime.clone()
    }

    /// Fetch and validate a static market index without executing any plugin
    /// code. The URL is supplied by the authenticated caller but remains
    /// subject to the managed HTTP client's target policy and strict HTTPS
    /// validation.
    pub async fn fetch_market_index(
        &self,
        index_url: &str,
    ) -> Result<PluginMarketIndex, AdminPluginLifecycleError> {
        let url =
            url::Url::parse(index_url).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || index_url.len() > 4096
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let clients = self
            .market_clients
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let client = clients
            .get(Some(Duration::from_secs(30)))
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        let response = execute_market_get(&client, index_url)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > MAX_PLUGIN_MARKET_INDEX_BYTES)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        if bytes.len() as u64 > MAX_PLUGIN_MARKET_INDEX_BYTES {
            return Err(AdminPluginLifecycleError::PayloadTooLarge);
        }
        let index: PluginMarketIndex =
            serde_json::from_slice(&bytes).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        index
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        Ok(index)
    }

    /// Invoke a plugin-defined, explicitly granted read-only capability. This
    /// generic entry point lets an admin use new plugin features without a new
    /// platform-specific Rust handler for every capability ID.
    pub async fn invoke_custom_capability(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        capability_id: &str,
        capability_version: u16,
        input: Value,
        request_id: &str,
    ) -> Result<AdminPluginCapabilityInvocationResult, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let plugin_id_value = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let capability = CapabilityId::new(capability_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if capability_version == 0
            || serde_json::to_vec(&input)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?
                .len()
                > 256 * 1024
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let _permit = self
            .custom_capability_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        let deployment = self
            .repository
            .get_deployment(plugin_id_value.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !custom_capability_is_callable(&deployment, capability.as_str(), capability_version) {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let now_ms = time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let deadline_unix_ms = u64::try_from(now_ms)
            .ok()
            .and_then(|now| {
                now.checked_add(
                    u64::try_from(PLUGIN_CUSTOM_CAPABILITY_CALL_BUDGET.as_millis()).ok()?,
                )
            })
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let invocation = af_plugin_runtime::CapabilityExecutionContext::new(
            format!("admin-extension:{}", plugin_id_value.as_str()),
            InvocationMetadata {
                request_id: request_id.to_owned(),
                deadline_unix_ms,
            },
            deployment_public_configuration(&deployment),
            PLUGIN_CUSTOM_CAPABILITY_MAX_BYTES,
        )
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let invoked = match self.storage_dispatcher.clone() {
            Some(storage_dispatcher) => {
                runtime
                    .invoke_capability_with_storage(
                        &self.repository,
                        plugin_id_value.as_str(),
                        deployment.generation,
                        &invocation,
                        capability.as_str(),
                        capability_version,
                        input,
                        storage_dispatcher,
                    )
                    .await
            }
            None => {
                runtime
                    .invoke_capability(
                        &self.repository,
                        plugin_id_value.as_str(),
                        deployment.generation,
                        &invocation,
                        capability.as_str(),
                        capability_version,
                        input,
                    )
                    .await
            }
        };
        let audit = audit_entry(
            principal,
            request_id,
            "/api/admin/plugins/{pluginId}/capabilities/{capabilityId}/invoke",
            "invoke_custom_capability",
            "plugin_capability",
            Some(format!("{plugin_id}:{capability_id}:{capability_version}")),
            if invoked.is_ok() {
                PlatformAuditOutcome::Succeeded
            } else {
                PlatformAuditOutcome::Failed
            },
            None,
            None,
        )?;
        self.audit.record(audit).await.map_err(map_audit_error)?;
        let result = invoked?;
        let latest = self
            .repository
            .get_deployment(plugin_id_value.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !custom_capability_is_callable(&latest, capability.as_str(), capability_version)
            || latest.generation != deployment.generation
        {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        Ok(AdminPluginCapabilityInvocationResult {
            plugin_id: plugin_id_value.as_str().to_owned(),
            deployment_generation: deployment.generation,
            capability_id: capability.as_str().to_owned(),
            capability_version,
            output: result.output,
        })
    }

    /// Invoke a manifest-declared administrator route through the same
    /// generation, capability and schema checks as other plugin operations.
    /// The public route path is intentionally fixed by the host; the manifest
    /// only selects the route ID and its bound capability.
    pub async fn invoke_plugin_route(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        route_id: &str,
        method: PluginRouteMethod,
        input: Value,
        request_id: &str,
    ) -> Result<AdminPluginRouteInvocationResult, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        self.invoke_plugin_route_for_audience(
            principal,
            plugin_id,
            route_id,
            method,
            input,
            request_id,
            af_plugin_contract::PluginRouteAudience::Admin,
            "/api/admin/plugins/{pluginId}/routes/{routeId}",
        )
        .await
    }

    /// Invoke a user-facing manifest route. User routes are deliberately
    /// separate from administrator routes and use the caller's own session
    /// identity in the invocation metadata.
    pub async fn invoke_user_plugin_route(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        route_id: &str,
        method: PluginRouteMethod,
        input: Value,
        request_id: &str,
    ) -> Result<AdminPluginRouteInvocationResult, AdminPluginLifecycleError> {
        self.invoke_plugin_route_for_audience(
            principal,
            plugin_id,
            route_id,
            method,
            input,
            request_id,
            af_plugin_contract::PluginRouteAudience::User,
            "/api/account/plugins/{pluginId}/routes/{routeId}",
        )
        .await
    }

    /// Invoke a model-key route with the already authenticated user identity.
    /// Model-key routes use a separate manifest audience so they cannot be
    /// reached through the browser session endpoints by accident.
    pub async fn invoke_model_plugin_route(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        route_id: &str,
        method: PluginRouteMethod,
        input: Value,
        request_id: &str,
    ) -> Result<AdminPluginRouteInvocationResult, AdminPluginLifecycleError> {
        self.invoke_plugin_route_for_audience(
            principal,
            plugin_id,
            route_id,
            method,
            input,
            request_id,
            af_plugin_contract::PluginRouteAudience::ModelKey,
            "/api/plugins/{pluginId}/routes/{routeId}",
        )
        .await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "插件路由调用同时携带认证、路由、受众和审计上下文"
    )]
    async fn invoke_plugin_route_for_audience(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        route_id: &str,
        method: PluginRouteMethod,
        input: Value,
        request_id: &str,
        audience: af_plugin_contract::PluginRouteAudience,
        audit_route: &'static str,
    ) -> Result<AdminPluginRouteInvocationResult, AdminPluginLifecycleError> {
        let plugin_id_value = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if route_id.is_empty()
            || route_id.len() > 64
            || !route_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        if serde_json::to_vec(&input)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?
            .len()
            > 1024 * 1024
        {
            return Err(AdminPluginLifecycleError::PayloadTooLarge);
        }
        let deployment = self
            .repository
            .get_deployment(plugin_id_value.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !deployment.enabled
            || deployment.artifact.approval.is_none()
            || deployment.artifact.revocation.is_some()
            || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let manifest: PluginManifest = serde_json::from_value(deployment.artifact.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let route = manifest
            .routes
            .iter()
            .find(|route| route.id == route_id)
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if route.method != method || route.audience != audience {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let capability = manifest
            .capabilities
            .iter()
            .find(|capability| {
                capability.id == route.capability_id
                    && capability.version == route.capability_version
            })
            .ok_or(AdminPluginLifecycleError::Forbidden)?;
        if !deployment
            .granted_capabilities
            .iter()
            .any(|grant| grant.id == capability.id.as_str() && grant.version == capability.version)
            || capability.input_schema.is_none()
            || capability.output_schema.is_none()
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let now_ms = time::OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .checked_div(1_000_000)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let deadline_unix_ms = now_ms
            .checked_add(u64::from(route.timeout_ms))
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let invocation = af_plugin_runtime::CapabilityExecutionContext::new(
            format!("plugin-route:{}:{}", plugin_id_value.as_str(), route_id),
            InvocationMetadata {
                request_id: request_id.to_owned(),
                deadline_unix_ms,
            },
            deployment_public_configuration(&deployment),
            route.max_body_bytes,
        )
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let invoked = runtime
            .invoke_route_with_storage(
                &self.repository,
                plugin_id_value.as_str(),
                deployment.generation,
                &invocation,
                route_id,
                capability.id.as_str(),
                capability.version,
                input,
                (audience != af_plugin_contract::PluginRouteAudience::Admin)
                    .then(|| principal.user_id().get()),
                self.storage_dispatcher.clone(),
            )
            .await;
        let audit = audit_entry(
            principal,
            request_id,
            audit_route,
            "invoke_plugin_route",
            "plugin_route",
            Some(format!("{plugin_id}:{route_id}")),
            if invoked.is_ok() {
                PlatformAuditOutcome::Succeeded
            } else {
                PlatformAuditOutcome::Failed
            },
            None,
            None,
        )?;
        self.audit.record(audit).await.map_err(map_audit_error)?;
        let result = invoked?;
        let latest = self
            .repository
            .get_deployment(plugin_id_value.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !latest.enabled || latest.generation != deployment.generation {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        Ok(AdminPluginRouteInvocationResult {
            plugin_id: plugin_id_value.as_str().to_owned(),
            route_id: route_id.to_owned(),
            deployment_generation: deployment.generation,
            capability_id: capability.id.as_str().to_owned(),
            capability_version: capability.version,
            output: result.output,
        })
    }

    async fn prepare_plugin_operation(
        &self,
        plugin_id: &str,
        capability_id: &str,
        capability_version: u16,
        request_id: &str,
        max_bytes: u32,
    ) -> Result<
        (
            PluginDeploymentRecord,
            af_plugin_runtime::CapabilityExecutionContext,
        ),
        AdminPluginLifecycleError,
    > {
        let plugin_id = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let capability_id = CapabilityId::new(capability_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let deployment = self
            .repository
            .get_deployment(plugin_id.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !deployment.enabled
            || deployment.generation <= 0
            || deployment.artifact.approval.is_none()
            || deployment.artifact.revocation.is_some()
            || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
            || !deployment.granted_capabilities.iter().any(|grant| {
                grant.id == capability_id.as_str() && grant.version == capability_version
            })
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let now_ms = time::OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .checked_div(1_000_000)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let invocation = af_plugin_runtime::CapabilityExecutionContext::new(
            format!("plugin-operation:{}", plugin_id.as_str()),
            InvocationMetadata {
                request_id: request_id.to_owned(),
                deadline_unix_ms: now_ms.saturating_add(30_000),
            },
            deployment_public_configuration(&deployment),
            max_bytes,
        )
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        Ok((deployment, invocation))
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "凭据生命周期入口需要固定携带绑定、回调和审计上下文"
    )]
    pub async fn invoke_plugin_credential(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        credential_id: &str,
        channel_id: i64,
        callback_state: Option<&str>,
        capability_version: u16,
        operation: PluginCredentialOperation,
        request_id: &str,
    ) -> Result<AdminPluginCredentialInvocationResult, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        if matches!(&operation, PluginCredentialOperation::WebSocketAuth { .. }) {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let credential = credential_id
            .parse::<i64>()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        self.bind_plugin_credential(
            principal,
            plugin_id,
            channel_id,
            credential,
            capability_version,
            request_id,
        )
        .await?;
        let mut result = self
            .credential_operation(
                plugin_id,
                channel_id,
                credential,
                operation,
                callback_state,
                request_id,
            )
            .await?;
        if matches!(&result, PluginCredentialResult::WebSocketAuth { .. }) {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        // Signing material is only consumed on the internal Relay path.
        if let PluginCredentialResult::Signed {
            headers,
            query,
            body,
        } = &mut result
        {
            headers.clear();
            *query = None;
            *body = None;
        }
        let deployment = self
            .repository
            .get_deployment(plugin_id)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        Ok(AdminPluginCredentialInvocationResult {
            plugin_id: plugin_id.to_owned(),
            deployment_generation: deployment.generation,
            credential_id: credential_id.to_owned(),
            result,
        })
    }

    pub async fn invoke_plugin_task_direct(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        capability_id: &str,
        capability_version: u16,
        task: PluginTaskOperation,
        request_id: &str,
    ) -> Result<AdminPluginTaskInvocationResult, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        task.validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let (deployment, invocation) = self
            .prepare_plugin_operation(
                plugin_id,
                capability_id,
                capability_version,
                request_id,
                PLUGIN_CUSTOM_CAPABILITY_MAX_BYTES,
            )
            .await?;
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let result = runtime
            .invoke_task(
                &self.repository,
                plugin_id,
                deployment.generation,
                &invocation,
                capability_id,
                capability_version,
                task,
            )
            .await?;
        Ok(AdminPluginTaskInvocationResult {
            plugin_id: plugin_id.to_owned(),
            deployment_generation: deployment.generation,
            result,
        })
    }

    pub async fn propose_capability_action(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        capability_id: &str,
        capability_version: u16,
        idempotency_key: &str,
        input: Value,
        request_id: &str,
    ) -> Result<AdminPluginCapabilityActionProposal, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let plugin_id_value = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let capability = CapabilityId::new(capability_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let input_sha256 = capability_action_input_digest(&input)?;
        let deployment = self
            .repository
            .get_deployment(plugin_id_value.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        let manifest: PluginManifest = serde_json::from_value(deployment.artifact.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let declaration = manifest
            .capabilities
            .iter()
            .find(|item| item.id == capability && item.version == capability_version)
            .ok_or(AdminPluginLifecycleError::Forbidden)?;
        if declaration.access != CapabilityAccess::Write
            || declaration.side_effect != SideEffectClass::ExternalWrite
            || !declaration
                .write_semantics
                .is_some_and(|semantics| semantics.native_idempotency_key)
            || declaration.input_schema.is_none()
            || declaration.output_schema.is_none()
            || deployment.artifact.approval.is_none()
            || deployment.artifact.revocation.is_some()
            || !deployment.enabled
            || !deployment
                .granted_capabilities
                .iter()
                .any(|grant| grant.id == capability.as_str() && grant.version == capability_version)
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        if let Some(schema) = declaration.input_schema.as_ref() {
            af_plugin_runtime::validate_plugin_configuration(schema, &input)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        }
        let now = current_timestamp()?;
        let (record, _) = self
            .repository
            .capability_action_repository()
            .create(af_db::PluginCapabilityActionCreate {
                proposal_key: uuid::Uuid::new_v4().simple().to_string(),
                idempotency_key: idempotency_key.to_owned(),
                requested_by_user_id: principal.user_id().get(),
                plugin_id: deployment.plugin_id.clone(),
                artifact_id: deployment.artifact.id,
                artifact_digest: deployment.artifact.digest.clone(),
                deployment_generation: deployment.generation,
                capability_id: capability.as_str().to_owned(),
                capability_version,
                input_sha256,
                created_at: now,
                expires_at: now.saturating_add(PLUGIN_CAPABILITY_ACTION_TTL_SECONDS),
            })
            .await
            .map_err(|error| match error {
                af_db::PluginCapabilityActionRepositoryError::Conflict => {
                    AdminPluginLifecycleError::Conflict
                }
                _ => AdminPluginLifecycleError::Internal,
            })?;
        self.audit_host_action(
            principal,
            request_id,
            "propose_plugin_capability_action",
            &record.proposal_key,
            PlatformAuditOutcome::Succeeded,
            None,
            Some(
                json!({"pluginId": plugin_id, "capabilityId": capability_id, "status": "pending"}),
            ),
        )
        .await?;
        Ok(AdminPluginCapabilityActionProposal {
            proposal_key: record.proposal_key,
            expires_at: record.expires_at,
            status: capability_action_status(record.status).to_owned(),
        })
    }

    pub async fn confirm_capability_action(
        &self,
        principal: SessionPrincipal,
        proposal_key: &str,
        confirmation_key: &str,
        input: Value,
        request_id: &str,
    ) -> Result<AdminPluginCapabilityActionResult, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let input_sha256 = capability_action_input_digest(&input)?;
        let repository = self.repository.capability_action_repository();
        let existing = repository
            .get_by_key(proposal_key)
            .await
            .map_err(|_| AdminPluginLifecycleError::NotFound)?;
        let task = serde_json::from_value::<PluginTaskOperation>(input.clone())
            .ok()
            .filter(|task| matches!(task, PluginTaskOperation::Submit { .. }));
        let claimed = repository
            .claim_for_execution(
                proposal_key,
                principal.user_id().get(),
                confirmation_key,
                &input_sha256,
                current_timestamp()?,
            )
            .await
            .map_err(|_| AdminPluginLifecycleError::Conflict)?;
        if matches!(
            claimed.status,
            af_db::PluginCapabilityActionStatus::Succeeded
                | af_db::PluginCapabilityActionStatus::Unknown
                | af_db::PluginCapabilityActionStatus::Expired
                | af_db::PluginCapabilityActionStatus::Revoked
        ) {
            return Ok(AdminPluginCapabilityActionResult {
                proposal_key: claimed.proposal_key,
                status: capability_action_status(claimed.status).to_owned(),
                outcome_code: claimed.outcome_code,
            });
        }
        let deadline = time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let deployment = self
            .repository
            .get_deployment(&existing.plugin_id)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !deployment.enabled
            || deployment.generation != existing.deployment_generation
            || deployment.artifact.digest != existing.artifact_digest
            || deployment.artifact.revocation.is_some()
            || deployment.artifact.approval.is_none()
        {
            let _ = repository
                .finish(
                    proposal_key,
                    af_db::PluginCapabilityActionStatus::Unknown,
                    "deployment_changed",
                )
                .await;
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let invocation = af_plugin_runtime::CapabilityExecutionContext::new(
            format!("plugin-action:{proposal_key}"),
            InvocationMetadata {
                request_id: format!("plugin-action:{proposal_key}"),
                deadline_unix_ms: u64::try_from(deadline)
                    .unwrap_or(u64::MAX)
                    .saturating_add(30_000),
            },
            deployment_public_configuration(&deployment),
            PLUGIN_CUSTOM_CAPABILITY_MAX_BYTES,
        )
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let runtime = self.sidecar_runtime.as_ref();
        let result = match runtime {
            Some(runtime) => {
                if let Some(task) = task {
                    runtime
                        .invoke_task(
                            &self.repository,
                            &existing.plugin_id,
                            existing.deployment_generation,
                            &invocation,
                            &existing.capability_id,
                            existing.capability_version,
                            task,
                        )
                        .await
                        .map(|result| match result {
                            PluginTaskResult::Submitted { .. } => "submitted",
                            PluginTaskResult::Status { .. } => "status",
                            PluginTaskResult::Result { .. } => "completed",
                        })
                        .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)
                } else {
                    runtime
                        .invoke_external_write(
                            &self.repository,
                            &existing.plugin_id,
                            existing.deployment_generation,
                            &invocation,
                            &existing.capability_id,
                            existing.capability_version,
                            input,
                        )
                        .await
                        .map(|_| "completed")
                        .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)
                }
            }
            None => Err(AdminPluginLifecycleError::RuntimeUnavailable),
        };
        let (status, outcome_code) = match result {
            Ok(code) => (af_db::PluginCapabilityActionStatus::Succeeded, code),
            Err(_) => (
                af_db::PluginCapabilityActionStatus::Unknown,
                "outcome_unknown",
            ),
        };
        let finished = repository
            .finish(proposal_key, status, outcome_code)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        self.audit_host_action(
            principal,
            request_id,
            "confirm_plugin_capability_action",
            proposal_key,
            if status == af_db::PluginCapabilityActionStatus::Succeeded {
                PlatformAuditOutcome::Succeeded
            } else {
                PlatformAuditOutcome::Failed
            },
            None,
            Some(json!({"status": outcome_code})),
        )
        .await?;
        Ok(AdminPluginCapabilityActionResult {
            proposal_key: finished.proposal_key,
            status: outcome_code.to_owned(),
            outcome_code: Some(outcome_code.to_owned()),
        })
    }

    pub(crate) async fn audit_host_action(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
        operation: &'static str,
        proposal_key: &str,
        outcome: PlatformAuditOutcome,
        before: Option<Value>,
        after: Option<Value>,
    ) -> Result<(), AdminPluginLifecycleError> {
        let entry = audit_entry(
            principal,
            request_id,
            "/api/admin/channel-monitor/action-proposals",
            operation,
            "plugin_action_proposal",
            Some(proposal_key.to_owned()),
            outcome,
            before,
            after,
        )?;
        self.audit.record(entry).await.map_err(map_audit_error)
    }

    pub async fn list_event_results(
        &self,
        principal: SessionPrincipal,
        before_id: Option<i64>,
        limit: u32,
    ) -> Result<AdminPluginEventResultPage, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let repository = self
            .event_delivery_repository
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let query = PluginEventDeliveryResultQuery::new(before_id, limit)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let page = repository
            .list_results(query, plugin_event_now_unix_seconds())
            .await
            .map_err(|error| match error {
                af_db::PluginEventDeliveryRepositoryError::Invariant
                | af_db::PluginEventDeliveryRepositoryError::InvalidResult => {
                    AdminPluginLifecycleError::InvalidInput
                }
                af_db::PluginEventDeliveryRepositoryError::Query
                | af_db::PluginEventDeliveryRepositoryError::Timeout
                | af_db::PluginEventDeliveryRepositoryError::Conflict => {
                    AdminPluginLifecycleError::Internal
                }
            })?;
        Ok(AdminPluginEventResultPage {
            results: page
                .results
                .into_iter()
                .map(|record| {
                    let summary = record
                        .result
                        .get("summary")
                        .and_then(Value::as_str)
                        .filter(|value| safe_event_result_summary(value))
                        .unwrap_or("插件已处理此事件")
                        .to_owned();
                    AdminPluginEventResult {
                        delivery_id: record.id,
                        plugin_id: record.plugin_id,
                        event_kind: record.event_kind,
                        deployment_generation: record.deployment_generation,
                        capability_id: record.capability_id,
                        capability_version: record.capability_version,
                        attempt_count: record.attempt_count,
                        status: "delivered".to_owned(),
                        completed_at: record.completed_at,
                        summary,
                        result: record.result,
                    }
                })
                .collect(),
            next_before_id: page.next_before_id,
        })
    }

    /// Authorize before accepting package bytes, and reserve a bounded upload slot.
    pub async fn begin_package_upload(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
    ) -> Result<PluginPackageUpload, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/install",
            "install",
            "plugin",
            None,
        )
        .await?;
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let permit = runtime.acquire_installation_slot()?;
        let artifact_root = runtime.installation_root()?;
        let upload_directory = artifact_root.join(".uploads");
        match std::fs::create_dir(&upload_directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(&upload_directory)
                    .map_err(|_| AdminPluginLifecycleError::Internal)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(AdminPluginLifecycleError::InvalidInput);
                }
            }
            Err(_) => return Err(AdminPluginLifecycleError::Internal),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&upload_directory, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| AdminPluginLifecycleError::Internal)?;
        }
        cleanup_stale_plugin_uploads(&upload_directory);
        let temporary = tempfile::Builder::new()
            .prefix(".plugin-upload-")
            .tempfile_in(&upload_directory)
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let file = tokio::fs::File::from_std(
            temporary
                .reopen()
                .map_err(|_| AdminPluginLifecycleError::Internal)?,
        );
        Ok(PluginPackageUpload {
            file,
            temporary,
            artifact_root,
            uploaded: 0,
            expected_plugin_id: None,
            expected_plugin_version: None,
            market_source: None,
            _permit: permit,
        })
    }

    /// One administrator action imports, authorizes and activates a package.
    pub async fn install_package(
        &self,
        principal: SessionPrincipal,
        upload: PluginPackageUpload,
        config: impl Into<Option<Value>> + Send,
        expected_generation: Option<i64>,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/install",
            "install",
            "plugin",
            None,
        )
        .await?;
        let result = self
            .install_package_authorized(
                principal,
                upload,
                config.into(),
                expected_generation,
                false,
                request_id,
            )
            .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/install",
            "install",
            "plugin",
            None,
            None,
            result.as_ref().ok().map(|deployment| {
                json!({
                    "pluginId": deployment.plugin_id(),
                    "generation": deployment.generation(),
                    "digest": deployment.artifact().digest(),
                })
            }),
            result,
        )
        .await
    }

    /// Download an immutable market artifact through the managed HTTP client,
    /// verify its digest and install it through the same package path as an
    /// uploaded archive. The caller never supplies a local filesystem path.
    pub async fn install_market_package(
        &self,
        principal: SessionPrincipal,
        request: PluginMarketInstallRequest,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let plugin_id = request.plugin_id.clone();
        let source_kind = request.source.kind.clone();
        let source_commit = request.source.commit.clone();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/market/install",
            "install",
            "plugin",
            Some(&plugin_id),
        )
        .await?;
        if PluginId::new(request.plugin_id.clone()).is_err()
            || af_plugin_contract::PluginVersion::new(request.plugin_version.clone()).is_err()
            || request
                .config
                .as_ref()
                .is_some_and(|config| !config.is_object())
            || request
                .expected_generation
                .is_some_and(|generation| generation <= 0)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let config = request.config;
        let source = request.source.clone().into_contract();
        source
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let clients = self
            .market_clients
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let client = clients
            .get(Some(Duration::from_secs(60)))
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        let response = execute_market_get(&client, &source.archive_url)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        if !response.status().is_success() {
            return Err(AdminPluginLifecycleError::Internal);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PLUGIN_PACKAGE_UPLOAD_BYTES as u64)
        {
            return Err(AdminPluginLifecycleError::PayloadTooLarge);
        }
        let mut upload = self
            .begin_package_upload(principal, request_id)
            .await?
            .bind_market_source(
                request.plugin_id.clone(),
                request.plugin_version.clone(),
                source.clone(),
            );
        let mut stream = response.into_bytes_stream();
        let mut hasher = Sha256::new();
        let mut downloaded = 0usize;
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk.map_err(|_| AdminPluginLifecycleError::Internal)?;
            downloaded = downloaded
                .checked_add(chunk.len())
                .filter(|size| *size <= MAX_PLUGIN_PACKAGE_UPLOAD_BYTES)
                .ok_or(AdminPluginLifecycleError::PayloadTooLarge)?;
            hasher.update(&chunk);
            upload.write_chunk(&chunk).await?;
        }
        let digest = format!("{:x}", hasher.finalize());
        if digest != source.sha256 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let result = self
            .install_package_authorized(
                principal,
                upload,
                config,
                request.expected_generation,
                true,
                request_id,
            )
            .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/market/install",
            "install",
            "plugin",
            Some(plugin_id),
            None,
            result.as_ref().ok().map(|deployment| {
                json!({
                    "pluginVersion": deployment.artifact().plugin_version(),
                    "generation": deployment.generation(),
                    "digest": deployment.artifact().digest(),
                    "sourceKind": source_kind,
                    "commit": source_commit,
                })
            }),
            result,
        )
        .await
    }

    async fn install_package_authorized(
        &self,
        principal: SessionPrincipal,
        mut upload: PluginPackageUpload,
        config: Option<Value>,
        expected_generation: Option<i64>,
        strict_generation: bool,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        use tokio::io::AsyncWriteExt as _;
        if upload.uploaded == 0
            || config.as_ref().is_some_and(|config| {
                !config.is_object()
                    || serde_json::to_vec(config)
                        .map_or(true, |bytes| bytes.len() > af_db::MAX_PLUGIN_CONFIG_BYTES)
            })
            || expected_generation.is_some_and(|generation| generation <= 0)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        upload
            .file
            .flush()
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        // A newly published cache directory is still owned by its import guard
        // until registration commits. Keep other imports, repair and cleanup
        // from reusing or deleting it before that ownership is transferred.
        let cache_guard = runtime.cache_operations.lock().await;
        let mut imported = crate::plugin_package::extract_plugin_package(
            upload.temporary.path().to_path_buf(),
            upload.artifact_root.clone(),
        )
        .await?;
        if let Some(existing) = self
            .repository
            .get_artifact_by_digest(&imported.plugin_id, &imported.digest)
            .await
            .map_err(map_catalog_error)?
        {
            if existing.revocation.is_some()
                || existing.plugin_version != imported.plugin_version
                || existing.manifest != imported.manifest
            {
                return Err(AdminPluginLifecycleError::Conflict);
            }
            let source: Value = serde_json::from_str(&existing.source_reference)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            let expected_path = source
                .get("packagePath")
                .and_then(Value::as_str)
                .ok_or(AdminPluginLifecycleError::InvalidInput)?;
            if expected_path != imported.package_path {
                imported = crate::plugin_package::extract_plugin_package_at(
                    upload.temporary.path().to_path_buf(),
                    upload.artifact_root.clone(),
                    Some(expected_path.to_owned()),
                )
                .await?;
            }
        }
        let manifest: PluginManifest = serde_json::from_value(imported.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if upload
            .expected_plugin_id
            .as_deref()
            .is_some_and(|expected| expected != imported.plugin_id)
            || upload
                .expected_plugin_version
                .as_deref()
                .is_some_and(|expected| expected != imported.plugin_version)
        {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let current = self
            .repository
            .get_deployment(&imported.plugin_id)
            .await
            .map_err(map_catalog_error)?;
        let current_generation = current.as_ref().map(|deployment| deployment.generation);
        if (strict_generation || expected_generation.is_some())
            && expected_generation != current_generation
        {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let configuration_was_provided = config.is_some();
        let existing_config = current.as_ref().map(|row| row.config.clone());
        let config = config.unwrap_or_else(|| existing_config.clone().unwrap_or_else(|| json!({})));
        manifest
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let configuration_is_valid = validate_public_plugin_configuration(
            &manifest.config_schema,
            &plugin_public_configuration(&config, &manifest.config_schema),
        )
        .is_ok();
        let may_defer_configuration = current.as_ref().is_none_or(|row| {
            row.config
                .get(PLUGIN_SETUP_PENDING_FIELD)
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        let install_unconfigured =
            !configuration_was_provided && may_defer_configuration && !configuration_is_valid;
        if !configuration_is_valid && !install_unconfigured {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let config = if install_unconfigured {
            let mut pending = existing_config.unwrap_or_else(|| json!({}));
            let fields = pending
                .as_object_mut()
                .ok_or(AdminPluginLifecycleError::InvalidInput)?;
            fields.insert(PLUGIN_SETUP_PENDING_FIELD.to_owned(), json!(true));
            pending
        } else {
            config
        };
        let capability_count = manifest.capabilities.len();
        let grants = manifest
            .capabilities
            .iter()
            .filter(|capability| simple_install_capability(capability))
            .map(|capability| {
                AdminPluginCapabilityGrant::new(
                    capability.id.as_str().to_owned(),
                    capability.version,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let requires_capability_review = grants.is_empty() || grants.len() != capability_count;
        // Upload and Install is an explicit operator action. When replacing an
        // existing version, bind it to the generation just read so concurrent
        // installs still fail compare-and-swap rather than silently racing.
        let deployment_expected_generation = expected_generation.or(current_generation);
        // A publisher unknown to this host is not a reason to block an
        // administrator-owned package, but it must never acquire a verified badge.
        let (signer_key_id, signature_base64) = if imported.signer_key_id.is_empty() {
            (String::new(), String::new())
        } else if runtime.recognizes_publisher(&imported.signer_key_id) {
            (
                imported.signer_key_id.clone(),
                imported.signature_base64.clone(),
            )
        } else {
            // Unknown publisher metadata does not block an explicit admin
            // upload, but the host records it as unverified rather than
            // presenting it as a trusted publisher.
            (String::new(), String::new())
        };
        let command = PluginArtifactRegistrationCommand::new(
            imported.plugin_id.clone(),
            imported.plugin_version.clone(),
            imported.manifest.clone(),
            imported.package_path.clone(),
            imported.executable_path.clone(),
            imported.digest.clone(),
            signer_key_id,
            signature_base64,
        )?;
        let command = if let Some(source) = upload.market_source.as_ref() {
            let source_reference = serde_json::to_string(&json!({
                "packagePath": imported.package_path.clone(),
                "executablePath": imported.executable_path.clone(),
                "origin": source,
            }))
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
            command.with_source_reference(source_reference)?
        } else {
            command
        };
        let artifact = self
            .register_artifact_candidate(principal, command, request_id)
            .await?;
        // The catalog now owns the source locator even if archive publication
        // or a later activation fails. A retry can safely resume this import.
        imported.retain();
        // Activation takes the same cache lock to restore and verify a replica.
        drop(cache_guard);
        // Publish the complete immutable archive before a shared deployment can
        // refer to it. Other nodes restore exactly this registered artifact.
        let archive = tokio::fs::read(upload.temporary.path())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        self.repository
            .distribution_repository()
            .store_package(
                artifact.id(),
                artifact.digest(),
                &archive,
                current_timestamp()?,
            )
            .await
            .map_err(crate::plugin_operations::map_distribution_error)?;
        if artifact.approval().is_none() {
            match self
                .approve_artifact(principal, artifact.id(), request_id)
                .await
            {
                Ok(()) => {}
                Err(AdminPluginLifecycleError::Conflict) => {
                    // Concurrent identical uploads can race to create the
                    // single durable approval row. Continue only if the
                    // winner approved the same artifact and it is not revoked.
                    let latest = self
                        .repository
                        .get_artifact(artifact.id())
                        .await
                        .map_err(map_catalog_error)?
                        .ok_or(AdminPluginLifecycleError::NotFound)?;
                    if latest.approval.is_none() || latest.revocation.is_some() {
                        return Err(AdminPluginLifecycleError::Conflict);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        if requires_capability_review {
            return Err(AdminPluginLifecycleError::CapabilityReviewRequired);
        }
        let command = PluginDeploymentCommand::new(
            artifact.plugin_id().to_owned(),
            artifact.digest().to_owned(),
            deployment_expected_generation,
            !install_unconfigured,
            grants,
            config,
        )?;
        self.deploy(principal, command, request_id).await
    }

    /// Reconcile durable enabled deployments before the host accepts traffic.
    pub async fn reconcile_runtime(&self) {
        if let Some(runtime) = &self.sidecar_runtime {
            runtime.reconcile(&self.repository).await;
            return;
        }
        let Ok(query) = PluginDeploymentQuery::new(None, MAX_PLUGIN_DEPLOYMENT_HISTORY) else {
            return;
        };
        if self
            .repository
            .list_deployments(query)
            .await
            .is_ok_and(|deployments| deployments.iter().any(|deployment| deployment.enabled))
        {
            tracing::warn!(
                "plugin deployments are configured as enabled while sidecar runtime is disabled"
            );
        }
    }

    /// Stop managed child processes after the HTTP server has drained requests.
    pub async fn shutdown_runtime(&self) {
        if let Some(runtime) = &self.sidecar_runtime {
            runtime.shutdown().await;
            if let Ok(now) = current_timestamp() {
                let _ = self
                    .repository
                    .distribution_repository()
                    .retire_node(runtime.node_id(), now)
                    .await;
            }
        }
    }

    async fn invoke_event_delivery(
        &self,
        lease: &PluginEventDeliveryLease,
    ) -> Result<Value, PluginEventFailureKind> {
        let event_capability_matches =
            af_plugin_contract::plugin_event_definition(lease.event_kind()).is_some_and(|event| {
                event.capability_id == lease.capability_id()
                    && event.capability_version == lease.capability_version()
            });
        if !event_capability_matches || lease.expires_at() <= plugin_event_now_unix_seconds() {
            return Err(PluginEventFailureKind::Permanent);
        }

        let deployment = self
            .repository
            .get_deployment(lease.plugin_id())
            .await
            .map_err(|_| PluginEventFailureKind::Transient)?;
        let Some(deployment) = deployment else {
            return Err(PluginEventFailureKind::Permanent);
        };
        if !event_deployment_matches(&deployment, lease) {
            return Err(PluginEventFailureKind::Permanent);
        }
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(PluginEventFailureKind::Transient)?;

        let now_millis = time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let call_budget_ms = u64::try_from(PLUGIN_EVENT_CALL_BUDGET.as_millis())
            .map_err(|_| PluginEventFailureKind::Transient)?;
        let deadline_unix_ms = u64::try_from(now_millis)
            .ok()
            .and_then(|now| now.checked_add(call_budget_ms))
            .ok_or(PluginEventFailureKind::Transient)?;
        let invocation = af_plugin_runtime::CapabilityExecutionContext::new(
            format!("plugin-event:{}", lease.id()),
            InvocationMetadata {
                request_id: uuid::Uuid::new_v4().to_string(),
                deadline_unix_ms,
            },
            deployment_public_configuration(&deployment),
            PLUGIN_EVENT_MAX_BYTES,
        )
        .map_err(|_| PluginEventFailureKind::Permanent)?;
        let started_at = Instant::now();
        let invoked = match self.storage_dispatcher.clone() {
            Some(storage_dispatcher) => {
                runtime
                    .invoke_capability_with_storage(
                        &self.repository,
                        lease.plugin_id(),
                        lease.deployment_generation(),
                        &invocation,
                        lease.capability_id(),
                        lease.capability_version(),
                        lease.payload().clone(),
                        storage_dispatcher,
                    )
                    .await
            }
            None => {
                runtime
                    .invoke_capability(
                        &self.repository,
                        lease.plugin_id(),
                        lease.deployment_generation(),
                        &invocation,
                        lease.capability_id(),
                        lease.capability_version(),
                        lease.payload().clone(),
                    )
                    .await
            }
        };
        let result = match invoked {
            Ok(result) => result,
            Err(
                AdminPluginLifecycleError::Forbidden
                | AdminPluginLifecycleError::InvalidInput
                | AdminPluginLifecycleError::CapabilityReviewRequired
                | AdminPluginLifecycleError::PayloadTooLarge
                | AdminPluginLifecycleError::NotFound
                | AdminPluginLifecycleError::Conflict,
            ) => {
                tracing::warn!(
                    target: "anyflows_plugin",
                    event_id = lease.id(),
                    event_kind = lease.event_kind(),
                    plugin_id = lease.plugin_id(),
                    capability_id = lease.capability_id(),
                    capability_version = lease.capability_version(),
                    deployment_generation = lease.deployment_generation(),
                    attempt = lease.attempt_count(),
                    outcome = "failed",
                    failure_kind = "permanent",
                    elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "plugin event capability invocation failed"
                );
                return Err(PluginEventFailureKind::Permanent);
            }
            Err(
                AdminPluginLifecycleError::RuntimeUnavailable | AdminPluginLifecycleError::Internal,
            ) => {
                tracing::warn!(
                    target: "anyflows_plugin",
                    event_id = lease.id(),
                    event_kind = lease.event_kind(),
                    plugin_id = lease.plugin_id(),
                    capability_id = lease.capability_id(),
                    capability_version = lease.capability_version(),
                    deployment_generation = lease.deployment_generation(),
                    attempt = lease.attempt_count(),
                    outcome = "failed",
                    failure_kind = "transient",
                    elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "plugin event capability invocation failed"
                );
                return Err(PluginEventFailureKind::Transient);
            }
        };
        tracing::info!(
            target: "anyflows_plugin",
            event_id = lease.id(),
            event_kind = lease.event_kind(),
            plugin_id = lease.plugin_id(),
            capability_id = lease.capability_id(),
            capability_version = lease.capability_version(),
            deployment_generation = lease.deployment_generation(),
            attempt = lease.attempt_count(),
            outcome = "success",
            elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            "plugin event capability invocation completed"
        );

        // A disable, upgrade, grant change, or revocation during execution makes this
        // delivery stale. Do not report it as delivered to the currently active deployment.
        let latest = self
            .repository
            .get_deployment(lease.plugin_id())
            .await
            .map_err(|_| PluginEventFailureKind::Transient)?;
        if latest
            .as_ref()
            .is_none_or(|deployment| !event_deployment_matches(deployment, lease))
        {
            return Err(PluginEventFailureKind::Permanent);
        }
        Ok(result.output)
    }

    async fn apply_runtime(
        &self,
        deployment: &PluginDeploymentRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        if deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess {
            return if deployment.enabled {
                Err(AdminPluginLifecycleError::RuntimeUnavailable)
            } else {
                Ok(())
            };
        }
        if deployment.enabled && self.sidecar_runtime.is_none() {
            return Err(AdminPluginLifecycleError::RuntimeUnavailable);
        }
        if let Some(runtime) = &self.sidecar_runtime {
            if deployment.enabled {
                let latest = self
                    .repository
                    .get_deployment(&deployment.plugin_id)
                    .await
                    .map_err(map_catalog_error)?
                    .ok_or(AdminPluginLifecycleError::NotFound)?;
                if !latest.enabled || latest.generation != deployment.generation {
                    return Err(AdminPluginLifecycleError::Conflict);
                }
            }
            runtime.apply(&self.repository, deployment).await?;
            if deployment.enabled {
                let latest = match self.repository.get_deployment(&deployment.plugin_id).await {
                    Ok(Some(latest)) => latest,
                    Ok(None) => {
                        let _ = runtime
                            .stop_generation(&deployment.plugin_id, deployment.generation)
                            .await;
                        return Err(AdminPluginLifecycleError::NotFound);
                    }
                    Err(error) => {
                        let _ = runtime
                            .stop_generation(&deployment.plugin_id, deployment.generation)
                            .await;
                        return Err(map_catalog_error(error));
                    }
                };
                if !latest.enabled || latest.generation != deployment.generation {
                    let _ = runtime
                        .stop_generation(&deployment.plugin_id, deployment.generation)
                        .await;
                    return Err(AdminPluginLifecycleError::Conflict);
                }
            }
        }
        Ok(())
    }

    async fn reconcile_revocation_runtime(
        &self,
        artifact: &PluginArtifactRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        if artifact.source_kind != PluginArtifactSourceKind::LocalProcess {
            return Ok(());
        }
        let Some(deployment) = self
            .repository
            .get_deployment(&artifact.plugin_id)
            .await
            .map_err(map_catalog_error)?
        else {
            return Ok(());
        };
        if let Err(error) = self.apply_runtime(&deployment).await {
            if !deployment.enabled
                && deployment.artifact.digest == artifact.digest
                && let Some(runtime) = &self.sidecar_runtime
            {
                let _ = runtime
                    .stop_through_generation(&deployment.plugin_id, deployment.generation)
                    .await;
            }
            return Err(error);
        }
        Ok(())
    }

    async fn disable_after_activation_failure(
        &self,
        deployment: &PluginDeploymentRecord,
        principal: SessionPrincipal,
        request_id: &str,
    ) {
        // A local node failure must not disable the shared desired generation
        // on every healthy node. Its failed observation remains visible and the
        // coordinator retries that node. Legacy manual installs retain compensation.
        if !self
            .repository
            .distribution_repository()
            .package_metadata(deployment.artifact.id)
            .await
            .is_ok_and(|package| package.is_none())
        {
            return;
        }
        if let Some(runtime) = &self.sidecar_runtime {
            let _ = runtime
                .stop_through_generation(&deployment.plugin_id, deployment.generation)
                .await;
        }
        let Some(updated_at) = current_timestamp().ok() else {
            return;
        };
        let compensation_request_id = format!("{request_id}-disabled");
        let compensation_result = self
            .repository
            .deploy(PluginDeploymentWrite {
                plugin_id: deployment.plugin_id.clone(),
                artifact_digest: deployment.artifact.digest.clone(),
                expected_generation: Some(deployment.generation),
                enabled: false,
                granted_capabilities: deployment.granted_capabilities.clone(),
                config: deployment.config.clone(),
                updated_by: principal.user_id(),
                updated_at,
                request_id: compensation_request_id,
            })
            .await;
        let audit = audit_entry(
            principal,
            &format!("{request_id}-disabled"),
            "/admin/plugins/{pluginId}/deployment",
            "disable_after_failed_activation",
            "plugin_deployment",
            Some(deployment.plugin_id.clone()),
            if compensation_result.is_ok() {
                PlatformAuditOutcome::Succeeded
            } else {
                PlatformAuditOutcome::Failed
            },
            None,
            Some(json!({
                "enabled": false,
                "generation": deployment.generation.saturating_add(1),
                "triggeringRequestId": request_id
            })),
        );
        let audit_failed = match audit {
            Ok(entry) => self.audit.record(entry).await.is_err(),
            Err(_) => true,
        };
        if audit_failed {
            tracing::warn!(
                plugin_id = deployment.plugin_id,
                generation = deployment.generation,
                "failed activation compensation could not be fully audited"
            );
        }
        if compensation_result.is_err() {
            tracing::warn!(
                plugin_id = deployment.plugin_id,
                generation = deployment.generation,
                "failed plugin deployment could not be compensated"
            );
        }
    }

    pub async fn list_artifacts(
        &self,
        principal: SessionPrincipal,
        before_id: Option<i64>,
        limit: u32,
    ) -> Result<AdminPluginArtifactPage, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let query = PluginArtifactQuery::new(before_id, limit).map_err(map_catalog_error)?;
        let records = self
            .repository
            .list_artifacts(query)
            .await
            .map_err(map_catalog_error)?;
        let next_before_id = (records.len() == limit as usize)
            .then(|| records.last().map(|item| item.id))
            .flatten();
        Ok(AdminPluginArtifactPage {
            artifacts: records
                .into_iter()
                .map(|record| self.artifact_view(record))
                .collect(),
            next_before_id,
        })
    }

    /// Return approved, non-revoked artifacts for public market discovery.
    /// Installation and capability approval remain administrator operations.
    pub async fn list_public_market(
        &self,
        before_id: Option<i64>,
        limit: u32,
    ) -> Result<Vec<AdminPluginMarketEntry>, AdminPluginLifecycleError> {
        let query =
            PluginArtifactQuery::new(before_id, limit.min(100)).map_err(map_catalog_error)?;
        let records = self
            .repository
            .list_artifacts(query)
            .await
            .map_err(map_catalog_error)?;
        Ok(records
            .into_iter()
            .filter(|record| record.approval.is_some() && record.revocation.is_none())
            .map(|record| {
                let manifest = record.manifest.clone();
                let display_name = manifest
                    .get("displayName")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&record.plugin_id)
                    .to_owned();
                let capabilities = manifest
                    .get("capabilities")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.get("id").and_then(Value::as_str))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                AdminPluginMarketEntry {
                    plugin_id: record.plugin_id,
                    display_name,
                    plugin_version: record.plugin_version,
                    digest: record.digest,
                    source_kind: record.source_kind.as_str().to_owned(),
                    capabilities,
                    icon: manifest
                        .get("icon")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    repository: manifest
                        .get("repository")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    published_at: record.created_at,
                    official: !record.signer_key_id.is_empty(),
                }
            })
            .collect())
    }

    pub async fn list_deployments(
        &self,
        principal: SessionPrincipal,
        before_plugin_id: Option<String>,
        limit: u32,
    ) -> Result<AdminPluginDeploymentPage, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let query =
            PluginDeploymentQuery::new(before_plugin_id, limit).map_err(map_catalog_error)?;
        let records = self
            .repository
            .list_deployments(query)
            .await
            .map_err(map_catalog_error)?;
        let next_before_plugin_id = (records.len() == limit as usize)
            .then(|| records.last().map(|item| item.plugin_id.clone()))
            .flatten();
        Ok(AdminPluginDeploymentPage {
            deployments: records
                .into_iter()
                .map(|record| self.deployment_view(record))
                .collect(),
            next_before_plugin_id,
        })
    }

    pub async fn deployment_history(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        limit: u32,
    ) -> Result<Vec<AdminPluginDeployment>, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let limit = limit.min(MAX_PLUGIN_DEPLOYMENT_HISTORY);
        self.repository
            .deployment_history(plugin_id, limit)
            .await
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| self.deployment_view(record))
                    .collect()
            })
            .map_err(map_catalog_error)
    }

    pub async fn get_deployment(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
    ) -> Result<Option<AdminPluginDeployment>, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        self.repository
            .get_deployment(plugin_id)
            .await
            .map(|record| record.map(|record| self.deployment_view(record)))
            .map_err(map_catalog_error)
    }

    pub async fn get_public_plugin_configuration(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
    ) -> Result<Option<AdminPluginPublicConfiguration>, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        self.repository
            .get_deployment(plugin_id)
            .await
            .map(|record| {
                record.map(|record| AdminPluginPublicConfiguration {
                    plugin_id: record.plugin_id,
                    generation: record.generation,
                    configuration: plugin_public_configuration(
                        &record.config,
                        &record.artifact.manifest["configSchema"],
                    ),
                })
            })
            .map_err(map_catalog_error)
    }

    /// Persist a host-verified artifact. Raw executable paths and trust keys are
    /// not accepted by this API; verification happens before this service call.
    pub async fn register_verified_artifact(
        &self,
        principal: SessionPrincipal,
        artifact: VerifiedPluginArtifact,
        request_id: &str,
    ) -> Result<AdminPluginArtifact, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            None,
        )
        .await?;
        let plugin_id = artifact.plugin_id().as_str().to_owned();
        let plugin_version = artifact.plugin_version().as_str().to_owned();
        let digest = artifact.sha256().to_owned();
        let result = self
            .persist_verified_artifact(artifact, principal.user_id(), request_id, None)
            .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            Some(plugin_id),
            None,
            Some(json!({ "pluginVersion": plugin_version, "digest": digest })),
            result,
        )
        .await
    }

    /// Verify a candidate using host-owned filesystem/key configuration, then
    /// persist it in the durable catalog under admin session and audit controls.
    pub async fn register_artifact_candidate(
        &self,
        principal: SessionPrincipal,
        command: PluginArtifactRegistrationCommand,
        request_id: &str,
    ) -> Result<AdminPluginArtifact, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            None,
        )
        .await?;
        let plugin_id = command.plugin_id.clone();
        let plugin_version = command.plugin_version.clone();
        let digest = command.sha256.clone();
        let result = async {
            let source_reference_override = command.source_reference.clone();
            let runtime = self
                .sidecar_runtime
                .as_ref()
                .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
            let artifact = runtime
                .verify_candidate(
                    command.plugin_id,
                    command.plugin_version,
                    command.manifest,
                    command.package_path,
                    command.executable_path,
                    command.sha256,
                    command.signer_key_id,
                    command.signature_base64,
                )
                .await?;
            self.persist_verified_artifact(
                artifact,
                principal.user_id(),
                request_id,
                source_reference_override,
            )
            .await
        }
        .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            Some(plugin_id),
            None,
            Some(json!({ "pluginVersion": plugin_version, "digest": digest })),
            result,
        )
        .await
    }

    async fn persist_verified_artifact(
        &self,
        artifact: VerifiedPluginArtifact,
        operator: af_domain::UserId,
        request_id: &str,
        source_reference_override: Option<String>,
    ) -> Result<AdminPluginArtifact, AdminPluginLifecycleError> {
        let source_kind = match artifact.source_kind() {
            "builtin" => PluginArtifactSourceKind::Builtin,
            "local_process" => PluginArtifactSourceKind::LocalProcess,
            _ => return Err(AdminPluginLifecycleError::InvalidInput),
        };
        let signature = BASE64
            .decode(artifact.signature_base64().as_bytes())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let input = PluginArtifactCreate {
            plugin_id: artifact.plugin_id().as_str().to_owned(),
            plugin_version: artifact.plugin_version().as_str().to_owned(),
            digest: artifact.sha256().to_owned(),
            manifest: serde_json::to_value(artifact.manifest())
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
            source_kind,
            source_reference: source_reference_override
                .unwrap_or_else(|| artifact.source_reference().to_owned()),
            signer_key_id: artifact.signer_key_id().to_owned(),
            signature,
            created_at: current_timestamp()?,
        };
        self.repository
            .register_artifact_audited(input, operator, request_id)
            .await
            .map(|record| self.artifact_view(record))
            .map_err(map_catalog_error)
    }

    pub async fn approve_artifact(
        &self,
        principal: SessionPrincipal,
        artifact_id: i64,
        request_id: &str,
    ) -> Result<(), AdminPluginLifecycleError> {
        let resource_id = artifact_id.to_string();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/approve",
            "approve",
            "plugin_artifact",
            Some(&resource_id),
        )
        .await?;
        let result = match current_timestamp() {
            Ok(approved_at) => self
                .repository
                .approve_artifact(PluginArtifactApproval {
                    artifact_id,
                    approved_by: principal.user_id(),
                    approved_at,
                    request_id: request_id.to_owned(),
                })
                .await
                .map_err(map_catalog_error),
            Err(error) => Err(error),
        };
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/approve",
            "approve",
            "plugin_artifact",
            Some(resource_id),
            None,
            Some(json!({ "artifactId": artifact_id })),
            result,
        )
        .await
    }

    pub async fn revoke_artifact(
        &self,
        principal: SessionPrincipal,
        artifact_id: i64,
        reason: String,
        request_id: &str,
    ) -> Result<(), AdminPluginLifecycleError> {
        let resource_id = artifact_id.to_string();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/revoke",
            "revoke",
            "plugin_artifact",
            Some(&resource_id),
        )
        .await?;
        let artifact = self
            .repository
            .get_artifact(artifact_id)
            .await
            .map_err(map_catalog_error);
        let artifact = match artifact {
            Ok(Some(artifact)) => Some(artifact),
            Ok(None) => None,
            Err(error) => {
                return self
                    .finish_mutation(
                        principal,
                        request_id,
                        "/admin/plugins/artifacts/{id}/revoke",
                        "revoke",
                        "plugin_artifact",
                        Some(resource_id),
                        None,
                        Some(json!({ "artifactId": artifact_id })),
                        Err(error),
                    )
                    .await;
            }
        };
        let Some(artifact) = artifact else {
            return self
                .finish_mutation(
                    principal,
                    request_id,
                    "/admin/plugins/artifacts/{id}/revoke",
                    "revoke",
                    "plugin_artifact",
                    Some(resource_id),
                    None,
                    Some(json!({ "artifactId": artifact_id })),
                    Err(AdminPluginLifecycleError::NotFound),
                )
                .await;
        };
        let result = match current_timestamp() {
            Ok(revoked_at) => self
                .repository
                .revoke_artifact(PluginArtifactRevocation {
                    artifact_id,
                    revoked_by: principal.user_id(),
                    revoked_at,
                    request_id: request_id.to_owned(),
                    reason,
                })
                .await
                .map_err(map_catalog_error),
            Err(error) => Err(error),
        };
        if result.is_ok()
            && let Err(error) = self.reconcile_revocation_runtime(&artifact).await
        {
            tracing::warn!(
                plugin_id = artifact.plugin_id,
                error_kind = ?error,
                "plugin runtime stopped after revocation reconciliation failed"
            );
        }
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/revoke",
            "revoke",
            "plugin_artifact",
            Some(resource_id),
            None,
            Some(json!({ "artifactId": artifact_id })),
            result,
        )
        .await
    }

    pub async fn configure_provider_selection(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        selection: PluginProviderSelection,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        selection.validate()?;
        let current = self
            .repository
            .get_deployment(plugin_id)
            .await
            .map_err(map_catalog_error)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if current.generation != expected_generation {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        if !current.enabled
            || !current
                .granted_capabilities
                .iter()
                .any(|grant| grant.id == "provider.lifecycle")
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        if PluginProviderSelection::from_config(&current.config).as_ref() == Some(&selection) {
            return Ok(self.deployment_view(current));
        }
        let mut config = current.config;
        let fields = config
            .as_object_mut()
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let existing = fields
            .entry("providerSelection")
            .or_insert_with(|| json!({}));
        let target = existing
            .as_object_mut()
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let values =
            serde_json::to_value(selection).map_err(|_| AdminPluginLifecycleError::Internal)?;
        target.extend(
            values
                .as_object()
                .ok_or(AdminPluginLifecycleError::Internal)?
                .clone(),
        );
        let command = PluginDeploymentCommand::new(
            plugin_id.to_owned(),
            current.artifact.digest,
            Some(expected_generation),
            true,
            current
                .granted_capabilities
                .into_iter()
                .map(AdminPluginCapabilityGrant::from)
                .collect(),
            config,
        )?;
        self.deploy(principal, command, request_id).await
    }

    pub async fn configure_public_plugin_configuration(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        public_configuration: Value,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        if !public_configuration.is_object()
            || plugin_configuration_contains_sensitive_fields(&public_configuration)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let current = self
            .repository
            .get_deployment(plugin_id)
            .await
            .map_err(map_catalog_error)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if current.generation != expected_generation {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let manifest: PluginManifest = serde_json::from_value(current.artifact.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        manifest
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        validate_public_plugin_configuration(&manifest.config_schema, &public_configuration)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;

        let mut config = public_configuration;
        let fields = config
            .as_object_mut()
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let setup_pending = current
            .config
            .get(PLUGIN_SETUP_PENDING_FIELD)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        fields.remove(PLUGIN_SETUP_PENDING_FIELD);
        for key in ["providerSelection", "taskPricing"] {
            if manifest.config_schema["properties"].get(key).is_none()
                && let Some(value) = current.config.get(key)
            {
                fields.insert(key.to_owned(), value.clone());
            }
        }
        let command = PluginDeploymentCommand::new(
            plugin_id.to_owned(),
            current.artifact.digest,
            Some(expected_generation),
            current.enabled || setup_pending,
            current
                .granted_capabilities
                .into_iter()
                .map(AdminPluginCapabilityGrant::from)
                .collect(),
            config,
        )?;
        self.deploy(principal, command, request_id).await
    }

    pub async fn deploy(
        &self,
        principal: SessionPrincipal,
        command: PluginDeploymentCommand,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let plugin_id = command.plugin_id.clone();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/deployment",
            "deploy",
            "plugin_deployment",
            Some(&plugin_id),
        )
        .await?;
        let config_validation = async {
            if !command.enabled {
                return Ok(());
            }
            if plugin_configuration_contains_sensitive_fields(&command.config) {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
            let artifact = self
                .repository
                .get_artifact_by_digest(&command.plugin_id, &command.artifact_digest)
                .await
                .map_err(map_catalog_error)?
                .ok_or(AdminPluginLifecycleError::NotFound)?;
            let manifest: PluginManifest = serde_json::from_value(artifact.manifest)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            manifest
                .validate()
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            validate_public_plugin_configuration(
                &manifest.config_schema,
                &plugin_public_configuration(&command.config, &manifest.config_schema),
            )
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)
        }
        .await;
        let result = if command.enabled && self.sidecar_runtime.is_none() {
            Err(AdminPluginLifecycleError::RuntimeUnavailable)
        } else if let Err(error) = config_validation {
            Err(error)
        } else {
            match current_timestamp() {
                Ok(updated_at) => match self
                    .repository
                    .deploy(PluginDeploymentWrite {
                        plugin_id: command.plugin_id.clone(),
                        artifact_digest: command.artifact_digest.clone(),
                        expected_generation: command.expected_generation,
                        enabled: command.enabled,
                        granted_capabilities: command
                            .granted_capabilities
                            .iter()
                            .map(|grant| PluginCapabilityGrant {
                                id: grant.id.clone(),
                                version: grant.version,
                            })
                            .collect(),
                        config: command.config.clone(),
                        updated_by: principal.user_id(),
                        updated_at,
                        request_id: request_id.to_owned(),
                    })
                    .await
                    .map_err(map_catalog_error)
                {
                    Ok(record) => match self.apply_runtime(&record).await {
                        Ok(()) => Ok(self.deployment_view(record)),
                        Err(error) => {
                            if record.enabled {
                                self.disable_after_activation_failure(
                                    &record, principal, request_id,
                                )
                                .await;
                            }
                            Err(error)
                        }
                    },
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            }
        };
        self.finish_mutation(principal, request_id, "/admin/plugins/{pluginId}/deployment", "deploy", "plugin_deployment", Some(plugin_id), None,
            Some(json!({ "artifactDigest": command.artifact_digest, "enabled": command.enabled, "generation": command.expected_generation })), result).await
    }

    pub async fn disable(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let resource_id = plugin_id.to_owned();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/disable",
            "disable",
            "plugin_deployment",
            Some(&resource_id),
        )
        .await?;
        let result = async {
            let current = self
                .repository
                .get_deployment(plugin_id)
                .await
                .map_err(map_catalog_error)?
                .ok_or(AdminPluginLifecycleError::NotFound)?;
            if current.generation != expected_generation {
                return Err(AdminPluginLifecycleError::Conflict);
            }
            if !current.enabled {
                self.apply_runtime(&current).await?;
                return Ok(self.deployment_view(current));
            }
            let record = self
                .repository
                .deploy(PluginDeploymentWrite {
                    plugin_id: current.plugin_id.clone(),
                    artifact_digest: current.artifact.digest.clone(),
                    expected_generation: Some(expected_generation),
                    enabled: false,
                    granted_capabilities: current.granted_capabilities.clone(),
                    config: current.config.clone(),
                    updated_by: principal.user_id(),
                    updated_at: current_timestamp()?,
                    request_id: request_id.to_owned(),
                })
                .await
                .map_err(map_catalog_error)?;
            self.apply_runtime(&record).await?;
            Ok(self.deployment_view(record))
        }
        .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/disable",
            "disable",
            "plugin_deployment",
            Some(resource_id),
            None,
            Some(json!({ "enabled": false, "expectedGeneration": expected_generation })),
            result,
        )
        .await
    }

    pub async fn rollback(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        target_generation: i64,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let resource_id = plugin_id.to_owned();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/rollback",
            "rollback",
            "plugin_deployment",
            Some(&resource_id),
        )
        .await?;
        let result = match current_timestamp() {
            Ok(updated_at) => match self
                .repository
                .rollback(
                    plugin_id,
                    expected_generation,
                    target_generation,
                    principal.user_id(),
                    updated_at,
                    request_id.to_owned(),
                )
                .await
                .map_err(map_catalog_error)
            {
                Ok(record) => match self.apply_runtime(&record).await {
                    Ok(()) => Ok(self.deployment_view(record)),
                    Err(error) => {
                        if record.enabled {
                            self.disable_after_activation_failure(&record, principal, request_id)
                                .await;
                        }
                        Err(error)
                    }
                },
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        self.finish_mutation(principal, request_id, "/admin/plugins/{pluginId}/rollback", "rollback", "plugin_deployment", Some(resource_id), None,
            Some(json!({ "expectedGeneration": expected_generation, "targetGeneration": target_generation })), result).await
    }

    pub(crate) async fn authorize(
        &self,
        principal: SessionPrincipal,
    ) -> Result<(), AdminPluginLifecycleError> {
        if principal.role() == SessionRole::Admin {
            PlatformPolicy::authorize(principal, PLUGIN_PERMISSION).map_err(map_audit_error)
        } else {
            Err(AdminPluginLifecycleError::Forbidden)
        }
    }

    pub(crate) async fn authorize_mutation(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: Option<&str>,
    ) -> Result<(), AdminPluginLifecycleError> {
        if self.authorize(principal).await.is_ok() {
            // Reject an unusable correlation id before any persistence call, so
            // a repository mutation cannot commit without an audit record.
            let _ = audit_entry(
                principal,
                request_id,
                route,
                operation,
                resource,
                resource_id.map(str::to_owned),
                PlatformAuditOutcome::Succeeded,
                None,
                None,
            )?;
            return Ok(());
        }
        let entry = audit_entry(
            principal,
            request_id,
            route,
            operation,
            resource,
            resource_id.map(str::to_owned),
            PlatformAuditOutcome::Denied,
            None,
            None,
        )?;
        self.audit.record(entry).await.map_err(map_audit_error)?;
        Err(AdminPluginLifecycleError::Forbidden)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finish_mutation<T>(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: Option<String>,
        before: Option<Value>,
        after: Option<Value>,
        result: Result<T, AdminPluginLifecycleError>,
    ) -> Result<T, AdminPluginLifecycleError> {
        let succeeded = result.is_ok();
        let outcome = if succeeded {
            PlatformAuditOutcome::Succeeded
        } else {
            PlatformAuditOutcome::Failed
        };
        // Catalog facts commit atomically with mutations. A failure in this
        // additional outcome log must not mask success or a runtime failure.
        let audit = audit_entry(
            principal,
            request_id,
            route,
            operation,
            resource,
            resource_id,
            outcome,
            before,
            if succeeded { after } else { None },
        );
        let audit_result = match audit {
            Ok(entry) => self.audit.record(entry).await.map_err(map_audit_error),
            Err(error) => Err(error),
        };
        if let Err(error) = audit_result {
            tracing::warn!(
                request_id,
                route,
                operation,
                resource,
                succeeded,
                error_kind = ?error,
                "plugin lifecycle outcome audit failed; preserving the operation result"
            );
        }
        result
    }
}

/// A single-process poller for host-projected plugin events. Leases and retries
/// live in the shared database, so multiple AnyFlows nodes may run this worker.
#[derive(Clone)]
pub struct PluginEventDeliveryWorker {
    repository: PluginEventDeliveryRepository,
    lifecycle: Arc<DatabaseAdminPluginLifecycleService>,
}

impl PluginEventDeliveryWorker {
    #[must_use]
    pub fn new(
        repository: PluginEventDeliveryRepository,
        lifecycle: Arc<DatabaseAdminPluginLifecycleService>,
    ) -> Self {
        Self {
            repository,
            lifecycle,
        }
    }

    /// Poll and deliver bounded batches until the supervisor requests shutdown.
    /// An in-flight Sidecar call is allowed to finish within its five-second budget;
    /// shutdown never interrupts it after the event has been claimed.
    pub async fn run_until<F>(&self, shutdown: F)
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => return,
                () = tokio::time::sleep(PLUGIN_EVENT_POLL_INTERVAL) => {}
            }

            for _ in 0..PLUGIN_EVENT_BATCH_SIZE {
                let now = plugin_event_now_unix_seconds();
                let claim = tokio::select! {
                    biased;
                    () = &mut shutdown => return,
                    result = self.repository.claim_next(now) => result,
                };
                let lease = match claim {
                    Ok(PluginEventDeliveryClaimOutcome::Claimed(lease)) => lease,
                    Ok(PluginEventDeliveryClaimOutcome::Empty) => break,
                    Err(error) => {
                        tracing::warn!(error_kind = ?error, "plugin event queue claim failed");
                        break;
                    }
                };

                let outcome = self.lifecycle.invoke_event_delivery(&lease).await;
                let completion = match outcome {
                    Ok(result) => match self
                        .repository
                        .mark_delivered(&lease, plugin_event_now_unix_seconds(), &result)
                        .await
                    {
                        Err(af_db::PluginEventDeliveryRepositoryError::InvalidResult) => {
                            self.repository
                                .record_failure(
                                    &lease,
                                    plugin_event_now_unix_seconds(),
                                    PluginEventFailureKind::Permanent,
                                )
                                .await
                        }
                        other => other,
                    },
                    Err(kind) => {
                        self.repository
                            .record_failure(&lease, plugin_event_now_unix_seconds(), kind)
                            .await
                    }
                };
                match completion {
                    Ok(PluginEventDeliveryCompletionOutcome::Completed) => {}
                    Ok(PluginEventDeliveryCompletionOutcome::Stale) => {
                        tracing::debug!(
                            delivery_id = lease.id(),
                            "plugin event lease was superseded"
                        );
                    }
                    Ok(PluginEventDeliveryCompletionOutcome::Expired) => {
                        tracing::debug!(delivery_id = lease.id(), "plugin event delivery expired");
                    }
                    Err(error) => {
                        tracing::warn!(
                            delivery_id = lease.id(),
                            error_kind = ?error,
                            "plugin event queue completion failed"
                        );
                    }
                }
            }
        }
    }
}

fn event_deployment_matches(
    deployment: &PluginDeploymentRecord,
    lease: &PluginEventDeliveryLease,
) -> bool {
    deployment.enabled
        && deployment.plugin_id == lease.plugin_id()
        && deployment.generation == lease.deployment_generation()
        && deployment.artifact.id == lease.artifact_id()
        && deployment.artifact.digest == lease.artifact_digest()
        && deployment.artifact.approval.is_some()
        && deployment.artifact.revocation.is_none()
        && deployment.artifact.source_kind == PluginArtifactSourceKind::LocalProcess
        && deployment.granted_capabilities.iter().any(|grant| {
            grant.id == lease.capability_id() && grant.version == lease.capability_version()
        })
}

fn custom_capability_is_callable(
    deployment: &PluginDeploymentRecord,
    capability_id: &str,
    capability_version: u16,
) -> bool {
    if !deployment.enabled
        || deployment.artifact.approval.is_none()
        || deployment.artifact.revocation.is_some()
        || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
        || !deployment
            .granted_capabilities
            .iter()
            .any(|grant| grant.id == capability_id && grant.version == capability_version)
    {
        return false;
    }
    serde_json::from_value::<PluginManifest>(deployment.artifact.manifest.clone())
        .ok()
        .and_then(|manifest| {
            manifest.capabilities.into_iter().find(|capability| {
                capability.id.as_str() == capability_id && capability.version == capability_version
            })
        })
        .is_some_and(|capability| simple_install_capability(&capability))
}

fn plugin_event_now_unix_seconds() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp().max(1)
}

fn safe_event_result_summary(value: &str) -> bool {
    if value.trim().is_empty()
        || value.len() > MAX_PLUGIN_EVENT_RESULT_SUMMARY_BYTES
        || value.chars().any(char::is_control)
    {
        return false;
    }
    let normalized = value.to_ascii_lowercase();
    ![
        "http://",
        "https://",
        "javascript:",
        "data:",
        "mailto:",
        "://",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

#[cfg(test)]
mod event_result_tests {
    use super::{MAX_PLUGIN_EVENT_RESULT_SUMMARY_BYTES, safe_event_result_summary};

    #[test]
    fn result_summaries_are_bounded_plain_text_without_urls_or_controls() {
        assert!(safe_event_result_summary("Channel recovered"));
        assert!(!safe_event_result_summary(" "));
        assert!(!safe_event_result_summary("http://example.invalid"));
        assert!(!safe_event_result_summary("See https://example.invalid"));
        assert!(!safe_event_result_summary("javascript:alert(1)"));
        assert!(!safe_event_result_summary("line\nbreak"));
        assert!(!safe_event_result_summary(
            &"x".repeat(MAX_PLUGIN_EVENT_RESULT_SUMMARY_BYTES + 1)
        ));
    }
}

#[cfg(test)]
mod market_source_tests {
    use super::market_redirect_host_allowed;

    #[test]
    fn github_release_redirects_are_limited_to_github_asset_hosts() {
        assert!(market_redirect_host_allowed(
            "github.com",
            "release-assets.githubusercontent.com"
        ));
        assert!(market_redirect_host_allowed("example.com", "example.com"));
        assert!(!market_redirect_host_allowed(
            "github.com",
            "cdn.example.com"
        ));
        assert!(!market_redirect_host_allowed("example.com", "10.0.0.1"));
    }
}

fn cleanup_stale_plugin_uploads(directory: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten().take(10_000) {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".plugin-upload-")
        {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        if metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_PLUGIN_UPLOAD_AGE)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl fmt::Debug for DatabaseAdminPluginLifecycleService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminPluginLifecycleService(<redacted>)")
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_entry(
    principal: SessionPrincipal,
    request_id: &str,
    route: &'static str,
    operation: &'static str,
    resource: &'static str,
    resource_id: Option<String>,
    outcome: PlatformAuditOutcome,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<PlatformAuditEntry, AdminPluginLifecycleError> {
    PlatformAuditEntry::new(
        principal,
        PLUGIN_PERMISSION,
        route,
        operation,
        resource,
        resource_id,
        outcome,
        before,
        after,
        None,
        request_id,
    )
    .map_err(map_audit_error)
}

pub(crate) fn current_timestamp() -> Result<i64, AdminPluginLifecycleError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .as_secs(),
    )
    .map_err(|_| AdminPluginLifecycleError::Internal)
}

pub(crate) fn map_catalog_error(error: PluginCatalogError) -> AdminPluginLifecycleError {
    match error {
        PluginCatalogError::InvalidInput => AdminPluginLifecycleError::InvalidInput,
        PluginCatalogError::NotFound => AdminPluginLifecycleError::NotFound,
        PluginCatalogError::Conflict => AdminPluginLifecycleError::Conflict,
        PluginCatalogError::Unavailable
        | PluginCatalogError::Timeout
        | PluginCatalogError::Invariant => AdminPluginLifecycleError::Internal,
    }
}

fn map_audit_error(error: PlatformAuditError) -> AdminPluginLifecycleError {
    match error {
        PlatformAuditError::InvalidInput => AdminPluginLifecycleError::InvalidInput,
        PlatformAuditError::Forbidden => AdminPluginLifecycleError::Forbidden,
        PlatformAuditError::Internal => AdminPluginLifecycleError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_plugin_configuration_hides_host_fields_and_sensitive_values() {
        assert_eq!(
            plugin_public_configuration(
                &json!({
                    "displayName": "Catalog",
                    "providerSelection": {"channelIds": [12]},
                    "taskPricing": {"units": 3}
                }),
                &json!({"properties": {"displayName": {"type": "string"}}}),
            ),
            json!({"displayName": "Catalog"})
        );
        assert_eq!(
            plugin_public_configuration(
                &json!({"displayName": "Catalog", "apiKey": "should-not-be-returned"}),
                &json!({"properties": {"displayName": {"type": "string"}}}),
            ),
            json!({})
        );
    }

    #[test]
    fn provider_selection_defaults_legacy_deployments_to_automatic_response_mode() {
        let selection = PluginProviderSelection::from_config(&json!({
            "providerSelection": {
                "channelIds": [12],
                "models": ["vendor-model"],
                "priority": 7
            }
        }))
        .unwrap();
        assert_eq!(
            selection.upstream_response_mode,
            PluginProviderUpstreamResponseMode::Auto
        );

        let configured = PluginProviderSelection::from_config(&json!({
            "providerSelection": {
                "channelIds": [12],
                "upstreamResponseMode": "non_streaming"
            }
        }))
        .unwrap();
        assert_eq!(
            configured.upstream_response_mode,
            PluginProviderUpstreamResponseMode::NonStreaming
        );
        let session_enabled = PluginProviderSelection::from_config(&json!({
            "providerSelection": { "channelIds": [12], "sessionEnabled": true }
        }))
        .unwrap();
        assert!(session_enabled.session_enabled);
        assert_eq!(session_enabled.session_timeout_ms, 30_000);

        let legacy_websocket = PluginProviderSelection::from_config(&json!({
            "providerSelection": {
                "channelIds": [12],
                "sessionEnabled": true,
                "webSocketEndpoints": [
                    { "id": "legacy-realtime", "url": "wss://realtime.example.invalid" }
                ]
            }
        }))
        .unwrap();
        assert_eq!(legacy_websocket.endpoints[0].allowed_paths, vec!["/"]);
        assert!(legacy_websocket.endpoints[0].websocket_enabled);
        assert!(legacy_websocket.validate().is_ok());
    }

    #[test]
    fn provider_endpoint_configuration_carries_scoped_https_and_websocket_grants() {
        let selection = PluginProviderSelection::from_config(&json!({
            "providerSelection": {
                "channelIds": [12],
                "sessionEnabled": true,
                "sessionTimeoutMs": 60000,
                "endpoints": [
                    {
                        "id": "capy-api",
                        "url": "https://api.example.invalid",
                        "allowedPaths": ["/v1", "/oauth"],
                        "allowedMethods": ["GET", "POST"],
                        "websocketEnabled": false
                    },
                    {
                        "id": "capy-realtime",
                        "url": "wss://realtime.example.invalid",
                        "allowedPaths": ["/realtime"],
                        "allowedMethods": [],
                        "websocketEnabled": true
                    }
                ]
            }
        }))
        .unwrap();
        assert_eq!(selection.session_timeout_ms, 60_000);
        assert_eq!(selection.endpoints.len(), 2);
        assert!(selection.validate().is_ok());
        assert!(
            serde_json::to_value(&selection)
                .unwrap()
                .get("endpoints")
                .is_some()
        );

        for url in [
            "ws://realtime.example.invalid",
            "wss://127.0.0.1",
            "https://user:secret@realtime.example.invalid",
            "https://realtime.example.invalid/path",
        ] {
            let mut invalid = selection.clone();
            invalid.endpoints[0].url = url.to_owned();
            assert!(invalid.validate().is_err());
        }

        let mut path_escape = selection.clone();
        path_escape.endpoints[0].allowed_paths = vec!["/v1/../admin".to_owned()];
        assert!(path_escape.validate().is_err());
    }

    fn capability(
        id: &str,
        access: CapabilityAccess,
        side_effect: SideEffectClass,
        schemas: bool,
    ) -> CapabilityDeclaration {
        CapabilityDeclaration {
            id: af_plugin_contract::CapabilityId::new(id).unwrap(),
            version: 1,
            access,
            side_effect,
            write_semantics: (side_effect == SideEffectClass::ExternalWrite).then_some(
                af_plugin_contract::ExternalWriteSemantics {
                    native_idempotency_key: true,
                    result_lookup: af_plugin_contract::ResultLookupSupport::ByRequestId,
                    reconciliation: true,
                },
            ),
            input_schema: schemas.then(|| json!({"type":"object"})),
            output_schema: schemas.then(|| json!({"type":"object"})),
        }
    }

    #[test]
    fn simple_installer_grants_only_schema_bound_read_only_capabilities() {
        assert!(simple_install_capability(&capability(
            "vendor.custom.lookup",
            CapabilityAccess::Read,
            SideEffectClass::None,
            true,
        )));
        assert!(!simple_install_capability(&capability(
            "vendor.external.profile",
            CapabilityAccess::Read,
            SideEffectClass::ExternalRead,
            true,
        )));
        assert!(!simple_install_capability(&capability(
            "vendor.custom.action",
            CapabilityAccess::Write,
            SideEffectClass::ExternalWrite,
            true,
        )));
        assert!(!simple_install_capability(&capability(
            "vendor.custom.untyped",
            CapabilityAccess::Read,
            SideEffectClass::None,
            false,
        )));

        let mut invalid_schema = capability(
            "vendor.custom.invalid-schema",
            CapabilityAccess::Read,
            SideEffectClass::None,
            true,
        );
        invalid_schema.input_schema = Some(json!({"type":"unsupported-json-schema-type"}));
        assert!(!simple_install_capability(&invalid_schema));
    }

    #[test]
    fn artifact_projection_never_serializes_storage_reference_or_signature() {
        let artifact = AdminPluginArtifact {
            official: false,
            id: 7,
            plugin_id: "org.example.catalog".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            digest: "a".repeat(64),
            manifest: json!({ "pluginId": "org.example.catalog" }),
            source_kind: "local_process",
            signer_key_id: "publisher-1".to_owned(),
            created_at: 1_800_000_000,
            approval: None,
            revocation: None,
        };

        let serialized = serde_json::to_value(&artifact).unwrap();
        assert!(serialized.get("sourceReference").is_none());
        assert!(serialized.get("signature").is_none());
        assert!(serialized.get("signatureBase64").is_none());
        let debug = format!("{artifact:?}");
        assert!(!debug.contains("sourceReference"));
        assert!(!debug.contains("signature"));
    }

    #[test]
    fn deployment_command_debug_redacts_configuration() {
        let command = PluginDeploymentCommand::new(
            "org.example.catalog".to_owned(),
            "a".repeat(64),
            None,
            true,
            vec![AdminPluginCapabilityGrant::new("catalog.models.read".to_owned(), 1).unwrap()],
            json!({ "credential": "sensitive-value" }),
        )
        .unwrap();

        let debug = format!("{command:?}");
        assert!(!debug.contains("sensitive-value"));
        assert!(debug.contains("<redacted>"));
    }
}
