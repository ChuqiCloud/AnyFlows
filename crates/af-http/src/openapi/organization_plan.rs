#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use crate::management_error::ManagementErrorBody;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub(crate) struct OrganizationPlanListQuerySchema {
    #[param(minimum = 1)]
    pub(crate) before: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub(crate) limit: Option<usize>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanCreateRequest)]
pub(crate) struct OrganizationPlanCreateRequestSchema {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) team_management_enabled: bool,
    pub(crate) sso_enabled: bool,
    pub(crate) custom_roles_enabled: bool,
    pub(crate) seat_limit: i64,
    pub(crate) team_limit: i64,
    pub(crate) organization_key_limit: i64,
    pub(crate) audit_retention_days: i64,
    pub(crate) validity_days: i64,
    pub(crate) price_provider: String,
    pub(crate) price_currency: String,
    pub(crate) price_amount_minor: i64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanDisableRequest)]
pub(crate) struct OrganizationPlanDisableRequestSchema {
    pub(crate) expected_version: i64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanStatus, rename_all = "snake_case")]
pub(crate) enum OrganizationPlanStatusSchema {
    Active,
    Disabled,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlan)]
pub(crate) struct OrganizationPlanResponseSchema {
    pub(crate) plan_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) status: OrganizationPlanStatusSchema,
    pub(crate) team_management_enabled: bool,
    pub(crate) sso_enabled: bool,
    pub(crate) custom_roles_enabled: bool,
    pub(crate) seat_limit: i64,
    pub(crate) team_limit: i64,
    pub(crate) organization_key_limit: i64,
    pub(crate) audit_retention_days: i64,
    pub(crate) validity_days: i64,
    pub(crate) price_provider: String,
    pub(crate) price_currency: String,
    pub(crate) price_amount_minor: i64,
    pub(crate) version: i64,
    pub(crate) created_by_user_id: i64,
    pub(crate) disabled_at: Option<i64>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanListResponse)]
pub(crate) struct OrganizationPlanListResponseSchema {
    pub(crate) plans: Vec<OrganizationPlanResponseSchema>,
    pub(crate) next_cursor: Option<i64>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanCatalogResponse)]
pub(crate) struct OrganizationPlanCatalogResponseSchema {
    pub(crate) plans: Vec<OrganizationPlanResponseSchema>,
    pub(crate) purchase_notice: String,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanOrderCreateRequest)]
pub(crate) struct OrganizationPlanOrderCreateRequestSchema {
    pub(crate) plan_id: String,
    pub(crate) idempotency_key: String,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanOrderPaymentRequest)]
pub(crate) struct OrganizationPlanOrderPaymentRequestSchema {
    pub(crate) payment_method: String,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanOrderStatus, rename_all = "snake_case")]
pub(crate) enum OrganizationPlanOrderStatusSchema {
    Created,
    Pending,
    Paid,
    Failed,
    Canceled,
    Expired,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanOrder)]
pub(crate) struct OrganizationPlanOrderResponseSchema {
    pub(crate) order_id: String,
    pub(crate) organization_id: i64,
    pub(crate) payer_user_id: i64,
    pub(crate) plan_id: String,
    pub(crate) plan_version: u64,
    pub(crate) provider: String,
    pub(crate) currency: String,
    pub(crate) amount_minor: i64,
    pub(crate) status: OrganizationPlanOrderStatusSchema,
    pub(crate) version: u64,
    pub(crate) payment_method: Option<String>,
    pub(crate) expires_at: Option<u64>,
    pub(crate) paid_at: Option<u64>,
    pub(crate) closed_at: Option<u64>,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) replayed: bool,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanPayment)]
pub(crate) struct OrganizationPlanPaymentResponseSchema {
    #[schema(required = true)]
    pub(crate) payment_intent_id: Option<String>,
    #[schema(required = true)]
    pub(crate) client_secret: Option<String>,
    #[schema(required = true)]
    pub(crate) redirect_url: Option<String>,
}

#[derive(ToSchema)]
#[schema(as = OrganizationPlanOrderPaymentResponse)]
pub(crate) struct OrganizationPlanOrderPaymentResponseSchema {
    pub(crate) order: OrganizationPlanOrderResponseSchema,
    pub(crate) payment: OrganizationPlanPaymentResponseSchema,
}

#[derive(ToSchema)]
#[schema(as = OrganizationEntitlementStatus, rename_all = "snake_case")]
pub(crate) enum OrganizationEntitlementStatusSchema {
    Active,
    Grace,
    ReadOnly,
    Suspended,
    Closed,
}

#[derive(ToSchema)]
#[schema(as = OrganizationEntitlementSource, rename_all = "snake_case")]
pub(crate) enum OrganizationEntitlementSourceSchema {
    PlatformProvisioning,
    OrganizationPlanOrder,
    Other,
}

