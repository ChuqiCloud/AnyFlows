use std::sync::Arc;

use af_admin::{
    AdminClientProfileError, ClientProfileQuery, ClientProfileRepositoryError, ClientProfileWrite,
    DatabaseAdminClientProfileService, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{
    ChannelId, ClientProfileBinding, ClientProfileDefinition, CredentialKind, Protocol,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct ProfileState {
    service: Arc<DatabaseAdminClientProfileService>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    after: Option<String>,
    search: Option<String>,
    enabled: Option<bool>,
    service: Option<String>,
    protocol: Option<Protocol>,
    credential_kind: Option<CredentialKind>,
    limit: Option<usize>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageQuery {
    before: Option<i64>,
    after: Option<i64>,
    limit: Option<usize>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientProfileSaveRequest {
    #[schema(minimum = 0)]
    expected_version: i64,
    #[schema(value_type = ClientProfileDefinitionSchema)]
    definition: ClientProfileDefinition,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientProfileEnabledRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
    enabled: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientProfileVersionRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientProfileBindRequest {
    #[schema(value_type = ClientProfileBindingSchema)]
    binding: ClientProfileBinding,
    #[schema(minimum = 0)]
    expected_version: i64,
}

// 文档镜像保持 HTTP 层对 OpenAPI 的所有权；领域层不依赖 HTTP schema。
#[derive(ToSchema)]
pub(crate) struct ClientProfileDefinitionSchema {
    #[schema(max_length = 128)]
    name: String,
    #[schema(max_length = 2048)]
    description: String,
    #[schema(max_length = 64)]
    icon: Option<String>,
    service: ClientProfileServiceSchema,
    protocols: Vec<crate::openapi::schema::AdminChannelProtocolSchema>,
    credential_kinds: Vec<crate::openapi::schema::AdminCredentialKindSchema>,
    identity_headers: std::collections::BTreeMap<String, String>,
    session_strategy: ClientProfileSessionStrategySchema,
    behavior: ClientProfileBehaviorSchema,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClientProfileServiceSchema {
    Standard,
    ClaudeCode,
    Codex,
    OpenCode,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClientProfileSessionStrategySchema {
    Explicit,
    FirstUserMessage,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClientProfileBehaviorSchema {
    Standard,
    ClaudeUtcDateV1,
    CodexV1,
    OpenCodeV1,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileBindingSchema {
    key: String,
    revision: i64,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileSnapshotSchema {
    binding: ClientProfileBindingSchema,
    definition: ClientProfileDefinitionSchema,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileRecordSchema {
    key: String,
    name: String,
    service: ClientProfileServiceSchema,
    built_in: bool,
    enabled: bool,
    latest_revision: i64,
    version: i64,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfilePageSchema {
    items: Vec<ClientProfileRecordSchema>,
    next_cursor: Option<String>,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileRevisionPageSchema {
    items: Vec<ClientProfileSnapshotSchema>,
    next_cursor: Option<i64>,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileReferenceSchema {
    channel_id: i64,
    binding: ClientProfileBindingSchema,
    version: i64,
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileReferencePageSchema {
    items: Vec<ClientProfileReferenceSchema>,
    next_cursor: Option<i64>,
}

/// 平台管理员专用；会话认证和角色授权分别执行。
pub fn build_client_profile_router(
    service: Arc<DatabaseAdminClientProfileService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route("/api/admin/client-profiles", get(list_profiles))
        .route(
            "/api/admin/client-profiles/validate",
            post(validate_profile),
        )
        .route(
            "/api/admin/client-profiles/channel-bindings/{channel_id}",
            get(get_binding).put(bind_profile),
        )
        .route(
            "/api/admin/client-profiles/{key}",
            get(get_profile).put(save_profile),
        )
        .route(
            "/api/admin/client-profiles/{key}/enabled",
            post(set_enabled),
        )
        .route(
            "/api/admin/client-profiles/{key}/revisions",
            get(list_revisions),
        )
        .route(
            "/api/admin/client-profiles/{key}/revisions/{revision}",
            get(get_revision).delete(delete_revision),
        )
        .route(
            "/api/admin/client-profiles/{key}/references",
            get(list_references),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(ProfileState { service })
}

async fn list_profiles(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    query: Result<Query<ListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::InvalidRequest)?;
    let page = state
        .service
        .list(
            auth.principal(),
            ClientProfileQuery {
                after: query.after,
                search: query.search,
                enabled: query.enabled,
                service: query.service,
                protocol: query.protocol,
                credential_kind: query.credential_kind,
                limit: query.limit.unwrap_or(25),
            },
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(page))
}
async fn get_profile(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(key): Path<String>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        state
            .service
            .get(auth.principal(), &key)
            .await
            .map_err(map_error)?,
    ))
}
async fn save_profile(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(key): Path<String>,
    request: Result<Json<ClientProfileSaveRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(
        state
            .service
            .save(
                auth.principal(),
                &key,
                ClientProfileWrite {
                    expected_version: request.expected_version,
                    definition: request.definition,
                },
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn set_enabled(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(key): Path<String>,
    request: Result<Json<ClientProfileEnabledRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(
        state
            .service
            .set_enabled(
                auth.principal(),
                &key,
                request.expected_version,
                request.enabled,
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn list_revisions(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(key): Path<String>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::InvalidRequest)?;
    if query.after.is_some() {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(no_store_json(
        state
            .service
            .revisions(
                auth.principal(),
                &key,
                query.before,
                query.limit.unwrap_or(25),
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn get_revision(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((key, revision)): Path<(String, i64)>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        state
            .service
            .revision(auth.principal(), &key, revision)
            .await
            .map_err(map_error)?,
    ))
}
async fn delete_revision(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((key, revision)): Path<(String, i64)>,
    request: Result<Json<ClientProfileVersionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    state
        .service
        .delete_revision(auth.principal(), &key, revision, request.expected_version)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(serde_json::json!({"deleted":true})))
}
async fn list_references(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(key): Path<String>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ManagementError> {
    let Query(query) = query.map_err(|_| ManagementError::InvalidRequest)?;
    if query.before.is_some() {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(no_store_json(
        state
            .service
            .references(
                auth.principal(),
                &key,
                query.after,
                query.limit.unwrap_or(25),
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn get_binding(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        state
            .service
            .channel_binding(
                auth.principal(),
                ChannelId::new(id).map_err(|_| ManagementError::InvalidRequest)?,
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn bind_profile(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
    request: Result<Json<ClientProfileBindRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(
        state
            .service
            .bind(
                auth.principal(),
                ChannelId::new(id).map_err(|_| ManagementError::InvalidRequest)?,
                request.binding,
                request.expected_version,
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn validate_profile(
    request: Result<Json<ClientProfileSaveRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    request
        .definition
        .validate()
        .map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(serde_json::json!({"valid":true})))
}
fn map_error(error: AdminClientProfileError) -> ManagementError {
    match error {
        AdminClientProfileError::Forbidden => ManagementError::Forbidden,
        AdminClientProfileError::Repository(error) => match error {
            ClientProfileRepositoryError::InvalidInput => ManagementError::InvalidRequest,
            ClientProfileRepositoryError::NotFound => ManagementError::ClientProfileNotFound,
            ClientProfileRepositoryError::Conflict => ManagementError::ClientProfileConflict,
            _ => ManagementError::Internal,
        },
    }
}
