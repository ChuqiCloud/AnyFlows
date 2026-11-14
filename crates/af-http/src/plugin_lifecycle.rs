//! Admin HTTP API for signed plugin artifact and deployment lifecycle.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use af_admin::{
    AdminPluginArtifact, AdminPluginArtifactApproval, AdminPluginArtifactPage,
    AdminPluginArtifactRevocation, AdminPluginCapabilityGrant, AdminPluginDeployment,
    AdminPluginDeploymentPage, AdminPluginLifecycleError, DEFAULT_PLUGIN_CATALOG_PAGE_SIZE,
    DatabaseAdminPluginLifecycleService, PluginDeploymentCommand, SessionAuthentication,
    SessionAuthenticator,
};
use af_config::PluginTrustedSigner;
use af_db::PluginCapabilityGrant as DbPluginCapabilityGrant;
use af_plugin_contract::{PluginId, PluginManifest, PluginVersion};
use af_plugin_runtime::{
    PluginArtifactDescriptor, SidecarError, TrustedPluginKey, VerifiedPluginArtifact,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::Request,
    extract::{
        Path as PathParam, Query, State, rejection::JsonRejection, rejection::QueryRejection,
    },
    middleware,
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
};

#[derive(Clone)]
struct PluginLifecycleHttpState {
    service: Arc<DatabaseAdminPluginLifecycleService>,
    trust: PluginArtifactTrust,
}

/// Host-owned root and public signing keys used to verify HTTP-registered packages.
/// The root and keys are never returned by an endpoint or included in Debug output.
#[derive(Clone)]
pub struct PluginArtifactTrust {
    artifact_root: Option<PathBuf>,
    trusted_keys: BTreeMap<String, TrustedPluginKey>,
}

impl PluginArtifactTrust {
    /// Build verification state from already validated server configuration.
    pub fn from_config(
        artifact_root: Option<&Path>,
        signers: &[PluginTrustedSigner],
    ) -> Result<Self, PluginArtifactTrustError> {
        let mut trusted_keys = BTreeMap::new();
        for signer in signers {
            let bytes = BASE64
                .decode(signer.public_key_base64().as_bytes())
                .map_err(|_| PluginArtifactTrustError::InvalidConfiguration)?;
            let public_key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| PluginArtifactTrustError::InvalidConfiguration)?;
            let key = TrustedPluginKey::new(signer.key_id().to_owned(), public_key)
                .map_err(|_| PluginArtifactTrustError::InvalidConfiguration)?;
            if trusted_keys
                .insert(signer.key_id().to_owned(), key)
                .is_some()
            {
                return Err(PluginArtifactTrustError::InvalidConfiguration);
            }
        }
        Ok(Self {
            artifact_root: artifact_root.map(Path::to_path_buf),
            trusted_keys,
        })
    }
}

