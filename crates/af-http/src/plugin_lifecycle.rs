//! Admin HTTP API for plugin installation and deployment lifecycle.

use std::sync::Arc;

use af_admin::{
    AdminPluginArtifact, AdminPluginArtifactApproval, AdminPluginArtifactPage,
    AdminPluginArtifactRevocation, AdminPluginCapabilityActionProposal,
    AdminPluginCapabilityActionResult, AdminPluginCapabilityGrant,
    AdminPluginCapabilityInvocationResult, AdminPluginDeployment, AdminPluginDeploymentPage,
    AdminPluginEventResult, AdminPluginEventResultPage, AdminPluginLifecycleError,
    AdminPluginRouteInvocationResult, DEFAULT_PLUGIN_CATALOG_PAGE_SIZE,
    DatabaseAdminPluginLifecycleService, MAX_PLUGIN_PACKAGE_UPLOAD_BYTES,
    PluginArtifactRegistrationCommand, PluginCatalogExtensionQuery, PluginDeploymentCommand,
    PluginMarketInstallRequest, PluginMarketInstallSource, PluginPackageUpload,
    PluginProviderSelection, PluginProviderUpstreamResponseMode, PluginProviderWebSocketEndpoint,
    SessionAuthentication, SessionAuthenticator,
};
use af_admin::{
    PluginCredentialOperation, PluginCredentialResult, PluginTaskOperation, PluginTaskResult,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::Request,
    extract::{
        DefaultBodyLimit, Multipart, Path as PathParam, Query, RawQuery, State,
        multipart::MultipartRejection, rejection::JsonRejection, rejection::QueryRejection,
    },
    middleware,
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    QueryApiKeyPolicy,
    authentication::{AuthenticationState, authenticate_api_key},
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
};

#[derive(Clone)]
struct PluginLifecycleHttpState {
    service: Arc<DatabaseAdminPluginLifecycleService>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginArtifactRegistrationRequest)]
pub(crate) struct PluginArtifactRegistrationRequest {
    #[schema(
        min_length = 3,
        max_length = 128,
        pattern = "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+$"
    )]
    plugin_id: String,
    #[schema(min_length = 5, max_length = 128)]
    plugin_version: String,
    #[schema(schema_with = crate::openapi::schema::sensitive_free_form_object_schema)]
    manifest: Value,
    /// POSIX-style path relative to the host-configured artifact root.
    #[schema(
        min_length = 1,
        max_length = 900,
        example = "publishers/acme/catalog-1.0.0"
    )]
    package_path: String,
    /// POSIX-style executable path relative to `package_path`.
    #[schema(min_length = 1, max_length = 900, example = "bin/catalog-plugin")]
    executable_path: String,
    #[schema(min_length = 64, max_length = 64, pattern = "^[a-f0-9]{64}$")]
    sha256: String,
    #[serde(default)]
    #[schema(max_length = 128)]
    signer_key_id: String,
    #[serde(default)]
    #[schema(max_length = 88, write_only)]
    signature_base64: String,
}

#[derive(ToSchema)]
#[allow(
    dead_code,
    reason = "Multipart schema is consumed by the OpenAPI generator"
)]
#[schema(as = PluginPackageInstallRequest)]
pub(crate) struct PluginPackageInstallRequest {
    /// ZIP/.afplugin archive containing anyflows-plugin.json and payload/.
    #[schema(value_type = Option<String>, format = Binary)]
    package: Option<String>,
    /// Public GitHub repository URL. Supply exactly one of package and repository.
    #[schema(max_length = 256)]
    repository: Option<String>,
    /// Exact release filename when multiple candidate packages are published.
    #[schema(max_length = 255)]
    asset_name: Option<String>,
    /// Optional public JSON configuration. Existing deployments retain it when omitted; first installs use an empty object.
    config: Option<String>,
    /// Required for an explicit update of an existing deployment.
    #[schema(minimum = 1)]
    expected_generation: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginMarketInstallHttpRequest)]
pub(crate) struct PluginMarketInstallHttpRequest {
    #[schema(min_length = 3, max_length = 128)]
    plugin_id: String,
    #[schema(min_length = 5, max_length = 128)]
    plugin_version: String,
    source: PluginMarketSourceRequest,
    #[schema(schema_with = crate::openapi::schema::sensitive_free_form_object_schema, required = false)]
    config: Option<Value>,
    #[schema(minimum = 1)]
    expected_generation: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginMarketSourceRequest {
    #[schema(min_length = 1, max_length = 32)]
    kind: String,
    #[schema(min_length = 8, max_length = 4096)]
    archive_url: String,
    #[schema(min_length = 64, max_length = 64, pattern = "^[a-fA-F0-9]{64}$")]
    sha256: String,
    #[schema(max_length = 2048)]
    repository: Option<String>,
    #[schema(min_length = 40, max_length = 40)]
    commit: Option<String>,
    #[schema(max_length = 900)]
    path: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginArtifactRevokeRequest)]
pub(crate) struct PluginArtifactRevokeRequest {
    #[schema(min_length = 1, max_length = 512)]
    reason: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginDeploymentRequest)]
pub(crate) struct PluginDeploymentRequest {
    #[schema(min_length = 64, max_length = 64, pattern = "^[a-f0-9]{64}$")]
    artifact_digest: String,
    #[schema(minimum = 1)]
    expected_generation: Option<i64>,
    enabled: bool,
    #[schema(max_items = 128)]
    granted_capabilities: Vec<PluginCapabilityGrantRequest>,
    #[schema(schema_with = crate::openapi::schema::sensitive_free_form_object_schema)]
    config: Value,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginCapabilityGrantRequest {
    #[schema(min_length = 3, max_length = 128)]
    id: String,
    #[schema(minimum = 1, maximum = 65535)]
    version: u16,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginCapabilityInvocationRequest)]
pub(crate) struct PluginCapabilityInvocationRequest {
    #[schema(minimum = 1, maximum = 65535)]
    capability_version: u16,
    #[schema(schema_with = crate::openapi::schema::free_form_json_schema)]
    input: Value,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginCredentialInvocationRequest)]
pub(crate) struct PluginCredentialInvocationRequest {
    #[schema(min_length = 1, max_length = 128)]
    credential_id: String,
    channel_id: i64,
    callback_state: Option<String>,
    #[schema(minimum = 1, maximum = 65535)]
    capability_version: u16,
    #[schema(value_type = serde_json::Value)]
    operation: PluginCredentialOperation,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginTaskInvocationRequest)]
pub(crate) struct PluginTaskInvocationRequest {
    #[schema(min_length = 3, max_length = 128)]
    capability_id: String,
    #[schema(minimum = 1, maximum = 65535)]
    capability_version: u16,
    #[schema(value_type = serde_json::Value)]
    task: PluginTaskOperation,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminPluginTaskListQuery)]
