#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_refunds::{
        AdminRefundDecisionRequest, AdminRefundListResponse, AdminRefundManualCompletionRequest,
        AdminRefundOrderKindResponse, AdminRefundRequestResponse,
        RefundReconciliationEntryResponse, RefundReconciliationListResponse,
        RefundReconciliationStatusResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/refund-reconciliations",
    operation_id = "listAccountRefundReconciliations",
    tag = "退款对账",
    summary = "读取当前用户退款对账",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的对账事实 ID"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "当前用户的退款成功对账事实", body = RefundReconciliationListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_account_refund_reconciliations() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/refund-reconciliations",
    operation_id = "listOrganizationRefundReconciliations",
    tag = "退款对账",
    summary = "读取企业退款对账",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的对账事实 ID"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "企业资金主体的退款成功对账事实", body = RefundReconciliationListResponse),
        (status = 400, description = "企业标识或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权查看钱包", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_refund_reconciliations() {}

#[utoipa::path(
    get,
    path = "/api/admin/refund-reconciliations",
    operation_id = "listAdminRefundReconciliations",
    tag = "退款对账",
    summary = "读取平台退款对账",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的对账事实 ID"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "平台管理员可见的退款成功对账事实", body = RefundReconciliationListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_refund_reconciliations() {}

#[utoipa::path(
    get,
    path = "/api/admin/refunds",
    operation_id = "listAdminRefunds",
    tag = "退款审批",
    summary = "读取退款审批列表",
    params(
        ("after" = Option<i64>, Query, description = "数据库 ID 游标"),
        ("approval_status" = Option<String>, Query, description = "pending、approved 或 rejected"),
        ("limit" = Option<usize>, Query, description = "每页数量，默认 50，最大 100")
    ),
    responses(
        (status = 200, description = "退款审批列表", body = AdminRefundListResponse),
        (status = 400, description = "分页或筛选参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_refunds() {}

#[utoipa::path(
    post,
    path = "/api/admin/refunds/{request_id}/approve",
    operation_id = "approveAdminRefund",
    tag = "退款审批",
    summary = "批准退款",
    params(("request_id" = String, Path, description = "32 位小写十六进制退款请求 ID")),
    request_body = AdminRefundDecisionRequest,
    responses(
        (status = 200, description = "退款已批准，若开启自动提交则返回最新状态", body = AdminRefundListResponse),
        (status = 400, description = "请求 ID 或审批理由无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "退款请求不存在", body = ManagementErrorBody),
        (status = 409, description = "审批状态冲突", body = ManagementErrorBody),
        (status = 502, description = "自动提交被 Provider 明确拒绝", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知或 Provider 不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn approve_admin_refund() {}

#[utoipa::path(
    post,
    path = "/api/admin/refunds/{request_id}/reject",
    operation_id = "rejectAdminRefund",
    tag = "退款审批",
    summary = "拒绝退款",
    params(("request_id" = String, Path, description = "32 位小写十六进制退款请求 ID")),
    request_body = AdminRefundDecisionRequest,
    responses(
        (status = 200, description = "退款已拒绝", body = AdminRefundListResponse),
        (status = 400, description = "请求 ID 或审批理由无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "退款请求不存在", body = ManagementErrorBody),
        (status = 409, description = "审批状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn reject_admin_refund() {}

#[utoipa::path(
    post,
    path = "/api/admin/refunds/{request_id}/submit",
    operation_id = "submitAdminRefund",
    tag = "退款审批",
    summary = "手动提交已批准退款",
    params(("request_id" = String, Path, description = "32 位小写十六进制退款请求 ID")),
    responses(
        (status = 200, description = "退款已提交或返回已存在事实", body = AdminRefundListResponse),
        (status = 400, description = "请求 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "退款请求不存在", body = ManagementErrorBody),
        (status = 409, description = "退款状态不允许提交", body = ManagementErrorBody),
        (status = 503, description = "Provider 不可用或提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn submit_admin_refund() {}

#[utoipa::path(
    post,
    path = "/api/admin/refunds/{request_id}/manual-complete",
    operation_id = "manualCompleteAdminRefund",
    tag = "退款审批",
    summary = "登记易支付人工退款结果",
    params(("request_id" = String, Path, description = "32 位小写十六进制退款请求 ID")),
    request_body = AdminRefundManualCompletionRequest,
    responses(
        (status = 200, description = "人工退款结果已登记", body = AdminRefundListResponse),
        (status = 400, description = "请求字段无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "退款请求不存在", body = ManagementErrorBody),
        (status = 409, description = "人工退款状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn manual_complete_admin_refund() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_account_refund_reconciliations,
        list_organization_refund_reconciliations,
        list_admin_refund_reconciliations,
        list_admin_refunds,
        approve_admin_refund,
        reject_admin_refund,
        submit_admin_refund,
        manual_complete_admin_refund
    ),
    components(schemas(
        AdminRefundRequestResponse,
        AdminRefundManualCompletionRequest,
        AdminRefundOrderKindResponse,
        RefundReconciliationStatusResponse,
        RefundReconciliationEntryResponse,
        RefundReconciliationListResponse
    ))
)]
struct RefundsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RefundsApi::openapi()
}
