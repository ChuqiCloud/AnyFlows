//! 公告公开投影与管理员版本化发布 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    announcements::{
        AnnouncementListResponse, AnnouncementMutationRequest, AnnouncementResponse,
        AnnouncementUpdateRequest, AnnouncementWriteRequest,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/announcements",
    operation_id = "listPublicAnnouncements",
    tag = "公告",
    summary = "读取当前已发布公告",
    responses(
        (status = 200, description = "可见时间窗内的已发布公告", body = AnnouncementListResponse),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn list_public_announcements() {}

#[utoipa::path(
    get,
    path = "/api/admin/announcements",
    operation_id = "listAdminAnnouncements",
    tag = "公告",
    summary = "读取公告版本事实",
    responses(
        (status = 200, description = "包含草稿、已发布和已撤回公告", body = AnnouncementListResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_announcements() {}

#[utoipa::path(
    post,
    path = "/api/admin/announcements",
    operation_id = "createAdminAnnouncement",
    tag = "公告",
    summary = "创建公告草稿",
    request_body = AnnouncementWriteRequest,
    responses(
        (status = 200, description = "创建后的公告草稿", body = AnnouncementResponse),
        (status = 400, description = "公告正文或时间窗口无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_announcement() {}

#[utoipa::path(
    put,
    path = "/api/admin/announcements/{id}",
    operation_id = "updateAdminAnnouncement",
    tag = "公告",
    summary = "按版本更新公告草稿",
    params(("id" = i64, Path, description = "公告标识")),
    request_body = AnnouncementUpdateRequest,
    responses(
        (status = 200, description = "更新后的公告草稿", body = AnnouncementResponse),
        (status = 400, description = "公告正文或版本无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "公告不存在", body = ManagementErrorBody),
        (status = 409, description = "公告版本或状态已变化", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_announcement() {}

#[utoipa::path(
    post,
    path = "/api/admin/announcements/{id}/publish",
    operation_id = "publishAdminAnnouncement",
    tag = "公告",
    summary = "发布公告草稿",
    params(("id" = i64, Path, description = "公告标识")),
    request_body = AnnouncementMutationRequest,
    responses(
        (status = 200, description = "已发布的公告", body = AnnouncementResponse),
        (status = 400, description = "公告版本无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "公告不存在", body = ManagementErrorBody),
        (status = 409, description = "公告版本或状态已变化", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn publish_admin_announcement() {}

#[utoipa::path(
    post,
    path = "/api/admin/announcements/{id}/revoke",
    operation_id = "revokeAdminAnnouncement",
    tag = "公告",
    summary = "撤回已发布公告",
    params(("id" = i64, Path, description = "公告标识")),
    request_body = AnnouncementMutationRequest,
    responses(
        (status = 200, description = "已撤回的公告", body = AnnouncementResponse),
        (status = 400, description = "公告版本无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "公告不存在", body = ManagementErrorBody),
        (status = 409, description = "公告版本或状态已变化", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_admin_announcement() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_public_announcements,
        list_admin_announcements,
        create_admin_announcement,
        update_admin_announcement,
        publish_admin_announcement,
        revoke_admin_announcement
    ),
    components(schemas(
        AnnouncementResponse,
        AnnouncementListResponse,
        AnnouncementWriteRequest,
        AnnouncementUpdateRequest,
        AnnouncementMutationRequest,
        ManagementErrorBody
    ))
)]
struct AnnouncementsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    AnnouncementsApi::openapi()
}
