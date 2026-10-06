//! 当前用户通知历史 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_notifications::{
        NotificationChannelResponse, NotificationDeliveryStateResponse, NotificationKindResponse,
        UserNotificationListResponse, UserNotificationMarkReadRequest,
        UserNotificationMarkReadResponse, UserNotificationResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/notifications",
    operation_id = "listUserNotifications",
    tag = "我的通知",
    summary = "读取当前用户通知历史",
    params(
        ("before" = Option<String>, Query, description = "occurred_at:id 复合游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "当前用户通知事实", body = UserNotificationListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_notifications() {}

#[utoipa::path(
    post,
    path = "/api/account/notifications/read",
    operation_id = "markUserNotificationsRead",
    tag = "我的通知",
    request_body = UserNotificationMarkReadRequest,
    responses(
        (status = 200, description = "已读回执已幂等写入", body = UserNotificationMarkReadResponse),
        (status = 400, description = "通知 ID 无效或不属于当前用户", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn mark_user_notifications_read() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_user_notifications, mark_user_notifications_read),
    components(schemas(
        NotificationKindResponse,
        NotificationChannelResponse,
        NotificationDeliveryStateResponse,
        UserNotificationResponse,
        UserNotificationListResponse,
        UserNotificationMarkReadRequest,
        UserNotificationMarkReadResponse,
        ManagementErrorBody
    ))
)]
struct UserNotificationsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserNotificationsApi::openapi()
}