pub(crate) struct AdminPluginTaskListQuery {
    cursor: Option<String>,
    limit: Option<u16>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminPluginTaskReconciliationRequest)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminPluginTaskReconciliationRequest {
    actual_units: u64,
    #[schema(min_length = 1, max_length = 256)]
    reason: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginCapabilityActionProposalRequest {
    #[schema(min_length = 3, max_length = 128)]
    capability_id: String,
    #[schema(minimum = 1, maximum = 65535)]
    capability_version: u16,
    #[schema(min_length = 1, max_length = 128)]
    idempotency_key: String,
    #[schema(schema_with = crate::openapi::schema::free_form_json_schema)]
    input: Value,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginCapabilityActionConfirmRequest {
    #[schema(min_length = 1, max_length = 128)]
    confirmation_key: String,
    #[schema(schema_with = crate::openapi::schema::free_form_json_schema)]
    input: Value,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginCapabilityActionProposalResponse)]
pub(crate) struct PluginCapabilityActionProposalResponse {
    proposal_key: String,
    expires_at: i64,
    status: String,
}

impl From<AdminPluginCapabilityActionProposal> for PluginCapabilityActionProposalResponse {
    fn from(value: AdminPluginCapabilityActionProposal) -> Self {
        Self {
            proposal_key: value.proposal_key,
            expires_at: value.expires_at,
            status: value.status,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginCapabilityActionResult)]
pub(crate) struct PluginCapabilityActionResultResponse {
    proposal_key: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome_code: Option<String>,
}

impl From<AdminPluginCapabilityActionResult> for PluginCapabilityActionResultResponse {
    fn from(value: AdminPluginCapabilityActionResult) -> Self {
        Self {
            proposal_key: value.proposal_key,
            status: value.status,
            outcome_code: value.outcome_code,
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginDeploymentDisableRequest)]
pub(crate) struct PluginDeploymentDisableRequest {
    #[schema(minimum = 1)]
    expected_generation: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PluginDeploymentRollbackRequest)]
pub(crate) struct PluginDeploymentRollbackRequest {
    #[schema(minimum = 1)]
    expected_generation: i64,
    #[schema(minimum = 1)]
    target_generation: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginArtifactResponse)]
pub(crate) struct PluginArtifactResponse {
    id: i64,
    plugin_id: String,
    plugin_version: String,
    digest: String,
    #[schema(schema_with = crate::openapi::schema::free_form_object_schema)]
    manifest: Value,
    source_kind: String,
    signer_key_id: String,
    official: bool,
    created_at: i64,
    approval: Option<PluginArtifactApprovalResponse>,
    revocation: Option<PluginArtifactRevocationResponse>,
}

impl From<AdminPluginArtifact> for PluginArtifactResponse {
    fn from(value: AdminPluginArtifact) -> Self {
        Self {
            id: value.id(),
            plugin_id: value.plugin_id().to_owned(),
            plugin_version: value.plugin_version().to_owned(),
            digest: value.digest().to_owned(),
            manifest: value.manifest().clone(),
            source_kind: value.source_kind().to_owned(),
            signer_key_id: value.signer_key_id().to_owned(),
            official: value.is_official(),
            created_at: value.created_at(),
            approval: value.approval().cloned().map(Into::into),
            revocation: value.revocation().cloned().map(Into::into),
        }
    }
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginArtifactApprovalResponse)]
pub(crate) struct PluginArtifactApprovalResponse {
    approved_by: i64,
    approved_at: i64,
    request_id: String,
}

impl From<AdminPluginArtifactApproval> for PluginArtifactApprovalResponse {
    fn from(value: AdminPluginArtifactApproval) -> Self {
        Self {
            approved_by: value.approved_by(),
            approved_at: value.approved_at(),
            request_id: value.request_id().to_owned(),
        }
    }
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginArtifactRevocationResponse)]
pub(crate) struct PluginArtifactRevocationResponse {
    revoked_by: i64,
    revoked_at: i64,
    request_id: String,
}

