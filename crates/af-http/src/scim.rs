use std::sync::Arc;

use af_admin::{
    OrganizationScimAuthenticationError, OrganizationScimGroupSyncError,
    OrganizationScimGroupSyncOutcome, OrganizationScimService, OrganizationScimTokenRecord,
    OrganizationScimUserSyncError, OrganizationScimUserSyncOutcome,
};
use af_domain::{
    OrganizationId, OrganizationScimAuditEventType, OrganizationScimAuditFacts,
    OrganizationScimAuditReason, OrganizationScimAuditResource, OrganizationScimAuditResult,
    OrganizationScimGroupResource, OrganizationScimRateLimitDecision,
    OrganizationScimRateLimitRejection, OrganizationScimRequestFingerprint,
    OrganizationScimResourceId, OrganizationScimResourceKind, OrganizationScimResourceVersion,
    OrganizationScimToken, OrganizationScimUserResource,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, Request, State, rejection::JsonRejection},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use http::{
    HeaderMap, HeaderValue, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, WWW_AUTHENTICATE},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

const SERVICE_PROVIDER_CONFIG_SCHEMA: &str =
    "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";
const RESOURCE_TYPE_LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
const RESOURCE_TYPE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ResourceType";
const USER_RESOURCE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP_RESOURCE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";

#[derive(Clone)]
struct ScimHttpState {
    service: Arc<dyn OrganizationScimService>,
}

#[derive(Clone)]
struct ScimAuthentication {
    organization_id: OrganizationId,
    token: OrganizationScimTokenRecord,
    request_fingerprint: OrganizationScimRequestFingerprint,
}

/// 构建企业绑定的 SCIM 发现与目录资源写入路由。
pub fn build_scim_router(service: Arc<dyn OrganizationScimService>) -> Router {
    let state = ScimHttpState { service };
    let authentication = middleware::from_fn_with_state(state.clone(), authenticate_scim);
    Router::new()
        .route(
            "/scim/v2/{organization_id}/ServiceProviderConfig",
            get(get_service_provider_config),
        )
        .route(
            "/scim/v2/{organization_id}/ResourceTypes",
            get(list_resource_types),
        )
        .route("/scim/v2/{organization_id}/Users", post(create_user))
        .route(
            "/scim/v2/{organization_id}/Users/{resource_id}",
            put(replace_user),
        )
        .route("/scim/v2/{organization_id}/Groups", post(create_group))
        .route(
            "/scim/v2/{organization_id}/Groups/{resource_id}",
            put(replace_group).delete(delete_group),
        )
        .route_layer(authentication)
        .with_state(state)
}

