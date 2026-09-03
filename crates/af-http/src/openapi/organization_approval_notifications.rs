//! 企业审批通知读取与已读回执的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_approval_notifications::{
        DecisionResponse, ListResponse, MarkReadRequest, MarkReadResponse, NotificationResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/organization-approval-notifications",
    operation_id = "listOrganizationApprovalNotifications",
    tag = "企业审批通知",
    summary = "读取当前用户企业审批通知",
    params(
        ("before" = Option<String>, Query, description = "occurred_at:id 复合游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "当前用户可见的企业审批通知事实", body = ListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_approval_notifications() {}

#[utoipa::path(
    post,
    path = "/api/account/organization-approval-notifications/read",
    operation_id = "markOrganizationApprovalNotificationsRead",
    tag = "企业审批通知",
    summary = "批量标记企业审批通知为已读",
    request_body = MarkReadRequest,
    responses(
        (status = 200, description = "已读回执已幂等写入", body = MarkReadResponse),
        (status = 400, description = "通知 ID 无效或不属于当前用户", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn mark_organization_approval_notifications_read() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_approval_notifications,
        mark_organization_approval_notifications_read
    ),
    components(schemas(
        DecisionResponse,
        NotificationResponse,
        ListResponse,
        MarkReadRequest,
        MarkReadResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationApprovalNotificationsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationApprovalNotificationsApi::openapi()
}