impl From<AdminPluginArtifactRevocation> for PluginArtifactRevocationResponse {
    fn from(value: AdminPluginArtifactRevocation) -> Self {
        Self {
            revoked_by: value.revoked_by(),
            revoked_at: value.revoked_at(),
            request_id: value.request_id().to_owned(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginArtifactPageResponse)]
pub(crate) struct PluginArtifactPageResponse {
    artifacts: Vec<PluginArtifactResponse>,
    next_before_id: Option<i64>,
}

impl From<AdminPluginArtifactPage> for PluginArtifactPageResponse {
    fn from(value: AdminPluginArtifactPage) -> Self {
        Self {
            artifacts: value.artifacts().iter().cloned().map(Into::into).collect(),
            next_before_id: value.next_before_id(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginDeploymentResponse)]
pub(crate) struct PluginDeploymentResponse {
    plugin_id: String,
    generation: i64,
    artifact: PluginArtifactResponse,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_selection: Option<PluginProviderSelectionBody>,
    granted_capabilities: Vec<PluginCapabilityGrantResponse>,
    updated_by: i64,
    updated_at: i64,
    request_id: String,
}

impl From<AdminPluginDeployment> for PluginDeploymentResponse {
    fn from(value: AdminPluginDeployment) -> Self {
        Self {
            plugin_id: value.plugin_id().to_owned(),
            generation: value.generation(),
            artifact: value.artifact().clone().into(),
            enabled: value.enabled(),
            provider_selection: value.provider_selection().map(|selection| {
                PluginProviderSelectionBody {
                    channel_ids: selection.channel_ids.clone(),
                    models: selection.models.clone(),
                    priority: selection.priority,
                    upstream_response_mode: match selection.upstream_response_mode {
                        PluginProviderUpstreamResponseMode::Auto => {
                            PluginProviderUpstreamResponseModeBody::Auto
                        }
                        PluginProviderUpstreamResponseMode::NonStreaming => {
                            PluginProviderUpstreamResponseModeBody::NonStreaming
                        }
                        PluginProviderUpstreamResponseMode::Streaming => {
                            PluginProviderUpstreamResponseModeBody::Streaming
                        }
                    },
                    session_enabled: selection.session_enabled,
                    session_timeout_ms: selection.session_timeout_ms,
                    websocket_endpoints: selection
                        .websocket_endpoints
                        .iter()
                        .map(|endpoint| PluginProviderWebSocketEndpointBody {
                            id: endpoint.id.clone(),
                            url: endpoint.url.clone(),
                        })
                        .collect(),
                }
            }),
            granted_capabilities: value
                .granted_capabilities()
                .iter()
                .map(PluginCapabilityGrantResponse::from)
                .collect(),
            updated_by: value.updated_by(),
            updated_at: value.updated_at(),
            request_id: value.request_id().to_owned(),
        }
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PluginProviderSelectionBody {
    channel_ids: Vec<i64>,
    models: Vec<String>,
    priority: i32,
    #[serde(default)]
    #[schema(required = false)]
    upstream_response_mode: PluginProviderUpstreamResponseModeBody,
    #[serde(default)]
    #[schema(required = false)]
    session_enabled: bool,
    #[serde(default)]
    #[schema(required = false, minimum = 1000, maximum = 360000)]
    session_timeout_ms: u32,
    #[serde(default, rename = "webSocketEndpoints", alias = "websocketEndpoints")]
    #[schema(required = false, max_items = 16, rename = "webSocketEndpoints")]
    websocket_endpoints: Vec<PluginProviderWebSocketEndpointBody>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PluginProviderWebSocketEndpointBody {
    id: String,
    url: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum PluginProviderUpstreamResponseModeBody {
    #[default]
    Auto,
    NonStreaming,
    Streaming,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginProviderSelectionRequest {
    #[schema(minimum = 1)]
    expected_generation: i64,
    selection: PluginProviderSelectionBody,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCapabilityGrantResponse {
    id: String,
    version: u16,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginCapabilityInvocationResponse)]
pub(crate) struct PluginCapabilityInvocationResponse {
    plugin_id: String,
    deployment_generation: i64,
    capability_id: String,
    capability_version: u16,
    #[schema(schema_with = crate::openapi::schema::free_form_json_schema)]
    output: Value,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginCredentialInvocationResponse)]
pub(crate) struct PluginCredentialInvocationResponse {
    plugin_id: String,
    deployment_generation: i64,
    credential_id: String,
    #[schema(value_type = serde_json::Value)]
    result: PluginCredentialResult,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginTaskInvocationResponse)]
pub(crate) struct PluginTaskInvocationResponse {
    plugin_id: String,
    deployment_generation: i64,
    #[schema(value_type = serde_json::Value)]
    result: PluginTaskResult,
}

impl From<af_admin::AdminPluginCredentialInvocationResult> for PluginCredentialInvocationResponse {
    fn from(value: af_admin::AdminPluginCredentialInvocationResult) -> Self {
        Self {
            plugin_id: value.plugin_id,
            deployment_generation: value.deployment_generation,
            credential_id: value.credential_id,
            result: value.result,
        }
    }
}

impl From<af_admin::AdminPluginTaskInvocationResult> for PluginTaskInvocationResponse {
    fn from(value: af_admin::AdminPluginTaskInvocationResult) -> Self {
        Self {
            plugin_id: value.plugin_id,
            deployment_generation: value.deployment_generation,
            result: value.result,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginRouteInvocationResponse)]
pub(crate) struct PluginRouteInvocationResponse {
    plugin_id: String,
    route_id: String,
    deployment_generation: i64,
    capability_id: String,
    capability_version: u16,
    #[schema(schema_with = crate::openapi::schema::free_form_json_schema)]
    output: Value,
}

impl From<AdminPluginRouteInvocationResult> for PluginRouteInvocationResponse {
    fn from(value: AdminPluginRouteInvocationResult) -> Self {
        Self {
            plugin_id: value.plugin_id,
            route_id: value.route_id,
            deployment_generation: value.deployment_generation,
            capability_id: value.capability_id,
            capability_version: value.capability_version,
            output: value.output,
        }
    }
}

impl From<AdminPluginCapabilityInvocationResult> for PluginCapabilityInvocationResponse {
    fn from(value: AdminPluginCapabilityInvocationResult) -> Self {
        Self {
            plugin_id: value.plugin_id,
            deployment_generation: value.deployment_generation,
            capability_id: value.capability_id,
            capability_version: value.capability_version,
            output: value.output,
        }
    }
}

impl From<&AdminPluginCapabilityGrant> for PluginCapabilityGrantResponse {
    fn from(value: &AdminPluginCapabilityGrant) -> Self {
        Self {
            id: value.id().to_owned(),
            version: value.version(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginDeploymentPageResponse)]
pub(crate) struct PluginDeploymentPageResponse {
    deployments: Vec<PluginDeploymentResponse>,
    next_cursor: Option<String>,
}

impl From<AdminPluginDeploymentPage> for PluginDeploymentPageResponse {
    fn from(value: AdminPluginDeploymentPage) -> Self {
        Self {
            deployments: value
                .deployments()
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            next_cursor: value.next_before_plugin_id().map(str::to_owned),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginEventResultResponse)]
pub(crate) struct PluginEventResultResponse {
    delivery_id: i64,
    plugin_id: String,
    event_kind: String,
    deployment_generation: i64,
    capability_id: String,
    capability_version: u16,
    attempt_count: u16,
    status: String,
    completed_at: i64,
    summary: String,
    #[schema(schema_with = crate::openapi::schema::free_form_object_schema)]
    result: Value,
}

impl From<AdminPluginEventResult> for PluginEventResultResponse {
    fn from(value: AdminPluginEventResult) -> Self {
        Self {
            delivery_id: value.delivery_id,
            plugin_id: value.plugin_id,
            event_kind: value.event_kind,
            deployment_generation: value.deployment_generation,
            capability_id: value.capability_id,
            capability_version: value.capability_version,
            attempt_count: value.attempt_count,
            status: value.status,
            completed_at: value.completed_at,
            summary: value.summary,
            result: value.result,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PluginEventResultPageResponse)]
pub(crate) struct PluginEventResultPageResponse {
    results: Vec<PluginEventResultResponse>,
    next_before_id: Option<i64>,
}

impl From<AdminPluginEventResultPage> for PluginEventResultPageResponse {
    fn from(value: AdminPluginEventResultPage) -> Self {
        Self {
            results: value.results.into_iter().map(Into::into).collect(),
            next_before_id: value.next_before_id,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginActionResponse {
    completed: bool,
    request_id: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginRuntimeOverviewResponse {
    local_node_id: Option<String>,
    isolation: String,
    nodes: Vec<PluginRuntimeNodeResponse>,
    states: Vec<PluginRuntimeStateResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginRuntimeNodeResponse {
    node_id: String,
    host_label: String,
    last_seen_at: i64,
    live: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginRuntimeStateResponse {
    node_id: String,
    plugin_id: String,
    generation: i64,
    digest: String,
    status: String,
    isolation: String,
    error_code: Option<String>,
    updated_at: i64,
    node_last_seen_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginUninstallResponse {
    plugin_id: String,
    generation: i64,
    pending_nodes: u32,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginArtifactCleanupResponse {
    artifact_id: i64,
    archive_removed: bool,
    local_files_removed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginCatalogListQuery {
    q: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginCatalogSourceQuery {
    cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCatalogSourceResponse {
    plugin_id: String,
    display_name: String,
    plugin_version: String,
    generation: i64,
    supports_search: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginMarketQuery {
    before_id: Option<i64>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginMarketIndexQuery {
    url: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginMarketEntryResponse {
    plugin_id: String,
    display_name: String,
    plugin_version: String,
    digest: String,
    source_kind: String,
    capabilities: Vec<String>,
    icon: Option<String>,
    repository: Option<String>,
    published_at: i64,
    official: bool,
}

impl From<af_admin::AdminPluginMarketEntry> for PluginMarketEntryResponse {
    fn from(value: af_admin::AdminPluginMarketEntry) -> Self {
        Self {
            plugin_id: value.plugin_id,
            display_name: value.display_name,
            plugin_version: value.plugin_version,
            digest: value.digest,
            source_kind: value.source_kind,
            capabilities: value.capabilities,
            icon: value.icon,
            repository: value.repository,
            published_at: value.published_at,
            official: value.official,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginMarketPageResponse {
    plugins: Vec<PluginMarketEntryResponse>,
    next_before_id: Option<i64>,
}

impl From<af_admin::PluginCatalogSource> for PluginCatalogSourceResponse {
    fn from(value: af_admin::PluginCatalogSource) -> Self {
        Self {
            plugin_id: value.plugin_id,
            display_name: value.display_name,
            plugin_version: value.plugin_version,
            generation: value.generation,
            supports_search: value.supports_search,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCatalogSourcePageResponse {
    sources: Vec<PluginCatalogSourceResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCatalogModelResponse {
    id: String,
    display_name: String,
    provider: String,
    description: Option<String>,
    context_window: Option<u32>,
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
    tags: Vec<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCatalogModelPageResponse {
    source: PluginCatalogSourceResponse,
    models: Vec<PluginCatalogModelResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactListQuery {
    before_id: Option<i64>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentListQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginEventResultListQuery {
    before_id: Option<i64>,
    limit: Option<u32>,
}

/// Build admin-only routes. Package paths are relative to the configured root;
/// signer public keys come from host configuration and are never request inputs.
pub fn build_plugin_lifecycle_router(
    service: Arc<DatabaseAdminPluginLifecycleService>,
    authenticator: Arc<dyn SessionAuthenticator>,
    model_authenticator: Arc<dyn af_admin::TokenAuthenticator>,
    query_api_key_policy: QueryApiKeyPolicy,
    server_config: &af_config::ServerConfig,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    let admin = Router::new()
        .route(
            "/api/admin/plugins/install",
            post(install_package).layer(DefaultBodyLimit::max(
                MAX_PLUGIN_PACKAGE_UPLOAD_BYTES + 256 * 1024,
            )),
        )
        .route(
            "/api/admin/plugins/market/install",
            post(install_market_package).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/admin/plugins/artifacts",
            get(list_artifacts).post(register_artifact),
        )
        .route("/api/admin/plugins/deployments", get(list_deployments))
        .route(
            "/api/admin/plugins/{plugin_id}/capabilities/{capability_id}/invoke",
            post(invoke_capability).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/credential-lifecycle",
            post(invoke_credential_lifecycle).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/tasks",
            post(invoke_task_direct).layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route(
            "/api/admin/plugins/tasks/unknown",
            get(list_unknown_plugin_tasks),
        )
        .route(
            "/api/admin/plugins/tasks/{task_id}/reconcile",
            post(reconcile_unknown_plugin_task),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/capability-actions",
            post(propose_capability_action).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route(
            "/api/admin/plugins/capability-actions/{proposal_key}/confirm",
            post(confirm_capability_action).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route("/api/admin/plugins/events/results", get(list_event_results))
        .route(
            "/api/admin/plugins/artifacts/{artifact_id}/approve",
            post(approve_artifact),
        )
        .route(
            "/api/admin/plugins/artifacts/{artifact_id}/revoke",
            post(revoke_artifact),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/deployment/history",
            get(deployment_history),
        )
        .route("/api/admin/plugins/{plugin_id}/deployment", post(deploy))
        .route(
            "/api/admin/plugins/{plugin_id}/provider-selection",
            post(configure_provider_selection),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/disable",
            post(disable_deployment),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/rollback",
            post(rollback_deployment),
        )
        .route("/api/admin/plugins/runtime", get(runtime_overview))
        .route(
            "/api/admin/plugins/{plugin_id}/uninstall",
            post(uninstall_plugin),
        )
        .route(
            "/api/admin/plugins/artifacts/{artifact_id}/cleanup",
            post(cleanup_artifact),
        )
        .route_layer(middleware::from_fn(authorize_management_admin));
    let model_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(model_authenticator, query_api_key_policy, server_config),
        authenticate_api_key,
    );
    let model_routes = Router::new()
        .route("/v1/plugin-tasks", post(submit_model_plugin_task))
        .route("/v1/plugin-tasks/{task_id}", get(get_model_plugin_task))
        .route(
            "/v1/plugin-tasks/{task_id}/cancel",
            post(cancel_model_plugin_task),
        )
        .route(
            "/v1/plugin-tasks/{task_id}/files/{file_id}",
            get(get_model_plugin_task_file),
        )
        .route(
            "/api/plugins/{plugin_id}/routes/{route_id}",
            get(invoke_model_plugin_route_get)
                .post(invoke_model_plugin_route_post)
                .put(invoke_model_plugin_route_put)
                .patch(invoke_model_plugin_route_patch)
                .delete(invoke_model_plugin_route_delete),
        )
        .route(
            "/v1/plugins/{plugin_id}/{route_id}",
            get(invoke_model_plugin_route_get)
                .post(invoke_model_plugin_route_post)
                .put(invoke_model_plugin_route_put)
                .patch(invoke_model_plugin_route_patch)
                .delete(invoke_model_plugin_route_delete),
        )
        .route_layer(model_authentication)
        .layer(DefaultBodyLimit::max(1024 * 1024));
    Router::new()
        .merge(admin)
        .route(
            "/api/admin/plugins/{plugin_id}/routes/{route_id}",
            get(invoke_plugin_route_get)
                .post(invoke_plugin_route_post)
                .put(invoke_plugin_route_put)
                .patch(invoke_plugin_route_patch)
                .delete(invoke_plugin_route_delete),
        )
        .route(
            "/api/account/plugins/{plugin_id}/routes/{route_id}",
            get(invoke_user_plugin_route_get)
                .post(invoke_user_plugin_route_post)
                .put(invoke_user_plugin_route_put)
                .patch(invoke_user_plugin_route_patch)
                .delete(invoke_user_plugin_route_delete),
        )
        .route("/api/account/plugins/market", get(plugin_market))
        .route(
            "/api/account/plugins/market/index",
            get(plugin_market_index),
        )
        .route("/api/account/plugin-models/sources", get(catalog_sources))
        .route(
            "/api/account/plugin-models/{plugin_id}",
            get(catalog_models),
        )
        .route_layer(authentication)
        .merge(model_routes)
        .layer(middleware::from_fn(ensure_request_id))
        .layer(middleware::from_fn(add_no_store))
        .with_state(PluginLifecycleHttpState { service })
}

async fn invoke_credential_lifecycle(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam(plugin_id): PathParam<String>,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<PluginCredentialInvocationRequest>,
) -> Result<Response, ManagementError> {
    let result = state
        .service
        .invoke_plugin_credential(
            authentication.principal(),
            &plugin_id,
            &request.credential_id,
            request.channel_id,
            request.callback_state.as_deref(),
            request.capability_version,
            request.operation,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginCredentialInvocationResponse::from(
        result,
    )))
}

async fn invoke_task_direct(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam(plugin_id): PathParam<String>,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<PluginTaskInvocationRequest>,
) -> Result<Response, ManagementError> {
    let result = state
        .service
        .invoke_plugin_task_direct(
            authentication.principal(),
            &plugin_id,
            &request.capability_id,
            request.capability_version,
            request.task,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginTaskInvocationResponse::from(result)))
}

async fn list_unknown_plugin_tasks(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    query: Result<Query<AdminPluginTaskListQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .list_unknown_plugin_tasks(
            authentication.principal(),
            query.cursor.as_deref(),
            query.limit.unwrap_or(100),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(result))
}

async fn reconcile_unknown_plugin_task(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(task_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<AdminPluginTaskReconciliationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .reconcile_unknown_plugin_task(
            authentication.principal(),
            &task_id,
            request.actual_units,
            &request.reason,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(result))
}

async fn invoke_plugin_route_get(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Get,
        plugin_route_query_input(raw_query.as_deref())?,
        request_id,
        false,
    )
    .await
}

fn plugin_route_query_input(raw_query: Option<&str>) -> Result<Value, ManagementError> {
    const MAX_QUERY_BYTES: usize = 16 * 1024;
    const MAX_QUERY_PAIRS: usize = 64;
    const MAX_QUERY_KEY_BYTES: usize = 128;
    const MAX_QUERY_VALUE_BYTES: usize = 4 * 1024;

    let Some(raw_query) = raw_query else {
        return Ok(Value::Object(serde_json::Map::new()));
    };
    if raw_query.len() > MAX_QUERY_BYTES {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let mut input = serde_json::Map::new();
    let mut pair_count = 0;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        pair_count += 1;
        if pair_count > MAX_QUERY_PAIRS
            || key.is_empty()
            || key.len() > MAX_QUERY_KEY_BYTES
            || value.len() > MAX_QUERY_VALUE_BYTES
            || key.chars().any(char::is_control)
            || value.chars().any(char::is_control)
        {
            return Err(ManagementError::PluginLifecycleInvalidRequest);
        }
        let value = Value::String(value.into_owned());
        match input.entry(key.into_owned()) {
            serde_json::map::Entry::Vacant(entry) => {
                entry.insert(value);
            }
            serde_json::map::Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if let Value::Array(values) = current {
                    values.push(value);
                } else {
                    let previous = std::mem::replace(current, Value::Null);
                    *current = Value::Array(vec![previous, value]);
                }
            }
        }
    }
    Ok(Value::Object(input))
}

async fn invoke_plugin_route_post(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Post,
        input,
        request_id,
        false,
    )
    .await
}

async fn invoke_plugin_route_put(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Put,
        input,
        request_id,
        false,
    )
    .await
}

async fn invoke_plugin_route_patch(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Patch,
        input,
        request_id,
        false,
    )
    .await
}

async fn invoke_plugin_route_delete(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Delete,
        input,
        request_id,
        false,
    )
    .await
}

async fn invoke_plugin_route_inner(
    state: PluginLifecycleHttpState,
    authentication: SessionAuthentication,
    plugin_id: String,
    route_id: String,
    method: af_admin::PluginRouteMethod,
    input: Value,
    request_id: RequestId,
    user_route: bool,
) -> Result<Response, ManagementError> {
    let result: AdminPluginRouteInvocationResult = if user_route {
        state
            .service
            .invoke_user_plugin_route(
                authentication.principal(),
                &plugin_id,
                &route_id,
                method,
                input,
                request_id.as_str(),
            )
            .await
    } else {
        state
            .service
            .invoke_plugin_route(
                authentication.principal(),
                &plugin_id,
                &route_id,
                method,
                input,
                request_id.as_str(),
            )
            .await
    }
    .map_err(map_service_error)?;
    Ok(no_store_json(result))
}

async fn invoke_user_plugin_route_get(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Get,
        plugin_route_query_input(raw_query.as_deref())?,
        request_id,
        true,
    )
    .await
}

async fn invoke_user_plugin_route_post(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Post,
        input,
        request_id,
        true,
    )
    .await
}

async fn invoke_user_plugin_route_put(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Put,
        input,
        request_id,
        true,
    )
    .await
}

async fn invoke_user_plugin_route_patch(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Patch,
        input,
        request_id,
        true,
    )
    .await
}

async fn invoke_user_plugin_route_delete(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Delete,
        input,
        request_id,
        true,
    )
    .await
}

async fn invoke_model_plugin_route_get(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<af_admin::TokenAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    invoke_model_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Get,
        plugin_route_query_input(raw_query.as_deref())?,
        request_id,
    )
    .await
}

async fn invoke_model_plugin_route_post(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<af_admin::TokenAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_model_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Post,
        input,
        request_id,
    )
    .await
}

async fn invoke_model_plugin_route_put(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<af_admin::TokenAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_model_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Put,
        input,
        request_id,
    )
    .await
}

async fn invoke_model_plugin_route_patch(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<af_admin::TokenAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_model_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Patch,
        input,
        request_id,
    )
    .await
}

async fn invoke_model_plugin_route_delete(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<af_admin::TokenAuthentication>,
    PathParam((plugin_id, route_id)): PathParam<(String, String)>,
    Extension(request_id): Extension<RequestId>,
    Json(input): Json<Value>,
) -> Result<Response, ManagementError> {
    invoke_model_plugin_route_inner(
        state,
        authentication,
        plugin_id,
        route_id,
        af_admin::PluginRouteMethod::Delete,
        input,
        request_id,
    )
    .await
}

async fn invoke_model_plugin_route_inner(
    state: PluginLifecycleHttpState,
    authentication: af_admin::TokenAuthentication,
    plugin_id: String,
    route_id: String,
    method: af_admin::PluginRouteMethod,
    input: Value,
    request_id: RequestId,
) -> Result<Response, ManagementError> {
    let model = input
        .get("model")
        .and_then(Value::as_str)
        .ok_or(ManagementError::PluginLifecycleInvalidRequest)?;
    if !authentication.model_policy().allows(model) {
        return Err(ManagementError::Forbidden);
    }
    let result = state
        .service
        .invoke_model_plugin_route(
            af_admin::SessionPrincipal::new(
                authentication.principal().user_id(),
                af_admin::SessionRole::User,
            ),
            &plugin_id,
            &route_id,
            method,
            input,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(result))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelPluginTaskRequest {
    plugin_id: String,
    capability_id: String,
    capability_version: u16,
    model: String,
    idempotency_key: String,
    #[schema(value_type = serde_json::Value)]
    request: Value,
}

async fn submit_model_plugin_task(
    State(state): State<PluginLifecycleHttpState>,
    Extension(auth): Extension<af_admin::TokenAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<ModelPluginTaskRequest>,
) -> Result<Response, ManagementError> {
    let result = state
        .service
        .submit_model_plugin_task(
            &auth,
            &request.plugin_id,
            &request.capability_id,
            request.capability_version,
            &request.model,
            request.request,
            &request.idempotency_key,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(result))
}
async fn get_model_plugin_task(
    State(state): State<PluginLifecycleHttpState>,
    Extension(auth): Extension<af_admin::TokenAuthentication>,
    PathParam(id): PathParam<String>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        state
            .service
            .get_model_plugin_task(&auth, &id)
            .await
            .map_err(map_service_error)?,
    ))
}
async fn cancel_model_plugin_task(
    State(state): State<PluginLifecycleHttpState>,
    Extension(auth): Extension<af_admin::TokenAuthentication>,
    PathParam(id): PathParam<String>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        state
            .service
            .cancel_model_plugin_task(&auth, &id, request_id.as_str())
            .await
            .map_err(map_service_error)?,
    ))
}
async fn get_model_plugin_task_file(
    State(state): State<PluginLifecycleHttpState>,
    Extension(auth): Extension<af_admin::TokenAuthentication>,
    PathParam((id, index)): PathParam<(String, usize)>,
) -> Result<Response, ManagementError> {
    let bytes = state
        .service
        .get_model_plugin_task_file(&auth, &id, index)
        .await
        .map_err(map_service_error)?;
    Response::builder()
        .header("content-type", "application/octet-stream")
        .header("content-disposition", "attachment")
        .header("x-content-type-options", "nosniff")
        .header("cache-control", "no-store")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| ManagementError::PluginLifecycleUnavailable)
}

async fn runtime_overview(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let overview = state
        .service
        .runtime_overview(authentication.principal())
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginRuntimeOverviewResponse {
        local_node_id: overview.local_node_id,
        isolation: overview.isolation,
        nodes: overview
            .nodes
            .into_iter()
            .map(|node| PluginRuntimeNodeResponse {
                node_id: node.node_id,
                host_label: node.host_label,
                last_seen_at: node.last_seen_at,
                live: node.live,
            })
            .collect(),
        states: overview
            .states
            .into_iter()
            .map(|item| PluginRuntimeStateResponse {
                node_id: item.node_id,
                plugin_id: item.plugin_id,
                generation: item.generation,
                digest: item.digest,
                status: item.status,
                isolation: item.isolation,
                error_code: item.error_code,
                updated_at: item.updated_at,
                node_last_seen_at: item.node_last_seen_at,
            })
            .collect(),
    }))
}

async fn uninstall_plugin(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginDeploymentDisableRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .uninstall(
            authentication.principal(),
            &plugin_id,
            request.expected_generation,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginUninstallResponse {
        plugin_id: result.plugin_id,
        generation: result.generation,
        pending_nodes: result.pending_nodes,
    }))
}

async fn cleanup_artifact(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(artifact_id): PathParam<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let result = state
        .service
        .cleanup_artifact(authentication.principal(), artifact_id, request_id.as_str())
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginArtifactCleanupResponse {
        artifact_id: result.artifact_id,
        archive_removed: result.archive_removed,
        local_files_removed: result.local_files_removed,
    }))
}

async fn catalog_sources(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    query: Result<Query<PluginCatalogSourceQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .catalog_sources(authentication.principal(), query.cursor)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginCatalogSourcePageResponse {
        sources: result.sources.into_iter().map(Into::into).collect(),
        next_cursor: result.next_cursor,
    }))
}

async fn plugin_market(
    State(state): State<PluginLifecycleHttpState>,
    Query(query): Query<PluginMarketQuery>,
) -> Result<Response, ManagementError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let plugins = state
        .service
        .list_public_market(query.before_id, limit)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginMarketPageResponse {
        plugins: plugins.into_iter().map(Into::into).collect(),
        next_before_id: None,
    }))
}

async fn plugin_market_index(
    State(state): State<PluginLifecycleHttpState>,
    Query(query): Query<PluginMarketIndexQuery>,
) -> Result<Response, ManagementError> {
    let index = state
        .service
        .fetch_market_index(&query.url)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(index))
}

async fn catalog_models(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    query: Result<Query<PluginCatalogListQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let query = PluginCatalogExtensionQuery::new(query.q, query.cursor, query.limit.unwrap_or(24))
        .map_err(map_service_error)?;
    let result = state
        .service
        .catalog_models(
            authentication.principal(),
            &plugin_id,
            query,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginCatalogModelPageResponse {
        source: result.source.into(),
        next_cursor: result.next_cursor,
        models: result
            .models
            .into_iter()
            .map(|item| PluginCatalogModelResponse {
                id: item.id,
                display_name: item.display_name,
                provider: item.provider,
                description: item.description,
                context_window: item.context_window,
                input_modalities: item.input_modalities,
                output_modalities: item.output_modalities,
                tags: item.tags,
            })
            .collect(),
    }))
}

async fn install_package(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<Response, ManagementError> {
    let multipart = multipart.map_err(|rejection| {
        if rejection.status() == http::StatusCode::PAYLOAD_TOO_LARGE {
            ManagementError::PluginLifecyclePayloadTooLarge
        } else {
            ManagementError::PluginLifecycleInvalidRequest
        }
    })?;
    let upload = state
        .service
        .begin_package_upload(authentication.principal(), request_id.as_str())
        .await
        .map_err(map_service_error)?;
    let parsed = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        parse_package_upload(multipart, upload),
    )
    .await
    .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let (mut upload, request) = parsed?;
    if let Some(repository) = request.repository {
        state
            .service
            .download_github_plugin_package(&mut upload, &repository, request.asset_name.as_deref())
            .await
            .map_err(map_service_error)?;
    }
    let deployment = state
        .service
        .install_package(
            authentication.principal(),
            upload,
            request.config,
            request.expected_generation,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(deployment)))
}

async fn install_market_package(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginMarketInstallHttpRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let deployment = state
        .service
        .install_market_package(
            authentication.principal(),
            PluginMarketInstallRequest {
                plugin_id: request.plugin_id,
                plugin_version: request.plugin_version,
                source: PluginMarketInstallSource {
                    kind: request.source.kind,
                    archive_url: request.source.archive_url,
                    sha256: request.source.sha256,
                    repository: request.source.repository,
                    commit: request.source.commit,
                    path: request.source.path,
                },
                config: request.config,
                expected_generation: request.expected_generation,
            },
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(deployment)))
}

struct ParsedPluginInstall {
    repository: Option<String>,
    asset_name: Option<String>,
    config: Option<Value>,
    expected_generation: Option<i64>,
}

async fn parse_package_upload(
    mut multipart: Multipart,
    mut upload: PluginPackageUpload,
) -> Result<(PluginPackageUpload, ParsedPluginInstall), ManagementError> {
    let invalid = || ManagementError::PluginLifecycleInvalidRequest;
    let mut package_seen = false;
    let mut repository = None;
    let mut asset_name = None;
    let mut config = None;
    let mut generation = None;
    while let Some(mut field) = multipart.next_field().await.map_err(map_multipart_error)? {
        match field.name().ok_or_else(invalid)? {
            "package" if !package_seen && repository.is_none() && asset_name.is_none() => {
                package_seen = true;
                while let Some(bytes) = field.chunk().await.map_err(map_multipart_error)? {
                    upload
                        .write_chunk(&bytes)
                        .await
                        .map_err(map_service_error)?;
                }
            }
            "repository" if !package_seen && repository.is_none() => {
                repository = Some(read_plugin_install_text(field, 256).await?);
            }
            "asset_name" if !package_seen && asset_name.is_none() => {
                asset_name = Some(read_plugin_install_text(field, 255).await?);
            }
            "config" if config.is_none() => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.chunk().await.map_err(map_multipart_error)? {
                    if bytes.len().saturating_add(chunk.len()) > 65_536 {
                        return Err(invalid());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                if !value.is_object() || contains_sensitive_config_fields(&value) {
                    return Err(invalid());
                }
                config = Some(value);
            }
            "expected_generation" if generation.is_none() => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.chunk().await.map_err(map_multipart_error)? {
                    if bytes.len().saturating_add(chunk.len()) > 20 {
                        return Err(invalid());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let value = std::str::from_utf8(&bytes)
                    .map_err(|_| invalid())?
                    .parse::<i64>()
                    .map_err(|_| invalid())?;
                if value <= 0 {
                    return Err(invalid());
                }
                generation = Some(value);
            }
            _ => return Err(invalid()),
        }
    }
    if !package_seen && repository.is_none() {
        return Err(invalid());
    }
    Ok((
        upload,
        ParsedPluginInstall {
            repository,
            asset_name,
            config,
            expected_generation: generation,
        },
    ))
}

async fn read_plugin_install_text(
    mut field: axum::extract::multipart::Field<'_>,
    limit: usize,
) -> Result<String, ManagementError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(map_multipart_error)? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(ManagementError::PluginLifecycleInvalidRequest);
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?
        .trim();
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    Ok(text.to_owned())
}

fn map_multipart_error(error: axum::extract::multipart::MultipartError) -> ManagementError {
    if error.status() == http::StatusCode::PAYLOAD_TOO_LARGE {
        ManagementError::PluginLifecyclePayloadTooLarge
    } else {
        ManagementError::PluginLifecycleInvalidRequest
    }
}

async fn add_no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn list_artifacts(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    query: Result<Query<ArtifactListQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let page = state
        .service
        .list_artifacts(
            authentication.principal(),
            query.before_id,
            query.limit.unwrap_or(DEFAULT_PLUGIN_CATALOG_PAGE_SIZE),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginArtifactPageResponse::from(page)))
}

async fn list_deployments(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    query: Result<Query<DeploymentListQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let page = state
        .service
        .list_deployments(
            authentication.principal(),
            query.cursor,
            query.limit.unwrap_or(DEFAULT_PLUGIN_CATALOG_PAGE_SIZE),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentPageResponse::from(page)))
}

async fn invoke_capability(
    State(state): State<PluginLifecycleHttpState>,
    PathParam((plugin_id, capability_id)): PathParam<(String, String)>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginCapabilityInvocationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .invoke_custom_capability(
            authentication.principal(),
            &plugin_id,
            &capability_id,
            request.capability_version,
            request.input,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginCapabilityInvocationResponse::from(
        result,
    )))
}

async fn propose_capability_action(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginCapabilityActionProposalRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .propose_capability_action(
            authentication.principal(),
            &plugin_id,
            &request.capability_id,
            request.capability_version,
            &request.idempotency_key,
            request.input,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginCapabilityActionProposalResponse::from(
        result,
    )))
}

async fn confirm_capability_action(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(proposal_key): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginCapabilityActionConfirmRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .confirm_capability_action(
            authentication.principal(),
            &proposal_key,
            &request.confirmation_key,
            request.input,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginCapabilityActionResultResponse::from(
        result,
    )))
}

