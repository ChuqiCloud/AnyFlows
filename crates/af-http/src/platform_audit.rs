use std::sync::Arc;

use af_admin::{
    PlatformAuditError, PlatformAuditListQuery, PlatformAuditLog, PlatformAuditOutcome,
    PlatformAuditScope, PlatformAuditService,
};
use axum::{
    Router,
    extract::{Extension, RawQuery, State},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
};

#[derive(Clone)]
struct PlatformAuditHttpState {
    service: Arc<dyn PlatformAuditService>,
}

/// 构建平台审计管理员视图和当前用户裁剪视图。
pub(crate) fn build_platform_audit_router(
    service: Arc<dyn PlatformAuditService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/audit-logs",
            get(list_admin_platform_audit_logs).route_layer(authentication.clone()),
        )
        .route(
            "/api/account/audit-logs",
            get(list_self_platform_audit_logs).route_layer(authentication),
        )
        .with_state(PlatformAuditHttpState { service })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = PlatformAuditOutcome)]
pub(crate) enum PlatformAuditOutcomeResponse {
    Succeeded,
    Denied,
    Failed,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlatformAuditLog)]
pub(crate) struct PlatformAuditLogResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    operator_user_id: i64,
    #[schema(min_length = 1, max_length = 64, required = true)]
    operator_username: Option<String>,
    #[schema(min_length = 1, max_length = 96)]
    permission_code: String,
    #[schema(min_length = 1, max_length = 128)]
    route: String,
    #[schema(min_length = 1, max_length = 96)]
    operation: String,
    #[schema(min_length = 1, max_length = 64)]
    resource: String,
    #[schema(min_length = 1, max_length = 128, required = true)]
    resource_id: Option<String>,
    outcome: PlatformAuditOutcomeResponse,
    #[schema(value_type = Option<Object>, required = true)]
    before_value: Option<Value>,
    #[schema(value_type = Option<Object>, required = true)]
    after_value: Option<Value>,
    #[schema(value_type = Option<Object>, required = true)]
    audit_info: Option<Value>,
    #[schema(min_length = 1, max_length = 128)]
    request_id: String,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlatformAuditLogListResponse)]
pub(crate) struct PlatformAuditLogListResponse {
    #[schema(max_items = 100)]
    logs: Vec<PlatformAuditLogResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

async fn list_admin_platform_audit_logs(
    State(state): State<PlatformAuditHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    list_platform_audit_logs(
        &state.service,
        authentication.principal(),
        PlatformAuditScope::All,
        raw_query.as_deref(),
    )
    .await
}

async fn list_self_platform_audit_logs(
    State(state): State<PlatformAuditHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    list_platform_audit_logs(
        &state.service,
        authentication.principal(),
        PlatformAuditScope::SelfOnly,
        raw_query.as_deref(),
    )
    .await
}

async fn list_platform_audit_logs(
    service: &Arc<dyn PlatformAuditService>,
    principal: af_admin::SessionPrincipal,
    scope: PlatformAuditScope,
    raw_query: Option<&str>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query)?;
    let page = service
        .list(principal, scope, query)
        .await
        .map_err(map_platform_audit_error)?;
    let response = PlatformAuditLogListResponse {
        logs: page
            .records()
            .iter()
            .map(PlatformAuditLogResponse::from_log)
            .collect::<Result<_, _>>()?,
        next_cursor: page.next_cursor(),
    };
    let mut response = axum::Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

impl PlatformAuditLogResponse {
    fn from_log(log: &PlatformAuditLog) -> Result<Self, ManagementError> {
        Ok(Self {
            id: log.id(),
            operator_user_id: log.operator_user_id().get(),
            operator_username: log.operator_username().map(str::to_owned),
            permission_code: log.permission_code().to_owned(),
            route: log.route().to_owned(),
            operation: log.operation().to_owned(),
            resource: log.resource().to_owned(),
            resource_id: log.resource_id().map(str::to_owned),
            outcome: match log.outcome() {
                PlatformAuditOutcome::Succeeded => PlatformAuditOutcomeResponse::Succeeded,
                PlatformAuditOutcome::Denied => PlatformAuditOutcomeResponse::Denied,
                PlatformAuditOutcome::Failed => PlatformAuditOutcomeResponse::Failed,
            },
            before_value: parse_optional_json(log.before_value())?,
            after_value: parse_optional_json(log.after_value())?,
            audit_info: parse_optional_json(log.audit_info())?,
            request_id: log.request_id().to_owned(),
            created_at: log.created_at(),
        })
    }
}

fn parse_optional_json(value: Option<&str>) -> Result<Option<Value>, ManagementError> {
    value
        .map(|value| serde_json::from_str(value).map_err(|_| ManagementError::Internal))
        .transpose()
}

fn parse_list_query(raw_query: Option<&str>) -> Result<PlatformAuditListQuery, ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok(PlatformAuditListQuery::default());
    };
    let mut before = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_usize(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    PlatformAuditListQuery::new(
        before,
        limit.unwrap_or(af_admin::DEFAULT_PLATFORM_AUDIT_PAGE_SIZE),
    )
    .map_err(map_platform_audit_error)
}

fn parse_positive_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_usize(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn map_platform_audit_error(error: PlatformAuditError) -> ManagementError {
    match error {
        PlatformAuditError::InvalidInput => ManagementError::InvalidRequest,
        PlatformAuditError::Forbidden => ManagementError::Forbidden,
        PlatformAuditError::Internal => ManagementError::Internal,
    }
}
