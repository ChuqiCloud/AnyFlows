//! 用户管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_user_security::{
        AdminUserSecurityPasskeyResponse, AdminUserSecurityResponse,
        AdminUserSecurityTwoFactorResponse,
    },
    management_users::{
        AdminUserCreateRequest, AdminUserListResponse, AdminUserResponse, AdminUserUpdateRequest,
    },
    openapi::schema::AdminUserStatusSchema,
};

#[utoipa::path(
    get,
    path = "/api/admin/users",
    operation_id = "listAdminUsers",
    tag = "用户管理",
    summary = "读取管理员用户列表",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的用户 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "用户列表", body = AdminUserListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_users() {}

#[utoipa::path(
    post,
    path = "/api/admin/users",
    operation_id = "createAdminUser",
    tag = "用户管理",
    summary = "创建管理员用户",
    request_body = AdminUserCreateRequest,
    responses(
        (status = 201, description = "用户已创建", body = AdminUserResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "用户名或邮箱冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_user() {}

#[utoipa::path(
    get,
    path = "/api/admin/users/{id}",
    operation_id = "getAdminUser",
    tag = "用户管理",
    summary = "读取管理员用户详情",
    params(("id" = i64, Path, minimum = 1, description = "用户 ID")),
    responses(
        (status = 200, description = "用户详情", body = AdminUserResponse),
        (status = 400, description = "用户 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_user() {}

#[utoipa::path(
    put,
    path = "/api/admin/users/{id}",
    operation_id = "updateAdminUser",
    tag = "用户管理",
    summary = "更新管理员用户",
    params(("id" = i64, Path, minimum = 1, description = "用户 ID")),
    request_body = AdminUserUpdateRequest,
    responses(
        (status = 200, description = "用户已更新", body = AdminUserResponse),
        (status = 400, description = "用户 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 409, description = "用户名或邮箱冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_user() {}

#[utoipa::path(
    delete,
    path = "/api/admin/users/{id}",
    operation_id = "deleteAdminUser",
    tag = "用户管理",
    summary = "软删除管理员用户",
    params(("id" = i64, Path, minimum = 1, description = "用户 ID")),
    responses(
        (status = 204, description = "用户已删除"),
        (status = 400, description = "用户 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_user() {}

#[utoipa::path(
    get,
    path = "/api/admin/users/{id}/security",
    operation_id = "getAdminUserSecurity",
    tag = "用户安全",
    summary = "读取用户个人安全设置",
    params(("id" = i64, Path, minimum = 1, description = "用户 ID")),
    responses(
        (status = 200, description = "用户安全设置脱敏快照", body = AdminUserSecurityResponse),
        (status = 400, description = "用户 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_user_security() {}

#[utoipa::path(
    post,
    path = "/api/admin/users/{id}/security/two-factor/reset",
    operation_id = "resetAdminUserTwoFactor",
    tag = "用户安全",
    summary = "重置用户 TOTP 二次验证",
    params(("id" = i64, Path, minimum = 1, description = "用户 ID")),
    responses(
        (status = 200, description = "TOTP 已重置并返回新的脱敏安全快照", body = AdminUserSecurityResponse),
        (status = 400, description = "用户 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn reset_admin_user_two_factor() {}

#[utoipa::path(
    delete,
    path = "/api/admin/users/{id}/security/passkeys/{passkey_id}",
    operation_id = "revokeAdminUserPasskey",
    tag = "用户安全",
    summary = "撤销用户 Passkey",
    params(
        ("id" = i64, Path, minimum = 1, description = "用户 ID"),
        ("passkey_id" = i64, Path, minimum = 1, description = "Passkey ID")
    ),
    responses(
        (status = 204, description = "Passkey 已撤销"),
        (status = 400, description = "用户 ID 或 Passkey ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户或 Passkey 不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_admin_user_passkey() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_users,
        create_admin_user,
        get_admin_user,
        update_admin_user,
        delete_admin_user,
        get_admin_user_security,
        reset_admin_user_two_factor,
        revoke_admin_user_passkey
    ),
    components(schemas(
        AdminUserStatusSchema,
        AdminUserResponse,
        AdminUserListResponse,
        AdminUserCreateRequest,
        AdminUserUpdateRequest,
        AdminUserSecurityTwoFactorResponse,
        AdminUserSecurityPasskeyResponse,
        AdminUserSecurityResponse,
        ManagementErrorBody
    ))
)]
struct UsersApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UsersApi::openapi()
}
