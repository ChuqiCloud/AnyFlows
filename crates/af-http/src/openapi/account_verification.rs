#![allow(dead_code, reason = "OpenAPI declarations")]
use crate::account_verification::{
    AccountVerificationDecisionRequest, AccountVerificationDetailResponse,
    AccountVerificationEligibilityResponse, AccountVerificationListResponse,
    AccountVerificationResponse,
};
use crate::management_error::ManagementErrorBody;
use utoipa::{OpenApi, ToSchema};
#[derive(ToSchema)]
struct AccountVerificationMultipartRequest {
    /// JSON containing kind, provider, document_country, document_type, optional document_number, subject_name, summary and materials [{kind,file_field}]. 支付宝实名认证时 materials 可以为空。
    metadata: String,
    #[schema(value_type = Option<String>, format = Binary)]
    file_0: Option<String>,
}
#[utoipa::path(get, path="/api/account/verifications", operation_id="listAccountVerifications", tag="账号认证",
    params(("before" = Option<i64>, Query),("status" = Option<i16>, Query),("limit" = Option<u64>, Query)),
    responses((status=200, body=AccountVerificationListResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn list_account_verifications() {}
#[utoipa::path(get, path="/api/account/verifications/{case_id}", operation_id="getAccountVerification", tag="账号认证",
    params(("case_id" = i64, Path)),
    responses((status=200, body=AccountVerificationDetailResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn get_account_verification() {}
#[utoipa::path(post, path="/api/account/verifications/{case_id}/provider-sync", operation_id="syncAccountVerificationProvider", tag="账号认证",
    params(("case_id" = i64, Path)),
    responses((status=200, body=AccountVerificationResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn sync_account_verification_provider() {}
#[utoipa::path(get, path="/api/account/verifications/{case_id}/materials/{material_id}", operation_id="downloadAccountVerificationMaterial", tag="账号认证",
    params(("case_id" = i64, Path),("material_id" = i64, Path)),
    responses((status=200, content_type="application/octet-stream", body=String), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn download_account_verification_material() {}
#[utoipa::path(get, path="/api/admin/account-verifications", operation_id="listAdminAccountVerifications", tag="账号认证",
    params(("before" = Option<i64>, Query),("status" = Option<i16>, Query),("limit" = Option<u64>, Query)),
    responses((status=200, body=AccountVerificationListResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn list_admin_account_verifications() {}
#[utoipa::path(get, path="/api/admin/account-verifications/{case_id}", operation_id="getAdminAccountVerification", tag="账号认证",
    params(("case_id" = i64, Path)),
    responses((status=200, body=AccountVerificationDetailResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn get_admin_account_verification() {}
#[utoipa::path(get, path="/api/admin/account-verifications/{case_id}/materials/{material_id}", operation_id="downloadAdminAccountVerificationMaterial", tag="账号认证",
    params(("case_id" = i64, Path),("material_id" = i64, Path)),
    responses((status=200, content_type="application/octet-stream", body=String), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn download_admin_account_verification_material() {}
#[utoipa::path(post, path="/api/account/verifications", operation_id="submitAccountVerification", tag="账号认证",
    request_body(content = AccountVerificationMultipartRequest, content_type = "multipart/form-data"),
    responses((status=200, body=AccountVerificationResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn submit_account_verification() {}
#[utoipa::path(get, path="/api/account/verifications/eligibility", operation_id="getAccountVerificationEligibility", tag="账号认证",
    responses((status=200, body=AccountVerificationEligibilityResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn get_account_verification_eligibility() {}
#[utoipa::path(post, path="/api/admin/account-verifications/{case_id}/decision", operation_id="decideAccountVerification", tag="账号认证",
    params(("case_id" = i64, Path)),
    request_body = AccountVerificationDecisionRequest,
    responses((status=200, body=AccountVerificationResponse), (status=400,body=ManagementErrorBody),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody),(status=404,body=ManagementErrorBody),(status=409,body=ManagementErrorBody),(status=503,body=ManagementErrorBody)),
    security(("bearerAuth"=[])))]
fn decide_account_verification() {}
#[utoipa::path(
    get,
    path = "/api/account/verifications/alipay/callback",
    operation_id = "completeAlipayVerificationCallback",
    tag = "账号认证",
    summary = "处理支付宝账号认证回调",
    description = "由支付宝浏览器重定向调用；使用一次性高熵 state 绑定待处理认证，不接受登录会话或 PAT。",
    params(
        ("auth_code" = Option<String>, Query, description = "支付宝授权码别名"),
        ("code" = Option<String>, Query, description = "支付宝授权码"),
        ("state" = Option<String>, Query, description = "一次性授权状态"),
        ("error" = Option<String>, Query, description = "授权错误")
    ),
    responses((status = 200, description = "返回授权结果页面", content_type = "text/html", body = String)),
    security(())
)]
fn complete_alipay_verification_callback() {}
#[derive(OpenApi)]
#[openapi(paths(
    list_account_verifications,
    get_account_verification,
    sync_account_verification_provider,
    download_account_verification_material,
    list_admin_account_verifications,
    get_admin_account_verification,
    download_admin_account_verification_material,
    submit_account_verification,
    get_account_verification_eligibility,
    decide_account_verification,
    complete_alipay_verification_callback
))]
struct AccountVerificationApi;
pub(super) fn document() -> utoipa::openapi::OpenApi {
    AccountVerificationApi::openapi()
}
