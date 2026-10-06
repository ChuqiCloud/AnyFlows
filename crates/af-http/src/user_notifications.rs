use std::sync::Arc;

use af_admin::{
    DEFAULT_USER_NOTIFICATION_PAGE_SIZE, SessionAuthentication, SessionAuthenticator,
    UserNotification, UserNotificationChannel, UserNotificationCursor,
    UserNotificationDeliveryState, UserNotificationError, UserNotificationKind,
    UserNotificationListQuery, UserNotificationMarkReadCommand, UserNotificationPage,
    UserNotificationService,
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
pub(crate) struct UserNotificationHttpState {
    service: Arc<dyn UserNotificationService>,
}

pub(crate) fn build_user_notification_router(
    service: Arc<dyn UserNotificationService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/account/notifications", get(list_user_notifications))
        .route(
            "/api/account/notifications/read",
            post(mark_user_notifications_read),
        )
        .layer(authentication)
        .with_state(UserNotificationHttpState { service })
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = NotificationKind)]
pub(crate) enum NotificationKindResponse {
    BalanceAlert,
    SubscriptionBalanceAlert,
    SubscriptionPurchase,
    ProductUpdate,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = NotificationChannel)]
pub(crate) enum NotificationChannelResponse {
    Email,
    InApp,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = NotificationDeliveryState)]
pub(crate) enum NotificationDeliveryStateResponse {
    Queued,
    Accepted,
    Failed,
    Canceled,
    Available,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserNotification)]
pub(crate) struct UserNotificationResponse {
    id: i64,
    kind: NotificationKindResponse,
    channel: NotificationChannelResponse,
    template_version: String,
    occurred_at: i64,
    delivery_state: NotificationDeliveryStateResponse,
    delivery_attempts: i16,
    observed_quota: Option<i64>,
    threshold_quota: Option<i64>,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    subscription_id: Option<String>,
    window_ends_at: Option<i64>,
    quota_amount: Option<i64>,
    quota_used: Option<i64>,
    threshold_percent: Option<i16>,
    read_at: Option<i64>,
    announcement_id: Option<i64>,
    announcement_version: Option<i64>,
    announcement_title_zh: Option<String>,
    announcement_title_en: Option<String>,
    announcement_body_zh: Option<String>,
    announcement_body_en: Option<String>,
    announcement_status: Option<i16>,
    announcement_visible_until: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserNotificationListResponse)]
pub(crate) struct UserNotificationListResponse {
    entries: Vec<UserNotificationResponse>,
    next_cursor: Option<String>,
    unread_count: u64,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserNotificationMarkReadRequest)]
pub(crate) struct UserNotificationMarkReadRequest {
    #[schema(min_items = 1, max_items = 100)]
    notification_ids: Vec<i64>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserNotificationMarkReadResponse)]
pub(crate) struct UserNotificationMarkReadResponse {
    marked_count: usize,
    unread_count: u64,
}

pub(crate) async fn list_user_notifications(
    State(state): State<UserNotificationHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let (before, limit) = parse_query(raw_query.as_deref())?;
    let query = UserNotificationListQuery::new(before, limit)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(UserNotificationListResponse::from_page(
        &page,
    )))
}

pub(crate) async fn mark_user_notifications_read(
    State(state): State<UserNotificationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserNotificationMarkReadRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserNotificationMarkReadCommand::new(request.notification_ids)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let result = state
        .service
        .mark_read(authentication.principal(), command)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(UserNotificationMarkReadResponse {
        marked_count: result.marked_count(),
        unread_count: result.unread_count(),
    }))
}

fn parse_query(
    raw_query: Option<&str>,
) -> Result<(Option<UserNotificationCursor>, usize), ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok((None, DEFAULT_USER_NOTIFICATION_PAGE_SIZE));
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
            "limit" if limit.is_none() => limit = Some(parse_positive_usize(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok((before, limit.unwrap_or(DEFAULT_USER_NOTIFICATION_PAGE_SIZE)))
}

fn parse_cursor(value: &str) -> Result<UserNotificationCursor, ManagementError> {
    let (occurred_at, id) = value
        .split_once(':')
        .ok_or(ManagementError::InvalidRequest)?;
    UserNotificationCursor::new(parse_positive_i64(occurred_at)?, parse_positive_i64(id)?)
        .map_err(map_error)
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

impl UserNotificationListResponse {
    fn from_page(page: &UserNotificationPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(UserNotificationResponse::from_notification)
                .collect(),
            next_cursor: page
                .next_cursor()
                .map(|cursor| format!("{}:{}", cursor.occurred_at(), cursor.id())),
            unread_count: page.unread_count(),
        }
    }
}

impl UserNotificationResponse {
    fn from_notification(notification: &UserNotification) -> Self {
        Self {
            id: notification.id(),
            kind: match notification.kind() {
                UserNotificationKind::BalanceAlert => NotificationKindResponse::BalanceAlert,
                UserNotificationKind::SubscriptionBalanceAlert => {
                    NotificationKindResponse::SubscriptionBalanceAlert
                }
                UserNotificationKind::SubscriptionPurchase => {
                    NotificationKindResponse::SubscriptionPurchase
                }
                UserNotificationKind::ProductUpdate => NotificationKindResponse::ProductUpdate,
            },
            channel: match notification.channel() {
                UserNotificationChannel::Email => NotificationChannelResponse::Email,
                UserNotificationChannel::InApp => NotificationChannelResponse::InApp,
            },
            template_version: notification.template_version().to_owned(),
            occurred_at: notification.occurred_at(),
            delivery_state: match notification.delivery_state() {
                UserNotificationDeliveryState::Queued => NotificationDeliveryStateResponse::Queued,
                UserNotificationDeliveryState::Accepted => {
                    NotificationDeliveryStateResponse::Accepted
                }
                UserNotificationDeliveryState::Failed => NotificationDeliveryStateResponse::Failed,
                UserNotificationDeliveryState::Canceled => {
                    NotificationDeliveryStateResponse::Canceled
                }
                UserNotificationDeliveryState::Available => {
                    NotificationDeliveryStateResponse::Available
                }
            },
            delivery_attempts: notification.delivery_attempts(),
            observed_quota: notification.observed_quota(),
            threshold_quota: notification.threshold_quota(),
            subscription_id: notification.subscription_id().map(str::to_owned),
            window_ends_at: notification.window_ends_at(),
            quota_amount: notification.quota_amount(),
            quota_used: notification.quota_used(),
            threshold_percent: notification.threshold_percent(),
            read_at: notification.read_at(),
            announcement_id: notification.announcement_id(),
            announcement_version: notification.announcement_version(),
            announcement_title_zh: notification.announcement_title_zh().map(str::to_owned),
            announcement_title_en: notification.announcement_title_en().map(str::to_owned),
            announcement_body_zh: notification.announcement_body_zh().map(str::to_owned),
            announcement_body_en: notification.announcement_body_en().map(str::to_owned),
            announcement_status: notification.announcement_status(),
            announcement_visible_until: notification.announcement_visible_until(),
        }
    }
}

fn map_error(error: UserNotificationError) -> ManagementError {
    match error {
        UserNotificationError::InvalidInput => ManagementError::InvalidRequest,
        UserNotificationError::Internal => ManagementError::Internal,
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
        ] {
            assert!(matches!(
                parse_query(Some(query)),
                Err(ManagementError::InvalidRequest)
            ));
        }
    }
}
