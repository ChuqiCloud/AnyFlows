//! 企业钱包、账本、调账与充值 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_wallet::{
        OrganizationTopupOrderCreateRequest, OrganizationTopupOrderResponse,
        OrganizationTopupOrderStatusResponse, OrganizationTopupPaymentSessionResponse,
        OrganizationWalletAdjustmentRequest, OrganizationWalletAdjustmentResponse,
        OrganizationWalletEntryResponse, OrganizationWalletEntryTypeResponse,
        OrganizationWalletListResponse, OrganizationWalletSummaryResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/wallet",
    operation_id = "getOrganizationWallet",
    tag = "企业钱包",
    summary = "读取企业共享钱包",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业钱包余额与额度状态", body = OrganizationWalletSummaryResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权查看钱包", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_wallet() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/wallet/entries",
    operation_id = "listOrganizationWalletEntries",
    tag = "企业钱包",
    summary = "读取企业钱包只追加账本",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的账本 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "企业钱包账本", body = OrganizationWalletListResponse),
        (status = 400, description = "企业标识或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权查看钱包", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_wallet_entries() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/wallet/adjustments",
    operation_id = "adjustOrganizationWallet",
    tag = "企业钱包",
    summary = "执行企业钱包受审计增量调账",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationWalletAdjustmentRequest,
    responses(
        (status = 201, description = "新调账已原子提交", body = OrganizationWalletAdjustmentResponse),
        (status = 200, description = "相同调账事实已存在", body = OrganizationWalletAdjustmentResponse),
        (status = 400, description = "企业标识、事件键、增量或原因无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权管理钱包", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "事件冲突、余额不足或溢出", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知，必须复用同一事件键确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn adjust_organization_wallet() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/topups",
    operation_id = "createOrganizationTopupOrder",
    tag = "企业钱包",
    summary = "创建企业钱包充值订单",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationTopupOrderCreateRequest,
    responses(
        (status = 201, description = "企业充值订单首次创建", body = OrganizationTopupOrderResponse),
        (status = 200, description = "相同幂等事实的企业充值订单已恢复", body = OrganizationTopupOrderResponse),
        (status = 400, description = "企业标识、幂等键、支付方式或金额无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权充值", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "幂等键已经绑定不同充值事实", body = ManagementErrorBody),
        (status = 502, description = "支付 Provider 确定拒绝服务端请求", body = ManagementErrorBody),
        (status = 503, description = "支付方式未配置、数据库暂不可用或提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_topup_order() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_organization_wallet,
        list_organization_wallet_entries,
        adjust_organization_wallet,
        create_organization_topup_order
    ),
    components(schemas(
        OrganizationWalletSummaryResponse,
        OrganizationWalletEntryTypeResponse,
        OrganizationWalletEntryResponse,
        OrganizationWalletListResponse,
        OrganizationWalletAdjustmentRequest,
        OrganizationWalletAdjustmentResponse,
        OrganizationTopupOrderCreateRequest,
        OrganizationTopupOrderStatusResponse,
        OrganizationTopupPaymentSessionResponse,
        OrganizationTopupOrderResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationWalletApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationWalletApi::openapi()
}
