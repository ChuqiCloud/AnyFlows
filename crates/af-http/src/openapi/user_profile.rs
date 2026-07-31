//! 当前用户个人资料与安全设置 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_profile::{
        UserNotificationPreferencesRequest, UserNotificationPreferencesResponse,
        UserPasswordChangeRequest, UserProfileResponse, UserProfileUpdateRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/profile",
    operation_id = "getUserProfile",
    tag = "个人资料与安全",
    summary = "读取当前用户个人资料",
    responses(
        (status = 200, description = "当前用户资料", body = UserProfileResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_profile() {}

#[utoipa::path(
    put,
    path = "/api/account/profile",
    operation_id = "updateUserProfile",
    tag = "个人资料与安全",
    summary = "更新当前用户登录用户名",
    request_body = UserProfileUpdateRequest,
    responses(
        (status = 200, description = "资料已更新", body = UserProfileResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "用户名冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_user_profile() {}

#[utoipa::path(
    put,
    path = "/api/account/password",
    operation_id = "changeUserPassword",
    tag = "个人资料与安全",
    summary = "修改当前用户密码",
    request_body = UserPasswordChangeRequest,
    responses(
        (status = 204, description = "密码已更新，旧会话已撤销"),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "当前密码不正确", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn change_user_password() {}

#[utoipa::path(
    put,
    path = "/api/account/notifications",
    operation_id = "updateUserNotificationPreferences",
    tag = "个人资料与安全",
    summary = "更新当前用户通知偏好",
    request_body = UserNotificationPreferencesRequest,
    responses(
        (status = 200, description = "通知偏好已更新", body = UserProfileResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_user_notification_preferences() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_user_profile,
        update_user_profile,
        change_user_password,
        update_user_notification_preferences
    ),
    components(schemas(
        UserProfileResponse,
        UserNotificationPreferencesResponse,
        UserProfileUpdateRequest,
        UserPasswordChangeRequest,
        UserNotificationPreferencesRequest,
        ManagementErrorBody
    ))
)]
struct UserProfileApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserProfileApi::openapi()
}
