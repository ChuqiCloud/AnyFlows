use std::sync::Arc;

use af_admin::{
    AdminUsageLogListQuery, OrganizationActivityError, OrganizationActivityService,
    OrganizationAuditChangeSummary, OrganizationAuditListQuery, OrganizationAuditLog,
    OrganizationAuditOutcome, OrganizationAuditRequestContext, OrganizationAuthorizationError,
    OrganizationAuthorizationService, OrganizationContext, OrganizationUsageLogPage,
    OrganizationUsageLogRows, OrganizationUsageLogScope, OrganizationUsageLogSummary,
    SessionAuthentication, SessionAuthenticator,
};
use af_domain::{OrganizationId, OrganizationPermission, OrganizationRole, UserId};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{
    HeaderMap, HeaderName, HeaderValue,
    header::{CACHE_CONTROL, USER_AGENT},
};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_usage_logs::{
        AdminFailedCallLogResponse, AdminUsageLogResponse, parse_limit, parse_positive_i64,
        validate_percent_encoding,
    },
    middleware::ensure_request_id,
};

pub(crate) const PLATFORM_REASON_HEADER: &str = "x-anyflows-platform-reason";

#[derive(Clone)]
struct OrganizationActivityHttpState {
    service: Arc<dyn OrganizationActivityService>,
}

