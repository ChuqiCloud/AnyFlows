//! 企业合同价管理 API 的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_contract_price::{
        ContractPriceCloseRequest, ContractPriceCreateRequest, ContractPriceListResponse,
        ContractPriceProtocol, ContractPriceResponse, ContractPriceValues,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/contract-prices",
    operation_id = "listOrganizationContractPrices",
    tag = "企业合同价",
    summary = "读取企业合同价版本",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业合同价历史事实", body = ContractPriceListResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权读取企业合同价", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_contract_prices() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/contract-prices/{price_id}",
    operation_id = "getOrganizationContractPrice",
    tag = "企业合同价",
    summary = "读取企业合同价详情",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("price_id" = i64, Path, minimum = 1, description = "合同价内部标识")
    ),
    responses(
        (status = 200, description = "企业合同价详情", body = ContractPriceResponse),
        (status = 400, description = "标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权读取企业合同价", body = ManagementErrorBody),
        (status = 404, description = "合同价不存在或不属于该企业", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_contract_price() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/contract-prices",
    operation_id = "createOrganizationContractPrice",
    tag = "企业合同价",
    summary = "创建企业合同价版本",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = ContractPriceCreateRequest,
    responses(
        (status = 201, description = "合同价版本已创建", body = ContractPriceResponse),
        (status = 400, description = "合同价参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业合同价", body = ManagementErrorBody),
        (status = 404, description = "企业或创建者不可用", body = ManagementErrorBody),
        (status = 409, description = "区间或版本冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_contract_price() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/contract-prices/{price_id}/close",
    operation_id = "closeOrganizationContractPrice",
    tag = "企业合同价",
    summary = "按版本关闭企业合同价区间",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("price_id" = i64, Path, minimum = 1, description = "合同价内部标识")
    ),
    request_body = ContractPriceCloseRequest,
    responses(
        (status = 200, description = "合同价区间已关闭", body = ContractPriceResponse),
        (status = 400, description = "关闭参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业合同价", body = ManagementErrorBody),
        (status = 404, description = "合同价不存在或不属于该企业", body = ManagementErrorBody),
        (status = 409, description = "版本或状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn close_organization_contract_price() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_contract_prices,
        get_organization_contract_price,
        create_organization_contract_price,
        close_organization_contract_price
    ),
    components(schemas(
        ContractPriceProtocol,
        ContractPriceValues,
        ContractPriceCreateRequest,
        ContractPriceCloseRequest,
        ContractPriceResponse,
        ContractPriceListResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationContractPriceApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationContractPriceApi::openapi()
}
