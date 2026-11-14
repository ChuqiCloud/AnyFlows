//! Admin HTTP API for signed plugin artifact and deployment lifecycle.

use std::sync::Arc;

use af_admin::{
    AdminPluginArtifact, AdminPluginArtifactApproval, AdminPluginArtifactPage,
    AdminPluginArtifactRevocation, AdminPluginCapabilityGrant, AdminPluginDeployment,
    AdminPluginDeploymentPage, AdminPluginLifecycleError, DEFAULT_PLUGIN_CATALOG_PAGE_SIZE,
    DatabaseAdminPluginLifecycleService, PluginArtifactRegistrationCommand,
    PluginDeploymentCommand, SessionAuthentication, SessionAuthenticator,
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
        .with_state(PluginLifecycleHttpState { service })
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
