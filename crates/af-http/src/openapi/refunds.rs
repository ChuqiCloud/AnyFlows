use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_refunds::{
        AdminRefundDecisionRequest, AdminRefundListResponse, AdminRefundRequestResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/refunds",
    operation_id = "listAdminRefunds",
    tag = "退款审批",
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

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_refunds,
        approve_admin_refund,
        reject_admin_refund,
        submit_admin_refund
    ),
    components(schemas(AdminRefundRequestResponse,))
)]
struct RefundsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RefundsApi::openapi()
}
