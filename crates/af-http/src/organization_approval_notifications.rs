use std::sync::Arc;

use af_admin::{
    DEFAULT_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE, OrganizationApprovalNotification,
    OrganizationApprovalNotificationCursor, OrganizationApprovalNotificationDecision,
    OrganizationApprovalNotificationError, OrganizationApprovalNotificationListQuery,
    OrganizationApprovalNotificationPage, OrganizationApprovalNotificationService,
};
use axum::{
    Router,
    extract::{Extension, Json, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct HttpState {
    service: Arc<dyn OrganizationApprovalNotificationService>,
}

/// 构建当前用户企业审批通知读取与已读接口；不混用 SMTP 通知事实。
pub(crate) fn build_organization_approval_notification_router(
    service: Arc<dyn OrganizationApprovalNotificationService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/account/organization-approval-notifications",
            get(list_notifications),
        )
        .route(
            "/api/account/organization-approval-notifications/read",
            post(mark_notifications_read),
        )
        .layer(authentication)
        .with_state(HttpState { service })
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = OrganizationApprovalNotificationDecision)]
pub(crate) enum DecisionResponse {
    Approved,
    Rejected,
    TimedOut,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationApprovalNotification)]
pub(crate) struct NotificationResponse {
    id: i64,
    organization_id: i64,
    request_id: i64,
    decision_id: i64,
    decision: DecisionResponse,
    template_version: String,
    occurred_at: i64,
    read_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationApprovalNotificationListResponse)]
pub(crate) struct ListResponse {
    entries: Vec<NotificationResponse>,
    next_cursor: Option<String>,
    unread_count: u64,
}

#[derive(serde::Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationApprovalNotificationMarkReadRequest)]
pub(crate) struct MarkReadRequest {
    #[schema(min_items = 1, max_items = 100)]
    notification_ids: Vec<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationApprovalNotificationMarkReadResponse)]
pub(crate) struct MarkReadResponse {
    marked_count: usize,
    unread_count: u64,
}

async fn list_notifications(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let (before, limit) = parse_query(raw_query.as_deref())?;
    let query = OrganizationApprovalNotificationListQuery::new(before, limit)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ListResponse::from_page(&page)))
}

async fn mark_notifications_read(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<MarkReadRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let result = state
        .service
        .mark_read(authentication.principal(), request.notification_ids)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(MarkReadResponse {
        marked_count: result.marked_count(),
        unread_count: result.unread_count(),
    }))
}

fn parse_query(
    raw_query: Option<&str>,
) -> Result<(Option<OrganizationApprovalNotificationCursor>, usize), ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok((None, DEFAULT_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE));
    };
    validate_percent_encoding(raw_query)?;
    let mut before = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" if before.is_none() => before = Some(parse_cursor(&value)?),
            "limit" if limit.is_none() => {
                limit = Some(parse_positive_usize(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok((
        before,
        limit.unwrap_or(DEFAULT_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE),
    ))
}

fn parse_cursor(value: &str) -> Result<OrganizationApprovalNotificationCursor, ManagementError> {
    let (occurred_at, id) = value
        .split_once(':')
        .ok_or(ManagementError::InvalidRequest)?;
    let occurred_at = parse_positive_i64(occurred_at)?;
    let id = parse_positive_i64(id)?;
    OrganizationApprovalNotificationCursor::new(occurred_at, id)
        .map_err(|_| ManagementError::InvalidRequest)
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

fn parse_positive_usize(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

impl ListResponse {
    fn from_page(page: &OrganizationApprovalNotificationPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(NotificationResponse::from_notification)
                .collect(),
            next_cursor: page
                .next_cursor()
                .map(|cursor| format!("{}:{}", cursor.occurred_at(), cursor.id())),
            unread_count: page.unread_count(),
        }
    }
}

impl NotificationResponse {
    fn from_notification(notification: &OrganizationApprovalNotification) -> Self {
        Self {
            id: notification.id(),
            organization_id: notification.organization_id(),
            request_id: notification.request_id(),
            decision_id: notification.decision_id(),
            decision: match notification.decision() {
                OrganizationApprovalNotificationDecision::Approved => DecisionResponse::Approved,
                OrganizationApprovalNotificationDecision::Rejected => DecisionResponse::Rejected,
                OrganizationApprovalNotificationDecision::TimedOut => DecisionResponse::TimedOut,
            },
            template_version: notification.template_version().to_owned(),
            occurred_at: notification.occurred_at(),
            read_at: notification.read_at(),
        }
    }
}

fn map_error(error: OrganizationApprovalNotificationError) -> ManagementError {
    match error {
        OrganizationApprovalNotificationError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationApprovalNotificationError::Internal => ManagementError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_accepts_encoded_composite_cursor() {
        let (before, limit) = parse_query(Some("before=1700000000%3A42&limit=10"))
            .expect("编码后的复合游标必须可解析");
        let before = before.expect("必须返回游标");
        assert_eq!(before.occurred_at(), 1_700_000_000);
        assert_eq!(before.id(), 42);
        assert_eq!(limit, 10);
    }

    #[test]
    fn query_rejects_duplicate_and_malformed_values() {
        for query in [
            "before=1700000000:42&before=1700000000:41",
            "limit=10&limit=20",
            "before=1700000000%3",
            "before=1700000000:not-a-number",
            "limit=-1",
        ] {
            assert!(matches!(
                parse_query(Some(query)),
                Err(ManagementError::InvalidRequest)
            ));
        }
    }
}