async fn list_event_results(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    query: Result<Query<PluginEventResultListQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let page = state
        .service
        .list_event_results(
            authentication.principal(),
            query.before_id,
            query.limit.unwrap_or(20),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginEventResultPageResponse::from(page)))
}

async fn deployment_history(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    query: Result<Query<LimitQuery>, QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let deployments = state
        .service
        .deployment_history(
            authentication.principal(),
            &plugin_id,
            query.limit.unwrap_or(DEFAULT_PLUGIN_CATALOG_PAGE_SIZE),
        )
        .await
        .map_err(map_service_error)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<PluginDeploymentResponse>>();
    Ok(no_store_json(deployments))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitQuery {
    limit: Option<u32>,
}

async fn register_artifact(
    State(state): State<PluginLifecycleHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginArtifactRegistrationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let command = PluginArtifactRegistrationCommand::new(
        request.plugin_id,
        request.plugin_version,
        request.manifest,
        request.package_path,
        request.executable_path,
        request.sha256,
        request.signer_key_id,
        request.signature_base64,
    )
    .map_err(map_service_error)?;
    let registered = state
        .service
        .register_artifact_candidate(authentication.principal(), command, request_id.as_str())
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginArtifactResponse::from(registered)))
}

async fn approve_artifact(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(artifact_id): PathParam<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    state
        .service
        .approve_artifact(authentication.principal(), artifact_id, request_id.as_str())
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginActionResponse {
        completed: true,
        request_id: request_id.as_str().to_owned(),
    }))
}

