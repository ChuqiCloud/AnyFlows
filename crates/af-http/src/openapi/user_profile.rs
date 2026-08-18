//! 当前用户个人资料与安全设置 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_profile::{
        UserNotificationPreferencesRequest, UserNotificationPreferencesResponse,
        UserPasskeyListResponse, UserPasskeyRegistrationOptionsResponse,
        UserPasskeyRegistrationVerifyRequest, UserPasskeyRenameRequest, UserPasskeyResponse,
        UserPasskeyRevokeRequest, UserPasswordChangeRequest, UserProfileResponse,
        UserProfileUpdateRequest, UserTwoFactorEnrollmentResponse, UserTwoFactorPasswordRequest,
        UserTwoFactorStatusResponse,
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
    tag = "涓汉璧勬枡涓庡畨鍏",
    summary = "璇诲彇褰撳墠鐢ㄦ埛 Passkey 鐩綍",
    responses(
        (status = 200, description = "Passkey 鐩綍", body = UserPasskeyListResponse),
        (status = 401, description = "浼氳瘽鏃犳晥", body = ManagementErrorBody),
        (status = 500, description = "鏈嶅姟鍐呴儴閿欒", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_passkeys() {}

#[utoipa::path(
    post,
    path = "/api/account/passkeys/registration/options",
    operation_id = "startUserPasskeyRegistration",
    tag = "涓汉璧勬枡涓庡畨鍏",
    summary = "鍒涘缓 Passkey 娉ㄥ唽閫夐」",
    responses(
        (status = 200, description = "WebAuthn 娉ㄥ唽閫夐」", body = UserPasskeyRegistrationOptionsResponse),
        (status = 401, description = "浼氳瘽鏃犳晥", body = ManagementErrorBody),
        (status = 500, description = "鏈嶅姟鍐呴儴閿欒", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn start_user_passkey_registration() {}

#[utoipa::path(
    post,
    path = "/api/account/passkeys/registration/verify",
    operation_id = "finishUserPasskeyRegistration",
    tag = "涓汉璧勬枡涓庡畨鍏",
    summary = "楠岃瘉骞朵繚瀛?Passkey",
    request_body = UserPasskeyRegistrationVerifyRequest,
    responses(
        (status = 200, description = "Passkey 宸插垱寤", body = UserPasskeyResponse),
        (status = 400, description = "璇锋眰姝ｆ枃鎴栭獙璇佹棤鏁", body = ManagementErrorBody),
        (status = 401, description = "浼氳瘽鏃犳晥", body = ManagementErrorBody),
        (status = 500, description = "鏈嶅姟鍐呴儴閿欒", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn finish_user_passkey_registration() {}

#[utoipa::path(
    patch,
    path = "/api/account/passkeys/{id}",
    operation_id = "renameUserPasskey",
    tag = "涓汉璧勬枡涓庡畨鍏",
    params(("id" = i64, Path, description = "Passkey 鍐呴儴 ID")),
    request_body = UserPasskeyRenameRequest,
    responses(
        (status = 200, description = "Passkey 宸查噸鍛藉悕", body = UserPasskeyResponse),
        (status = 400, description = "璇锋眰姝ｆ枃鏃犳晥", body = ManagementErrorBody),
        (status = 401, description = "浼氳瘽鏃犳晥", body = ManagementErrorBody),
        (status = 500, description = "鏈嶅姟鍐呴儴閿欒", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn rename_user_passkey() {}

#[utoipa::path(
    delete,
    path = "/api/account/passkeys/{id}",
    operation_id = "revokeUserPasskey",
    tag = "涓汉璧勬枡涓庡畨鍏",
    params(("id" = i64, Path, description = "Passkey 鍐呴儴 ID")),
    request_body = UserPasskeyRevokeRequest,
    responses(
        (status = 204, description = "Passkey 宸茶鎾ら攢"),
        (status = 400, description = "璇锋眰姝ｆ枃鏃犳晥", body = ManagementErrorBody),
        (status = 401, description = "瀵嗙爜鎴栦簩娆￠獙璇佹棤鏁", body = ManagementErrorBody),
        (status = 500, description = "鏈嶅姟鍐呴儴閿欒", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_user_passkey() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_user_profile,
        update_user_profile,
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
