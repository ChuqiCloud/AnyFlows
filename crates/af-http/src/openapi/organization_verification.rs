#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use crate::management_error::ManagementErrorBody;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub(crate) struct OrganizationVerificationListQuerySchema {
    #[param(minimum = 1)]
    pub(crate) before: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub(crate) limit: Option<u64>,
    pub(crate) status: Option<OrganizationVerificationStatusSchema>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationStatus, rename_all = "snake_case")]
pub(crate) enum OrganizationVerificationStatusSchema {
    Submitted,
    InReview,
    NeedsSupplement,
    Approved,
    Rejected,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationMultipartSubmitRequest)]
pub(crate) struct OrganizationVerificationMultipartSubmitRequestSchema {
    pub(crate) summary: String,
    /// JSON 数组，元素为 `{kind, file_field}`；正文使用同名 `file:<file_field>` 部件。
    pub(crate) materials: String,
    /// 文档工具展示的二进制部件示例；服务端实际接受多个 `file:<file_field>` 部件。
    #[schema(value_type = String, format = Binary)]
    pub(crate) file_0: String,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationMaterialRequest)]
pub(crate) struct OrganizationVerificationMaterialRequestSchema {
    pub(crate) kind: String,
    /// multipart 中对应的文件字段后缀，例如 `0` 对应 `file:0`。
    pub(crate) file_field: String,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationSubmitRequest)]
pub(crate) struct OrganizationVerificationSubmitRequestSchema {
    pub(crate) summary: String,
    pub(crate) materials: Vec<OrganizationVerificationMaterialRequestSchema>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationMaterial)]
pub(crate) struct OrganizationVerificationMaterialSchema {
    pub(crate) id: i64,
    pub(crate) case_id: i64,
    pub(crate) kind: String,
    pub(crate) file_name: String,
    pub(crate) content_type: String,
    pub(crate) size_bytes: i64,
    pub(crate) content_available: bool,
    pub(crate) created_at: u64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationCase)]
pub(crate) struct OrganizationVerificationCaseSchema {
    pub(crate) id: i64,
    pub(crate) organization_id: i64,
    pub(crate) applicant_user_id: i64,
    pub(crate) status: OrganizationVerificationStatusSchema,
    pub(crate) version: i64,
    pub(crate) summary: String,
    pub(crate) reviewer_user_id: Option<i64>,
    pub(crate) review_reason: Option<String>,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) decided_at: Option<u64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationResponse)]
pub(crate) struct OrganizationVerificationResponseSchema {
    pub(crate) case: OrganizationVerificationCaseSchema,
    pub(crate) materials: Vec<OrganizationVerificationMaterialSchema>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationListResponse)]
pub(crate) struct OrganizationVerificationListResponseSchema {
    pub(crate) cases: Vec<OrganizationVerificationResponseSchema>,
    pub(crate) next_cursor: Option<i64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationDecisionKind, rename_all = "snake_case")]
pub(crate) enum OrganizationVerificationDecisionKindSchema {
    RequestSupplement,
    Approve,
    Reject,
}

#[derive(ToSchema)]
#[schema(as = OrganizationVerificationDecisionRequest)]
pub(crate) struct OrganizationVerificationDecisionRequestSchema {
    pub(crate) expected_version: i64,
    pub(crate) decision: OrganizationVerificationDecisionKindSchema,
    pub(crate) reason: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/verification",
    operation_id = "getOrganizationVerification",
    tag = "企业认证",
    params(("organization_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = OrganizationVerificationResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_verification() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/verification",
    operation_id = "submitOrganizationVerification",
    tag = "企业认证",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body(content = OrganizationVerificationMultipartSubmitRequestSchema, content_type = "multipart/form-data"),
    responses((status = 200, body = OrganizationVerificationResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn submit_organization_verification() {}

#[utoipa::path(
    get,
    path = "/api/admin/organization-verifications/{case_id}/materials/{material_id}",
    operation_id = "downloadAdminOrganizationVerificationMaterial",
    tag = "企业认证",
    params(("case_id" = i64, Path, minimum = 1), ("material_id" = i64, Path, minimum = 1)),
    responses((status = 200, description = "受平台权限与审计保护的材料正文", content((String = "application/octet-stream"))), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn download_admin_organization_verification_material() {}

#[utoipa::path(
    get,
    path = "/api/admin/organization-verifications",
    operation_id = "listAdminOrganizationVerifications",
    tag = "企业认证",
    params(OrganizationVerificationListQuerySchema),
    responses((status = 200, body = OrganizationVerificationListResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_admin_organization_verifications() {}

#[utoipa::path(
    get,
    path = "/api/admin/organization-verifications/{case_id}",
    operation_id = "getAdminOrganizationVerification",
    tag = "企业认证",
    params(("case_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = OrganizationVerificationResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_admin_organization_verification() {}

#[utoipa::path(
    post,
    path = "/api/admin/organization-verifications/{case_id}/decision",
    operation_id = "decideOrganizationVerification",
    tag = "企业认证",
    params(("case_id" = i64, Path, minimum = 1)),
    request_body = OrganizationVerificationDecisionRequestSchema,
    responses((status = 200, body = OrganizationVerificationResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn decide_organization_verification() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_self_verification_history,
        get_admin_verification_history,
        download_self_verification_material,
        get_organization_verification,
        submit_organization_verification,
        list_admin_organization_verifications,
        get_admin_organization_verification,
        decide_organization_verification,
        download_admin_organization_verification_material
    ),
    components(schemas(
        OrganizationVerificationListQuerySchema,
        OrganizationVerificationStatusSchema,
        OrganizationVerificationMultipartSubmitRequestSchema,
        OrganizationVerificationMaterialRequestSchema,
        OrganizationVerificationSubmitRequestSchema,
        OrganizationVerificationMaterialSchema,
        OrganizationVerificationCaseSchema,
        OrganizationVerificationResponseSchema,
        OrganizationVerificationListResponseSchema,
        OrganizationVerificationDecisionKindSchema,
        OrganizationVerificationDecisionRequestSchema,
        ManagementErrorBody
    ))
)]
struct OrganizationVerificationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationVerificationApi::openapi()
}

#[derive(ToSchema)]
struct OrganizationVerificationHistoryResponse {
    entries: Vec<serde_json::Value>,
    next_cursor: Option<i64>,
}
#[utoipa::path(get,path="/api/organizations/{organization_id}/verification/history",operation_id="getSelfVerificationHistory",tag="企业认证",
    params(("organization_id"=i64,Path),("before"=Option<i64>,Query)),responses((status=200,body=OrganizationVerificationHistoryResponse),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn get_self_verification_history() {}
#[utoipa::path(get,path="/api/admin/organization-verifications/{case_id}/history",operation_id="getAdminVerificationHistory",tag="企业认证",
    params(("case_id"=i64,Path),("before"=Option<i64>,Query)),responses((status=200,body=OrganizationVerificationHistoryResponse),(status=401,body=ManagementErrorBody),(status=403,body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn get_admin_verification_history() {}
#[utoipa::path(get,path="/api/organizations/{organization_id}/verification/materials/{material_id}",operation_id="downloadSelfVerificationMaterial",tag="企业认证",
    params(("organization_id"=i64,Path),("material_id"=i64,Path)),responses((status=200,content_type="application/octet-stream",body=String),(status=401,body=ManagementErrorBody),(status=404,body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn download_self_verification_material() {}
