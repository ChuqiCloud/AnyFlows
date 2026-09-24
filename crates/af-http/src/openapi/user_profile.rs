//! 当前用户个人资料与安全设置 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_profile::{
        UserEmailBindingConfirmRequest, UserEmailBindingVerificationRequest,
        UserEmailBindingVerificationResponse, UserNotificationPreferencesRequest,
        UserNotificationPreferencesResponse, UserPasskeyListResponse,
        UserPasskeyRegistrationOptionsResponse, UserPasskeyRegistrationVerifyRequest,
        UserPasskeyRenameRequest, UserPasskeyResponse, UserPasskeyRevokeRequest,
        UserPasswordChangeRequest, UserProfileResponse, UserProfileUpdateRequest,
        UserTwoFactorEnrollmentResponse, UserTwoFactorPasswordRequest, UserTwoFactorStatusResponse,
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
    post,
    path = "/api/account/profile/email-verification",
    operation_id = "sendUserEmailBindingVerification",
    tag = "个人资料与安全",
    summary = "向待绑定邮箱发送验证码",
    request_body = UserEmailBindingVerificationRequest,
    responses(
        (status = 200, description = "验证码已发送", body = UserEmailBindingVerificationResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "邮件服务未配置", body = ManagementErrorBody),
        (status = 429, description = "发送过于频繁", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody),
        (status = 502, description = "验证码投递失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn send_user_email_binding_verification() {}

#[utoipa::path(
    put,
    path = "/api/account/profile/email",
    operation_id = "confirmUserEmailBinding",
    tag = "个人资料与安全",
    summary = "确认并绑定邮箱",
    request_body = UserEmailBindingConfirmRequest,
    responses(
        (status = 200, description = "邮箱已绑定", body = UserProfileResponse),
        (status = 400, description = "请求正文或验证码无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "邮箱已被其他用户使用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn confirm_user_email_binding() {}

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
    get,
    path = "/api/account/two-factor",
    operation_id = "getUserTwoFactor",
    tag = "个人资料与安全",
    summary = "读取当前用户二次验证状态",
    responses(
        (status = 200, description = "二次验证状态", body = UserTwoFactorStatusResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_two_factor() {}

#[utoipa::path(
    post,
    path = "/api/account/two-factor",
    operation_id = "enableUserTwoFactor",
    tag = "个人资料与安全",
    summary = "启用当前用户 TOTP 二次验证",
    request_body = UserTwoFactorPasswordRequest,
    responses(
        (status = 200, description = "TOTP 入网材料，仅返回一次", body = UserTwoFactorEnrollmentResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "当前密码错误或已经启用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn enable_user_two_factor() {}

#[utoipa::path(
    delete,
    path = "/api/account/two-factor",
    operation_id = "disableUserTwoFactor",
    tag = "个人资料与安全",
    summary = "停用当前用户 TOTP 二次验证",
    request_body = UserTwoFactorPasswordRequest,
    responses(
        (status = 204, description = "TOTP 已停用，旧会话已撤销"),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "当前密码错误或尚未启用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_user_two_factor() {}

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

#[utoipa::path(
    get,
    path = "/api/account/passkeys",
    operation_id = "listUserPasskeys",
    tag = "个人资料与安全",
    summary = "读取当前用户 Passkey 目录",
    responses(
        (status = 200, description = "Passkey 目录", body = UserPasskeyListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_passkeys() {}

#[utoipa::path(
    post,
    path = "/api/account/passkeys/registration/options",
    operation_id = "startUserPasskeyRegistration",
    tag = "个人资料与安全",
    summary = "创建 Passkey 注册选项",
    responses(
        (status = 200, description = "WebAuthn 注册选项", body = UserPasskeyRegistrationOptionsResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn start_user_passkey_registration() {}

#[utoipa::path(
    post,
    path = "/api/account/passkeys/registration/verify",
    operation_id = "finishUserPasskeyRegistration",
    tag = "个人资料与安全",
    summary = "验证并保存 Passkey",
    request_body = UserPasskeyRegistrationVerifyRequest,
    responses(
        (status = 200, description = "Passkey 已创建", body = UserPasskeyResponse),
        (status = 400, description = "请求正文或验证无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn finish_user_passkey_registration() {}

#[utoipa::path(
    patch,
    path = "/api/account/passkeys/{id}",
    operation_id = "renameUserPasskey",
    tag = "个人资料与安全",
    summary = "修改 Passkey 展示名称",
    params(("id" = i64, Path, description = "Passkey 内部 ID")),
    request_body = UserPasskeyRenameRequest,
    responses(
        (status = 200, description = "Passkey 已重命名", body = UserPasskeyResponse),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn rename_user_passkey() {}

#[utoipa::path(
    delete,
    path = "/api/account/passkeys/{id}",
    operation_id = "revokeUserPasskey",
    tag = "个人资料与安全",
    summary = "撤销 Passkey",
    params(("id" = i64, Path, description = "Passkey 内部 ID")),
    request_body = UserPasskeyRevokeRequest,
    responses(
        (status = 204, description = "Passkey 已撤销"),
        (status = 400, description = "请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "密码或二次验证无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_user_passkey() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_user_profile,
        update_user_profile,
        send_user_email_binding_verification,
        confirm_user_email_binding,
        change_user_password,
        get_user_two_factor,
        enable_user_two_factor,
        disable_user_two_factor,
        update_user_notification_preferences,
        list_user_passkeys,
        start_user_passkey_registration,
        finish_user_passkey_registration,
        rename_user_passkey,
        revoke_user_passkey
    ),
    components(schemas(
        UserProfileResponse,
        UserNotificationPreferencesResponse,
        UserProfileUpdateRequest,
        UserEmailBindingVerificationRequest,
        UserEmailBindingVerificationResponse,
        UserEmailBindingConfirmRequest,
        UserPasswordChangeRequest,
        UserTwoFactorStatusResponse,
        UserTwoFactorPasswordRequest,
        UserTwoFactorEnrollmentResponse,
        UserNotificationPreferencesRequest,
        UserPasskeyResponse,
        UserPasskeyListResponse,
        UserPasskeyRegistrationOptionsResponse,
        UserPasskeyRegistrationVerifyRequest,
        UserPasskeyRenameRequest,
        UserPasskeyRevokeRequest,
        ManagementErrorBody
    ))
)]
struct UserProfileApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserProfileApi::openapi()
}
