//! 企业预算策略、窗口和预留查询的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_budget::{
        OrganizationBudgetAllocationResponse, OrganizationBudgetOverviewResponse,
        OrganizationBudgetPeriodRequest, OrganizationBudgetPeriodResponse,
        OrganizationBudgetPolicyCreateRequest, OrganizationBudgetPolicyResponse,
        OrganizationBudgetReservationResponse, OrganizationBudgetReservationStatusResponse,
        OrganizationBudgetScopeRequest, OrganizationBudgetScopeResponse,
        OrganizationBudgetWindowResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/budgets",
    operation_id = "getOrganizationBudgets",
    tag = "企业预算",
    summary = "读取企业预算策略与当前窗口",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业预算策略和当前窗口快照", body = OrganizationBudgetOverviewResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业预算", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_budgets() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/budgets",
    operation_id = "createOrganizationBudgetPolicy",
    tag = "企业预算",
    summary = "创建企业预算策略版本",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationBudgetPolicyCreateRequest,
    responses(
        (status = 201, description = "预算策略版本已创建", body = OrganizationBudgetPolicyResponse),
        (status = 400, description = "预算策略参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业预算", body = ManagementErrorBody),
        (status = 404, description = "企业、团队或成员不存在", body = ManagementErrorBody),
        (status = 409, description = "策略版本冲突", body = ManagementErrorBody),
        (status = 503, description = "写入结果未知，调用方必须查询策略列表确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_budget_policy() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/budgets/reservations/{reservation_id}",
    operation_id = "getOrganizationBudgetReservation",
    tag = "企业预算",
    summary = "查询企业计费预留状态",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("reservation_id" = String, Path, min_length = 32, max_length = 32, description = "预留幂等键")
    ),
    responses(
        (status = 200, description = "企业预留安全快照", body = OrganizationBudgetReservationResponse),
        (status = 400, description = "企业标识或预留键无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业预算", body = ManagementErrorBody),
        (status = 404, description = "预留不存在或不属于该企业", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_budget_reservation() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_organization_budgets,
        create_organization_budget_policy,
        get_organization_budget_reservation
    ),
    components(schemas(
        OrganizationBudgetScopeRequest,
        OrganizationBudgetPeriodRequest,
        OrganizationBudgetPolicyCreateRequest,
        OrganizationBudgetScopeResponse,
        OrganizationBudgetPeriodResponse,
        OrganizationBudgetPolicyResponse,
        OrganizationBudgetWindowResponse,
        OrganizationBudgetOverviewResponse,
        OrganizationBudgetAllocationResponse,
        OrganizationBudgetReservationStatusResponse,
        OrganizationBudgetReservationResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationBudgetApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationBudgetApi::openapi()
}
