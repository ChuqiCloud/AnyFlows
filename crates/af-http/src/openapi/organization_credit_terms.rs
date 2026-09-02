//! 企业授信与账期策略管理 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_credit_terms::{
        CreditAllocationListResponse, CreditAllocationResponse, CreditInvoiceListResponse,
        CreditInvoiceResponse, CreditInvoiceStatusResponse, CreditRepaymentListResponse,
        CreditRepaymentResponse, CreditRepaymentSourceKindResponse, CreditTermCreateRequest,
        CreditTermListResponse, CreditTermPeriodRequest, CreditTermPeriodResponse,
        CreditTermResponse, CreditTermStatusRequest, CreditTermStatusResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-terms",
    operation_id = "listOrganizationCreditTerms",
    tag = "企业授信",
    summary = "查询企业授信策略历史",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"), ("before" = Option<i64>, Query, minimum = 1), ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)),
    responses((status = 200, body = CreditTermListResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_credit_terms() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-terms/{term_id}",
    operation_id = "getOrganizationCreditTerm",
    tag = "企业授信",
    summary = "查询企业授信策略",
    params(("organization_id" = i64, Path, minimum = 1), ("term_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = CreditTermResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_credit_term() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/credit-terms",
    operation_id = "createOrganizationCreditTerm",
    tag = "企业授信",
    summary = "创建企业授信策略版本",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = CreditTermCreateRequest,
    responses((status = 201, body = CreditTermResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_organization_credit_term() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/credit-terms/{term_id}/status",
    operation_id = "updateOrganizationCreditTermStatus",
    tag = "企业授信",
    summary = "暂停或关闭企业授信策略",
    params(("organization_id" = i64, Path, minimum = 1), ("term_id" = i64, Path, minimum = 1)),
    request_body = CreditTermStatusRequest,
    responses((status = 200, body = CreditTermResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn update_organization_credit_term_status() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-invoices",
    operation_id = "listOrganizationCreditInvoices",
    tag = "企业授信",
    summary = "查询企业账期账单",
    params(("organization_id" = i64, Path, minimum = 1), ("before" = Option<i64>, Query, minimum = 1), ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)),
    responses((status = 200, body = CreditInvoiceListResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_credit_invoices() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-invoices/{invoice_id}",
    operation_id = "getOrganizationCreditInvoice",
    tag = "企业授信",
    summary = "查询企业账单详情",
    params(("organization_id" = i64, Path, minimum = 1), ("invoice_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = CreditInvoiceResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_credit_invoice() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-repayments",
    operation_id = "listOrganizationCreditRepayments",
    tag = "企业授信",
    summary = "查询企业还款事实",
    params(("organization_id" = i64, Path, minimum = 1), ("before" = Option<i64>, Query, minimum = 1), ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)),
    responses((status = 200, body = CreditRepaymentListResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_credit_repayments() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-repayments/{repayment_id}",
    operation_id = "getOrganizationCreditRepayment",
    tag = "企业授信",
    summary = "查询企业还款详情",
    params(("organization_id" = i64, Path, minimum = 1), ("repayment_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = CreditRepaymentResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_credit_repayment() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-allocations",
    operation_id = "listOrganizationCreditAllocations",
    tag = "企业授信",
    summary = "查询企业还款核销",
    params(("organization_id" = i64, Path, minimum = 1), ("before" = Option<i64>, Query, minimum = 1), ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)),
    responses((status = 200, body = CreditAllocationListResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_credit_allocations() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/credit-allocations/{allocation_id}",
    operation_id = "getOrganizationCreditAllocation",
    tag = "企业授信",
    summary = "查询企业核销详情",
    params(("organization_id" = i64, Path, minimum = 1), ("allocation_id" = i64, Path, minimum = 1)),
    responses((status = 200, body = CreditAllocationResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_credit_allocation() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_credit_terms,
        get_organization_credit_term,
        create_organization_credit_term,
        update_organization_credit_term_status,
        list_organization_credit_invoices,
        get_organization_credit_invoice,
        list_organization_credit_repayments,
        get_organization_credit_repayment,
        list_organization_credit_allocations,
        get_organization_credit_allocation
    ),
    components(schemas(
        CreditTermPeriodRequest,
        CreditTermCreateRequest,
        CreditTermStatusRequest,
        CreditTermResponse,
        CreditTermListResponse,
        CreditTermPeriodResponse,
        CreditTermStatusResponse,
        CreditInvoiceListResponse,
        CreditInvoiceResponse,
        CreditInvoiceStatusResponse,
        CreditRepaymentListResponse,
        CreditRepaymentResponse,
        CreditRepaymentSourceKindResponse,
        CreditAllocationListResponse,
        CreditAllocationResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationCreditTermsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationCreditTermsApi::openapi()
}