async fn authenticate_scim(
    State(state): State<ScimHttpState>,
    mut request: Request,
    next: Next,
) -> Response {
    let organization_id = match parse_organization_id(request.uri().path()) {
        Some(value) => value,
        None => return ScimError::Unauthorized.into_response(),
    };
    let request_fingerprint = scim_request_fingerprint(&request);
    let token = match extract_bearer_token(&mut request) {
        Ok(value) => value,
        Err(error) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                None,
                request_fingerprint,
                OrganizationScimAuditReason::InvalidToken,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return error.into_response();
        }
    };
    let token = match OrganizationScimToken::new(token) {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                None,
                request_fingerprint,
                OrganizationScimAuditReason::InvalidToken,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::Unauthorized.into_response();
        }
    };
    match state.service.authenticate(organization_id, token).await {
        Ok(record) => {
            match state
                .service
                .admit_rate_limit(organization_id, &record)
                .await
            {
                OrganizationScimRateLimitDecision::Admitted { .. } => {}
                OrganizationScimRateLimitDecision::Limited {
                    retry_after_seconds,
                    ..
                } => {
                    let reason = OrganizationScimAuditReason::RateLimitExceeded;
                    let facts = OrganizationScimAuditFacts::new(
                        organization_id,
                        Some(record.token_key().clone()),
                        OrganizationScimAuditEventType::RateLimitRejected,
                        OrganizationScimAuditResult::Rejected,
                        None,
                        request_fingerprint.clone(),
                        Some(reason),
                        Some(retry_after_seconds),
                    );
                    if !record_audit(&state.service, facts).await {
                        return ScimError::Unavailable.into_response();
                    }
                    return ScimError::RateLimited(retry_after_seconds).into_response();
                }
                OrganizationScimRateLimitDecision::Rejected(rejection) => {
                    let reason = match rejection {
                        OrganizationScimRateLimitRejection::StorageUnavailable
                        | OrganizationScimRateLimitRejection::InvalidFact
                        | OrganizationScimRateLimitRejection::InvalidTimestamp => {
                            OrganizationScimAuditReason::StorageUnavailable
                        }
                    };
                    let facts = OrganizationScimAuditFacts::new(
                        organization_id,
                        Some(record.token_key().clone()),
                        OrganizationScimAuditEventType::RequestRejected,
                        OrganizationScimAuditResult::Failed,
                        None,
                        request_fingerprint.clone(),
                        Some(reason),
                        None,
                    );
                    if !record_audit(&state.service, facts).await {
                        return ScimError::Unavailable.into_response();
                    }
                    return ScimError::Unavailable.into_response();
                }
            }
            let accepted = OrganizationScimAuditFacts::new(
                organization_id,
                Some(record.token_key().clone()),
                OrganizationScimAuditEventType::RequestAccepted,
                OrganizationScimAuditResult::Applied,
                None,
                request_fingerprint.clone(),
                None,
                None,
            );
            if !record_audit(&state.service, accepted).await {
                return ScimError::Unavailable.into_response();
            }
            request.extensions_mut().insert(ScimAuthentication {
                organization_id,
                token: record,
                request_fingerprint,
            });
            next.run(request).await
        }
        Err(OrganizationScimAuthenticationError::InvalidToken) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                None,
                request_fingerprint,
                OrganizationScimAuditReason::InvalidToken,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            ScimError::Unauthorized.into_response()
        }
        Err(OrganizationScimAuthenticationError::Internal) => {
            if !record_failed_audit(
                &state.service,
                organization_id,
                None,
                request_fingerprint,
                OrganizationScimAuditReason::InternalFailure,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            ScimError::Unavailable.into_response()
        }
    }
}

async fn create_user(
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<ScimAuthentication>,
    State(state): State<ScimHttpState>,
    request: Result<Json<ScimUserWriteRequest>, JsonRejection>,
) -> Response {
    write_user(
        organization_id,
        None,
        authentication,
        state,
        request,
        StatusCode::CREATED,
    )
    .await
}

async fn replace_user(
    Path((organization_id, resource_id)): Path<(i64, String)>,
    Extension(authentication): Extension<ScimAuthentication>,
    State(state): State<ScimHttpState>,
    request: Result<Json<ScimUserWriteRequest>, JsonRejection>,
) -> Response {
    write_user(
        organization_id,
        Some(resource_id),
        authentication,
        state,
        request,
        StatusCode::OK,
    )
    .await
}