impl fmt::Debug for PluginArtifactTrust {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginArtifactTrust")
            .field(
                "artifact_root",
                &self.artifact_root.as_ref().map(|_| "<configured>"),
            )
            .field("trusted_key_count", &self.trusted_keys.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PluginArtifactTrustError {
    #[error("plugin artifact trust configuration is invalid")]
    InvalidConfiguration,
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
    #[schema(value_type = Object)]
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
    #[schema(min_length = 1, max_length = 128)]
    signer_key_id: String,
    #[schema(min_length = 88, max_length = 88, write_only)]
    signature_base64: String,
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
    #[schema(value_type = Object, write_only)]
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
    #[schema(value_type = Object)]
    manifest: Value,
    source_kind: String,
    signer_key_id: String,
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

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCapabilityGrantResponse {
    id: String,
    version: u16,
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
pub(crate) struct PluginActionResponse {
    completed: bool,
    request_id: String,
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

/// Build admin-only routes. Package paths are relative to the configured root;
/// signer public keys come from host configuration and are never request inputs.
pub fn build_plugin_lifecycle_router(
    service: Arc<DatabaseAdminPluginLifecycleService>,
    authenticator: Arc<dyn SessionAuthenticator>,
    trust: PluginArtifactTrust,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/plugins/artifacts",
            get(list_artifacts).post(register_artifact),
        )
        .route("/api/admin/plugins/deployments", get(list_deployments))
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
            "/api/admin/plugins/{plugin_id}/disable",
            post(disable_deployment),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/rollback",
            post(rollback_deployment),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .layer(middleware::from_fn(add_no_store))
        .with_state(PluginLifecycleHttpState { service, trust })
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
    let artifact = verify_artifact_request(&state.trust, request).await?;
    let registered = state
        .service
        .register_verified_artifact(authentication.principal(), artifact, request_id.as_str())
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
        .map(|grant| DbPluginCapabilityGrant {
            id: grant.id,
            version: grant.version,
        })
        .collect();
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

async fn verify_artifact_request(
    trust: &PluginArtifactTrust,
    request: PluginArtifactRegistrationRequest,
) -> Result<VerifiedPluginArtifact, ManagementError> {
    let artifact_root = trust
        .artifact_root
        .as_deref()
        .ok_or(ManagementError::PluginLifecycleUnavailable)?;
    if trust.trusted_keys.is_empty() {
        return Err(ManagementError::PluginLifecycleUnavailable);
    }
    let package_relative = parse_relative_package_path(&request.package_path)?;
    let executable_relative = parse_relative_package_path(&request.executable_path)?;
    let plugin_id = PluginId::new(request.plugin_id)
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let plugin_version = PluginVersion::new(request.plugin_version)
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let manifest: PluginManifest = serde_json::from_value(request.manifest)
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    manifest
        .validate()
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    if manifest.plugin_id != plugin_id || manifest.plugin_version != plugin_version {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }

    let key = trust
        .trusted_keys
        .get(&request.signer_key_id)
        .ok_or(ManagementError::PluginLifecycleInvalidRequest)?;
    let canonical_root = tokio::fs::canonicalize(artifact_root)
        .await
        .map_err(|_| ManagementError::PluginLifecycleUnavailable)?;
    let working_directory =
        tokio::fs::canonicalize(std::env::current_dir().map_err(|_| ManagementError::Internal)?)
            .await
            .map_err(|_| ManagementError::Internal)?;
    if !canonical_root.starts_with(&working_directory) {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let package_root = tokio::fs::canonicalize(canonical_root.join(&package_relative))
        .await
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    if !package_root.starts_with(&canonical_root) {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let executable = tokio::fs::canonicalize(package_root.join(&executable_relative))
        .await
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    if !executable.starts_with(&package_root)
        || !tokio::fs::metadata(&executable)
            .await
            .is_ok_and(|metadata| metadata.is_file())
    {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }

    // The stored reference is a relative locator for reconciliation, never a local absolute path.
    let canonical_package_path = canonical_relative_path(&canonical_root, &package_root)?;
    let canonical_executable_path = canonical_relative_path(&package_root, &executable)?;
    let source_reference = serde_json::to_string(&json!({
        "packagePath": canonical_package_path,
        "executablePath": canonical_executable_path,
    }))
    .map_err(|_| ManagementError::Internal)?;
    if source_reference.len() > 2048 {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let descriptor = PluginArtifactDescriptor::new(
        plugin_id,
        plugin_version,
        manifest,
        executable,
        package_root,
        request.sha256,
        request.signer_key_id,
        &request.signature_base64,
        "local_process".to_owned(),
        source_reference,
    )
    .map_err(map_artifact_verification_error)?;
    VerifiedPluginArtifact::verify(descriptor, key)
        .await
        .map_err(map_artifact_verification_error)
}

fn canonical_relative_path(base: &Path, target: &Path) -> Result<String, ManagementError> {
    let relative = target
        .strip_prefix(base)
        .map_err(|_| ManagementError::PluginLifecycleInvalidRequest)?;
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_owned)
                .ok_or(ManagementError::PluginLifecycleInvalidRequest),
            _ => Err(ManagementError::PluginLifecycleInvalidRequest),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let value = components.join("/");
    if value.len() > 900 || value.chars().any(char::is_control) {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    Ok(value)
}

fn parse_relative_package_path(value: &str) -> Result<PathBuf, ManagementError> {
    if value.is_empty()
        || value.len() > 900
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
    {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || value
            .split('/')
            .any(|component| component.is_empty() || component.len() > 255)
    {
        return Err(ManagementError::PluginLifecycleInvalidRequest);
    }
    Ok(path.to_path_buf())
}

fn map_artifact_verification_error(_: SidecarError) -> ManagementError {
    ManagementError::PluginLifecycleInvalidRequest
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
    use serde_json::json;

    #[test]
    fn artifact_paths_must_be_normalized_relative_paths() {
        assert!(parse_relative_package_path("publishers/acme/plugin").is_ok());
        for unsafe_path in [
            "",
            "/etc/plugin",
            "../plugin",
            "packages/../../outside",
            "C:/plugins/example",
            "packages\\plugin",
            "packages//plugin",
            "packages/./plugin",
        ] {
            assert!(
                parse_relative_package_path(unsafe_path).is_err(),
                "{unsafe_path}"
            );
        }
    }

    #[test]
    fn trust_debug_does_not_reveal_root_path_or_key_material() {
        let key = TrustedPluginKey::new("publisher", [0xa5; 32]).unwrap();
        let mut trusted_keys = BTreeMap::new();
        trusted_keys.insert("publisher".to_owned(), key);
        let trust = PluginArtifactTrust {
            artifact_root: Some(PathBuf::from("private/artifact/root")),
            trusted_keys,
        };
        let debug = format!("{trust:?}");
        assert!(!debug.contains("private/artifact/root"));
        assert!(!debug.contains("a5a5a5"));
        assert!(debug.contains("<configured>"));
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