async fn revoke_artifact(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(artifact_id): PathParam<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginArtifactRevokeRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    state
        .service
        .revoke_artifact(
            authentication.principal(),
            artifact_id,
            request.reason,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginActionResponse {
        completed: true,
        request_id: request_id.as_str().to_owned(),
    }))
}

async fn deploy(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginDeploymentRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    if contains_sensitive_config_fields(&request.config) {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let grants = request
        .granted_capabilities
        .into_iter()
        .map(|grant| AdminPluginCapabilityGrant::new(grant.id, grant.version))
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_service_error)?;
    let command = PluginDeploymentCommand::new(
        plugin_id,
        request.artifact_digest,
        request.expected_generation,
        request.enabled,
        grants,
        request.config,
    )
    .map_err(map_service_error)?;
    let result = state
        .service
        .deploy(authentication.principal(), command, request_id.as_str())
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(result)))
}

async fn configure_provider_selection(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginProviderSelectionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let selection = PluginProviderSelection {
        channel_ids: request.selection.channel_ids,
        models: request.selection.models,
        priority: request.selection.priority,
        upstream_response_mode: match request.selection.upstream_response_mode {
            PluginProviderUpstreamResponseModeBody::Auto => {
                PluginProviderUpstreamResponseMode::Auto
            }
            PluginProviderUpstreamResponseModeBody::NonStreaming => {
                PluginProviderUpstreamResponseMode::NonStreaming
            }
            PluginProviderUpstreamResponseModeBody::Streaming => {
                PluginProviderUpstreamResponseMode::Streaming
            }
        },
        session_enabled: request.selection.session_enabled,
        session_timeout_ms: if request.selection.session_timeout_ms == 0 {
            30_000
        } else {
            request.selection.session_timeout_ms
        },
        websocket_endpoints: request
            .selection
            .websocket_endpoints
            .into_iter()
            .map(|endpoint| PluginProviderWebSocketEndpoint {
                id: endpoint.id,
                url: endpoint.url,
            })
            .collect(),
    };
    let deployment = state
        .service
        .configure_provider_selection(
            authentication.principal(),
            &plugin_id,
            request.expected_generation,
            selection,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(deployment)))
}

