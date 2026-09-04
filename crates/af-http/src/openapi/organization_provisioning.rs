#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use crate::management_error::ManagementErrorBody;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub(crate) struct OrganizationProvisioningListQuerySchema {
    #[param(minimum = 1)]
    pub(crate) before: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub(crate) limit: Option<u64>,
    pub(crate) status: Option<OrganizationProvisioningStatusSchema>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningStatus, rename_all = "snake_case")]
pub(crate) enum OrganizationProvisioningStatusSchema {
    Pending,
    Approved,
    Rejected,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningDecisionKind, rename_all = "snake_case")]
pub(crate) enum OrganizationProvisioningDecisionKindSchema {
    Approve,
    Reject,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningCreateRequest)]
pub(crate) struct OrganizationProvisioningCreateRequestSchema {
    pub(crate) idempotency_key: String,
    pub(crate) organization_name: String,
    pub(crate) organization_slug: String,
    pub(crate) business_reason: String,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningEntitlement)]
pub(crate) struct OrganizationProvisioningEntitlementSchema {
    pub(crate) seat_limit: i64,
    pub(crate) team_limit: i64,
    pub(crate) organization_key_limit: i64,
    pub(crate) team_management_enabled: bool,
    pub(crate) sso_enabled: bool,
    pub(crate) custom_roles_enabled: bool,
    pub(crate) audit_retention_days: i64,
    pub(crate) valid_from: i64,
    pub(crate) valid_until: i64,
    pub(crate) grace_until: Option<i64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningDecisionRequest)]
pub(crate) struct OrganizationProvisioningDecisionRequestSchema {
    pub(crate) expected_version: u64,
    pub(crate) decision: OrganizationProvisioningDecisionKindSchema,
    pub(crate) reason: Option<String>,
    pub(crate) entitlement: Option<OrganizationProvisioningEntitlementSchema>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningRequest)]
pub(crate) struct OrganizationProvisioningRequestSchema {
    pub(crate) id: i64,
    pub(crate) request_key: String,
    pub(crate) idempotency_key: String,
    pub(crate) applicant_user_id: i64,
    pub(crate) organization_name: String,
    pub(crate) organization_slug: String,
    pub(crate) business_reason: String,
    pub(crate) status: OrganizationProvisioningStatusSchema,
    pub(crate) version: u64,
    pub(crate) reviewer_user_id: Option<i64>,
    pub(crate) decision_reason: Option<String>,
    pub(crate) organization_id: Option<i64>,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) decided_at: Option<u64>,
    pub(crate) replayed: bool,
}

#[derive(ToSchema)]
#[schema(as = OrganizationProvisioningRequestListResponse)]
pub(crate) struct OrganizationProvisioningRequestListResponseSchema {
    pub(crate) requests: Vec<OrganizationProvisioningRequestSchema>,
    pub(crate) next_before: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/account/organization-provisioning",
    operation_id = "createOrganizationProvisioningRequest",
    tag = "企业开通",
    request_body = OrganizationProvisioningCreateRequestSchema,
    responses((status = 201, body = OrganizationProvisioningRequestSchema), (status = 200, body = OrganizationProvisioningRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_organization_provisioning_request() {}

#[utoipa::path(
    get,
    path = "/api/account/organization-provisioning/active",
    operation_id = "getActiveOrganizationProvisioningRequest",
    tag = "企业开通",
    responses((status = 200, body = OrganizationProvisioningRequestSchema), (status = 401, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_active_organization_provisioning_request() {}

#[utoipa::path(
    get,
    path = "/api/account/organization-provisioning/{request_key}",
    operation_id = "getOrganizationProvisioningRequest",
    tag = "企业开通",
    params(("request_key" = String, Path)),
    responses((status = 200, body = OrganizationProvisioningRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_provisioning_request() {}

#[utoipa::path(
    get,
    path = "/api/admin/organization-provisioning",
    operation_id = "listAdminOrganizationProvisioningRequests",
    tag = "企业开通",
    params(OrganizationProvisioningListQuerySchema),
    responses((status = 200, body = OrganizationProvisioningRequestListResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_admin_organization_provisioning_requests() {}

#[utoipa::path(
    get,
    path = "/api/admin/organization-provisioning/{request_key}",
    operation_id = "getAdminOrganizationProvisioningRequest",
    tag = "企业开通",
    params(("request_key" = String, Path)),
    responses((status = 200, body = OrganizationProvisioningRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_admin_organization_provisioning_request() {}

#[utoipa::path(
    post,
    path = "/api/admin/organization-provisioning/{request_key}/decision",
    operation_id = "decideOrganizationProvisioningRequest",
    tag = "企业开通",
    params(("request_key" = String, Path)),
    request_body = OrganizationProvisioningDecisionRequestSchema,
    responses((status = 202, body = OrganizationProvisioningRequestSchema), (status = 200, body = OrganizationProvisioningRequestSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn decide_organization_provisioning_request() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_organization_provisioning_request,
        get_active_organization_provisioning_request,
        get_organization_provisioning_request,
        list_admin_organization_provisioning_requests,
        get_admin_organization_provisioning_request,
        decide_organization_provisioning_request
    ),
    components(schemas(
        OrganizationProvisioningListQuerySchema,
        OrganizationProvisioningStatusSchema,
        OrganizationProvisioningDecisionKindSchema,
        OrganizationProvisioningCreateRequestSchema,
        OrganizationProvisioningEntitlementSchema,
        OrganizationProvisioningDecisionRequestSchema,
        OrganizationProvisioningRequestSchema,
        OrganizationProvisioningRequestListResponseSchema,
        ManagementErrorBody
    ))
)]
struct OrganizationProvisioningApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationProvisioningApi::openapi()
}