/// 构建企业活动读取接口；平台代理入口与企业成员入口保持独立。
pub fn build_organization_activity_router(
    service: Arc<dyn OrganizationActivityService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/usage-logs",
            get(list_organization_usage_logs),
        )
        .route(
            "/api/organizations/{organization_id}/audit-logs",
            get(list_organization_audit_logs),
        )
        .route(
            "/api/platform/organizations/{organization_id}/usage-logs",
            get(list_platform_organization_usage_logs),
        )
        .route(
            "/api/platform/organizations/{organization_id}/audit-logs",
            get(list_platform_organization_audit_logs),
        )
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationActivityHttpState { service })
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationUsageLogListResponse {
    scope: &'static str,
    #[schema(max_items = 100)]
    logs: Vec<AdminUsageLogResponse>,
    #[schema(max_items = 100)]
    summaries: Vec<OrganizationUsageLogSummaryResponse>,
    #[schema(max_items = 100)]
    failed_logs: Vec<AdminFailedCallLogResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
    #[schema(minimum = 1, required = true)]
    failed_next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationUsageLogSummaryResponse {
    #[schema(required = true)]
    model: Option<String>,
    #[schema(
        value_type = Option<crate::openapi::schema::UsageLogProtocolSchema>,
        required = true
    )]
    protocol: Option<af_admin::UsageLogProtocol>,
    #[schema(
        value_type = Option<crate::openapi::schema::UsageLogOperationSchema>,
        required = true
    )]
    operation: Option<af_admin::UsageLogOperation>,
    #[schema(minimum = 1)]
    request_count: i64,
    #[schema(minimum = 0)]
    input_tokens: i64,
    #[schema(minimum = 0)]
    output_tokens: i64,
    #[schema(minimum = 0)]
    reasoning_tokens: i64,
    #[schema(minimum = 0)]
    quota: i64,
    #[schema(minimum = 0)]
    duration_ms: i64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrganizationAuditOutcomeResponse {
    Succeeded,
    Denied,
    Failed,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationAuditLogResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    actor_user_id: i64,
    actor_username: String,
    #[schema(required = true)]
    actor_role: Option<&'static str>,
    permission_code: String,
    operation: String,
    target_type: String,
    #[schema(required = true)]
    target_identifier: Option<String>,
    outcome: OrganizationAuditOutcomeResponse,
    platform_proxy: bool,
    #[schema(required = true)]
    platform_reason: Option<String>,
    request_id: String,
    #[schema(required = true)]
    client_fingerprint: Option<String>,
    #[schema(required = true)]
    change_summary: Option<String>,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationAuditLogListResponse {
    #[schema(max_items = 100)]
    logs: Vec<OrganizationAuditLogResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

async fn list_organization_usage_logs(
    State(state): State<OrganizationActivityHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let (scope, query) = parse_usage_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_usage(authentication.principal(), organization_id, scope, query)
        .await
        .map_err(map_activity_error)?;
    Ok(no_store_json(OrganizationUsageLogListResponse::from_page(
        page,
    )))
}

async fn list_platform_organization_usage_logs(
    State(state): State<OrganizationActivityHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let query = parse_usage_pagination(raw_query.as_deref())?;
    let reason = platform_reason(&headers)?;
    let request = audit_request_context(&request_id, &headers)?;
    let page = state
        .service
        .list_usage_as_platform(
            authentication.principal(),
            organization_id,
            query,
            reason,
            request,
        )
        .await
        .map_err(map_activity_error)?;
    Ok(no_store_json(OrganizationUsageLogListResponse::from_page(
        page,
    )))
}

async fn list_organization_audit_logs(
    State(state): State<OrganizationActivityHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let query = parse_audit_query(raw_query.as_deref())?;
    let request = audit_request_context(&request_id, &headers)?;
    let page = state
        .service
        .list_audit(authentication.principal(), organization_id, query, request)
        .await
        .map_err(map_activity_error)?;
    Ok(no_store_json(OrganizationAuditLogListResponse {
        logs: page
            .records()
            .iter()
            .map(OrganizationAuditLogResponse::from_log)
            .collect(),
        next_cursor: page.next_cursor(),
    }))
}

async fn list_platform_organization_audit_logs(
    State(state): State<OrganizationActivityHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let query = parse_audit_query(raw_query.as_deref())?;
    let reason = platform_reason(&headers)?;
    let request = audit_request_context(&request_id, &headers)?;
    let page = state
        .service
        .list_audit_as_platform(
            authentication.principal(),
            organization_id,
            query,
            reason,
            request,
        )
        .await
        .map_err(map_activity_error)?;
    Ok(no_store_json(OrganizationAuditLogListResponse {
        logs: page
            .records()
            .iter()
            .map(OrganizationAuditLogResponse::from_log)
            .collect(),
        next_cursor: page.next_cursor(),
    }))
}

impl OrganizationUsageLogListResponse {
    fn from_page(page: OrganizationUsageLogPage) -> Self {
        let (logs, summaries) = match page.rows() {
            OrganizationUsageLogRows::Detailed(logs) => (
                logs.iter().map(AdminUsageLogResponse::from_log).collect(),
                Vec::new(),
            ),
            OrganizationUsageLogRows::Summary(summaries) => (
                Vec::new(),
                summaries
                    .iter()
                    .map(OrganizationUsageLogSummaryResponse::from_summary)
                    .collect(),
            ),
        };
        let failed_logs = page
            .failed_logs()
            .iter()
            .map(AdminFailedCallLogResponse::from_log)
            .collect();
        Self {
            scope: match page.scope() {
                OrganizationUsageLogScope::Organization => "organization",
                OrganizationUsageLogScope::SelfOnly => "self",
                OrganizationUsageLogScope::Summary => "summary",
            },
            logs,
            summaries,
            failed_logs,
            next_cursor: page.next_cursor(),
            failed_next_cursor: page.failed_next_cursor(),
        }
    }
}

impl OrganizationUsageLogSummaryResponse {
    fn from_summary(summary: &OrganizationUsageLogSummary) -> Self {
        Self {
            model: summary.model().map(str::to_owned),
            protocol: summary.protocol(),
            operation: summary.operation(),
            request_count: summary.request_count(),
            input_tokens: summary.input_tokens(),
            output_tokens: summary.output_tokens(),
            reasoning_tokens: summary.reasoning_tokens(),
            quota: summary.quota(),
            duration_ms: summary.duration_ms(),
        }
    }
}

impl OrganizationAuditLogResponse {
    fn from_log(log: &OrganizationAuditLog) -> Self {
        Self {
            id: log.id(),
            actor_user_id: log.actor_user_id().get(),
            actor_username: log.actor_username().to_owned(),
            actor_role: log.actor_role().map(organization_role_code),
            permission_code: log.permission_code().to_owned(),
            operation: log.operation().to_owned(),
            target_type: log.target_type().to_owned(),
            target_identifier: log.target_identifier().map(str::to_owned),
            outcome: match log.outcome() {
                OrganizationAuditOutcome::Succeeded => OrganizationAuditOutcomeResponse::Succeeded,
                OrganizationAuditOutcome::Denied => OrganizationAuditOutcomeResponse::Denied,
                OrganizationAuditOutcome::Failed => OrganizationAuditOutcomeResponse::Failed,
            },
            platform_proxy: log.platform_proxy(),
            platform_reason: log.platform_reason().map(str::to_owned),
            request_id: log.request_id().to_owned(),
            client_fingerprint: log.client_fingerprint().map(str::to_owned),
            change_summary: log.change_summary().map(str::to_owned),
            created_at: log.created_at(),
        }
    }
}

fn parse_usage_query(
    raw_query: Option<&str>,
) -> Result<(OrganizationUsageLogScope, AdminUsageLogListQuery), ManagementError> {
    let mut scope = None;
    let mut pagination = Vec::new();
    for (key, value) in parse_query_pairs(raw_query)? {
        match key.as_str() {
            "scope" if scope.is_none() => {
                scope = Some(match value.as_str() {
                    "organization" => OrganizationUsageLogScope::Organization,
                    "self" => OrganizationUsageLogScope::SelfOnly,
                    "summary" => OrganizationUsageLogScope::Summary,
                    _ => return Err(ManagementError::InvalidRequest),
                });
            }
            "scope" => return Err(ManagementError::InvalidRequest),
            "before" | "failed_before" | "limit" => pagination.push((key, value)),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok((
        scope.unwrap_or(OrganizationUsageLogScope::SelfOnly),
        pagination_query(pagination)?,
    ))
}

fn parse_usage_pagination(
    raw_query: Option<&str>,
) -> Result<AdminUsageLogListQuery, ManagementError> {
    pagination_query(parse_query_pairs(raw_query)?)
}

fn pagination_query(
    pairs: Vec<(String, String)>,
) -> Result<AdminUsageLogListQuery, ManagementError> {
    let mut before = None;
    let mut failed_before = None;
    let mut limit = None;
    for (key, value) in pairs {
        match key.as_str() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "failed_before" if failed_before.is_none() => {
                failed_before = Some(parse_positive_i64(&value)?)
            }
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminUsageLogListQuery::new_with_failed_before(
        before,
        failed_before,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE),
    )
    .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_audit_query(
    raw_query: Option<&str>,
) -> Result<OrganizationAuditListQuery, ManagementError> {
    let mut before = None;
    let mut limit = None;
    let mut actor_user_id = None;
    let mut operation = None;
    let mut target_type = None;
    let mut outcome = None;
    for (key, value) in parse_query_pairs(raw_query)? {
        match key.as_str() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            "actor_user_id" if actor_user_id.is_none() => {
                actor_user_id = Some(
                    UserId::new(parse_positive_i64(&value)?)
                        .map_err(|_| ManagementError::InvalidRequest)?,
                );
            }
            "operation" if operation.is_none() => operation = Some(value),
            "target_type" if target_type.is_none() => target_type = Some(value),
            "outcome" if outcome.is_none() => {
                outcome = Some(match value.as_str() {
                    "succeeded" => OrganizationAuditOutcome::Succeeded,
                    "denied" => OrganizationAuditOutcome::Denied,
                    "failed" => OrganizationAuditOutcome::Failed,
                    _ => return Err(ManagementError::InvalidRequest),
                });
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    OrganizationAuditListQuery::new(
        before,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE),
        actor_user_id,
        operation,
        target_type,
        outcome,
    )
    .map_err(map_activity_error)
}

fn parse_query_pairs(raw_query: Option<&str>) -> Result<Vec<(String, String)>, ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    validate_percent_encoding(raw_query)?;
    url::form_urlencoded::parse(raw_query.as_bytes())
        .map(|(key, value)| {
            if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
                return Err(ManagementError::InvalidRequest);
            }
            Ok((key.into_owned(), value.into_owned()))
        })
        .collect()
}

fn parse_organization_id(value: i64) -> Result<OrganizationId, ManagementError> {
    OrganizationId::new(value).map_err(|_| ManagementError::InvalidRequest)
}

pub(crate) fn platform_reason(headers: &HeaderMap) -> Result<String, ManagementError> {
    let name = HeaderName::from_static(PLATFORM_REASON_HEADER);
    let mut values = headers.get_all(name).iter();
    let value = values.next().ok_or(ManagementError::InvalidRequest)?;
    if values.next().is_some() {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .to_str()
        .map(str::to_owned)
        .map_err(|_| ManagementError::InvalidRequest)
}

pub(crate) fn audit_request_context(
    request_id: &RequestId,
    headers: &HeaderMap,
) -> Result<OrganizationAuditRequestContext, ManagementError> {
    let client_hint = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok());
    OrganizationAuditRequestContext::new(request_id.as_str(), client_hint)
        .map_err(map_activity_error)
}

/// 为已经通过企业授权的写操作追加一条有限、脱敏的管理审计。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_organization_action(
    service: &Arc<dyn OrganizationActivityService>,
    context: OrganizationContext,
    request_id: &RequestId,
    headers: &HeaderMap,
    permission_code: &'static str,
    operation: &'static str,
    target_type: &'static str,
    target_identifier: String,
    outcome: OrganizationAuditOutcome,
    summary: Option<Value>,
) -> Result<(), ManagementError> {
    let request = audit_request_context(request_id, headers)?;
    let summary = summary
        .map(OrganizationAuditChangeSummary::new)
        .transpose()
        .map_err(map_activity_error)?;
    service
        .record_member_action(
            context,
            request,
            permission_code,
            operation,
            target_type,
            target_identifier,
            outcome,
            summary,
        )
        .await
        .map_err(map_activity_error)
}

/// 解析企业成员上下文，并把活动成员的权限拒绝写入管理审计。
#[allow(
    clippy::too_many_arguments,
    reason = "参数与企业授权和审计事实一一对应"
)]
pub(crate) async fn authorize_organization_action(
    authorization_service: &Arc<dyn OrganizationAuthorizationService>,
    activity_service: &Arc<dyn OrganizationActivityService>,
    authentication: &SessionAuthentication,
    organization_id: OrganizationId,
    permission: OrganizationPermission,
    request_id: &RequestId,
    headers: &HeaderMap,
    operation: &'static str,
    target_type: &'static str,
    target_identifier: String,
) -> Result<OrganizationContext, ManagementError> {
    let context = authorization_service
        .context(
            authentication.principal(),
            organization_id,
            OrganizationPermission::OverviewRead,
        )
        .await
        .map_err(map_authorization_error)?;
    if context.allows(permission) {
        return Ok(context);
    }
    record_organization_action(
        activity_service,
        context,
        request_id,
        headers,
        permission.code(),
        operation,
        target_type,
        target_identifier,
        OrganizationAuditOutcome::Denied,
        None,
    )
    .await?;
    Err(ManagementError::Forbidden)
}

/// 保留原业务错误，同时为已通过授权但未完成的写操作追加失败审计。
#[allow(clippy::too_many_arguments, reason = "参数与企业审计事实一一对应")]
pub(crate) async fn audit_organization_action_result<T>(
    activity_service: &Arc<dyn OrganizationActivityService>,
    context: OrganizationContext,
    request_id: &RequestId,
    headers: &HeaderMap,
    permission: OrganizationPermission,
    operation: &'static str,
    target_type: &'static str,
    target_identifier: String,
    result: Result<T, ManagementError>,
) -> Result<T, ManagementError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            record_organization_action(
                activity_service,
                context,
                request_id,
                headers,
                permission.code(),
                operation,
                target_type,
                target_identifier,
                OrganizationAuditOutcome::Failed,
                None,
            )
            .await?;
            Err(error)
        }
    }
}

fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}

fn organization_role_code(role: OrganizationRole) -> &'static str {
    match role {
        OrganizationRole::Owner => "owner",
        OrganizationRole::Admin => "admin",
        OrganizationRole::Billing => "billing",
        OrganizationRole::Member => "member",
        OrganizationRole::Viewer => "viewer",
    }
}

fn map_activity_error(error: OrganizationActivityError) -> ManagementError {
    match error {
        OrganizationActivityError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationActivityError::Forbidden => ManagementError::Forbidden,
        OrganizationActivityError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationActivityError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_activity_query_parser_is_strict() {
        let (scope, query) = parse_usage_query(Some("scope=summary&before=9&limit=100")).unwrap();
        assert_eq!(scope, OrganizationUsageLogScope::Summary);
        assert_eq!(query.before(), Some(9));
        for query in [
            "scope=summary&scope=self",
            "scope=all",
            "before=0",
            "limit=101",
            "unknown=1",
            "before=%",
        ] {
            assert!(parse_usage_query(Some(query)).is_err());
        }
        assert!(parse_audit_query(Some("actor_user_id=1&outcome=denied&target_type=team")).is_ok());
    }

    #[test]
    fn platform_reason_requires_one_utf8_header() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            platform_reason(&headers),
            Err(ManagementError::InvalidRequest)
        );
        headers.insert(
            HeaderName::from_static(PLATFORM_REASON_HEADER),
            HeaderValue::from_static("support-ticket-1"),
        );
        assert_eq!(platform_reason(&headers).unwrap(), "support-ticket-1");
    }
}
