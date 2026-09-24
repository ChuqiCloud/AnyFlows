#![allow(dead_code, reason = "OpenAPI declarations")]

use crate::{
    management_error::ManagementErrorBody,
    verification_settings::{AdminVerificationSettingsRequest, AdminVerificationSettingsResponse},
};
use utoipa::OpenApi;

#[utoipa::path(get, path="/api/admin/account-verification-settings", operation_id="getAdminVerificationSettings",
    tag="账号认证", summary="读取实名认证配置",
    responses((status=200, body=AdminVerificationSettingsResponse), (status=401,body=ManagementErrorBody),
        (status=403,body=ManagementErrorBody), (status=500,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn get_admin_verification_settings() {}

#[utoipa::path(put, path="/api/admin/account-verification-settings", operation_id="updateAdminVerificationSettings",
    tag="账号认证", summary="更新实名认证配置",
    request_body=AdminVerificationSettingsRequest,
    responses((status=200, body=AdminVerificationSettingsResponse), (status=400,body=ManagementErrorBody),
        (status=401,body=ManagementErrorBody), (status=403,body=ManagementErrorBody),
        (status=409,body=ManagementErrorBody), (status=500,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn update_admin_verification_settings() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_verification_settings, update_admin_verification_settings),
    components(schemas(
        AdminVerificationSettingsResponse,
        AdminVerificationSettingsRequest,
        ManagementErrorBody
    ))
)]
struct VerificationSettingsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    VerificationSettingsApi::openapi()
}
