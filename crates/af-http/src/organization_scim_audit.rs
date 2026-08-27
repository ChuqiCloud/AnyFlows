use std::sync::Arc;

use af_admin::{
    OrganizationAuthorizationService, OrganizationScimAuditQuery, OrganizationScimAuditRecord,
    OrganizationScimService, SessionAuthentication,
};
use af_domain::{
    OrganizationPermission, OrganizationScimAuditEventType, OrganizationScimAuditReason,
    OrganizationScimAuditResult, OrganizationScimResourceKind, OrganizationScimTokenKey,
};
use axum::{
    Extension, Router,
    extract::{Path, RawQuery, State},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{
    HeaderValue,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_usage_logs::{parse_limit, parse_positive_i64, validate_percent_encoding},
    middleware::ensure_request_id,
};

#[derive(Clone)]
struct OrganizationScimAuditHttpState {
    service: Arc<dyn OrganizationScimService>,
    authorization: Arc<dyn OrganizationAuthorizationService>,
}

/// 构建企业范围的 SCIM 脱敏审计只读路由。
pub fn build_organization_scim_audit_router(
    service: Arc<dyn OrganizationScimService>,
    authorization: Arc<dyn OrganizationAuthorizationService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/scim/audit-logs",
            get(list_organization_scim_audit_logs),
        )
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationScimAuditHttpState {
            service,
            authorization,
        })
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrganizationScimAuditLogResponse {
    id: i64,
    token_key: Option<String>,
    event_type: String,
    result: String,
    reason: Option<String>,
    resource_kind: Option<String>,
    resource_digest: Option<String>,
    resource_version: Option<i64>,
    request_fingerprint: String,
    retry_after_seconds: Option<i32>,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OrganizationScimAuditLogListResponse {
    #[schema(max_items = 100)]
    logs: Vec<OrganizationScimAuditLogResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

async fn list_organization_scim_audit_logs(
    State(state): State<OrganizationScimAuditHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = af_domain::OrganizationId::new(organization_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = state
        .authorization
        .context(
            authentication.principal(),
            organization_id,
            OrganizationPermission::AuditRead,
        )
        .await
        .map_err(|error| match error {
            af_admin::OrganizationAuthorizationError::NotFound => {
                ManagementError::OrganizationNotFound
            }
            af_admin::OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
            af_admin::OrganizationAuthorizationError::Internal => ManagementError::Internal,
        })?;
    let query = parse_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_audit(
            organization_id,
            query,
            context.entitlement().audit_retention_days(),
        )
        .await
        .map_err(|_| ManagementError::Internal)?;
    let (records, next_cursor) = page.into_parts();
    Ok(no_store_json(OrganizationScimAuditLogListResponse {
        logs: records
            .into_iter()
            .map(OrganizationScimAuditLogResponse::from_record)
            .collect(),
        next_cursor,
    }))
}

impl OrganizationScimAuditLogResponse {
    fn from_record(record: OrganizationScimAuditRecord) -> Self {
        Self {
            id: record.id(),
            token_key: record
                .token_key()
                .map(OrganizationScimTokenKey::as_str)
                .map(str::to_owned),
            event_type: record.event_type().code().to_owned(),
            result: record.result().code().to_owned(),
            reason: record
                .reason()
                .map(OrganizationScimAuditReason::code)
                .map(str::to_owned),
            resource_kind: record
                .resource_kind()
                .map(resource_kind_code)
                .map(str::to_owned),
            resource_digest: record.resource_digest().map(str::to_owned),
            resource_version: record.resource_version(),
            request_fingerprint: record.request_fingerprint().to_owned(),
            retry_after_seconds: record.retry_after_seconds(),
            created_at: record.created_at(),
        }
    }
}

fn parse_query(raw_query: Option<&str>) -> Result<OrganizationScimAuditQuery, ManagementError> {
    let mut before = None;
    let mut limit = None;
    let mut event_type = None;
    let mut result = None;
    let mut reason = None;
    let mut token_key = None;
    let mut resource_kind = None;
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return OrganizationScimAuditQuery::new(None, 50, None, None, None, None, None)
            .map_err(|_| ManagementError::InvalidRequest);
    };
    validate_percent_encoding(raw_query)?;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        let key = key.into_owned();
        let value = value.into_owned();
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_str() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            "event_type" if event_type.is_none() => event_type = Some(parse_event_type(&value)?),
            "result" if result.is_none() => result = Some(parse_result(&value)?),
            "reason" if reason.is_none() => reason = Some(parse_reason(&value)?),
            "token_key" if token_key.is_none() => {
                token_key = Some(
                    OrganizationScimTokenKey::new(value)
                        .map_err(|_| ManagementError::InvalidRequest)?,
                )
            }
            "resource_kind" if resource_kind.is_none() => {
                resource_kind = Some(parse_resource_kind(&value)?)
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    OrganizationScimAuditQuery::new(
        before,
        limit.unwrap_or(50),
        event_type,
        result,
        reason,
        token_key,
        resource_kind,
    )
    .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_event_type(value: &str) -> Result<OrganizationScimAuditEventType, ManagementError> {
    [
        OrganizationScimAuditEventType::TokenIssued,
        OrganizationScimAuditEventType::TokenRevoked,
        OrganizationScimAuditEventType::RequestAccepted,
        OrganizationScimAuditEventType::RequestRejected,
        OrganizationScimAuditEventType::ResourceSynchronized,
        OrganizationScimAuditEventType::LifecycleTransition,
        OrganizationScimAuditEventType::RateLimitRejected,
    ]
    .into_iter()
    .find(|item| item.code() == value)
    .ok_or(ManagementError::InvalidRequest)
}

fn parse_result(value: &str) -> Result<OrganizationScimAuditResult, ManagementError> {
    [
        OrganizationScimAuditResult::Applied,
        OrganizationScimAuditResult::Noop,
        OrganizationScimAuditResult::Rejected,
        OrganizationScimAuditResult::Failed,
    ]
    .into_iter()
    .find(|item| item.code() == value)
    .ok_or(ManagementError::InvalidRequest)
}

fn parse_reason(value: &str) -> Result<OrganizationScimAuditReason, ManagementError> {
    [
        OrganizationScimAuditReason::InvalidToken,
        OrganizationScimAuditReason::TokenRevoked,
        OrganizationScimAuditReason::TokenExpired,
        OrganizationScimAuditReason::CrossOrganization,
        OrganizationScimAuditReason::StaleResource,
        OrganizationScimAuditReason::EntitlementUnavailable,
        OrganizationScimAuditReason::SeatCapacityExceeded,
        OrganizationScimAuditReason::RateLimitExceeded,
        OrganizationScimAuditReason::StorageUnavailable,
        OrganizationScimAuditReason::InvalidRequest,
        OrganizationScimAuditReason::InternalFailure,
    ]
    .into_iter()
    .find(|item| item.code() == value)
    .ok_or(ManagementError::InvalidRequest)
}

fn parse_resource_kind(value: &str) -> Result<OrganizationScimResourceKind, ManagementError> {
    match value {
        "users" => Ok(OrganizationScimResourceKind::Users),
        "groups" => Ok(OrganizationScimResourceKind::Groups),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn resource_kind_code(value: OrganizationScimResourceKind) -> &'static str {
    match value {
        OrganizationScimResourceKind::Users => "users",
        OrganizationScimResourceKind::Groups => "groups",
    }
}

fn no_store_json<T: Serialize>(value: T) -> Response {
    let mut response = axum::Json(value).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