async fn disable_deployment(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginDeploymentDisableRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .disable(
            authentication.principal(),
            &plugin_id,
            request.expected_generation,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(result)))
}

async fn rollback_deployment(
    State(state): State<PluginLifecycleHttpState>,
    PathParam(plugin_id): PathParam<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<PluginDeploymentRollbackRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let result = state
        .service
        .rollback(
            authentication.principal(),
            &plugin_id,
            request.expected_generation,
            request.target_generation,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(result)))
}

fn contains_sensitive_config_fields(value: &Value) -> bool {
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
                || contains_sensitive_config_fields(value)
        }),
        Value::Array(items) => items.iter().any(contains_sensitive_config_fields),
        _ => false,
    }
}

fn map_service_error(error: AdminPluginLifecycleError) -> ManagementError {
    match error {
        AdminPluginLifecycleError::Forbidden => ManagementError::Forbidden,
        AdminPluginLifecycleError::InvalidInput => ManagementError::PluginLifecycleInvalidRequest,
        AdminPluginLifecycleError::CapabilityReviewRequired => {
            ManagementError::PluginLifecycleCapabilityReviewRequired
        }
        AdminPluginLifecycleError::PayloadTooLarge => {
            ManagementError::PluginLifecyclePayloadTooLarge
        }
        AdminPluginLifecycleError::NotFound => ManagementError::PluginLifecycleNotFound,
        AdminPluginLifecycleError::Conflict => ManagementError::PluginLifecycleConflict,
        AdminPluginLifecycleError::RuntimeUnavailable => {
            ManagementError::PluginLifecycleUnavailable
        }
        AdminPluginLifecycleError::Internal => ManagementError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::{
        LoginCredentials, SessionAuthenticationError, SessionAuthenticationFuture,
        SessionLoginFuture, SessionPrincipal, SessionRole,
    };
    use af_domain::{GroupId, UserId};
    use axum::body::Body;
    use http::{Request as HttpRequest, StatusCode, header::AUTHORIZATION};
    use serde_json::json;
    use tower::ServiceExt;

    #[test]
    fn plugin_get_query_becomes_bounded_schema_input() {
        assert_eq!(plugin_route_query_input(None).unwrap(), json!({}));
        assert_eq!(
            plugin_route_query_input(Some("model=gpt-6.1-sol&tag=a&tag=b")).unwrap(),
            json!({"model":"gpt-6.1-sol","tag":["a","b"]})
        );
        assert!(matches!(
            plugin_route_query_input(Some("name=%00")),
            Err(ManagementError::PluginLifecycleInvalidRequest)
        ));
        assert!(matches!(
            plugin_route_query_input(Some(&"a=1&".repeat(65))),
            Err(ManagementError::PluginLifecycleInvalidRequest)
        ));
    }

    struct FixtureAuthenticator;

    impl SessionAuthenticator for FixtureAuthenticator {
        fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
            Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
        }

        fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
            Box::pin(async move {
                let role = match token {
                    "plugin-admin-fixture" => SessionRole::Admin,
                    "plugin-user-fixture" => SessionRole::User,
                    _ => return Err(SessionAuthenticationError::InvalidSession),
                };
                Ok(SessionAuthentication::new(
                    SessionPrincipal::new(UserId::new(1).unwrap(), role),
                    GroupId::new(1).unwrap(),
                    u64::MAX,
                ))
            })
        }
    }

    fn authorization_fixture() -> Router {
        let admin = Router::new()
            .route(
                "/api/admin/plugins/runtime",
                get(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/tasks/unknown",
                get(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/tasks/{task_id}/reconcile",
                post(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/events/results",
                get(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/{plugin_id}/capabilities/{capability_id}/invoke",
                post(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/{plugin_id}/capability-actions",
                post(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/capability-actions/{proposal_key}/confirm",
                post(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/{plugin_id}/uninstall",
                post(|| async { StatusCode::OK }),
            )
            .route(
                "/api/admin/plugins/artifacts/{artifact_id}/cleanup",
                post(|| async { StatusCode::OK }),
            )
            .route_layer(middleware::from_fn(authorize_management_admin));
        Router::new()
            .merge(admin)
            .route(
                "/api/account/plugin-models/sources",
                get(|| async { StatusCode::OK }),
            )
            .route(
                "/api/account/plugin-models/{plugin_id}",
                get(|| async { StatusCode::OK }),
            )
            .route_layer(middleware::from_fn_with_state(
                ManagementAuthenticationState::new(Arc::new(FixtureAuthenticator)),
                authenticate_management_session,
            ))
    }

    #[tokio::test]
    async fn plugin_admin_operations_require_admin_but_catalog_accepts_authenticated_users() {
        for (method, path) in [
            ("GET", "/api/admin/plugins/runtime"),
            ("GET", "/api/admin/plugins/tasks/unknown"),
            ("POST", "/api/admin/plugins/tasks/task-123/reconcile"),
            ("GET", "/api/admin/plugins/events/results"),
            (
                "POST",
                "/api/admin/plugins/org.example.test/capabilities/vendor.custom.lookup/invoke",
            ),
            (
                "POST",
                "/api/admin/plugins/org.example.test/capability-actions",
            ),
            (
                "POST",
                "/api/admin/plugins/capability-actions/proposal-123/confirm",
            ),
            ("POST", "/api/admin/plugins/org.example.catalog/uninstall"),
            ("POST", "/api/admin/plugins/artifacts/1/cleanup"),
        ] {
            for (token, expected) in [
                (None, StatusCode::UNAUTHORIZED),
                (Some("plugin-user-fixture"), StatusCode::FORBIDDEN),
                (Some("plugin-admin-fixture"), StatusCode::OK),
            ] {
                let mut request = HttpRequest::builder().method(method).uri(path);
                if let Some(token) = token {
                    request = request.header(AUTHORIZATION, format!("Bearer {token}"));
                }
                let response = authorization_fixture()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), expected, "{method} {path}");
            }
        }
        for path in [
            "/api/account/plugin-models/sources",
            "/api/account/plugin-models/org.example.catalog",
        ] {
            let response = authorization_fixture()
                .oneshot(
                    HttpRequest::builder()
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            let response = authorization_fixture()
                .oneshot(
                    HttpRequest::builder()
                        .uri(path)
                        .header(AUTHORIZATION, "Bearer plugin-user-fixture")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[test]
    fn plugin_catalog_pat_scopes_are_model_read_only_and_admin_actions_are_distinct() {
        for path in [
            "/api/account/plugin-models/sources",
            "/api/account/plugin-models/{plugin_id}",
        ] {
            let policy = crate::access_token_routes::policy("GET", path).unwrap();
            assert_eq!(policy.required_scopes, ["models:read"]);
            assert!(!policy.admin_only && !policy.session_only);
        }
        let policies = [
            ("GET", "/api/admin/plugins/runtime"),
            ("GET", "/api/admin/plugins/tasks/unknown"),
            ("POST", "/api/admin/plugins/tasks/{task_id}/reconcile"),
            ("GET", "/api/admin/plugins/events/results"),
            (
                "POST",
                "/api/admin/plugins/{plugin_id}/capabilities/{capability_id}/invoke",
            ),
            ("POST", "/api/admin/plugins/{plugin_id}/capability-actions"),
            (
                "POST",
                "/api/admin/plugins/capability-actions/{proposal_key}/confirm",
            ),
            ("POST", "/api/admin/plugins/{plugin_id}/uninstall"),
            ("POST", "/api/admin/plugins/artifacts/{artifact_id}/cleanup"),
        ]
        .into_iter()
        .map(|(method, path)| crate::access_token_routes::policy(method, path).unwrap())
        .collect::<Vec<_>>();
        let mut grants = std::collections::BTreeSet::new();
        for policy in policies {
            assert!(policy.admin_only && !policy.session_only);
            assert_eq!(policy.required_scopes.len(), 1);
            assert!(policy.required_scopes[0].starts_with("admin:"));
            assert!(grants.insert(policy.required_scopes[0].clone()));
        }
        let event_results =
            crate::access_token_routes::policy("GET", "/api/admin/plugins/events/results").unwrap();
        assert_eq!(
            event_results.required_scopes,
            ["admin:list_admin_plugin_event_results:execute"]
        );
        assert!(event_results.admin_only && !event_results.session_only);
    }

    #[test]
    fn plugin_event_result_response_contains_only_result_metadata_and_output() {
        let response = PluginEventResultPageResponse::from(AdminPluginEventResultPage {
            results: vec![AdminPluginEventResult {
                delivery_id: 31,
                plugin_id: "org.example.events".to_owned(),
                event_kind: "channel.health.changed".to_owned(),
                deployment_generation: 2,
                capability_id: "events.channel-health.changed.consume".to_owned(),
                capability_version: 1,
                attempt_count: 1,
                status: "delivered".to_owned(),
                completed_at: 1_800_000_012,
                summary: "Channel recovered".to_owned(),
                result: serde_json::json!({ "summary": "Channel recovered", "checks": [] }),
            }],
            next_before_id: None,
        });
        let value = serde_json::to_value(response).unwrap();
        let result = &value["results"][0];
        assert_eq!(result["deliveryId"], 31);
        assert!(result.get("eventKey").is_none());
        assert!(result.get("payload").is_none());
        assert!(result.get("pluginUrl").is_none());
        assert!(result.get("credential").is_none());
    }

    #[test]
    fn plugin_custom_capability_preserves_all_json_value_shapes() {
        let request_schema = serde_json::to_value(
            <PluginCapabilityInvocationRequest as utoipa::PartialSchema>::schema(),
        )
        .unwrap();
        let response_schema = serde_json::to_value(
            <PluginCapabilityInvocationResponse as utoipa::PartialSchema>::schema(),
        )
        .unwrap();
        assert_eq!(request_schema["properties"]["input"], json!({}));
        assert_eq!(response_schema["properties"]["output"], json!({}));
        for value in [
            json!([]),
            json!("value"),
            json!(0),
            json!(false),
            Value::Null,
        ] {
            let request = serde_json::from_value::<PluginCapabilityInvocationRequest>(json!({
                "capability_version": 1,
                "input": value.clone(),
            }))
            .unwrap();
            assert_eq!(request.input, value);
            let response = PluginCapabilityInvocationResponse {
                plugin_id: "org.example.custom".to_owned(),
                deployment_generation: 1,
                capability_id: "vendor.custom.lookup".to_owned(),
                capability_version: 1,
                output: value.clone(),
            };
            assert_eq!(serde_json::to_value(response).unwrap()["output"], value);
        }
    }

    #[test]
    fn deployment_config_rejects_credential_like_field_names() {
        assert!(contains_sensitive_config_fields(&json!({
            "proxy": { "client_secret": "not-accepted" }
        })));
        assert!(contains_sensitive_config_fields(&json!({
            "items": [{ "apiKey": "not-accepted" }]
        })));
        assert!(!contains_sensitive_config_fields(&json!({
            "region": "west",
            "pageSize": 20
        })));
    }
}
