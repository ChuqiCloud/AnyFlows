use std::sync::Arc;

use af_admin::{
    AdminClientProfileError, ClientProfileQuery, ClientProfileRepositoryError, ClientProfileWrite,
    DatabaseAdminClientProfileService, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{
    ChannelId, ChannelType, ClientProfileBinding, ClientProfileDefinition, CredentialKind, Protocol,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
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
    channel_type: Option<ChannelType>,
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

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientProfileOpenCodeEntryRequest {
    #[schema(max_length = 128)]
    version: String,
    entry: ClientProfileOpenCodeEntrySchema,
}
#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClientProfileOpenCodeEntrySchema {
    InferenceV1,
    ZenV1,
}
impl From<ClientProfileOpenCodeEntrySchema> for af_domain::OpenCodeEntry {
    fn from(value: ClientProfileOpenCodeEntrySchema) -> Self {
        match value {
            ClientProfileOpenCodeEntrySchema::InferenceV1 => Self::InferenceV1,
            ClientProfileOpenCodeEntrySchema::ZenV1 => Self::ZenV1,
        }
    }
}
#[derive(ToSchema)]
pub(crate) struct ClientProfileOpenCodeSettingsSchema {
    entry: ClientProfileOpenCodeEntrySchema,
    version: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ClientProfileCapabilitiesSchema {
    #[schema(value_type = Vec<ClientProfileDefinitionSchema>)]
    templates: Vec<ClientProfileDefinition>,
    identity_headers: Vec<&'static str>,
    session_strategies: Vec<ClientProfileSessionStrategySchema>,
    opencode_entries: Vec<ClientProfileOpenCodeEntrySchema>,
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
        .route("/api/admin/client-profiles/capabilities", get(capabilities))
        .route(
            "/api/admin/client-profiles/opencode-settings/{channel_id}",
            get(get_opencode_settings).put(set_opencode_settings),
        )
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
    RawQuery(raw): RawQuery,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw.as_deref())?;
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
                channel_type: query.channel_type,
                limit: query.limit.unwrap_or(25),
            },
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(page))
}

async fn capabilities() -> Response {
    no_store_json(ClientProfileCapabilitiesSchema {
        templates: af_domain::built_in_client_profiles()
            .into_iter()
            .map(|p| p.definition)
            .collect(),
        identity_headers: vec![
            "user-agent",
            "x-app",
            "x-client-name",
            "x-client-version",
            "x-opencode-client",
        ],
        session_strategies: vec![
            ClientProfileSessionStrategySchema::Explicit,
            ClientProfileSessionStrategySchema::FirstUserMessage,
        ],
        opencode_entries: vec![
            ClientProfileOpenCodeEntrySchema::InferenceV1,
            ClientProfileOpenCodeEntrySchema::ZenV1,
        ],
    })
}
async fn get_opencode_settings(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        state
            .service
            .opencode_settings(
                auth.principal(),
                ChannelId::new(id).map_err(|_| ManagementError::InvalidRequest)?,
            )
            .await
            .map_err(map_error)?,
    ))
}
async fn set_opencode_settings(
    State(state): State<ProfileState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
    request: Result<Json<ClientProfileOpenCodeEntryRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(
        state
            .service
            .set_opencode_entry(
                auth.principal(),
                ChannelId::new(id).map_err(|_| ManagementError::InvalidRequest)?,
                &request.version,
                request.entry.into(),
            )
            .await
            .map_err(map_error)?,
    ))
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
    RawQuery(raw): RawQuery,
) -> Result<Response, ManagementError> {
    let query = parse_page_query(raw.as_deref())?;
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
    RawQuery(raw): RawQuery,
) -> Result<Response, ManagementError> {
    let query = parse_page_query(raw.as_deref())?;
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

fn query_fields(
    raw: Option<&str>,
) -> Result<std::collections::BTreeMap<String, String>, ManagementError> {
    let raw = raw.unwrap_or_default();
    if raw.len() > 4096 {
        return Err(ManagementError::InvalidRequest);
    }
    let mut fields = std::collections::BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if fields
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(ManagementError::InvalidRequest);
        }
    }
    Ok(fields)
}

fn take_query_value<T: std::str::FromStr>(
    fields: &mut std::collections::BTreeMap<String, String>,
    name: &str,
) -> Result<Option<T>, ManagementError> {
    fields
        .remove(name)
        .map(|value| value.parse().map_err(|_| ManagementError::InvalidRequest))
        .transpose()
}

fn parse_list_query(raw: Option<&str>) -> Result<ListQuery, ManagementError> {
    let mut fields = query_fields(raw)?;
    let query = ListQuery {
        after: fields.remove("after"),
        search: fields.remove("search"),
        enabled: take_query_value(&mut fields, "enabled")?,
        service: fields.remove("service"),
        protocol: take_query_value(&mut fields, "protocol")?,
        credential_kind: take_query_value(&mut fields, "credential_kind")?,
        channel_type: take_query_value(&mut fields, "channel_type")?,
        limit: take_query_value(&mut fields, "limit")?,
    };
    if !fields.is_empty() || query.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(query)
}

fn parse_page_query(raw: Option<&str>) -> Result<PageQuery, ManagementError> {
    let mut fields = query_fields(raw)?;
    let query = PageQuery {
        before: take_query_value(&mut fields, "before")?,
        after: take_query_value(&mut fields, "after")?,
        limit: take_query_value(&mut fields, "limit")?,
    };
    if !fields.is_empty()
        || query.before.is_some_and(|v| v < 1)
        || query.after.is_some_and(|v| v < 1)
        || query.limit.is_some_and(|limit| !(1..=100).contains(&limit))
    {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_decode_filters_and_reject_ambiguous_or_unbounded_inputs() {
        let query = parse_list_query(Some("search=Example+client&enabled=false&protocol=openai_chat&credential_kind=oauth&limit=25")).unwrap();
        assert_eq!(query.search.as_deref(), Some("Example client"));
        assert_eq!(query.enabled, Some(false));
        assert_eq!(query.protocol, Some(Protocol::OpenAiChat));
        assert_eq!(query.credential_kind, Some(CredentialKind::Oauth));
        assert_eq!(query.limit, Some(25));
        assert_eq!(
            parse_list_query(Some("channel_type=opencode"))
                .unwrap()
                .channel_type,
            Some(ChannelType::OpenCode)
        );
        assert!(parse_list_query(None).unwrap().limit.is_none());
        for raw in [
            "limit=0",
            "limit=101",
            "limit=25&limit=50",
            "enabled=invalid",
            "protocol=unknown",
            "credential_kind=unknown",
            "channel_type=unknown",
            "channel_type=opencode&channel_type=openai",
            "unknown=value",
        ] {
            assert!(parse_list_query(Some(raw)).is_err(), "{raw}");
        }
        assert!(parse_list_query(Some(&format!("search={}", "a".repeat(4096)))).is_err());
        assert_eq!(
            parse_page_query(Some("before=3&limit=100")).unwrap().before,
            Some(3)
        );
        for raw in [
            "before=0",
            "after=-1",
            "limit=101",
            "after=1&after=2",
            "unknown=value",
        ] {
            assert!(parse_page_query(Some(raw)).is_err(), "{raw}");
        }
    }
}
