#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use crate::management_error::ManagementErrorBody;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub(crate) struct OrganizationApprovalCursorQuery {
    #[param(minimum = 1)]
    pub(crate) after: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub(crate) limit: Option<u64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalResourceType, rename_all = "snake_case")]
pub(crate) enum OrganizationApprovalResourceTypeSchema {
    Member,
    Budget,
    ServiceAccount,
    ContractPrice,
    Funds,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalDecisionKind, rename_all = "snake_case")]
pub(crate) enum OrganizationApprovalDecisionKindSchema {
    Approved,
    Rejected,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalTemplateCreateRequest)]
pub(crate) struct OrganizationApprovalTemplateCreateRequestSchema {
    pub(crate) template_key: String,
    pub(crate) resource_type: OrganizationApprovalResourceTypeSchema,
    pub(crate) definition_digest: String,
    pub(crate) expected_version: Option<i64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalRequestCreateRequest)]
pub(crate) struct OrganizationApprovalRequestCreateRequestSchema {
    pub(crate) request_key: String,
    pub(crate) resource_type: OrganizationApprovalResourceTypeSchema,
    pub(crate) resource_summary: String,
    pub(crate) template_id: i64,
    pub(crate) template_version: i64,
    pub(crate) expires_at: i64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalDecisionRequest)]
pub(crate) struct OrganizationApprovalDecisionRequestSchema {
    pub(crate) decision_key: String,
    pub(crate) kind: OrganizationApprovalDecisionKindSchema,
    pub(crate) reason: Option<String>,
    pub(crate) decided_at: Option<i64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalTemplate)]
pub(crate) struct OrganizationApprovalTemplateSchema {
    pub(crate) id: i64,
    pub(crate) organization_id: i64,
    pub(crate) template_key: String,
    pub(crate) resource_type: String,
    pub(crate) version: i64,
    pub(crate) definition_digest: String,
    pub(crate) created_by_user_id: i64,
    pub(crate) created_at: i64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalTemplateListResponse)]
pub(crate) struct OrganizationApprovalTemplateListResponseSchema {
    pub(crate) templates: Vec<OrganizationApprovalTemplateSchema>,
    pub(crate) next_after: Option<i64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalDecision)]
pub(crate) struct OrganizationApprovalDecisionSchema {
    pub(crate) id: i64,
    pub(crate) decision_key: String,
    pub(crate) kind: String,
    pub(crate) decided_by_user_id: Option<i64>,
    pub(crate) reason: Option<String>,
    pub(crate) decided_at: i64,
    pub(crate) created_at: i64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalRequest)]
pub(crate) struct OrganizationApprovalRequestSchema {
    pub(crate) id: i64,
    pub(crate) request_key: String,
    pub(crate) organization_id: i64,
    pub(crate) resource_type: String,
    pub(crate) resource_summary: String,
    pub(crate) template_id: i64,
    pub(crate) template_version: i64,
    pub(crate) requested_by_user_id: i64,
    pub(crate) expires_at: i64,
    pub(crate) created_at: i64,
    pub(crate) status: String,
    pub(crate) decision: Option<OrganizationApprovalDecisionSchema>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationApprovalRequestListResponse)]
pub(crate) struct OrganizationApprovalRequestListResponseSchema {
    pub(crate) requests: Vec<OrganizationApprovalRequestSchema>,
    pub(crate) next_after: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/approvals/templates",
    operation_id = "listOrganizationApprovalTemplates",
    tag = "企业审批",
    params(("organization_id" = i64, Path, minimum = 1), OrganizationApprovalCursorQuery),
    responses((status = 200, body = OrganizationApprovalTemplateListResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_approval_templates() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/approvals/templates",
    operation_id = "createOrganizationApprovalTemplate",
    tag = "企业审批",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationApprovalTemplateCreateRequestSchema,
    responses((status = 201, body = OrganizationApprovalTemplateSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_organization_approval_template() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/approvals/requests",
    operation_id = "listOrganizationApprovalRequests",
    tag = "企业审批",
    params(("organization_id" = i64, Path, minimum = 1), OrganizationApprovalCursorQuery),
    responses((status = 200, body = OrganizationApprovalRequestListResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_approval_requests() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/approvals/requests/{request_id}",
    operation_id = "getOrganizationApprovalRequest",
    tag = "企业审批",
    params(("organization_id" = i64, Path, minimum = 1), ("request_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = OrganizationApprovalRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_approval_request() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/approvals/requests",
    operation_id = "createOrganizationApprovalRequest",
    tag = "企业审批",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationApprovalRequestCreateRequestSchema,
    responses((status = 201, body = OrganizationApprovalRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_organization_approval_request() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/approvals/requests/{request_id}/decision",
    operation_id = "decideOrganizationApprovalRequest",
    tag = "企业审批",
    params(("organization_id" = i64, Path, minimum = 1), ("request_id" = i64, Path, minimum = 1)),
    request_body = OrganizationApprovalDecisionRequestSchema,
    responses((status = 200, body = OrganizationApprovalRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn decide_organization_approval_request() {}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_organization_approval_templates,
        create_organization_approval_template,
        list_organization_approval_requests,
        get_organization_approval_request,
        create_organization_approval_request,
        decide_organization_approval_request
    ),
    components(schemas(
        OrganizationApprovalCursorQuery,
        OrganizationApprovalResourceTypeSchema,
        OrganizationApprovalDecisionKindSchema,
        OrganizationApprovalTemplateCreateRequestSchema,
        OrganizationApprovalRequestCreateRequestSchema,
        OrganizationApprovalDecisionRequestSchema,
        OrganizationApprovalTemplateSchema,
        OrganizationApprovalTemplateListResponseSchema,
        OrganizationApprovalDecisionSchema,
        OrganizationApprovalRequestSchema,
        OrganizationApprovalRequestListResponseSchema,
        ManagementErrorBody
    ))
)]
struct OrganizationApprovalApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationApprovalApi::openapi()
}