#[derive(ToSchema)]
#[schema(as = OrganizationEntitlementProjection)]
pub(crate) struct OrganizationEntitlementProjectionResponseSchema {
    pub(crate) organization_id: i64,
    pub(crate) status: OrganizationEntitlementStatusSchema,
    pub(crate) source: OrganizationEntitlementSourceSchema,
    pub(crate) team_management_enabled: bool,
    pub(crate) sso_enabled: bool,
    pub(crate) custom_roles_enabled: bool,
    pub(crate) seat_limit: i64,
    pub(crate) team_limit: i64,
    pub(crate) organization_key_limit: i64,
    pub(crate) audit_retention_days: i64,
    pub(crate) valid_from: i64,
    pub(crate) valid_until: i64,
    #[schema(required = true)]
    pub(crate) grace_until: Option<i64>,
    pub(crate) version: i64,
    pub(crate) updated_at: i64,
    #[schema(required = true)]
    pub(crate) applied_order_id: Option<String>,
    #[schema(required = true)]
    pub(crate) plan_id: Option<String>,
    #[schema(required = true)]
    pub(crate) plan_version: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/admin/organization-plans",
    operation_id = "listAdminOrganizationPlans",
    tag = "企业套餐",
    params(OrganizationPlanListQuerySchema),
    responses((status = 200, body = OrganizationPlanListResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_admin_organization_plans() {}

#[utoipa::path(
    post,
    path = "/api/admin/organization-plans",
    operation_id = "createAdminOrganizationPlan",
    tag = "企业套餐",
    request_body = OrganizationPlanCreateRequestSchema,
    responses((status = 201, body = OrganizationPlanResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_admin_organization_plan() {}

#[utoipa::path(
    post,
    path = "/api/admin/organization-plans/{plan_id}/disable",
    operation_id = "disableAdminOrganizationPlan",
    tag = "企业套餐",
    params(("plan_id" = String, Path)),
    request_body = OrganizationPlanDisableRequestSchema,
    responses((status = 200, body = OrganizationPlanResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn disable_admin_organization_plan() {}

#[utoipa::path(
    get,
    path = "/api/account/organization-plan-catalog",
    operation_id = "getOrganizationPlanCatalog",
    tag = "企业套餐",
    responses((status = 200, body = OrganizationPlanCatalogResponseSchema), (status = 401, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_plan_catalog() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/plan-orders",
    operation_id = "createOrganizationPlanOrder",
    tag = "企业套餐",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationPlanOrderCreateRequestSchema,
    responses((status = 201, body = OrganizationPlanOrderResponseSchema), (status = 200, body = OrganizationPlanOrderResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_organization_plan_order() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/plan-orders/{order_id}",
    operation_id = "getOrganizationPlanOrder",
    tag = "企业套餐",
    params(("organization_id" = i64, Path, minimum = 1), ("order_id" = String, Path)),
    responses((status = 200, body = OrganizationPlanOrderResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_plan_order() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/plan-orders/{order_id}/payment",
    operation_id = "submitOrganizationPlanOrderPayment",
    tag = "企业套餐",
    params(("organization_id" = i64, Path, minimum = 1), ("order_id" = String, Path)),
    request_body = OrganizationPlanOrderPaymentRequestSchema,
    responses((status = 200, body = OrganizationPlanOrderPaymentResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn submit_organization_plan_order_payment() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/entitlement",
    operation_id = "getOrganizationEntitlement",
    tag = "企业套餐",
    params(("organization_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = OrganizationEntitlementProjectionResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_entitlement() {}

#[utoipa::path(
    get,
    path = "/api/platform/organizations/{organization_id}/entitlement",
    operation_id = "getPlatformOrganizationEntitlement",
    tag = "企业套餐",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业访问理由")
    ),
    responses((status = 200, body = OrganizationEntitlementProjectionResponseSchema), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 503, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_platform_organization_entitlement() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_organization_plans,
        create_admin_organization_plan,
        disable_admin_organization_plan,
        get_organization_plan_catalog,
        create_organization_plan_order,
        get_organization_plan_order,
        submit_organization_plan_order_payment,
        get_organization_entitlement,
        get_platform_organization_entitlement
    ),
    components(schemas(
        OrganizationPlanListQuerySchema,
        OrganizationPlanCreateRequestSchema,
        OrganizationPlanDisableRequestSchema,
        OrganizationPlanStatusSchema,
        OrganizationPlanResponseSchema,
        OrganizationPlanListResponseSchema,
        OrganizationPlanCatalogResponseSchema,
        OrganizationPlanOrderCreateRequestSchema,
        OrganizationPlanOrderPaymentRequestSchema,
        OrganizationPlanOrderStatusSchema,
        OrganizationPlanOrderResponseSchema,
        OrganizationPlanPaymentResponseSchema,
        OrganizationPlanOrderPaymentResponseSchema,
        OrganizationEntitlementStatusSchema,
        OrganizationEntitlementSourceSchema,
        OrganizationEntitlementProjectionResponseSchema,
        ManagementErrorBody
    ))
)]
struct OrganizationPlanApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationPlanApi::openapi()
}