async fn write_user(
    organization_id: i64,
    path_resource_id: Option<String>,
    authentication: ScimAuthentication,
    state: ScimHttpState,
    request: Result<Json<ScimUserWriteRequest>, JsonRejection>,
    success_status: StatusCode,
) -> Response {
    let Some(organization_id) = OrganizationId::new(organization_id).ok() else {
        return ScimError::Unauthorized.into_response();
    };
    if authentication.organization_id != organization_id {
        return ScimError::Unauthorized.into_response();
    }
    let request_fingerprint = authentication.request_fingerprint.clone();
    let token_key = authentication.token.token_key().clone();
    let Json(request) = match request {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let resource_id = match path_resource_id {
        Some(path_id) if path_id == request.id => path_id,
        Some(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::CrossOrganization,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::Conflict.into_response();
        }
        None => request.id.clone(),
    };
    let resource_id = match OrganizationScimResourceId::new(resource_id) {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let version = match request
        .meta
        .version
        .parse::<u64>()
        .ok()
        .and_then(|value| OrganizationScimResourceVersion::new(value).ok())
    {
        Some(value) => value,
        None => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let resource = match OrganizationScimUserResource::new(
        organization_id,
        resource_id,
        version,
        request.user_name.clone(),
        request.display_name.clone(),
        request.active,
    ) {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let audit_resource = audit_resource(
        organization_id,
        OrganizationScimResourceKind::Users,
        resource.key().resource_id(),
        resource.version(),
    );
    let result = match state
        .service
        .synchronize_user(authentication.token, resource)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            if !record_sync_error_audit(
                &state.service,
                organization_id,
                Some(token_key),
                request_fingerprint,
                audit_resource,
                user_sync_audit_reason(error),
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return map_user_sync_error(error).into_response();
        }
    };
    let audit_result = match result.outcome() {
        OrganizationScimUserSyncOutcome::Applied => OrganizationScimAuditResult::Applied,
        OrganizationScimUserSyncOutcome::Noop => OrganizationScimAuditResult::Noop,
    };
    if !record_audit_result(
        &state.service,
        organization_id,
        Some(token_key),
        request_fingerprint,
        audit_resource,
        audit_result,
    )
    .await
    {
        return ScimError::Unavailable.into_response();
    }
    let mut response = scim_json(ScimUserResponse::from_request(&request));
    *response.status_mut() = success_status;
    response
}

fn map_user_sync_error(error: OrganizationScimUserSyncError) -> ScimError {
    match error {
        OrganizationScimUserSyncError::Restricted => ScimError::Unauthorized,
        OrganizationScimUserSyncError::Forbidden => ScimError::Forbidden,
        OrganizationScimUserSyncError::Conflict => ScimError::Conflict,
        OrganizationScimUserSyncError::UserUnavailable => ScimError::NotFound,
        OrganizationScimUserSyncError::LifecycleRejected => ScimError::Unprocessable,
        OrganizationScimUserSyncError::Internal => ScimError::Unavailable,
    }
}

async fn create_group(
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<ScimAuthentication>,
    State(state): State<ScimHttpState>,
    request: Result<Json<ScimGroupWriteRequest>, JsonRejection>,
) -> Response {
    write_group(
        organization_id,
        None,
        authentication,
        state,
        request,
        StatusCode::CREATED,
    )
    .await
}

async fn replace_group(
    Path((organization_id, resource_id)): Path<(i64, String)>,
    Extension(authentication): Extension<ScimAuthentication>,
    State(state): State<ScimHttpState>,
    request: Result<Json<ScimGroupWriteRequest>, JsonRejection>,
) -> Response {
    write_group(
        organization_id,
        Some(resource_id),
        authentication,
        state,
        request,
        StatusCode::OK,
    )
    .await
}

async fn write_group(
    organization_id: i64,
    path_resource_id: Option<String>,
    authentication: ScimAuthentication,
    state: ScimHttpState,
    request: Result<Json<ScimGroupWriteRequest>, JsonRejection>,
    success_status: StatusCode,
) -> Response {
    let Some(organization_id) = OrganizationId::new(organization_id).ok() else {
        return ScimError::Unauthorized.into_response();
    };
    if authentication.organization_id != organization_id {
        return ScimError::Unauthorized.into_response();
    }
    let request_fingerprint = authentication.request_fingerprint.clone();
    let token_key = authentication.token.token_key().clone();
    let Json(request) = match request {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let resource_id = match path_resource_id {
        Some(path_id) if path_id == request.id => path_id,
        Some(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::CrossOrganization,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::Conflict.into_response();
        }
        None => request.id.clone(),
    };
    let resource_id = match OrganizationScimResourceId::new(resource_id) {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let version = match request
        .meta
        .version
        .parse::<u64>()
        .ok()
        .and_then(|value| OrganizationScimResourceVersion::new(value).ok())
    {
        Some(value) => value,
        None => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let member_ids = request
        .members
        .iter()
        .map(|member| OrganizationScimResourceId::new(member.value.clone()))
        .collect::<Result<Vec<_>, _>>();
    let member_ids = match member_ids {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let resource = match OrganizationScimGroupResource::new(
        organization_id,
        resource_id,
        version,
        request.display_name.clone(),
        member_ids,
    ) {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let audit_resource = audit_resource(
        organization_id,
        OrganizationScimResourceKind::Groups,
        resource.key().resource_id(),
        resource.version(),
    );
    let result = match state
        .service
        .synchronize_group(authentication.token, resource)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            if !record_sync_error_audit(
                &state.service,
                organization_id,
                Some(token_key),
                request_fingerprint,
                audit_resource,
                group_sync_audit_reason(error),
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return map_group_sync_error(error).into_response();
        }
    };
    let audit_result = match result.outcome() {
        OrganizationScimGroupSyncOutcome::Applied => OrganizationScimAuditResult::Applied,
        OrganizationScimGroupSyncOutcome::Noop => OrganizationScimAuditResult::Noop,
    };
    if !record_audit_result(
        &state.service,
        organization_id,
        Some(token_key),
        request_fingerprint,
        audit_resource,
        audit_result,
    )
    .await
    {
        return ScimError::Unavailable.into_response();
    }
    let mut response = scim_json(ScimGroupResponse::from_request(&request));
    *response.status_mut() = success_status;
    response
}

async fn delete_group(
    Path((organization_id, resource_id)): Path<(i64, String)>,
    Extension(authentication): Extension<ScimAuthentication>,
    State(state): State<ScimHttpState>,
    headers: HeaderMap,
) -> Response {
    let Some(organization_id) = OrganizationId::new(organization_id).ok() else {
        return ScimError::Unauthorized.into_response();
    };
    if authentication.organization_id != organization_id {
        return ScimError::Unauthorized.into_response();
    }
    let request_fingerprint = authentication.request_fingerprint.clone();
    let token_key = authentication.token.token_key().clone();
    let resource_id = match OrganizationScimResourceId::new(resource_id) {
        Ok(value) => value,
        Err(_) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return ScimError::BadRequest.into_response();
        }
    };
    let expected_version = match parse_if_match_version(&headers) {
        Ok(value) => value,
        Err(error) => {
            if !record_rejected_audit(
                &state.service,
                organization_id,
                Some(token_key.clone()),
                request_fingerprint.clone(),
                OrganizationScimAuditReason::InvalidRequest,
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return error.into_response();
        }
    };
    let audit_resource = audit_resource(
        organization_id,
        OrganizationScimResourceKind::Groups,
        &resource_id,
        expected_version,
    );
    let result = match state
        .service
        .delete_group(authentication.token, resource_id, expected_version)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            if !record_sync_error_audit(
                &state.service,
                organization_id,
                Some(token_key),
                request_fingerprint,
                audit_resource,
                group_sync_audit_reason(error),
            )
            .await
            {
                return ScimError::Unavailable.into_response();
            }
            return map_group_sync_error(error).into_response();
        }
    };
    if !record_audit_result(
        &state.service,
        organization_id,
        Some(token_key),
        request_fingerprint,
        audit_resource,
        match result.outcome() {
            OrganizationScimGroupSyncOutcome::Applied => OrganizationScimAuditResult::Applied,
            OrganizationScimGroupSyncOutcome::Noop => OrganizationScimAuditResult::Noop,
        },
    )
    .await
    {
        return ScimError::Unavailable.into_response();
    }
    scim_empty(StatusCode::NO_CONTENT)
}

fn parse_if_match_version(
    headers: &HeaderMap,
) -> Result<OrganizationScimResourceVersion, ScimError> {
    let mut values = headers.get_all("if-match").iter();
    let Some(value) = values.next() else {
        return Err(ScimError::BadRequest);
    };
    if values.next().is_some() {
        return Err(ScimError::BadRequest);
    }
    let raw = value.to_str().map_err(|_| ScimError::BadRequest)?.trim();
    let raw = raw.strip_prefix("W/").unwrap_or(raw);
    let raw = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(raw);
    if raw.is_empty() || raw.chars().any(|value| !value.is_ascii_digit()) {
        return Err(ScimError::BadRequest);
    }
    raw.parse::<u64>()
        .ok()
        .and_then(|value| OrganizationScimResourceVersion::new(value).ok())
        .ok_or(ScimError::BadRequest)
}

fn scim_request_fingerprint(request: &Request) -> OrganizationScimRequestFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows-scim-request-fingerprint-v1");
    if let Some(request_id) = request.extensions().get::<RequestId>() {
        hasher.update(request_id.as_str().as_bytes());
    } else {
        hasher.update(Uuid::new_v4().as_bytes());
    }
    hasher.update(request.method().as_str().as_bytes());
    hasher.update(request.uri().path().as_bytes());
    OrganizationScimRequestFingerprint::new(hasher.finalize().into())
        .expect("SHA-256 请求指纹不应为全零")
}

fn audit_resource(
    organization_id: OrganizationId,
    kind: OrganizationScimResourceKind,
    resource_id: &OrganizationScimResourceId,
    version: OrganizationScimResourceVersion,
) -> OrganizationScimAuditResource {
    OrganizationScimAuditResource::new(
        kind,
        af_domain::OrganizationScimResourceDigest::from_resource(
            organization_id,
            kind,
            resource_id,
        ),
        version,
    )
}

async fn record_audit(
    service: &Arc<dyn OrganizationScimService>,
    facts: Result<OrganizationScimAuditFacts, af_domain::OrganizationScimAuditFactsError>,
) -> bool {
    let Ok(facts) = facts else {
        return false;
    };
    service.record_audit(facts).await.is_ok()
}

async fn record_rejected_audit(
    service: &Arc<dyn OrganizationScimService>,
    organization_id: OrganizationId,
    token_key: Option<af_domain::OrganizationScimTokenKey>,
    request_fingerprint: OrganizationScimRequestFingerprint,
    reason: OrganizationScimAuditReason,
) -> bool {
    record_audit(
        service,
        OrganizationScimAuditFacts::new(
            organization_id,
            token_key,
            OrganizationScimAuditEventType::RequestRejected,
            OrganizationScimAuditResult::Rejected,
            None,
            request_fingerprint,
            Some(reason),
            None,
        ),
    )
    .await
}

async fn record_failed_audit(
    service: &Arc<dyn OrganizationScimService>,
    organization_id: OrganizationId,
    token_key: Option<af_domain::OrganizationScimTokenKey>,
    request_fingerprint: OrganizationScimRequestFingerprint,
    reason: OrganizationScimAuditReason,
) -> bool {
    record_audit(
        service,
        OrganizationScimAuditFacts::new(
            organization_id,
            token_key,
            OrganizationScimAuditEventType::RequestRejected,
            OrganizationScimAuditResult::Failed,
            None,
            request_fingerprint,
            Some(reason),
            None,
        ),
    )
    .await
}

async fn record_audit_result(
    service: &Arc<dyn OrganizationScimService>,
    organization_id: OrganizationId,
    token_key: Option<af_domain::OrganizationScimTokenKey>,
    request_fingerprint: OrganizationScimRequestFingerprint,
    resource: OrganizationScimAuditResource,
    result: OrganizationScimAuditResult,
) -> bool {
    record_audit(
        service,
        OrganizationScimAuditFacts::new(
            organization_id,
            token_key,
            OrganizationScimAuditEventType::ResourceSynchronized,
            result,
            Some(resource),
            request_fingerprint,
            None,
            None,
        ),
    )
    .await
}

async fn record_sync_error_audit(
    service: &Arc<dyn OrganizationScimService>,
    organization_id: OrganizationId,
    token_key: Option<af_domain::OrganizationScimTokenKey>,
    request_fingerprint: OrganizationScimRequestFingerprint,
    resource: OrganizationScimAuditResource,
    reason: OrganizationScimAuditReason,
) -> bool {
    let result = if matches!(reason, OrganizationScimAuditReason::InternalFailure) {
        OrganizationScimAuditResult::Failed
    } else {
        OrganizationScimAuditResult::Rejected
    };
    record_audit(
        service,
        OrganizationScimAuditFacts::new(
            organization_id,
            token_key,
            OrganizationScimAuditEventType::ResourceSynchronized,
            result,
            Some(resource),
            request_fingerprint,
            Some(reason),
            None,
        ),
    )
    .await
}

fn user_sync_audit_reason(error: OrganizationScimUserSyncError) -> OrganizationScimAuditReason {
    match error {
        OrganizationScimUserSyncError::Restricted => OrganizationScimAuditReason::TokenRevoked,
        OrganizationScimUserSyncError::Forbidden => OrganizationScimAuditReason::InvalidRequest,
        OrganizationScimUserSyncError::Conflict => OrganizationScimAuditReason::StaleResource,
        OrganizationScimUserSyncError::UserUnavailable => {
            OrganizationScimAuditReason::InvalidRequest
        }
        OrganizationScimUserSyncError::LifecycleRejected => {
            OrganizationScimAuditReason::EntitlementUnavailable
        }
        OrganizationScimUserSyncError::Internal => OrganizationScimAuditReason::InternalFailure,
    }
}

fn group_sync_audit_reason(error: OrganizationScimGroupSyncError) -> OrganizationScimAuditReason {
    match error {
        OrganizationScimGroupSyncError::Restricted => OrganizationScimAuditReason::TokenRevoked,
        OrganizationScimGroupSyncError::Forbidden => OrganizationScimAuditReason::InvalidRequest,
        OrganizationScimGroupSyncError::Conflict => OrganizationScimAuditReason::StaleResource,
        OrganizationScimGroupSyncError::MemberUnavailable
        | OrganizationScimGroupSyncError::TeamUnavailable => {
            OrganizationScimAuditReason::InvalidRequest
        }
        OrganizationScimGroupSyncError::LifecycleRejected => {
            OrganizationScimAuditReason::EntitlementUnavailable
        }
        OrganizationScimGroupSyncError::Internal => OrganizationScimAuditReason::InternalFailure,
    }
}

fn map_group_sync_error(error: OrganizationScimGroupSyncError) -> ScimError {
    match error {
        OrganizationScimGroupSyncError::Restricted => ScimError::Unauthorized,
        OrganizationScimGroupSyncError::Forbidden => ScimError::Forbidden,
        OrganizationScimGroupSyncError::Conflict => ScimError::Conflict,
        OrganizationScimGroupSyncError::MemberUnavailable
        | OrganizationScimGroupSyncError::TeamUnavailable => ScimError::NotFound,
        OrganizationScimGroupSyncError::LifecycleRejected => ScimError::Unprocessable,
        OrganizationScimGroupSyncError::Internal => ScimError::Unavailable,
    }
}

fn extract_bearer_token(request: &mut Request) -> Result<String, ScimError> {
    let values = request.headers().get_all(AUTHORIZATION);
    let mut iter = values.iter();
    let Some(value) = iter.next() else {
        return Err(ScimError::Unauthorized);
    };
    if iter.next().is_some() {
        return Err(ScimError::Unauthorized);
    }
    let raw = value.to_str().map_err(|_| ScimError::Unauthorized)?;
    let Some((scheme, token)) = raw.split_once(' ') else {
        return Err(ScimError::Unauthorized);
    };
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err(ScimError::Unauthorized);
    }
    let token = token.to_owned();
    request.headers_mut().remove(AUTHORIZATION);
    Ok(token)
}

fn parse_organization_id(path: &str) -> Option<OrganizationId> {
    let mut segments = path.split('/');
    if segments.next() != Some("")
        || segments.next() != Some("scim")
        || segments.next() != Some("v2")
    {
        return None;
    }
    segments
        .next()?
        .parse()
        .ok()
        .and_then(|value| OrganizationId::new(value).ok())
}

async fn get_service_provider_config(
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<ScimAuthentication>,
) -> Response {
    let Some(organization_id) = OrganizationId::new(organization_id).ok() else {
        return ScimError::Unauthorized.into_response();
    };
    if authentication.organization_id != organization_id {
        return ScimError::Unauthorized.into_response();
    }
    scim_json(ServiceProviderConfigResponse::new())
}

async fn list_resource_types(
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<ScimAuthentication>,
) -> Response {
    let Some(organization_id) = OrganizationId::new(organization_id).ok() else {
        return ScimError::Unauthorized.into_response();
    };
    if authentication.organization_id != organization_id {
        return ScimError::Unauthorized.into_response();
    }
    scim_json(ResourceTypeListResponse::new(organization_id))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ServiceProviderConfigResponse {
    schemas: Vec<String>,
    patch: ScimSupportResponse,
    bulk: ScimBulkResponse,
    filter: ScimFilterResponse,
    change_password: ScimSupportResponse,
    sort: ScimSupportResponse,
    etag: ScimSupportResponse,
    authentication_schemes: Vec<ScimAuthenticationSchemeResponse>,
}

impl ServiceProviderConfigResponse {
    fn new() -> Self {
        Self {
            schemas: vec![SERVICE_PROVIDER_CONFIG_SCHEMA.to_owned()],
            patch: ScimSupportResponse { supported: false },
            bulk: ScimBulkResponse {
                supported: false,
                max_operations: 0,
                max_payload_size: 0,
            },
            filter: ScimFilterResponse {
                supported: false,
                max_results: 0,
            },
            change_password: ScimSupportResponse { supported: false },
            sort: ScimSupportResponse { supported: false },
            etag: ScimSupportResponse { supported: false },
            authentication_schemes: vec![ScimAuthenticationSchemeResponse {
                r#type: "oauthbearertoken".to_owned(),
                name: "SCIM Bearer Token".to_owned(),
                description: "企业绑定的 SCIM Bearer 令牌".to_owned(),
                spec_uri: "https://www.rfc-editor.org/rfc/rfc6750".to_owned(),
                documentation_uri: String::new(),
            }],
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimSupportResponse {
    supported: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimBulkResponse {
    supported: bool,
    max_operations: u32,
    max_payload_size: u32,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimFilterResponse {
    supported: bool,
    max_results: u32,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimAuthenticationSchemeResponse {
    #[serde(rename = "type")]
    r#type: String,
    name: String,
    description: String,
    spec_uri: String,
    documentation_uri: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResourceTypeListResponse {
    schemas: Vec<String>,
    #[serde(rename = "totalResults")]
    total_results: u32,
    #[serde(rename = "Resources")]
    resources: Vec<ResourceTypeResponse>,
    #[serde(rename = "startIndex")]
    start_index: u32,
    #[serde(rename = "itemsPerPage")]
    items_per_page: u32,
}

impl ResourceTypeListResponse {
    fn new(organization_id: OrganizationId) -> Self {
        let base = format!("/scim/v2/{}/", organization_id.get());
        let resources = [
            (
                "ServiceProviderConfig",
                "ServiceProviderConfig",
                SERVICE_PROVIDER_CONFIG_SCHEMA,
            ),
            ("ResourceType", "ResourceTypes", RESOURCE_TYPE_SCHEMA),
            ("User", "Users", USER_RESOURCE_SCHEMA),
            ("Group", "Groups", GROUP_RESOURCE_SCHEMA),
        ]
        .into_iter()
        .map(|(name, endpoint_name, schema)| ResourceTypeResponse {
            id: name.to_owned(),
            name: name.to_owned(),
            endpoint: format!("{base}{endpoint_name}"),
            schema: schema.to_owned(),
        })
        .collect::<Vec<_>>();
        let items_per_page = resources.len() as u32;
        Self {
            schemas: vec![RESOURCE_TYPE_LIST_SCHEMA.to_owned()],
            total_results: items_per_page,
            resources,
            start_index: 1,
            items_per_page,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResourceTypeResponse {
    id: String,
    name: String,
    endpoint: String,
    schema: String,
}

#[derive(serde::Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimUserWriteRequest {
    id: String,
    #[serde(rename = "userName")]
    user_name: String,
    #[serde(default)]
    display_name: Option<String>,
    active: bool,
    meta: ScimUserMeta,
}

#[derive(serde::Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimUserMeta {
    version: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimUserResponse {
    schemas: Vec<String>,
    id: String,
    #[serde(rename = "userName")]
    user_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    active: bool,
    meta: ScimUserMeta,
}

#[derive(serde::Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimGroupWriteRequest {
    id: String,
    display_name: String,
    #[serde(default)]
    members: Vec<ScimGroupMember>,
    meta: ScimGroupMeta,
}

#[derive(Clone, serde::Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimGroupMember {
    value: String,
}

#[derive(serde::Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimGroupMeta {
    version: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimGroupResponse {
    schemas: Vec<String>,
    id: String,
    display_name: String,
    members: Vec<ScimGroupMember>,
    meta: ScimGroupMeta,
}

impl ScimGroupResponse {
    fn from_request(request: &ScimGroupWriteRequest) -> Self {
        Self {
            schemas: vec![GROUP_RESOURCE_SCHEMA.to_owned()],
            id: request.id.clone(),
            display_name: request.display_name.clone(),
            members: request.members.clone(),
            meta: ScimGroupMeta {
                version: request.meta.version.clone(),
            },
        }
    }
}

impl ScimUserResponse {
    fn from_request(request: &ScimUserWriteRequest) -> Self {
        Self {
            schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:User".to_owned()],
            id: request.id.clone(),
            user_name: request.user_name.clone(),
            display_name: request.display_name.clone(),
            active: request.active,
            meta: ScimUserMeta {
                version: request.meta.version.clone(),
            },
        }
    }
}

fn scim_json<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/scim+json"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn scim_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/scim+json"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[derive(Clone, Copy, Debug)]
enum ScimError {
    Unauthorized,
    BadRequest,
    Forbidden,
    Conflict,
    NotFound,
    Unprocessable,
    Unavailable,
    RateLimited(u32),
}

impl IntoResponse for ScimError {
    fn into_response(self) -> Response {
        let (status, detail, challenge) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "SCIM Bearer 令牌无效", true),
            Self::BadRequest => (StatusCode::BAD_REQUEST, "SCIM 目录资源请求无效", false),
            Self::Forbidden => (StatusCode::FORBIDDEN, "SCIM 目录资源范围不足", false),
            Self::Conflict => (StatusCode::CONFLICT, "SCIM 目录资源版本或绑定冲突", false),
            Self::NotFound => (StatusCode::NOT_FOUND, "SCIM 目录资源目标不可用", false),
            Self::Unprocessable => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "SCIM 目录资源生命周期被拒绝",
                false,
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SCIM 认证服务暂不可用",
                false,
            ),
            Self::RateLimited(_) => (
                StatusCode::TOO_MANY_REQUESTS,
                "SCIM 请求频率超出限制",
                false,
            ),
        };
        let mut response = Json(ScimErrorResponse {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:Error".to_owned()],
            detail: detail.to_owned(),
            status: status.as_u16().to_string(),
        })
        .into_response();
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/scim+json"),
        );
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Self::RateLimited(retry_after_seconds) = self
            && let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string())
        {
            response.headers_mut().insert("retry-after", value);
        }
        if challenge {
            response
                .headers_mut()
                .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScimErrorResponse {
    schemas: Vec<String>,
    detail: String,
    status: String,
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};

    use super::*;

    #[test]
    fn bearer_extraction_is_single_header_and_removes_secret() {
        let mut request = Request::builder()
            .header(AUTHORIZATION, "bEaReR scim_token")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&mut request).unwrap(), "scim_token");
        assert!(!request.headers().contains_key(AUTHORIZATION));
        for value in ["Bearer", "Basic token", "Bearer ", "Bearer token value"] {
            let mut request = Request::builder()
                .header(AUTHORIZATION, value)
                .body(Body::empty())
                .unwrap();
            assert!(matches!(
                extract_bearer_token(&mut request),
                Err(ScimError::Unauthorized)
            ));
        }
    }

    #[test]
    fn discovery_responses_are_scim_json_without_resource_payloads() {
        let config = serde_json::to_value(ServiceProviderConfigResponse::new()).unwrap();
        assert_eq!(config["schemas"][0], SERVICE_PROVIDER_CONFIG_SCHEMA);
        assert_eq!(config["patch"]["supported"], false);
        assert!(config.get("email").is_none());
        let resources = ResourceTypeListResponse::new(OrganizationId::new(7).unwrap());
        assert_eq!(resources.total_results, 4);
        assert!(
            resources
                .resources
                .iter()
                .all(|resource| !resource.schema.is_empty())
        );
    }
}
