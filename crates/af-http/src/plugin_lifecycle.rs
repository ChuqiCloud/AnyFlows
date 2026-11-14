//! Admin HTTP API for plugin installation and deployment lifecycle.

use std::sync::Arc;

use af_admin::{
    AdminPluginArtifact, AdminPluginArtifactApproval, AdminPluginArtifactPage,
    AdminPluginArtifactRevocation, AdminPluginCapabilityGrant, AdminPluginDeployment,
    AdminPluginDeploymentPage, AdminPluginLifecycleError, DEFAULT_PLUGIN_CATALOG_PAGE_SIZE,
    DatabaseAdminPluginLifecycleService, MAX_PLUGIN_PACKAGE_UPLOAD_BYTES,
    PluginArtifactRegistrationCommand, PluginCatalogExtensionQuery, PluginDeploymentCommand,
    PluginPackageUpload, SessionAuthentication, SessionAuthenticator,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::Request,
    extract::{
        DefaultBodyLimit, Multipart, Path as PathParam, Query, State,
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
    #[schema(value_type = String, format = Binary)]
    package: String,
    /// Public JSON configuration, defaults to {}.
    config: Option<String>,
    /// Required for an explicit update of an existing deployment.
    #[schema(minimum = 1)]
    expected_generation: Option<i64>,
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

/// Build admin-only routes. Package paths are relative to the configured root;
/// signer public keys come from host configuration and are never request inputs.
pub fn build_plugin_lifecycle_router(
    service: Arc<DatabaseAdminPluginLifecycleService>,
    authenticator: Arc<dyn SessionAuthenticator>,
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
    Router::new()
        .merge(admin)
        .route("/api/account/plugin-models/sources", get(catalog_sources))
        .route(
            "/api/account/plugin-models/{plugin_id}",
            get(catalog_models),
        )
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .layer(middleware::from_fn(add_no_store))
        .with_state(PluginLifecycleHttpState { service })
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
    let (upload, config, expected_generation) = match parsed {
        Ok(parsed) => parsed,
        Err(ManagementError::PluginLifecycleInvalidRequest) => {
            return Err(ManagementError::PluginLifecycleInvalidRequest);
        }
        Err(error) => return Err(error),
    };
    let deployment = state
        .service
        .install_package(
            authentication.principal(),
            upload,
            config,
            expected_generation,
            request_id.as_str(),
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(PluginDeploymentResponse::from(deployment)))
}

async fn parse_package_upload(
    mut multipart: Multipart,
    mut upload: PluginPackageUpload,
) -> Result<(PluginPackageUpload, Value, Option<i64>), ManagementError> {
    let invalid = || ManagementError::PluginLifecycleInvalidRequest;
    let mut package_seen = false;
    let mut config = None;
    let mut generation = None;
    while let Some(mut field) = multipart.next_field().await.map_err(map_multipart_error)? {
        match field.name().ok_or_else(invalid)? {
            "package" if !package_seen => {
                package_seen = true;
                while let Some(bytes) = field.chunk().await.map_err(map_multipart_error)? {
                    upload
                        .write_chunk(&bytes)
                        .await
                        .map_err(map_service_error)?;
                }
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
    if !package_seen {
        return Err(invalid());
    }
    Ok((
        upload,
        config.unwrap_or_else(|| serde_json::json!({})),
        generation,
    ))
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
