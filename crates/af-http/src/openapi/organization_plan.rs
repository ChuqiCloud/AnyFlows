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

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_organization_plans,
        create_admin_organization_plan,
        disable_admin_organization_plan,
        get_organization_plan_catalog
    ),
    components(schemas(
        OrganizationPlanListQuerySchema,
        OrganizationPlanCreateRequestSchema,
        OrganizationPlanDisableRequestSchema,
        OrganizationPlanStatusSchema,
        OrganizationPlanResponseSchema,
        OrganizationPlanListResponseSchema,
        OrganizationPlanCatalogResponseSchema,
        ManagementErrorBody
    ))
)]
struct OrganizationPlanApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationPlanApi::openapi()
}
